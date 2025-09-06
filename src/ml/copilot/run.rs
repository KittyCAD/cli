use anyhow::Result;
use crossterm::{
    event::{Event, EventStream},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::{SinkExt, StreamExt};
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

pub async fn run_copilot_tui(ctx: &mut crate::context::Context<'_>, project_name: Option<String>) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    execute!(std::io::stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let _use_color = ctx.io.color_enabled() && ctx.io.is_stderr_tty();

    // Connect websocket
    let client = ctx.api_client("")?;
    let (upgraded, _headers) = client.ml().copilot_ws().await?;
    let ws = WebSocketStream::from_raw_socket(upgraded, Role::Client, None).await;
    let (mut write, mut read) = ws.split();

    // Channels
    let (tx_server, mut rx_server) = mpsc::unbounded_channel::<kittycad::types::MlCopilotServerMessage>();
    let debug = ctx.debug;
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

        while let Some(msg) = read.next().await {
            let Ok(msg) = msg else { break };
            if msg.is_text() {
                match msg.into_text() {
                    Ok(t) => {
                        dbg(
                            format!("[copilot/ws<-] text {} bytes: {}", t.len(), truncate(&t, 200)),
                            debug,
                            &tx_server,
                        );
                        match serde_json::from_str::<kittycad::types::MlCopilotServerMessage>(&t) {
                            Ok(parsed) => {
                                let _ = tx_server.send(parsed);
                            }
                            Err(err) => {
                                dbg(format!("[copilot/ws<-] parse error: {err}"), debug, &tx_server);
                            }
                        }
                    }
                    Err(e) => {
                        dbg(format!("[copilot/ws<-] to_text error: {e}"), debug, &tx_server);
                    }
                }
            } else if msg.is_binary() {
                let b = msg.into_data();
                dbg(format!("[copilot/ws<-] binary {} bytes", b.len()), debug, &tx_server);
            } else if msg.is_ping() {
                dbg("[copilot/ws<-] ping".to_string(), debug, &tx_server);
            } else if msg.is_pong() {
                dbg("[copilot/ws<-] pong".to_string(), debug, &tx_server);
            } else if msg.is_close() {
                dbg("[copilot/ws<-] close frame".to_string(), debug, &tx_server);
                break;
            } else if debug {
                dbg("[copilot/ws<-] other frame".to_string(), debug, &tx_server);
            }
        }
        dbg("[copilot/ws<-] reader task end".to_string(), debug, &tx_server);
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
    let mut files_opt: Option<std::collections::HashMap<String, Vec<u8>>> = None;
    let mut events = EventStream::new();
    let mut exit = false;
    while !exit {
        terminal.draw(|f| draw(f, &app))?;
        tokio::select! {
            maybe_ev = events.next() => {
                if let Some(Ok(Event::Key(key))) = maybe_ev {
                    if ctx.debug { app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: format!("[copilot] key: {key:?}") })); }
                    match app.handle_key_action(key) {
                        KeyAction::Exit => { exit = true; }
                        KeyAction::Submit(submit) => {
                            if submit == "/quit" || submit == "/exit" { exit = true; continue; }
                            let files_ready = files_opt.is_some();
                            if let Some(to_send) = app.try_submit(submit, files_ready) {
                                if let Some(files) = &files_opt {
                                    if ctx.debug {
                                        let disp = if to_send.len() > 200 { format!("{}… ({} chars)", &to_send[..200], to_send.len()) } else { to_send.clone() };
                                        app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: format!("[copilot/ws->] user: {disp}") }));
                                    }
                                    let msg = kittycad::types::MlCopilotClientMessage::User { content: to_send, current_files: Some(files.clone()), project_name: project_name.clone(), source_ranges: None };
                                    let body = serde_json::to_string(&msg)?;
                                    if ctx.debug { app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: format!("[copilot] send user ({} files)", files.len()) })); }
                                    write.send(Message::Text(body)).await?;
                                }
                            } else if ctx.debug { app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: "[copilot] queued user".to_string() })); }
                        }
                        KeyAction::Inserted | KeyAction::None => {}
                    }
                } else if maybe_ev.is_none() { exit = true; }
            }
            Some(server_msg) = rx_server.recv() => {
                if ctx.debug { app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: format!("[copilot] server: {:?}", &server_msg) })); }
                if let kittycad::types::MlCopilotServerMessage::EndOfStream{..} = server_msg {
                    if let Some(files) = &files_opt {
                        if let Some(next) = app.on_end_of_stream(true) {
                            let msg = kittycad::types::MlCopilotClientMessage::User { content: next, current_files: Some(files.clone()), project_name: project_name.clone(), source_ranges: None };
                            let body = serde_json::to_string(&msg)?;
                            if ctx.debug { app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: format!("[copilot] send queued user after EOS ({} files)", files.len()) })); }
                            write.send(Message::Text(body)).await?;
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
                                if ctx.debug {
                                    let disp = if next.len() > 200 { format!("{}… ({} chars)", &next[..200], next.len()) } else { next.clone() };
                                    app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: format!("[copilot/ws->] user(after-scan): {disp}") }));
                                }
                                let msg = kittycad::types::MlCopilotClientMessage::User { content: next, current_files: Some(files.clone()), project_name: project_name.clone(), source_ranges: None };
                                let body = serde_json::to_string(&msg)?;
                                if ctx.debug { app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text: format!("[copilot] send first queued user after scan ({} files)", files.len()) })); }
                                write.send(Message::Text(body)).await?;
                            }
                        }
                    }
                    ScanEvent::Error(e) => { app.events.push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Error{ detail: e })); }
                }
            }
        }
    }

    // Teardown
    let _ = write.close().await;
    disable_raw_mode()?;
    execute!(std::io::stdout(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
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
