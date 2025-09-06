use ratatui::{prelude::*, widgets::*};

use super::state::{App, ChatEvent};

pub fn draw(frame: &mut Frame, app: &App) {
    let size = frame.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // messages
            Constraint::Length(3), // input
        ])
        .split(size);

    // Messages view: merge deltas into assistant lines, show reasoning when enabled
    let mut lines: Vec<Line> = Vec::new();
    let mut assistant_buf = String::new();
    for ev in &app.events {
        match ev {
            ChatEvent::User(s) => {
                if !assistant_buf.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("ML-ephant: ", Style::default().fg(Color::Green)),
                        Span::raw(assistant_buf.clone()),
                    ]));
                    assistant_buf.clear();
                }
                lines.push(Line::from(vec![
                    Span::styled("You> ", Style::default().fg(Color::Cyan)),
                    Span::raw(s.clone()),
                ]));
            }
            ChatEvent::Server(msg) => match msg {
                kittycad::types::MlCopilotServerMessage::Delta { delta } => {
                    assistant_buf.push_str(delta);
                }
                kittycad::types::MlCopilotServerMessage::EndOfStream { .. } => {
                    if !assistant_buf.is_empty() {
                        lines.push(Line::from(vec![
                            Span::styled("ML-ephant> ", Style::default().fg(Color::Green)),
                            Span::raw(assistant_buf.clone()),
                        ]));
                        assistant_buf.clear();
                    }
                }
                kittycad::types::MlCopilotServerMessage::Reasoning(reason) => {
                    if app.show_reasoning {
                        // Dimmed reasoning output from the bot
                        lines.push(Line::from(vec![Span::styled(
                            "ML-ephant (reasoning)> ",
                            Style::default().add_modifier(Modifier::DIM),
                        )]));
                        for l in crate::context::format_reasoning(reason.clone(), false) {
                            lines.push(Line::from(Span::styled(
                                l,
                                Style::default().add_modifier(Modifier::DIM),
                            )));
                        }
                    }
                }
                kittycad::types::MlCopilotServerMessage::Info { text } => {
                    if !assistant_buf.is_empty() {
                        lines.push(Line::from(vec![
                            Span::styled("ML-ephant> ", Style::default().fg(Color::Green)),
                            Span::raw(assistant_buf.clone()),
                        ]));
                        assistant_buf.clear();
                    }
                    lines.push(Line::from(vec![
                        Span::styled("ML-ephant> ", Style::default().fg(Color::Green)),
                        Span::raw(text.clone()),
                    ]));
                }
                kittycad::types::MlCopilotServerMessage::Error { detail } => {
                    if !assistant_buf.is_empty() {
                        lines.push(Line::from(vec![
                            Span::styled("ML-ephant> ", Style::default().fg(Color::Green)),
                            Span::raw(assistant_buf.clone()),
                        ]));
                        assistant_buf.clear();
                    }
                    lines.push(Line::from(vec![
                        Span::styled("ML-ephant> ", Style::default().fg(Color::Green)),
                        Span::styled(detail.clone(), Style::default().fg(Color::Red)),
                    ]));
                }
                kittycad::types::MlCopilotServerMessage::ToolOutput { result } => {
                    if !assistant_buf.is_empty() {
                        lines.push(Line::from(vec![
                            Span::styled("ML-ephant> ", Style::default().fg(Color::Green)),
                            Span::raw(assistant_buf.clone()),
                        ]));
                        assistant_buf.clear();
                    }
                    lines.push(Line::from(vec![
                        Span::styled("ML-ephant> ", Style::default().fg(Color::Green)),
                        Span::styled("tool output → ", Style::default().fg(Color::Yellow)),
                        Span::raw(format!("{result:#?}")),
                    ]));
                }
            },
        }
    }
    if !assistant_buf.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("ML-ephant> ", Style::default().fg(Color::Green)),
            Span::raw(assistant_buf),
        ]));
    }
    let messages = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Chat"));
    frame.render_widget(messages, chunks[0]);

    // If there are pending edits, render a diff preview with accept/reject hint.
    if let Some(edits) = &app.pending_edits {
        let mut diff_lines: Vec<Line> = Vec::new();
        diff_lines.push(Line::from(vec![
            Span::styled("Proposed Changes:", Style::default().fg(Color::Yellow)),
            Span::raw("  type /accept to apply, /reject to discard"),
        ]));
        for edit in edits {
            diff_lines.push(Line::from(vec![
                Span::styled("\nML-ephant> ", Style::default().fg(Color::Green)),
                Span::styled(
                    edit.path.clone(),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
            ]));
            for l in &edit.diff_lines {
                let style = if l.starts_with('+') {
                    Style::default().fg(Color::Green)
                } else if l.starts_with('-') {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default()
                };
                diff_lines.push(Line::from(Span::styled(l.clone(), style)));
            }
        }
        let diffs = Paragraph::new(diff_lines).wrap(Wrap { trim: false });
        frame.render_widget(diffs, chunks[0]);
    }

    // Input view
    let title = if app.scanning {
        format!("You> (Scanning files… {} read)", app.scanned_files)
    } else if app.awaiting_response {
        "You> (Waiting for response…)".to_string()
    } else {
        "You> ".to_string()
    };
    let input = Paragraph::new(app.input.as_str())
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: false });
    frame.render_widget(input, chunks[1]);
}

#[cfg(test)]
mod tests {
    use ratatui::{backend::TestBackend, Terminal};

    use super::*;

    #[test]
    fn render_merges_deltas_and_labels_ml() {
        // Arrange
        let backend = TestBackend::new(60, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();
        app.events.push(ChatEvent::User("make it blue".into()));
        app.events
            .push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info {
                text: "🤖 ML-ephant Copilot".into(),
            }));
        app.events
            .push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Delta {
                delta: "hello".into(),
            }));
        app.events
            .push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Delta {
                delta: " world".into(),
            }));
        app.events.push(ChatEvent::Server(
            kittycad::types::MlCopilotServerMessage::EndOfStream { whole_response: None },
        ));

        // Act
        terminal.draw(|f| draw(f, &app)).unwrap();

        // Assert buffer contains "ML-ephant> hello world" and "You> make it blue"
        let buf = terminal.backend().buffer();
        let screen = buf.area; // just ensure we can scan rows
        let mut content = String::new();
        for y in 0..screen.height {
            let mut line = String::new();
            for x in 0..screen.width {
                line.push(buf.get(x, y).symbol().chars().next().unwrap_or(' '));
            }
            content.push_str(&line);
            content.push('\n');
        }
        assert!(content.contains("You>"));
        assert!(content.contains("make it blue"));
        assert!(content.contains("ML-ephant>"));
        assert!(content.contains("hello world"));
    }

    #[test]
    fn render_waiting_and_scanning_titles() {
        let backend = TestBackend::new(40, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();
        // waiting
        app.scanning = false;
        app.awaiting_response = true;
        terminal.draw(|f| draw(f, &app)).unwrap();
        let buf = terminal.backend().buffer();
        let area = buf.area;
        let mut content = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                content.push(buf.get(x, y).symbol().chars().next().unwrap_or(' '));
            }
            content.push('\n');
        }
        assert!(content.contains("Waiting for response"));

        // scanning
        app.awaiting_response = false;
        app.scanning = true;
        app.scanned_files = 123;
        terminal.draw(|f| draw(f, &app)).unwrap();
        let buf = terminal.backend().buffer();
        let area = buf.area;
        let mut content2 = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                content2.push(buf.get(x, y).symbol().chars().next().unwrap_or(' '));
            }
            content2.push('\n');
        }
        assert!(content2.contains("Scanning files"));
    }
}
