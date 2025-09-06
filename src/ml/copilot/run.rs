use anyhow::Result;
use crossterm::{
    event::{Event, EventStream},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::{SinkExt, StreamExt};
use log::LevelFilter;
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::mpsc;
use tokio_tungstenite::{
    tungstenite::{protocol::Role, Message},
    WebSocketStream,
};

use crate::ml::copilot::{
    state::{App, ChatEvent, KeyAction},
    ui::draw,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum AttachMode {
    All,
    MainOnly,
    None,
}

fn attach_mode_from_env() -> AttachMode {
    match std::env::var("ZOO_COPILOT_ATTACH")
        .unwrap_or_else(|_| "all".to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "none" => AttachMode::None,
        "main" | "main-only" => AttachMode::MainOnly,
        _ => AttachMode::All,
    }
}

fn max_json_len_from_env() -> usize {
    std::env::var("ZOO_COPILOT_MAX_JSON")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(50_000)
}

fn select_files_for_mode(
    mut files: std::collections::HashMap<String, Vec<u8>>,
    mode: AttachMode,
) -> std::collections::HashMap<String, Vec<u8>> {
    match mode {
        AttachMode::All => files,
        AttachMode::None => std::collections::HashMap::new(),
        AttachMode::MainOnly => {
            let mut out = std::collections::HashMap::new();
            // Prefer root main.kcl; otherwise first *.kcl
            if let Some(v) = files.remove("main.kcl") {
                out.insert("main.kcl".to_string(), v);
                return out;
            }
            if let Some((k, v)) = files
                .into_iter()
                .find(|(k, _)| k.to_ascii_lowercase().ends_with(".kcl"))
            {
                out.insert(k, v);
            }
            out
        }
    }
}

fn build_user_body_with_fallback(
    content: String,
    files_map: &std::collections::HashMap<String, Vec<u8>>,
    project_name: &Option<String>,
    shrink_after_first_send: bool,
) -> (String, AttachMode, bool) {
    let mut mode = if shrink_after_first_send {
        AttachMode::None
    } else {
        attach_mode_from_env()
    };
    let max_len = max_json_len_from_env();
    let mut shrunk = false;
    loop {
        let files = match mode {
            AttachMode::All => files_map.clone(),
            AttachMode::MainOnly => select_files_for_mode(files_map.clone(), AttachMode::MainOnly),
            AttachMode::None => std::collections::HashMap::new(),
        };
        let msg = kittycad::types::MlCopilotClientMessage::User {
            content: content.clone(),
            current_files: Some(files),
            project_name: project_name.clone(),
            source_ranges: None,
        };
        if let Ok(body) = serde_json::to_string(&msg) {
            if body.len() <= max_len || mode == AttachMode::None {
                return (body, mode, shrunk);
            }
            // shrink
            shrunk = true;
            mode = match mode {
                AttachMode::All => AttachMode::MainOnly,
                AttachMode::MainOnly | AttachMode::None => AttachMode::None,
            };
            continue;
        } else {
            // if serialization fails, fallback to no files
            mode = AttachMode::None;
            shrunk = true;
            continue;
        }
    }
}

fn should_log_payload() -> bool {
    match std::env::var("ZOO_COPILOT_LOG_PAYLOAD") {
        Ok(v) => {
            let v = v.to_ascii_lowercase();
            !(v == "0" || v == "false" || v == "no")
        }
        Err(_) => true,
    }
}

fn payload_log_limit() -> usize {
    std::env::var("ZOO_COPILOT_LOG_LIMIT")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(10_000)
}

fn push_payload_lines(app: &mut App, body: &str) {
    let limit = payload_log_limit();
    let body = if body.len() > limit {
        format!("{}… (truncated to {limit} of {} bytes)", &body[..limit], body.len())
    } else {
        body.to_string()
    };
    // Chunk into ~1000 byte lines for readability.
    let mut start = 0usize;
    let step = 1000usize;
    while start < body.len() {
        let end = (start + step).min(body.len());
        let chunk = &body[start..end];
        app.events
            .push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info {
                text: chunk.to_string(),
            }));
        start = end;
    }
}

pub async fn run_copilot_tui(ctx: &mut crate::context::Context<'_>, project_name: Option<String>) -> Result<()> {
    // Preflight: ensure we are authenticated before starting the TUI.
    let client = ctx.api_client("")?;
    if let Err(err) = client.users().get_self().await {
        anyhow::bail!("Authentication failed or missing. Try `zoo auth login` (details: {err})");
    }

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

        let dbg = |text: String, debug: bool, tx: &mpsc::UnboundedSender<kittycad::types::MlCopilotServerMessage>| {
            if debug {
                let _ = tx.send(kittycad::types::MlCopilotServerMessage::Info { text });
            }
        };

        let mut end_reason: Option<String> = None;
        while let Some(msg_res) = read.next().await {
            let msg = match msg_res {
                Ok(m) => m,
                Err(e) => {
                    dbg(format!("[copilot/ws<-] reader error: {e}"), debug, &tx_server_reader);
                    end_reason = Some(format!("error: {e}"));
                    break;
                }
            };
            if msg.is_text() {
                match msg.into_text() {
                    Ok(t) => {
                        dbg(
                            format!("[copilot/ws<-] text {} bytes: {}", t.len(), truncate(&t, 200)),
                            debug,
                            &tx_server_reader,
                        );
                        match serde_json::from_str::<kittycad::types::MlCopilotServerMessage>(&t) {
                            Ok(parsed) => {
                                let _ = tx_server_reader.send(parsed);
                            }
                            Err(err) => {
                                dbg(format!("[copilot/ws<-] parse error: {err}"), debug, &tx_server_reader);
                            }
                        }
                    }
                    Err(e) => {
                        dbg(format!("[copilot/ws<-] to_text error: {e}"), debug, &tx_server_reader);
                    }
                }
            } else if msg.is_binary() {
                let b = msg.into_data();
                dbg(
                    format!("[copilot/ws<-] binary {} bytes", b.len()),
                    debug,
                    &tx_server_reader,
                );
            } else if msg.is_ping() {
                dbg("[copilot/ws<-] ping".to_string(), debug, &tx_server_reader);
            } else if msg.is_pong() {
                dbg("[copilot/ws<-] pong".to_string(), debug, &tx_server_reader);
            } else if let Message::Close(cf) = msg {
                if let Some(cf) = cf {
                    dbg(
                        format!("[copilot/ws<-] close frame code={} reason='{}'", cf.code, cf.reason),
                        debug,
                        &tx_server_reader,
                    );
                    end_reason = Some(format!("close code {}", cf.code));
                } else {
                    dbg("[copilot/ws<-] close frame".to_string(), debug, &tx_server_reader);
                    end_reason = Some("close frame".to_string());
                }
                break;
            } else if debug {
                dbg("[copilot/ws<-] other frame".to_string(), debug, &tx_server_reader);
            }
        }
        let reason = end_reason.unwrap_or_else(|| "eof".to_string());
        dbg(
            format!("[copilot/ws<-] reader task end ({reason})"),
            debug,
            &tx_server_reader,
        );
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
    let (tx_out, mut rx_out) = mpsc::unbounded_channel::<Message>();
    let tx_dbg = tx_server.clone();
    let writer_debug = debug;
    let writer_task = tokio::spawn(async move {
        while let Some(msg) = rx_out.recv().await {
            if let Err(e) = write.send(msg).await {
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
            if tx_out_ping.send(Message::Ping(Vec::new())).is_err() {
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
                            if submit == "/quit" || submit == "/exit" { exit = true; continue; }
                            let files_ready = files_opt.is_some();
                            if let Some(to_send) = app.try_submit(submit, files_ready) {
                                if let Some(files) = &files_opt {
                                    let (body, mode, shrunk) = build_user_body_with_fallback(to_send, files, &project_name, app.sent_files_once);
                                    if ctx.debug {
                                        let mut note = format!("[copilot/ws->] sending client message: {} bytes, files={} (mode={:?})", body.len(), files.len(), mode);
                                        if shrunk { note.push_str(" [payload shrunk]"); }
                                        app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: note }));
                                    }
                                    if ctx.debug && should_log_payload() {
                                        app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: format!("payload ({} bytes):", body.len()) }));
                                        push_payload_lines(&mut app, &body);
                                    }
                                    let _ = tx_out.send(Message::Text(body));
                                    if ctx.debug { app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: "[copilot/ws->] sent".to_string() })); }
                                    app.sent_files_once = true;
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
                            let (body, mode, shrunk) = build_user_body_with_fallback(next, files, &project_name, app.sent_files_once);
                            if ctx.debug {
                                let mut note = format!("[copilot/ws->] sending client message (after EOS): {} bytes, files={} (mode={:?})", body.len(), files.len(), mode);
                                if shrunk { note.push_str(" [payload shrunk]"); }
                                app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: note }));
                            }
                            if ctx.debug && should_log_payload() {
                                app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: format!("payload ({} bytes) [after EOS]:", body.len()) }));
                                push_payload_lines(&mut app, &body);
                            }
                            let _ = tx_out.send(Message::Text(body));
                            if ctx.debug { app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: "[copilot/ws->] sent".to_string() })); }
                            app.sent_files_once = true;
                        }
                    } else {
                        let _ = app.on_end_of_stream(false);
                    }
                }
                app.events.push(ChatEvent::Server(server_msg));
            }
            Some(scan_ev) = scan_rx.recv() => {
                match scan_ev {
                    ScanEvent::Progress(n) => { app.scanned_files = n; app.scanning = true; }
                    ScanEvent::Done(map) => {
                        files_opt = Some(map);
                        app.scanning = false;
                        if let Some(files) = &files_opt {
                            if let Some(next) = app.on_scan_done() {
                                let (body, mode, shrunk) = build_user_body_with_fallback(next, files, &project_name, app.sent_files_once);
                                if ctx.debug {
                                    let mut note = format!("[copilot/ws->] sending client message (after scan): {} bytes, files={} (mode={:?})", body.len(), files.len(), mode);
                                    if shrunk { note.push_str(" [payload shrunk]"); }
                                    app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: note }));
                                }
                                if ctx.debug && should_log_payload() {
                                    app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: format!("payload ({} bytes) [after scan]:", body.len()) }));
                                    push_payload_lines(&mut app, &body);
                                }
                                let _ = tx_out.send(Message::Text(body));
                                if ctx.debug { app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: "[copilot/ws->] sent".to_string() })); }
                                app.sent_files_once = true;
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
    let _ = tx_out.send(Message::Close(None));
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
}
