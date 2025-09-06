use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::collections::VecDeque;

#[derive(Debug, Default, Clone)]
pub struct App {
    pub input: String,
    pub events: Vec<ChatEvent>,
    pub show_reasoning: bool,
    pub scanning: bool,
    pub scanned_files: usize,
    pub awaiting_response: bool,
    pub queue: VecDeque<String>,
}

#[derive(Debug, Clone)]
pub enum ChatEvent {
    User(String),
    Server(kittycad::types::MlCopilotServerMessage),
}

impl App {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            events: Vec::new(),
            show_reasoning: true,
            scanning: true,
            scanned_files: 0,
            awaiting_response: false,
            queue: VecDeque::new(),
        }
    }

    /// Handle a key event. Returns Some(submitted_content) when Enter (without Shift) submits.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<String> {
        // Be lenient on kind: some terminals/platforms emit other kinds for Enter.
        match key.code {
            KeyCode::Enter => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.input.push('\n');
                    None
                } else {
                    let submitted = self.input.clone();
                    // Submit even if empty, per product requirement.
                    self.events.push(ChatEvent::User(submitted.clone()));
                    self.input.clear();
                    Some(submitted)
                }
            }
            KeyCode::Backspace => {
                self.input.pop();
                None
            }
            KeyCode::Tab => {
                self.input.push_str("    ");
                None
            }
            KeyCode::Char(c) => {
                let c = if key.modifiers.contains(KeyModifiers::SHIFT) {
                    c.to_ascii_uppercase()
                } else {
                    c
                };
                self.input.push(c);
                None
            }
            _ => None,
        }
    }

    /// Handle a key event with richer outcomes, including Exit on Ctrl+C.
    pub fn handle_key_action(&mut self, key: KeyEvent) -> KeyAction {
        // Ctrl+C always exits
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            if let KeyCode::Char('c') | KeyCode::Char('C') = key.code {
                return KeyAction::Exit;
            }
        }

        match key.code {
            KeyCode::Enter => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.input.push('\n');
                    KeyAction::Inserted
                } else {
                    let submitted = self.input.clone();
                    self.events.push(ChatEvent::User(submitted.clone()));
                    self.input.clear();
                    KeyAction::Submit(submitted)
                }
            }
            KeyCode::Backspace => {
                self.input.pop();
                KeyAction::None
            }
            KeyCode::Tab => {
                self.input.push_str("    ");
                KeyAction::Inserted
            }
            KeyCode::Char(c) => {
                let c = if key.modifiers.contains(KeyModifiers::SHIFT) {
                    c.to_ascii_uppercase()
                } else {
                    c
                };
                self.input.push(c);
                KeyAction::Inserted
            }
            _ => KeyAction::None,
        }
    }

    /// Decide whether to send now or queue, based on files readiness and awaiting state.
    pub fn try_submit(&mut self, content: String, files_ready: bool) -> Option<String> {
        if files_ready && !self.awaiting_response {
            self.awaiting_response = true;
            Some(content)
        } else {
            self.queue.push_back(content);
            None
        }
    }

    /// On EndOfStream, mark not awaiting, and if files are ready and a queue exists, return next to send.
    pub fn on_end_of_stream(&mut self, files_ready: bool) -> Option<String> {
        self.awaiting_response = false;
        if files_ready {
            if let Some(next) = self.queue.pop_front() {
                self.awaiting_response = true;
                return Some(next);
            }
        }
        None
    }

    /// On scanning done, if not awaiting, return next queued to send.
    pub fn on_scan_done(&mut self) -> Option<String> {
        if !self.awaiting_response {
            if let Some(next) = self.queue.pop_front() {
                self.awaiting_response = true;
                return Some(next);
            }
        }
        None
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum KeyAction {
    Submit(String),
    Inserted,
    Exit,
    None,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    #[test]
    fn enter_submits_and_clears_input() {
        let mut app = App::new();
        app.input = "make it blue".into();
        let submitted = app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(submitted.as_deref(), Some("make it blue"));
        assert!(app.input.is_empty());
        match app.events.last().unwrap() {
            ChatEvent::User(s) => assert_eq!(s, "make it blue"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn shift_enter_inserts_newline_and_does_not_submit() {
        let mut app = App::new();
        app.input = "line1".into();
        let out = app.handle_key_action(key(KeyCode::Enter, KeyModifiers::SHIFT));
        assert_eq!(out, KeyAction::Inserted);
        assert_eq!(app.input, "line1\n");
        assert!(app.events.is_empty());
    }

    #[test]
    fn ctrl_c_exits() {
        let mut app = App::new();
        let out = app.handle_key_action(key(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(out, KeyAction::Exit);
        assert!(app.events.is_empty());
    }

    #[test]
    fn single_flight_queue_and_eos_release() {
        let mut app = App::new();
        app.scanning = false;
        // First submit with files ready -> send now
        assert_eq!(app.try_submit("one".into(), true).as_deref(), Some("one"));
        assert!(app.awaiting_response);
        assert!(app.queue.is_empty());
        // Second submit while awaiting -> queued
        assert!(app.try_submit("two".into(), true).is_none());
        assert_eq!(app.queue.len(), 1);
        // EndOfStream -> next queued is released
        assert_eq!(app.on_end_of_stream(true).as_deref(), Some("two"));
        assert!(app.awaiting_response);
        assert!(app.queue.is_empty());
        // Another EOS with no queue -> nothing, awaiting false
        assert!(app.on_end_of_stream(true).is_none());
        assert!(!app.awaiting_response);
    }
}
