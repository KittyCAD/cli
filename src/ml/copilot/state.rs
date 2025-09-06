use std::collections::VecDeque;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone)]
pub struct App {
    pub input: String,
    pub events: Vec<ChatEvent>,
    pub show_reasoning: bool,
    pub scanning: bool,
    pub scanned_files: usize,
    pub awaiting_response: bool,
    pub queue: VecDeque<String>,
    pub pending_edits: Option<Vec<PendingFileEdit>>, // prepared diffs to accept/reject
    pub msg_scroll: u16,
    pub diff_scroll: u16,
}

#[derive(Debug, Clone)]
pub enum ChatEvent {
    User(String),
    Server(kittycad::types::MlCopilotServerMessage),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingFileEdit {
    pub path: String,
    pub old: String,
    pub new: String,
    pub diff_lines: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashCommand {
    Accept,
    Reject,
    Quit,
    Exit,
}

pub fn parse_slash_command(input: &str) -> Option<SlashCommand> {
    let s = input.trim();
    match s {
        "/accept" => Some(SlashCommand::Accept),
        "/reject" => Some(SlashCommand::Reject),
        "/quit" => Some(SlashCommand::Quit),
        "/exit" => Some(SlashCommand::Exit),
        _ => None,
    }
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
            pending_edits: None,
            msg_scroll: 0,
            diff_scroll: 0,
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
            KeyCode::PageDown => {
                if self.pending_edits.is_some() {
                    self.diff_scroll = self.diff_scroll.saturating_add(10);
                } else {
                    self.msg_scroll = self.msg_scroll.saturating_add(10);
                }
                KeyAction::Inserted
            }
            KeyCode::PageUp => {
                if self.pending_edits.is_some() {
                    self.diff_scroll = self.diff_scroll.saturating_sub(10);
                } else {
                    self.msg_scroll = self.msg_scroll.saturating_sub(10);
                }
                KeyAction::Inserted
            }
            KeyCode::Up => {
                if self.pending_edits.is_some() {
                    self.diff_scroll = self.diff_scroll.saturating_sub(1);
                } else {
                    self.msg_scroll = self.msg_scroll.saturating_sub(1);
                }
                KeyAction::Inserted
            }
            KeyCode::Down => {
                if self.pending_edits.is_some() {
                    self.diff_scroll = self.diff_scroll.saturating_add(1);
                } else {
                    self.msg_scroll = self.msg_scroll.saturating_add(1);
                }
                KeyAction::Inserted
            }
            KeyCode::Backspace => {
                self.input.pop();
                KeyAction::None
            }
            KeyCode::Tab => {
                // Slash command autocomplete
                if self.input.trim_start().starts_with('/') {
                    let current = self.input.trim().to_string();
                    if let Some(completed) = autocomplete_slash(&current) {
                        self.input = completed;
                    } else {
                        // Show suggestions if no progress could be made
                        let sugg = slash_commands().join(" ");
                        self.events
                            .push(ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info {
                                text: format!("suggest: {sugg}"),
                            }));
                    }
                    KeyAction::Inserted
                } else {
                    self.input.push_str("    ");
                    KeyAction::Inserted
                }
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

fn slash_commands() -> Vec<&'static str> {
    vec!["/accept", "/reject", "/quit", "/exit"]
}

fn common_prefix(strings: &[&str]) -> String {
    if strings.is_empty() {
        return String::new();
    }
    let mut prefix = strings[0].to_string();
    for s in strings.iter().skip(1) {
        let mut i = 0;
        let bytes_p = prefix.as_bytes();
        let bytes_s = s.as_bytes();
        while i < bytes_p.len() && i < bytes_s.len() && bytes_p[i] == bytes_s[i] {
            i += 1;
        }
        prefix.truncate(i);
        if prefix.is_empty() {
            break;
        }
    }
    prefix
}

fn autocomplete_slash(current: &str) -> Option<String> {
    let cmds = slash_commands();
    let matches: Vec<&str> = cmds.iter().copied().filter(|c| c.starts_with(current)).collect();
    if matches.is_empty() {
        return None;
    }
    if matches.len() == 1 {
        return Some(matches[0].to_string());
    }
    let cp = common_prefix(&matches);
    if cp.len() > current.len() {
        Some(cp)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyEventKind;

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
        let out = app.handle_key_action(key(KeyCode::Enter, KeyModifiers::NONE));
        match out {
            KeyAction::Submit(ref s) => assert_eq!(s, "make it blue"),
            other => panic!("expected Submit, got {other:?}"),
        }
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

    #[test]
    fn parse_slash_commands() {
        assert_eq!(parse_slash_command("/accept"), Some(SlashCommand::Accept));
        assert_eq!(parse_slash_command("/reject"), Some(SlashCommand::Reject));
        assert_eq!(parse_slash_command("/quit"), Some(SlashCommand::Quit));
        assert_eq!(parse_slash_command("/exit"), Some(SlashCommand::Exit));
        assert_eq!(parse_slash_command("/nope"), None);
        assert_eq!(parse_slash_command("   /accept   "), Some(SlashCommand::Accept));
    }

    #[test]
    fn tab_autocomplete_unique() {
        let mut app = App::new();
        app.input = "/a".into();
        let _ = app.handle_key_action(key(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.input, "/accept");
    }

    #[test]
    fn tab_autocomplete_suggestions() {
        let mut app = App::new();
        app.input = "/".into();
        let _ = app.handle_key_action(key(KeyCode::Tab, KeyModifiers::NONE));
        // input unchanged, suggestions printed
        assert_eq!(app.input, "/");
        let last = app.events.last().unwrap();
        match last {
            ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text }) => {
                assert!(text.contains("/accept"));
                assert!(text.contains("/reject"));
            }
            other => panic!("expected Info, got {other:?}"),
        }
    }

    #[test]
    fn page_down_scrolls_diff() {
        let mut app = App::new();
        app.pending_edits = Some(vec![PendingFileEdit {
            path: "main.kcl".into(),
            old: "".into(),
            new: "".into(),
            diff_lines: vec!["a".into(); 200],
        }]);
        let _ = app.handle_key_action(key(KeyCode::PageDown, KeyModifiers::NONE));
        assert!(app.diff_scroll >= 10);
        let _ = app.handle_key_action(key(KeyCode::PageUp, KeyModifiers::NONE));
        assert!(app.diff_scroll <= 10);
    }

    #[test]
    fn page_down_scrolls_messages_when_no_diff() {
        let mut app = App::new();
        app.pending_edits = None;
        let _ = app.handle_key_action(key(KeyCode::PageDown, KeyModifiers::NONE));
        assert!(app.msg_scroll >= 10);
        let _ = app.handle_key_action(key(KeyCode::PageUp, KeyModifiers::NONE));
        assert!(app.msg_scroll <= 10);
    }
}
