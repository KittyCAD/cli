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
                        Span::styled("ML: ", Style::default().fg(Color::Green)),
                        Span::raw(assistant_buf.clone()),
                    ]));
                    assistant_buf.clear();
                }
                lines.push(Line::from(vec![
                    Span::styled("You: ", Style::default().fg(Color::Cyan)),
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
                            Span::styled("ML: ", Style::default().fg(Color::Green)),
                            Span::raw(assistant_buf.clone()),
                        ]));
                        assistant_buf.clear();
                    }
                }
                kittycad::types::MlCopilotServerMessage::Reasoning(reason) => {
                    if app.show_reasoning {
                        lines.push(Line::from(Span::styled(
                            "reasoning:",
                            Style::default().fg(Color::Magenta),
                        )));
                        for l in crate::context::format_reasoning(reason.clone(), true) {
                            lines.push(Line::from(l));
                        }
                    }
                }
                kittycad::types::MlCopilotServerMessage::Info { text } => {
                    lines.push(Line::from(vec![
                        Span::styled("info: ", Style::default().fg(Color::Yellow)),
                        Span::raw(text.clone()),
                    ]));
                }
                kittycad::types::MlCopilotServerMessage::Error { detail } => {
                    lines.push(Line::from(vec![
                        Span::styled("error: ", Style::default().fg(Color::Red)),
                        Span::raw(detail.clone()),
                    ]));
                }
                kittycad::types::MlCopilotServerMessage::ToolOutput { result } => {
                    lines.push(Line::from(vec![
                        Span::styled("tool: ", Style::default().fg(Color::Yellow)),
                        Span::raw(format!("{result:#?}")),
                    ]));
                }
            },
        }
    }
    if !assistant_buf.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("ML: ", Style::default().fg(Color::Green)),
            Span::raw(assistant_buf),
        ]));
    }
    let messages = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Chat"));
    frame.render_widget(messages, chunks[0]);

    // Input view
    let input = Paragraph::new(app.input.as_str())
        .block(Block::default().borders(Borders::ALL).title("You> "))
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

        // Assert buffer contains "ML: hello world" and "You: make it blue"
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
        assert!(content.contains("You:"));
        assert!(content.contains("make it blue"));
        assert!(content.contains("ML:"));
        assert!(content.contains("hello world"));
    }
}
