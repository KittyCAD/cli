use ratatui::{prelude::*, widgets::*};

use super::state::{App, ChatEvent};

fn render_markdown_to_lines(md: &str) -> Vec<String> {
    use pulldown_cmark::{Event, Options, Parser, Tag};
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut list_prefix = String::new();
    let _in_code = false;
    let parser = Parser::new_ext(md, Options::all());
    for ev in parser {
        match ev {
            Event::Start(Tag::List(_)) => {
                list_prefix = "- ".to_string();
            }
            Event::End(Tag::List(_)) => {
                if !cur.is_empty() {
                    lines.push(std::mem::take(&mut cur));
                }
                list_prefix.clear();
            }
            Event::Start(Tag::Item) => {
                if !cur.is_empty() {
                    lines.push(std::mem::take(&mut cur));
                }
                cur.push_str(&list_prefix);
            }
            Event::End(Tag::Item) => {
                if !cur.is_empty() {
                    lines.push(std::mem::take(&mut cur));
                }
            }
            Event::Start(Tag::Paragraph) => {
                if !cur.is_empty() {
                    lines.push(std::mem::take(&mut cur));
                }
            }
            Event::End(Tag::Paragraph) => {
                if !cur.is_empty() {
                    lines.push(std::mem::take(&mut cur));
                }
            }
            Event::Start(Tag::Heading(_level, _, _)) => {
                if !cur.is_empty() {
                    lines.push(std::mem::take(&mut cur));
                }
            }
            Event::End(Tag::Heading(_, _, _)) => {
                if !cur.is_empty() {
                    lines.push(std::mem::take(&mut cur));
                }
            }
            Event::Text(t) => cur.push_str(&t),
            Event::Code(t) => {
                cur.push_str(&t);
            }
            Event::SoftBreak | Event::HardBreak => {
                lines.push(std::mem::take(&mut cur));
            }
            Event::Rule => {
                if !cur.is_empty() {
                    lines.push(std::mem::take(&mut cur));
                }
            }
            _ => {}
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(md.to_string());
    }
    lines
}

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
                    for l in crate::context::format_reasoning(reason.clone(), false) {
                        lines.push(Line::from(vec![
                            Span::styled("ML-ephant> ", Style::default().fg(Color::Green)),
                            Span::raw(l),
                        ]));
                    }
                }
                kittycad::types::MlCopilotServerMessage::Info { text } => {
                    // Render info text as markdown, split into lines; print each on its own row.
                    for part in render_markdown_to_lines(text) {
                        lines.push(Line::from(vec![
                            Span::styled("ML-ephant> ", Style::default().fg(Color::Green)),
                            Span::raw(part),
                        ]));
                    }
                }
                kittycad::types::MlCopilotServerMessage::Error { detail } => {
                    for part in detail.split('\n') {
                        lines.push(Line::from(vec![
                            Span::styled("ML-ephant> ", Style::default().fg(Color::Green)),
                            Span::styled(part.to_string(), Style::default().fg(Color::Red)),
                        ]));
                    }
                }
                kittycad::types::MlCopilotServerMessage::ToolOutput { result } => {
                    let raw = format!("{result:#?}");
                    for part in raw.split('\n') {
                        lines.push(Line::from(vec![
                            Span::styled("ML-ephant> ", Style::default().fg(Color::Green)),
                            Span::styled("tool output → ", Style::default().fg(Color::Yellow)),
                            Span::raw(part.to_string()),
                        ]));
                    }
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
    if app.pending_edits.is_none() {
        let messages = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((app.msg_scroll, 0))
            .block(Block::default().borders(Borders::ALL).title("Chat"));
        frame.render_widget(messages, chunks[0]);
    }

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
        // Clamp scroll to available content
        let total = diff_lines.len() as u16;
        let height = chunks[0].height;
        let max_off = total.saturating_sub(height);
        let off = app.diff_scroll.min(max_off);
        let diffs = Paragraph::new(diff_lines)
            .wrap(Wrap { trim: false })
            .scroll((off, 0))
            .block(Block::default().borders(Borders::ALL).title("Chat"));
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

    #[test]
    fn messages_wrap_and_scroll() {
        let backend = TestBackend::new(20, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();
        // very long user message to force wrap
        app.events
            .push(ChatEvent::User("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()));
        // draw without scroll
        terminal.draw(|f| draw(f, &app)).unwrap();
        let buf = terminal.backend().buffer();
        let area = buf.area;
        // Content should include label and long text split across lines
        let mut content = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                content.push(buf.get(x, y).symbol().chars().next().unwrap_or(' '));
            }
            content.push('\n');
        }
        assert!(content.contains("You>"));
        assert!(content.matches('a').count() > 20);

        // Now add many lines and verify scroll moves viewport
        app.msg_scroll = 0;
        for i in 0..20 {
            app.events.push(ChatEvent::User(format!("line {i:02}")));
        }
        terminal.draw(|f| draw(f, &app)).unwrap();
        let content_before = {
            let buf = terminal.backend().buffer();
            let mut s = String::new();
            for y in 0..area.height {
                for x in 0..area.width {
                    s.push(buf.get(x, y).symbol().chars().next().unwrap_or(' '));
                }
                s.push('\n');
            }
            s
        };
        app.msg_scroll = 5;
        terminal.draw(|f| draw(f, &app)).unwrap();
        let content_after = {
            let buf = terminal.backend().buffer();
            let mut s = String::new();
            for y in 0..area.height {
                for x in 0..area.width {
                    s.push(buf.get(x, y).symbol().chars().next().unwrap_or(' '));
                }
                s.push('\n');
            }
            s
        };
        assert_ne!(content_before, content_after);
    }

    #[test]
    fn diff_wraps_and_scrolls() {
        let backend = TestBackend::new(30, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();
        // create a big diff
        let mut lines = Vec::new();
        lines.push("--- a/main.kcl".to_string());
        lines.push("+++ b/main.kcl".to_string());
        for i in 0..100 {
            lines.push(format!("-old line {i}"));
            lines.push(format!("+new line {i}"));
        }
        app.pending_edits = Some(vec![crate::ml::copilot::state::PendingFileEdit {
            path: "main.kcl".into(),
            old: String::new(),
            new: String::new(),
            diff_lines: lines,
        }]);
        app.diff_scroll = 0;
        terminal.draw(|f| draw(f, &app)).unwrap();
        // The header should be visible at top initially
        let buf = terminal.backend().buffer();
        let area = buf.area;
        let mut row0 = String::new();
        for x in 0..area.width {
            row0.push(buf.get(x, 0).symbol().chars().next().unwrap_or(' '));
        }
        assert!(row0.contains("Proposed Changes"));
        // Scroll and ensure top row changes away from header
        app.diff_scroll = 10;
        terminal.draw(|f| draw(f, &app)).unwrap();
        let buf2 = terminal.backend().buffer();
        let mut row0b = String::new();
        for x in 0..area.width {
            row0b.push(buf2.get(x, 0).symbol().chars().next().unwrap_or(' '));
        }
        assert!(!row0b.contains("Proposed Changes"));
    }

    #[test]
    fn no_overlay_when_diff_is_present() {
        let backend = TestBackend::new(80, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();
        // Some regular chat content
        app.events.push(ChatEvent::User("hello".into()));
        app.events
            .push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Delta {
                delta: "Hello".into(),
            }));
        app.events.push(ChatEvent::Server(
            kittycad::types::MlCopilotServerMessage::EndOfStream { whole_response: None },
        ));
        // A diff to show
        let mut lines = Vec::new();
        lines.push("Proposed Changes:".to_string());
        for i in 0..30 {
            lines.push(format!("-old {i}"));
            lines.push(format!("+new {i}"));
        }
        app.pending_edits = Some(vec![crate::ml::copilot::state::PendingFileEdit {
            path: "main.kcl".into(),
            old: String::new(),
            new: String::new(),
            diff_lines: lines,
        }]);
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
        // The diff should be visible and chat messages should not overlay beneath
        assert!(content.contains("Proposed Changes"));
        assert!(!content.contains("You> hello"));
        assert!(!content.contains("ML-ephant> Hello"));
    }

    #[test]
    fn info_markdown_renders_and_splits() {
        let backend = TestBackend::new(60, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();
        let md = "# Title\n\n- item1\n- item2\n\nA `code` span.";
        app.events
            .push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info {
                text: md.into(),
            }));
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
        assert!(content.contains("ML-ephant> Title"));
        assert!(content.contains("ML-ephant> - item1"));
        assert!(content.contains("ML-ephant> - item2"));
        assert!(content.contains("ML-ephant> A code span."));
    }

    #[test]
    fn two_turns_no_prepend() {
        let backend = TestBackend::new(60, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();
        // Turn 1
        app.events.push(ChatEvent::User("first".into()));
        app.events
            .push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Delta {
                delta: "Hello".into(),
            }));
        app.events
            .push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Delta {
                delta: ", Jess".into(),
            }));
        app.events.push(ChatEvent::Server(
            kittycad::types::MlCopilotServerMessage::EndOfStream { whole_response: None },
        ));
        // Turn 2
        app.events.push(ChatEvent::User("second".into()));
        app.events
            .push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Delta {
                delta: "Hi again".into(),
            }));
        app.events.push(ChatEvent::Server(
            kittycad::types::MlCopilotServerMessage::EndOfStream { whole_response: None },
        ));

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

        // Ensure ordering and no prepend of first into second
        let p1 = content.find("You> first").expect("missing first user line");
        let p2 = content.find("You> second").expect("missing second user line");
        assert!(p2 > p1);
        assert!(
            !content.contains("firstYou> second"),
            "first message prepended to second user line"
        );
        // Assistant segments present separately
        assert!(content.contains("ML-ephant> Hello, Jess"));
        assert!(content.contains("ML-ephant> Hi again"));
    }
}
