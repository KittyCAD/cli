use std::{
    io::Write as _,
    path::{Path, PathBuf},
};

use anyhow::Result;
use crossterm::{
    cursor::MoveTo,
    event::{Event, EventStream},
    execute, queue,
    terminal::{disable_raw_mode, enable_raw_mode, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::{SinkExt, StreamExt};
use kcl_lib::TypedPath;
use log::LevelFilter;
use ratatui::{backend::CrosstermBackend, Terminal};
use similar::TextDiff;
use tokio::sync::mpsc;
use tokio_tungstenite::{
    tungstenite::{protocol::Role, Message},
    WebSocketStream,
};

use crate::ml::copilot::{
    state,
    state::{App, ChatEvent, KeyAction},
    ui::draw,
    util::{join_secure, scan_relevant_files},
};

const PAYLOAD_LOG_LIMIT: usize = 10_000;

// Strongly-typed outbound messages permitted for the WebSocket writer.
enum WsSend {
    Client {
        msg: kittycad::types::MlCopilotClientMessage,
    },
    Ping,
    Close,
}

fn all_files(files: &std::collections::HashMap<String, Vec<u8>>) -> std::collections::HashMap<String, Vec<u8>> {
    files.clone()
}

fn build_user_message(
    content: String,
    files_map: &std::collections::HashMap<String, Vec<u8>>,
    project_name: &Option<String>,
    forced_tools: Vec<kittycad::types::MlCopilotTool>,
) -> (kittycad::types::MlCopilotClientMessage, usize) {
    let files = all_files(files_map);
    let forced_tools = if forced_tools.is_empty() {
        None
    } else {
        Some(forced_tools)
    };
    let msg = kittycad::types::MlCopilotClientMessage::User {
        content,
        current_files: Some(files),
        forced_tools,
        project_name: project_name.clone(),
        source_ranges: None,
        mode: None,
        model: None,
        reasoning_effort: None,
    };
    let len = serde_json::to_string(&msg).map(|s| s.len()).unwrap_or(0);
    (msg, len)
}

fn push_forced_tool_summary(app: &mut App) {
    let summary = if app.forced_tools.is_empty() {
        "Required tools: (none)".to_string()
    } else {
        let joined = app.forced_tool_names().join(", ");
        format!("Required tools: {joined}")
    };
    app.events
        .push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info {
            text: summary,
        }));
}

fn emit_payload_lines(tx: &mpsc::UnboundedSender<kittycad::types::MlCopilotServerMessage>, body: &str) {
    let limit = PAYLOAD_LOG_LIMIT;
    let body = if body.len() > limit {
        format!("{}… (truncated to {limit} of {} bytes)", &body[..limit], body.len())
    } else {
        body.to_string()
    };
    let mut start = 0usize;
    let step = 1000usize;
    while start < body.len() {
        let end = (start + step).min(body.len());
        let chunk = &body[start..end];
        let _ = tx.send(kittycad::types::MlCopilotServerMessage::Info {
            text: chunk.to_string(),
        });
        start = end;
    }
}

pub async fn run_copilot_tui(
    ctx: &mut crate::context::Context<'_>,
    project_name: Option<String>,
    host: String,
) -> Result<()> {
    let client = ctx.api_client(&host)?;

    // Suppress global logging/tracing while the TUI is active to avoid corrupting the UI.
    // We still surface our own debug messages inside the chat pane.
    let _log_guard = if ctx.debug {
        let prev = log::max_level();
        log::set_max_level(LevelFilter::Off);
        Some(prev)
    } else {
        None
    };
    // Setup terminal
    enable_raw_mode()?;
    execute!(std::io::stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let _use_color = ctx.io.color_enabled() && ctx.io.is_stderr_tty();

    // Connect websocket
    let param_conversation_id = None;
    let param_pr = None;
    let param_replay = None;
    let (upgraded, _headers) = client
        .ml()
        .copilot_ws(param_conversation_id, param_pr, param_replay)
        .await?;
    let ws = WebSocketStream::from_raw_socket(upgraded, Role::Client, None).await;
    let (mut write, mut read) = ws.split();

    // Channels
    let (tx_server, mut rx_server) = mpsc::unbounded_channel::<kittycad::types::MlCopilotServerMessage>();
    if ctx.debug {
        app.events
            .push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info {
                text: "[copilot/ws] connected".to_string(),
            }));
    }
    let debug = ctx.debug;
    let tx_server_reader = tx_server.clone();
    tokio::spawn(async move {
        fn truncate(s: &str, n: usize) -> String {
            if s.len() <= n {
                s.to_string()
            } else {
                format!("{}… ({} chars)", &s[..n], s.len())
            }
        }

        // Forward events 1:1 (no batching) so server messages are visible immediately on connect.
        let mut end_reason: Option<String> = None;
        while let Some(msg_res) = read.next().await {
            let msg = match msg_res {
                Ok(m) => m,
                Err(e) => {
                    if debug {
                        let _ = tx_server_reader.send(kittycad::types::MlCopilotServerMessage::Info {
                            text: format!("[copilot/ws<-] reader error: {e}"),
                        });
                    }
                    end_reason = Some(format!("error: {e}"));
                    break;
                }
            };
            if msg.is_text() {
                match msg.into_text() {
                    Ok(t) => {
                        if debug {
                            let _ = tx_server_reader.send(kittycad::types::MlCopilotServerMessage::Info {
                                text: format!("[copilot/ws<-] text {} bytes: {}", t.len(), truncate(&t, 200)),
                            });
                        }
                        match serde_json::from_str::<kittycad::types::MlCopilotServerMessage>(&t) {
                            Ok(parsed) => {
                                let _ = tx_server_reader.send(parsed);
                            }
                            Err(err) => {
                                if debug {
                                    let _ = tx_server_reader.send(kittycad::types::MlCopilotServerMessage::Info {
                                        text: format!("[copilot/ws<-] parse error: {err}"),
                                    });
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if debug {
                            let _ = tx_server_reader.send(kittycad::types::MlCopilotServerMessage::Info {
                                text: format!("[copilot/ws<-] to_text error: {e}"),
                            });
                        }
                    }
                }
            } else if msg.is_binary() {
                let b = msg.into_data();
                if debug {
                    let _ = tx_server_reader.send(kittycad::types::MlCopilotServerMessage::Info {
                        text: format!("[copilot/ws<-] binary {} bytes", b.len()),
                    });
                }
            } else if msg.is_ping() {
                if debug {
                    let _ = tx_server_reader.send(kittycad::types::MlCopilotServerMessage::Info {
                        text: "[copilot/ws<-] ping".to_string(),
                    });
                }
            } else if msg.is_pong() {
                if debug {
                    let _ = tx_server_reader.send(kittycad::types::MlCopilotServerMessage::Info {
                        text: "[copilot/ws<-] pong".to_string(),
                    });
                }
            } else if let Message::Close(cf) = msg {
                if let Some(cf) = cf {
                    if debug {
                        let _ = tx_server_reader.send(kittycad::types::MlCopilotServerMessage::Info {
                            text: format!("[copilot/ws<-] close frame code={} reason='{}'", cf.code, cf.reason),
                        });
                    }
                    end_reason = Some(format!("close code {}", cf.code));
                } else {
                    if debug {
                        let _ = tx_server_reader.send(kittycad::types::MlCopilotServerMessage::Info {
                            text: "[copilot/ws<-] close frame".to_string(),
                        });
                    }
                    end_reason = Some("close frame".to_string());
                }
                break;
            } else if debug {
                let _ = tx_server_reader.send(kittycad::types::MlCopilotServerMessage::Info {
                    text: "[copilot/ws<-] other frame".to_string(),
                });
            }
        }
        let reason = end_reason.unwrap_or_else(|| "eof".to_string());
        if debug {
            let _ = tx_server_reader.send(kittycad::types::MlCopilotServerMessage::Info {
                text: format!("[copilot/ws<-] reader task end ({reason})"),
            });
        }
    });

    // Start scanning files in the background with progress.
    #[derive(Debug)]
    enum ScanEvent {
        Progress(usize),
        Done(std::collections::HashMap<String, Vec<u8>>),
        Error(String),
    }
    let (scan_tx, mut scan_rx) = mpsc::unbounded_channel::<ScanEvent>();
    tokio::task::spawn_blocking(move || {
        let root = match std::env::current_dir() {
            Ok(p) => p,
            Err(e) => {
                let _ = scan_tx.send(ScanEvent::Error(format!("{e}")));
                return;
            }
        };
        let out = scan_relevant_files(&root);
        let count = out.len();
        let _ = scan_tx.send(ScanEvent::Progress(count));
        let _ = scan_tx.send(ScanEvent::Done(out));
    });
    // Dedicated writer task and ping keepalive.
    let (tx_out, mut rx_out) = mpsc::unbounded_channel::<WsSend>();
    let tx_dbg = tx_server.clone();
    let writer_debug = debug;
    let writer_task = tokio::spawn(async move {
        while let Some(out) = rx_out.recv().await {
            match out {
                WsSend::Ping => {
                    if let Err(e) = write.send(Message::Ping(Vec::new())).await {
                        if writer_debug {
                            let _ = tx_dbg.send(kittycad::types::MlCopilotServerMessage::Info {
                                text: format!("[copilot/ws->] writer error: {e}"),
                            });
                        }
                        break;
                    }
                    let _ = write.flush().await;
                    if writer_debug {
                        let _ = tx_dbg.send(kittycad::types::MlCopilotServerMessage::Info {
                            text: "[copilot/ws->] writer flushed".to_string(),
                        });
                    }
                }
                WsSend::Close => {
                    let _ = write.send(Message::Close(None)).await;
                    let _ = write.flush().await;
                    break;
                }
                WsSend::Client { msg } => match serde_json::to_string(&msg) {
                    Ok(body) => {
                        if writer_debug {
                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&body) {
                                if let Some(content) = val.get("content").and_then(|v| v.as_str()) {
                                    let disp = if content.len() > 200 {
                                        format!("{}… ({} chars)", &content[..200], content.len())
                                    } else {
                                        content.to_string()
                                    };
                                    let _ = tx_dbg.send(kittycad::types::MlCopilotServerMessage::Info {
                                        text: format!("[copilot/ws->] content: {disp}"),
                                    });
                                }
                                if let Some(files) = val.get("current_files").and_then(|v| v.as_object()) {
                                    let mut keys: Vec<_> = files.keys().cloned().collect();
                                    keys.sort();
                                    let preview = if keys.len() > 10 {
                                        format!("{}… (+{} more)", keys[..10].join(","), keys.len() - 10)
                                    } else {
                                        keys.join(",")
                                    };
                                    let _ = tx_dbg.send(kittycad::types::MlCopilotServerMessage::Info {
                                        text: format!("[copilot/ws->] files[{}]: {}", files.len(), preview),
                                    });
                                }
                            }
                            let _ = tx_dbg.send(kittycad::types::MlCopilotServerMessage::Info {
                                text: format!("[copilot/ws->] sending client message: {} bytes", body.len()),
                            });
                            let _ = tx_dbg.send(kittycad::types::MlCopilotServerMessage::Info {
                                text: format!("payload ({} bytes):", body.len()),
                            });
                            emit_payload_lines(&tx_dbg, &body);
                        }
                        if let Err(e) = write.send(Message::Text(body)).await {
                            if writer_debug {
                                let _ = tx_dbg.send(kittycad::types::MlCopilotServerMessage::Info {
                                    text: format!("[copilot/ws->] writer error: {e}"),
                                });
                            }
                            break;
                        }
                        let _ = write.flush().await;
                        if writer_debug {
                            let _ = tx_dbg.send(kittycad::types::MlCopilotServerMessage::Info {
                                text: "[copilot/ws->] writer flushed".to_string(),
                            });
                        }
                    }
                    Err(e) => {
                        if writer_debug {
                            let _ = tx_dbg.send(kittycad::types::MlCopilotServerMessage::Error {
                                detail: format!("serialize error: {e}"),
                            });
                        }
                    }
                },
            }
        }
        if writer_debug {
            let _ = tx_dbg.send(kittycad::types::MlCopilotServerMessage::Info {
                text: "[copilot/ws->] writer task end".to_string(),
            });
        }
    });
    let tx_out_ping = tx_out.clone();
    let tx_dbg_ping = tx_server.clone();
    let ping_debug = debug;
    let ping_task = tokio::spawn(async move {
        let mut iv = tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            iv.tick().await;
            if tx_out_ping.send(WsSend::Ping).is_err() {
                break;
            }
            if ping_debug {
                let _ = tx_dbg_ping.send(kittycad::types::MlCopilotServerMessage::Info {
                    text: "[copilot/ws->] ping".to_string(),
                });
            }
        }
    });

    let mut files_opt: Option<std::collections::HashMap<String, Vec<u8>>> = None;
    let mut events = EventStream::new();
    let mut exit = false;
    while !exit {
        terminal.draw(|f| draw(f, &app))?;
        tokio::select! {
            maybe_ev = events.next() => {
                if let Some(Ok(Event::Key(key))) = maybe_ev {
                    match app.handle_key_action(key) {
                        KeyAction::Exit => { exit = true; }
                        KeyAction::Submit(submit) => {
                            if let Some(cmd) = state::parse_slash_command(&submit) {
                                match cmd {
                                    state::SlashCommand::Quit | state::SlashCommand::Exit => { exit = true; }
                                    state::SlashCommand::Accept => {
                                        if let Some(edits) = app.pending_edits.take() {
                                            match apply_pending_edits(&edits) {
                                                Ok(n) => app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: format!("Applied {n} file(s)") })),
                                                Err(e) => app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Error { detail: format!("apply failed: {e}") })),
                                            }
                                        } else {
                                            app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: "No pending changes".to_string() }));
                                        }
                                    }
                                    state::SlashCommand::Reject => {
                                        if app.pending_edits.take().is_some() {
                                            app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: "Discarded pending changes".to_string() }));
                                        } else {
                                            app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: "No pending changes".to_string() }));
                                        }
                                    }
                                    state::SlashCommand::Render => {
                                        if app.pending_edits.is_none() {
                                            app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: "No pending changes to render".to_string() }));
                                        } else {
                                            app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: "Rendering snapshots for old vs new…".to_string() }));
                                            let edits = app.pending_edits.clone().unwrap();
                                            match render_side_by_side(ctx, &host, &edits).await {
                                                Ok(path) => {
                                                    // Try to preview in-terminal; fall back to just a message on error
                                                    if let Err(e) = preview_image_terminal(ctx, &path) {
                                                        app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Error { detail: format!("preview failed: {e}") }));
                                                    }
                                                    app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: format!("Rendered comparison saved to: {}", path.display()) }));
                                                }
                                                Err(e) => {
                                                    app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Error { detail: format!("render failed: {e}") }));
                                                }
                                            }
                                        }
                                    }
                                    state::SlashCommand::System(command) => {
                                        let _ = tx_out.send(WsSend::Client { msg: kittycad::types::MlCopilotClientMessage::System { command } });
                                    }
                                    state::SlashCommand::ForceTool(tool) => {
                                        match app.toggle_forced_tool(tool.clone()) {
                                            state::ForcedToolChange::Added(_) => {
                                                app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info {
                                                    text: format!("Requiring tool `{tool}` for future prompts."),
                                                }));
                                            }
                                            state::ForcedToolChange::Removed(_) => {
                                                app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info {
                                                    text: format!("No longer requiring tool `{tool}`."),
                                                }));
                                            }
                                        }
                                        push_forced_tool_summary(&mut app);
                                    }
                                    state::SlashCommand::ClearForcedTools => {
                                        if app.clear_forced_tools() {
                                            app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info {
                                                text: "Cleared all required tools.".to_string(),
                                            }));
                                        } else {
                                            app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info {
                                                text: "No tools were required.".to_string(),
                                            }));
                                        }
                                        push_forced_tool_summary(&mut app);
                                    }
                                    state::SlashCommand::ShowForcedTools => {
                                        push_forced_tool_summary(&mut app);
                                    }
                                }
                                continue;
                            }
                            let files_ready = files_opt.is_some();
                            if let Some(prompt) = app.try_submit(submit, files_ready) {
                                if let Some(files) = &files_opt {
                                    let state::QueuedPrompt { content, forced_tools } = prompt;
                                    let (msg, _len) =
                                        build_user_message(content, files, &project_name, forced_tools);
                                    let _ = tx_out.send(WsSend::Client { msg });
                                }
                            }
                        }
                        KeyAction::Inserted | KeyAction::None => {}
                    }
                } else if maybe_ev.is_none() { exit = true; }
            }
            Some(server_msg) = rx_server.recv() => {
                if let kittycad::types::MlCopilotServerMessage::EndOfStream{..} = server_msg {
                    if let Some(files) = &files_opt {
                        if let Some(next) = app.on_end_of_stream(true) {
                            let state::QueuedPrompt { content, forced_tools } = next;
                            let (msg, _len) =
                                build_user_message(content, files, &project_name, forced_tools);
                            let _ = tx_out.send(WsSend::Client { msg });
                        }
                    } else {
                        let _ = app.on_end_of_stream(false);
                    }
                } else if let kittycad::types::MlCopilotServerMessage::ToolOutput { result } = &server_msg {
                    handle_tool_output(&mut app, result);
                } else {
                    app.events.push(ChatEvent::Server(server_msg));
                }
            }
            Some(scan_ev) = scan_rx.recv() => {
                match scan_ev {
                    ScanEvent::Progress(n) => { app.scanned_files = n; app.scanning = true; }
                    ScanEvent::Done(map) => {
                        files_opt = Some(map);
                        app.scanning = false;
                        if let Some(files) = &files_opt {
                            if let Some(next) = app.on_scan_done() {
                                let state::QueuedPrompt { content, forced_tools } = next;
                                let (msg, _len) =
                                    build_user_message(content, files, &project_name, forced_tools);
                                let _ = tx_out.send(WsSend::Client { msg });
                            }
                        }
                    }
                    ScanEvent::Error(e) => { app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Error{ detail: e })); }
                }
            }
        }
    }

    // Teardown
    // Attempt graceful close via channel, then end tasks.
    let _ = tx_out.send(WsSend::Close);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    writer_task.abort();
    ping_task.abort();
    disable_raw_mode()?;
    execute!(std::io::stdout(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    // Restore previous log filter if we changed it.
    if let Some(prev) = _log_guard {
        log::set_max_level(prev);
    }
    Ok(())
}

fn handle_tool_output(app: &mut App, result: &kittycad::types::MlToolResult) {
    // Convert to JSON for robust field access across variants.
    let Ok(val) = serde_json::to_value(result) else { return };
    let tool_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let status = val.get("status_code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if let Some(err) = val.get("error").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        let name = match tool_type {
            "text_to_cad" => "TextToCad",
            "edit_kcl_code" => "EditKclCode",
            _ => tool_type,
        };
        app.events
            .push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Error {
                detail: format!("{name} failed (status {status}): {err}"),
            }));
        return;
    }
    // Outputs -> propose diffs
    let outputs = val.get("outputs").and_then(|v| v.as_object());
    if let Some(map) = outputs {
        let mut edits = Vec::new();
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut encountered_error = false;
        for (path, new_val) in map {
            let new = new_val.as_str().unwrap_or("").to_string();
            // Resolve securely under the project root
            let safe_abs = match join_secure(&root, Path::new(path)) {
                Ok(p) => p,
                Err(e) => {
                    app.events
                        .push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Error {
                            detail: format!("Invalid path '{path}': {e}"),
                        }));
                    encountered_error = true;
                    continue;
                }
            };
            // Read old contents; surface errors to the UI instead of silently defaulting.
            let old = match std::fs::read_to_string(&safe_abs) {
                Ok(content) => content,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // NotFound: likely a new file; inform the user as info.
                    app.events
                        .push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info {
                            text: format!(
                                "Note: previous version of '{}' not found; treating as new file.",
                                safe_abs.strip_prefix(&root).unwrap_or(&safe_abs).display()
                            ),
                        }));
                    String::new()
                }
                Err(e) => {
                    // Other IO errors: bubble up as an error so the user can act.
                    app.events
                        .push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Error {
                            detail: format!("Failed to read '{}': {e}", safe_abs.display()),
                        }));
                    encountered_error = true;
                    String::new()
                }
            };
            if encountered_error {
                continue;
            }
            let safe_rel = safe_abs
                .strip_prefix(&root)
                .unwrap_or(&safe_abs)
                .to_string_lossy()
                .to_string();
            let diff = TextDiff::from_lines(&old, &new)
                .unified_diff()
                .context_radius(3)
                .header(&format!("a/{safe_rel}"), &format!("b/{safe_rel}"))
                .to_string();
            let diff_lines: Vec<String> = diff.lines().map(|s| s.to_string()).collect();
            edits.push(state::PendingFileEdit {
                path: safe_rel,
                old,
                new,
                diff_lines,
            });
        }
        if encountered_error {
            app.pending_edits = None;
            app.events
                .push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Error {
                    detail: "Aborting preview due to previous errors.".into(),
                }));
            return;
        }
        app.pending_edits = Some(edits);
        app.events
            .push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info {
                text: format!(
                    "Proposed changes from {} (status {}) — type /accept or /reject",
                    match tool_type {
                        "text_to_cad" => "TextToCad",
                        "edit_kcl_code" => "EditKclCode",
                        _ => tool_type,
                    },
                    status
                ),
            }));
    }
}

fn apply_pending_edits(edits: &[state::PendingFileEdit]) -> anyhow::Result<usize> {
    let mut n = 0usize;
    let root = std::env::current_dir()?;
    for e in edits {
        let pb = join_secure(&root, Path::new(&e.path))?;
        if let Some(parent) = pb.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&pb, &e.new)?;
        n += 1;
    }
    Ok(n)
}

fn get_modeling_settings_from_project_toml(input: &std::path::Path) -> anyhow::Result<kcl_lib::ExecutorSettings> {
    // Start with default settings, set current file
    let mut settings: kcl_lib::ExecutorSettings = Default::default();
    let typed = TypedPath::from(input.display().to_string().as_str());
    settings.with_current_file(typed);

    if input.to_str() == Some("-") {
        return Ok(settings);
    }
    if !input.exists() {
        let input_display = input.display().to_string();
        anyhow::bail!("file `{input_display}` does not exist");
    }
    let dir = if input.is_dir() {
        input.to_path_buf()
    } else {
        input.parent().unwrap().to_path_buf()
    };
    if let Some(p) = crate::cmd_kcl::find_project_toml(&dir)? {
        let s = std::fs::read_to_string(&p)?;
        let project: kcl_lib::ProjectConfiguration = toml::from_str(&s)?;
        let mut derived: kcl_lib::ExecutorSettings = project.into();
        let typed = TypedPath::from(input.display().to_string().as_str());
        derived.with_current_file(typed);
        Ok(derived)
    } else {
        Ok(settings)
    }
}

fn preview_image_terminal(ctx: &mut crate::context::Context<'_>, path: &std::path::Path) -> anyhow::Result<()> {
    let (w, h) = (ctx.io.tty_size)()?;
    let cfg = viuer::Config {
        x: 0,
        y: 0,
        width: Some(w as u32),
        height: Some(h.saturating_sub(1) as u32),
        ..Default::default()
    };
    viuer::print_from_file(path, &cfg)?;
    let mut out = std::io::stdout();
    // Show hint in the bottom line
    queue!(
        out,
        MoveTo(0, h.saturating_sub(1) as u16),
        crossterm::terminal::Clear(ClearType::CurrentLine)
    )?;
    write!(out, "Press any key to return…")?;
    out.flush()?;
    // Block until a key press to return to TUI (raw mode already enabled)
    let _ = crossterm::event::read();
    // Clear the screen so the next TUI draw fully repaints

    queue!(out, crossterm::terminal::Clear(ClearType::All), MoveTo(0, 0))?;
    out.flush()?;
    Ok(())
}

// scan_relevant_files lives in crate::ml::copilot::util

async fn render_side_by_side(
    ctx: &mut crate::context::Context<'_>,
    host: &str,
    edits: &[state::PendingFileEdit],
) -> anyhow::Result<std::path::PathBuf> {
    use kittycad_modeling_cmds::{ImageFormat, ModelingCmd, TakeSnapshot};
    // Determine old code and path from current directory
    let (old_code, old_main_path) = ctx.get_code_and_file_path(&std::path::PathBuf::from(".")).await?;
    let old_settings = get_modeling_settings_from_project_toml(&old_main_path)?;
    let (old_resp, _sd_old) = ctx
        .send_kcl_modeling_cmd(
            host,
            &old_main_path.display().to_string(),
            &old_code,
            ModelingCmd::TakeSnapshot(TakeSnapshot::builder().format(ImageFormat::Png).build()),
            old_settings.clone(),
        )
        .await?;
    let old_png = match old_resp {
        kittycad_modeling_cmds::websocket::OkWebSocketResponseData::Modeling {
            modeling_response: kittycad_modeling_cmds::ok_response::OkModelingCmdResponse::TakeSnapshot(data),
        } => data.contents.0,
        other => anyhow::bail!("unexpected modeling response for old snapshot: {other:?}"),
    };

    // Prepare NEW tree
    let tmp = tempfile::tempdir()?;
    let new_root = tmp.path().to_path_buf();
    let cwd = std::env::current_dir()?;
    for (rel, bytes) in scan_relevant_files(&cwd) {
        let dest = join_secure(&new_root, Path::new(&rel))?;
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, &bytes)?;
    }
    for e in edits {
        let dest = join_secure(&new_root, Path::new(&e.path))?;
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, e.new.as_bytes())?;
    }

    let (new_code, new_main_path) = ctx.get_code_and_file_path(&new_root).await?;
    let mut new_settings = old_settings.clone();
    let typed = TypedPath::from(new_main_path.display().to_string().as_str());
    new_settings.with_current_file(typed);
    let (new_resp, _sd_new) = ctx
        .send_kcl_modeling_cmd(
            host,
            &new_main_path.display().to_string(),
            &new_code,
            ModelingCmd::TakeSnapshot(TakeSnapshot::builder().format(ImageFormat::Png).build()),
            new_settings,
        )
        .await?;
    let new_png = match new_resp {
        kittycad_modeling_cmds::websocket::OkWebSocketResponseData::Modeling {
            modeling_response: kittycad_modeling_cmds::ok_response::OkModelingCmdResponse::TakeSnapshot(data),
        } => data.contents.0,
        other => anyhow::bail!("unexpected modeling response for new snapshot: {other:?}"),
    };

    let left = image::load_from_memory(&old_png)?.to_rgba8();
    let right = image::load_from_memory(&new_png)?.to_rgba8();
    let (lw, lh) = left.dimensions();
    let (rw, rh) = right.dimensions();
    let out_w = lw + rw;
    let out_h = std::cmp::max(lh, rh);
    let mut out = image::RgbaImage::new(out_w, out_h);
    image::imageops::overlay(&mut out, &left, 0, (out_h - lh) as i64 / 2);
    image::imageops::overlay(&mut out, &right, lw as i64, (out_h - rh) as i64 / 2);
    let mut outfile = std::env::temp_dir();
    outfile.push(format!("zoo-copilot-render-{}.png", uuid::Uuid::new_v4()));
    out.save(&outfile)?;
    Ok(outfile)
}

#[cfg(test)]
mod tests {
    // Copilot run.rs tests trimmed; helpers and scanning tests live in util.rs.
    use super::*;

    #[test]
    fn build_user_message_attaches_all() {
        let mut map = std::collections::HashMap::new();
        map.insert("main.kcl".to_string(), b"a".to_vec());
        map.insert("thing.kcl".to_string(), b"b".to_vec());
        map.insert("blah.obj".to_string(), b"c".to_vec());
        let project_name = None;
        let (msg, _len) = build_user_message("hi".into(), &map, &project_name, Vec::new());
        match msg {
            kittycad::types::MlCopilotClientMessage::User {
                current_files: Some(files),
                ..
            } => {
                let mut keys: Vec<_> = files.keys().cloned().collect();
                keys.sort();
                assert_eq!(keys, vec!["blah.obj", "main.kcl", "thing.kcl"]);
            }
            _ => panic!("unexpected client message variant"),
        }
    }

    #[test]
    fn client_message_content_does_not_carryover() {
        let mut files = std::collections::HashMap::new();
        files.insert("main.kcl".to_string(), b"cube(1)".to_vec());
        let project_name = Some("proj".to_string());

        let (m1, _) = build_user_message("first".into(), &files, &project_name, Vec::new());
        let v1 = serde_json::to_value(&m1).unwrap();
        assert_eq!(v1.get("content").unwrap().as_str().unwrap(), "first");

        let (m2, _) = build_user_message("second".into(), &files, &project_name, Vec::new());
        let v2 = serde_json::to_value(&m2).unwrap();
        assert_eq!(v2.get("content").unwrap().as_str().unwrap(), "second");
        assert_ne!(v1, v2);
    }

    #[test]
    fn build_user_message_includes_forced_tools() {
        let mut files = std::collections::HashMap::new();
        files.insert("main.kcl".to_string(), b"cube(1)".to_vec());
        let project_name = None;
        let tools = vec![kittycad::types::MlCopilotTool::EditKclCode];

        let (msg, _) = build_user_message("with tools".into(), &files, &project_name, tools.clone());
        match msg {
            kittycad::types::MlCopilotClientMessage::User { forced_tools, .. } => {
                assert_eq!(forced_tools, Some(tools));
            }
            other => panic!("unexpected client message variant: {other:?}"),
        }
    }

    #[test]
    fn event_loop_two_submits_send_verbatim_and_files() {
        let mut app = App::new();
        let mut files = std::collections::HashMap::new();
        files.insert("main.kcl".to_string(), b"cube(1)".to_vec());
        let project_name = Some("proj".to_string());

        // First submission
        let s1 = app
            .try_submit("hi im jess".into(), true)
            .expect("first should send now");
        let state::QueuedPrompt {
            content: content1,
            forced_tools: tools1,
        } = s1;
        let (m1, _len1) = build_user_message(content1, &files, &project_name, tools1);
        let v1 = serde_json::to_value(&m1).unwrap();
        assert_eq!(v1.get("content").unwrap().as_str().unwrap(), "hi im jess");
        let files1 = v1.get("current_files").unwrap().as_object().unwrap();
        assert!(files1.contains_key("main.kcl"));

        // Simulate end-of-stream to clear awaiting state
        assert!(app.on_end_of_stream(true).is_none());

        // Second submission
        let s2 = app
            .try_submit("can you edit the kcl code to make the button blue".into(), true)
            .expect("second should send now");
        let state::QueuedPrompt {
            content: content2,
            forced_tools: tools2,
        } = s2;
        let (m2, _len2) = build_user_message(content2, &files, &project_name, tools2);
        let v2 = serde_json::to_value(&m2).unwrap();
        assert_eq!(
            v2.get("content").unwrap().as_str().unwrap(),
            "can you edit the kcl code to make the button blue"
        );
        let files2 = v2.get("current_files").unwrap().as_object().unwrap();
        assert!(files2.contains_key("main.kcl"));
    }

    #[test]
    fn tool_output_error_displays_error() {
        let mut app = App::new();
        let val = serde_json::json!({
            "type": "text_to_cad",
            "status_code": 500,
            "error": "boom"
        });
        let tool: kittycad::types::MlToolResult = serde_json::from_value(val).unwrap();
        handle_tool_output(&mut app, &tool);
        // Expect last event to be an Error with details
        match app.events.last().unwrap() {
            ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Error { detail }) => {
                assert!(detail.contains("TextToCad"));
                assert!(detail.contains("500"));
                assert!(detail.contains("boom"));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn tool_output_missing_old_file_bubbles_info() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let mut app = App::new();
        let val = serde_json::json!({
            "type": "edit_kcl_code",
            "status_code": 200,
            "outputs": {"newfile.kcl": "cube(2)\n"}
        });
        let tool: kittycad::types::MlToolResult = serde_json::from_value(val).unwrap();
        handle_tool_output(&mut app, &tool);

        // Expect an Info about missing old file
        let has_info = app.events.iter().any(|e| match e {
            ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text }) => text.contains("not found"),
            _ => false,
        });
        assert!(has_info, "expected info event about missing old file");

        std::env::set_current_dir(cwd).unwrap();
    }

    #[test]
    fn apply_pending_edits_blocks_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let edits = vec![state::PendingFileEdit {
            path: "../evil.txt".into(),
            old: String::new(),
            new: "hacked".into(),
            diff_lines: vec![],
        }];
        let res = apply_pending_edits(&edits);
        assert!(res.is_err(), "expected traversal to be rejected");
        // Ensure outside file not created
        let outside = tmp.path().parent().unwrap().join("evil.txt");
        assert!(!outside.exists());

        std::env::set_current_dir(cwd).unwrap();
    }

    #[test]
    fn join_secure_rejects_absolute_and_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // absolute
        let abs = if cfg!(windows) {
            std::path::Path::new("C:/windows/system32")
        } else {
            std::path::Path::new("/etc/passwd")
        };
        assert!(join_secure(root, abs).is_err());
        // escape via ..
        assert!(join_secure(root, std::path::Path::new("../../evil.txt")).is_err());
    }

    #[test]
    fn tool_output_rejects_path_traversal_and_aborts() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let mut app = App::new();
        let val = serde_json::json!({
            "type": "edit_kcl_code",
            "status_code": 200,
            "outputs": {"../../evil.txt": "cube(2)\n"}
        });
        let tool: kittycad::types::MlToolResult = serde_json::from_value(val).unwrap();
        handle_tool_output(&mut app, &tool);
        assert!(app.pending_edits.is_none());
        // Expect an error mentioning invalid path
        let has_invalid = app.events.iter().any(|e| match e {
            ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Error { detail }) => {
                detail.contains("Invalid path")
            }
            _ => false,
        });
        assert!(has_invalid);

        std::env::set_current_dir(cwd).unwrap();
    }

    #[test]
    fn tool_output_aborts_preview_on_read_error() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        // Create directory named foo.kcl so reading as a file fails with an IO error
        std::fs::create_dir("foo.kcl").unwrap();

        let mut app = App::new();
        let val = serde_json::json!({
            "type": "edit_kcl_code",
            "status_code": 200,
            "outputs": {"foo.kcl": "cube(2)\n"}
        });
        let tool: kittycad::types::MlToolResult = serde_json::from_value(val).unwrap();
        handle_tool_output(&mut app, &tool);
        // Preview should be aborted
        assert!(app.pending_edits.is_none());
        let has_abort = app.events.iter().any(|e| match e {
            ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Error { detail }) => {
                detail.contains("Aborting preview")
            }
            _ => false,
        });
        assert!(has_abort, "expected abort message");

        std::env::set_current_dir(cwd).unwrap();
    }
    #[test]
    fn tool_output_outputs_sets_pending_edits_and_accept_applies() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        // existing file
        std::fs::write("main.kcl", "cube(1)\n").unwrap();

        let mut app = App::new();
        let val = serde_json::json!({
            "type": "edit_kcl_code",
            "status_code": 200,
            "outputs": {"main.kcl": "cube(2)\n"}
        });
        let tool: kittycad::types::MlToolResult = serde_json::from_value(val).unwrap();
        handle_tool_output(&mut app, &tool);
        assert!(app.pending_edits.is_some());
        let edits = app.pending_edits.clone().unwrap();
        assert_eq!(edits.len(), 1);
        assert!(edits[0].diff_lines.iter().any(|l| l.starts_with("-")));
        assert!(edits[0].diff_lines.iter().any(|l| l.starts_with("+")));

        // Apply
        let n = apply_pending_edits(&edits).unwrap();
        assert_eq!(n, 1);
        let new = std::fs::read_to_string("main.kcl").unwrap();
        assert_eq!(new, "cube(2)\n");

        // restore cwd
        std::env::set_current_dir(cwd).unwrap();
    }
}
