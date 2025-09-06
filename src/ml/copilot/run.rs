use anyhow::Result;
use crossterm::{
    event::{Event, EventStream, KeyCode, KeyModifiers},
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

    let files = gather_cwd_files()?;
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
                            let msg = kittycad::types::MlCopilotClientMessage::User { content: submit, current_files: Some(files.clone()), project_name: project_name.clone(), source_ranges: None };
                            let body = serde_json::to_string(&msg)?;
                            write.send(Message::Text(body)).await?;
                        }
                        KeyAction::Inserted | KeyAction::None => {}
                    }
                } else if maybe_ev.is_none() { exit = true; }
            }
            Some(server_msg) = rx_server.recv() => { app.events.push(ChatEvent::Server(server_msg)); }
        }
    }

    // Teardown
    let _ = write.close().await;
    disable_raw_mode()?;
    execute!(std::io::stdout(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn gather_cwd_files() -> Result<std::collections::HashMap<String, Vec<u8>>> {
    use std::{collections::HashMap, fs, path::Path};
    let root = std::env::current_dir()?;
    let mut out: HashMap<String, Vec<u8>> = HashMap::new();
    fn walk(dir: &Path, root: &Path, out: &mut HashMap<String, Vec<u8>>) -> anyhow::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if entry.file_type()?.is_dir() {
                if name == ".git" || name == "target" || name == "node_modules" || name.starts_with('.') {
                    continue;
                }
                walk(&path, root, out)?;
            } else if entry.file_type()?.is_file() {
                let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();
                if let Ok(bytes) = fs::read(&path) {
                    out.insert(rel, bytes);
                }
            }
        }
        Ok(())
    }
    walk(&root, &root, &mut out)?;
    Ok(out)
}
