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
    tokio::spawn(async move {
        while let Some(msg) = read.next().await {
            let Ok(msg) = msg else { break };
            if msg.is_text() {
                if let Ok(parsed) = serde_json::from_str::<kittycad::types::MlCopilotServerMessage>(
                    &msg.into_text().unwrap_or_default(),
                ) {
                    let _ = tx_server.send(parsed);
                }
            } else if msg.is_close() {
                break;
            }
        }
    });

    // Start scanning files in the background with progress.
    #[derive(Debug)]
    enum ScanEvent { Progress(usize), Done(std::collections::HashMap<String, Vec<u8>>), Error(String) }
    let (scan_tx, mut scan_rx) = mpsc::unbounded_channel::<ScanEvent>();
    tokio::task::spawn_blocking(move || {
        let mut count = 0usize;
        let mut out = std::collections::HashMap::new();
        let root = match std::env::current_dir() { Ok(p) => p, Err(e) => { let _=scan_tx.send(ScanEvent::Error(format!("{e}"))); return; } };
        fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut std::collections::HashMap<String, Vec<u8>>, count: &mut usize, scan_tx: &mpsc::UnboundedSender<ScanEvent>) {
            if let Ok(rd) = std::fs::read_dir(dir) {
                for ent in rd.flatten() {
                    let path = ent.path();
                    let name = ent.file_name().to_string_lossy().to_string();
                    if let Ok(ft) = ent.file_type() {
                        if ft.is_dir() {
                            if name == ".git" || name == "target" || name == "node_modules" || name.starts_with('.') { continue; }
                            walk(&path, root, out, count, scan_tx);
                        } else if ft.is_file() {
                            let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();
                            if let Ok(bytes) = std::fs::read(&path) { out.insert(rel, bytes); }
                            *count += 1;
                            if *count % 100 == 0 { let _ = scan_tx.send(ScanEvent::Progress(*count)); }
                        }
                    }
                }
            }
        }
        walk(&root, &root, &mut out, &mut count, &scan_tx);
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
                    if ctx.debug { eprintln!("[copilot] key: {key:?}"); }
                    match app.handle_key_action(key) {
                        KeyAction::Exit => { exit = true; }
                        KeyAction::Submit(submit) => {
                            if submit == "/quit" || submit == "/exit" { exit = true; continue; }
                            let files_ready = files_opt.is_some();
                            if let Some(to_send) = app.try_submit(submit, files_ready) {
                                if let Some(files) = &files_opt {
                                    let msg = kittycad::types::MlCopilotClientMessage::User { content: to_send, current_files: Some(files.clone()), project_name: project_name.clone(), source_ranges: None };
                                    let body = serde_json::to_string(&msg)?;
                                    if ctx.debug { eprintln!("[copilot] send user ({} files)", files.len()); }
                                    write.send(Message::Text(body)).await?;
                                }
                            } else if ctx.debug { eprintln!("[copilot] queued user"); }
                        }
                        KeyAction::Inserted | KeyAction::None => {}
                    }
                } else if maybe_ev.is_none() { exit = true; }
            }
            Some(server_msg) = rx_server.recv() => { 
                if ctx.debug { eprintln!("[copilot] server: {:?}", &server_msg); }
                if let kittycad::types::MlCopilotServerMessage::EndOfStream{..} = server_msg { 
                    if let Some(files) = &files_opt {
                        if let Some(next) = app.on_end_of_stream(true) {
                            let msg = kittycad::types::MlCopilotClientMessage::User { content: next, current_files: Some(files.clone()), project_name: project_name.clone(), source_ranges: None };
                            let body = serde_json::to_string(&msg)?;
                            if ctx.debug { eprintln!("[copilot] send queued user after EOS ({} files)", files.len()); }
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
                                let msg = kittycad::types::MlCopilotClientMessage::User { content: next, current_files: Some(files.clone()), project_name: project_name.clone(), source_ranges: None };
                                let body = serde_json::to_string(&msg)?;
                                if ctx.debug { eprintln!("[copilot] send first queued user after scan ({} files)", files.len()); }
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
