use std::path::PathBuf;

use anyhow::Result;
use crossterm::{
    event::{Event, EventStream},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::{SinkExt, StreamExt};
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
) -> (kittycad::types::MlCopilotClientMessage, usize) {
    let files = all_files(files_map);
    let msg = kittycad::types::MlCopilotClientMessage::User {
        content,
        current_files: Some(files),
        project_name: project_name.clone(),
        source_ranges: None,
    };
    let len = serde_json::to_string(&msg).map(|s| s.len()).unwrap_or(0);
    (msg, len)
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
    let (upgraded, _headers) = client.ml().copilot_ws().await?;
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
                                }
                                continue;
                            }
                            let files_ready = files_opt.is_some();
                            if let Some(to_send) = app.try_submit(submit, files_ready) {
                                if let Some(files) = &files_opt {
                                    let (msg, _len) = build_user_message(to_send, files, &project_name);
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
                            let (msg, _len) = build_user_message(next, files, &project_name);
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
                                let (msg, _len) = build_user_message(next, files, &project_name);
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
        for (path, new_val) in map {
            let new = new_val.as_str().unwrap_or("").to_string();
            let mut pb = PathBuf::from(path);
            // Normalize path: prevent escaping upward
            if pb.is_absolute() {
                pb = pb
                    .strip_prefix(std::path::MAIN_SEPARATOR.to_string())
                    .unwrap_or(&pb)
                    .to_path_buf();
            }
            let old = std::fs::read_to_string(&pb).unwrap_or_default();
            let diff = TextDiff::from_lines(&old, &new)
                .unified_diff()
                .context_radius(3)
                .header(&format!("a/{path}"), &format!("b/{path}"))
                .to_string();
            let diff_lines: Vec<String> = diff.lines().map(|s| s.to_string()).collect();
            edits.push(state::PendingFileEdit {
                path: path.clone(),
                old,
                new,
                diff_lines,
            });
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
    for e in edits {
        let pb = PathBuf::from(&e.path);
        if let Some(parent) = pb.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&pb, &e.new)?;
        n += 1;
    }
    Ok(n)
}

/// Walk the `root` directory and collect only files with extensions present in
/// `kcl_lib::RELEVANT_FILE_EXTENSIONS`. Returns a map of relative path -> file bytes.
pub(crate) fn scan_relevant_files(root: &std::path::Path) -> std::collections::HashMap<String, Vec<u8>> {
    let mut out = std::collections::HashMap::new();
    fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut std::collections::HashMap<String, Vec<u8>>) {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for ent in rd.flatten() {
                let path = ent.path();
                let name = ent.file_name().to_string_lossy().to_string();
                if let Ok(ft) = ent.file_type() {
                    if ft.is_dir() {
                        if name == ".git" || name == "target" || name == "node_modules" || name.starts_with('.') {
                            continue;
                        }
                        walk(&path, root, out);
                    } else if ft.is_file() {
                        let is_relevant = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(|e| e.to_ascii_lowercase())
                            .map(|e| kcl_lib::RELEVANT_FILE_EXTENSIONS.contains(&e))
                            .unwrap_or(false);
                        if is_relevant {
                            let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();
                            if let Ok(bytes) = std::fs::read(&path) {
                                out.insert(rel, bytes);
                            }
                        }
                    }
                }
            }
        }
    }
    walk(root, root, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn scan_only_relevant_file_extensions() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let root = tmp.path();

        // Relevant files
        std::fs::write(root.join("main.kcl"), b"cube(1)").unwrap();
        std::fs::write(root.join("foo.KCL"), b"sphere(2)").unwrap(); // case-insensitive
        std::fs::create_dir(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/bar.kcl"), b"cylinder(3)").unwrap();

        // Irrelevant files
        std::fs::write(root.join("README.md"), b"docs").unwrap();
        std::fs::write(root.join("notes.txt"), b"hello").unwrap();
        std::fs::create_dir(root.join("target")).unwrap();
        std::fs::write(root.join("target/skip.kcl"), b"should not be read").unwrap();
        std::fs::create_dir(root.join("node_modules")).unwrap();
        std::fs::write(root.join("node_modules/also_skip.kcl"), b"nope").unwrap();
        std::fs::create_dir(root.join(".git")).unwrap();
        std::fs::write(root.join(".git/also_skip.kcl"), b"nope").unwrap();
        std::fs::create_dir(root.join(".hidden")).unwrap();
        std::fs::write(root.join(".hidden/also_skip.kcl"), b"nope").unwrap();

        let files = scan_relevant_files(root);
        let mut keys: Vec<_> = files.keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec!["foo.KCL".to_string(), "main.kcl".to_string(), "sub/bar.kcl".to_string(),]
        );

        // Ensure contents match something small and non-empty
        assert_eq!(files.get("main.kcl").unwrap(), b"cube(1)");
    }

    #[test]
    fn scan_includes_obj_and_kcl_without_extra_list() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let root = tmp.path();
        std::fs::write(root.join("main.kcl"), b"cube(1)\n").unwrap();
        std::fs::write(root.join("thing.kcl"), b"sphere(2)\n").unwrap();
        std::fs::write(root.join("blah.obj"), b"o cube\n").unwrap();
        let files = scan_relevant_files(root);
        let mut keys: Vec<_> = files.keys().cloned().collect();
        keys.sort();
        assert!(keys.contains(&"main.kcl".to_string()));
        assert!(keys.contains(&"thing.kcl".to_string()));
        assert!(keys.contains(&"blah.obj".to_string()));
    }

    #[test]
    fn build_user_message_attaches_all() {
        let mut map = std::collections::HashMap::new();
        map.insert("main.kcl".to_string(), b"a".to_vec());
        map.insert("thing.kcl".to_string(), b"b".to_vec());
        map.insert("blah.obj".to_string(), b"c".to_vec());
        let project_name = None;
        let (msg, _len) = build_user_message("hi".into(), &map, &project_name);
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

        let (m1, _) = build_user_message("first".into(), &files, &project_name);
        let v1 = serde_json::to_value(&m1).unwrap();
        assert_eq!(v1.get("content").unwrap().as_str().unwrap(), "first");

        let (m2, _) = build_user_message("second".into(), &files, &project_name);
        let v2 = serde_json::to_value(&m2).unwrap();
        assert_eq!(v2.get("content").unwrap().as_str().unwrap(), "second");
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
        let (m1, _len1) = build_user_message(s1, &files, &project_name);
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
        let (m2, _len2) = build_user_message(s2, &files, &project_name);
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
