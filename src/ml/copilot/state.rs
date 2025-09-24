use std::collections::VecDeque;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone)]
pub struct App {
    pub input: String,
    pub events: Vec<ChatEvent>,
    pub scanning: bool,
    pub scanned_files: usize,
    pub awaiting_response: bool,
    pub queue: VecDeque<QueuedPrompt>,
    pub pending_edits: Option<Vec<PendingFileEdit>>, // prepared diffs to accept/reject
    pub msg_scroll: u16,
    pub diff_scroll: u16,
    pub forced_tools: Vec<kittycad::types::MlCopilotTool>,
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

#[derive(Debug, Clone, PartialEq)]
pub struct QueuedPrompt {
    pub content: String,
    pub forced_tools: Vec<kittycad::types::MlCopilotTool>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SlashCommand {
    Accept,
    Reject,
    Quit,
    Exit,
    System(kittycad::types::MlCopilotSystemCommand),
    Render,
    ForceTool(kittycad::types::MlCopilotTool),
    ClearForcedTools,
    ShowForcedTools,
}

pub fn parse_slash_command(input: &str) -> Option<SlashCommand> {
    let s = input.trim();
    match s {
        "/accept" => return Some(SlashCommand::Accept),
        "/reject" => return Some(SlashCommand::Reject),
        "/quit" => return Some(SlashCommand::Quit),
        "/exit" => return Some(SlashCommand::Exit),
        "/render" => return Some(SlashCommand::Render),
        _ => {}
    }

    if let Some(rest) = s.strip_prefix("/tool") {
        let name = rest.trim();
        if name.is_empty() {
            return Some(SlashCommand::ShowForcedTools);
        }
        if name.eq_ignore_ascii_case("clear") {
            return Some(SlashCommand::ClearForcedTools);
        }
        if let Ok(tool) = name.parse::<kittycad::types::MlCopilotTool>() {
            return Some(SlashCommand::ForceTool(tool));
        }
        let normalized = name.replace('-', "_");
        if normalized != name {
            if let Ok(tool) = normalized.parse::<kittycad::types::MlCopilotTool>() {
                return Some(SlashCommand::ForceTool(tool));
            }
        }
        return None;
    }

    // System commands from the SDK, auto-updating as the enum evolves.
    if let Some(rest) = s.strip_prefix('/') {
        let name = rest.trim();
        if name.is_empty() {
            return None;
        }
        if let Ok(cmd) = name.parse::<kittycad::types::MlCopilotSystemCommand>() {
            return Some(SlashCommand::System(cmd));
        }
    }
    None
}

impl App {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            events: Vec::new(),
            scanning: true,
            scanned_files: 0,
            awaiting_response: false,
            queue: VecDeque::new(),
            pending_edits: None,
            msg_scroll: 0,
            diff_scroll: 0,
            forced_tools: Vec::new(),
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
    pub fn try_submit(&mut self, content: String, files_ready: bool) -> Option<QueuedPrompt> {
        let prompt = QueuedPrompt {
            content,
            forced_tools: self.snapshot_forced_tools(),
        };
        if files_ready && !self.awaiting_response {
            self.awaiting_response = true;
            Some(prompt)
        } else {
            self.queue.push_back(prompt);
            None
        }
    }

    /// On EndOfStream, mark not awaiting, and if files are ready and a queue exists, return next to send.
    pub fn on_end_of_stream(&mut self, files_ready: bool) -> Option<QueuedPrompt> {
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
    pub fn on_scan_done(&mut self) -> Option<QueuedPrompt> {
        if !self.awaiting_response {
            if let Some(next) = self.queue.pop_front() {
                self.awaiting_response = true;
                return Some(next);
            }
        }
        None
    }

    fn snapshot_forced_tools(&self) -> Vec<kittycad::types::MlCopilotTool> {
        self.forced_tools.clone()
    }

    pub fn toggle_forced_tool(&mut self, tool: kittycad::types::MlCopilotTool) -> ForcedToolChange {
        if let Some(pos) = self.forced_tools.iter().position(|t| t == &tool) {
            self.forced_tools.remove(pos);
            ForcedToolChange::Removed(tool)
        } else {
            self.forced_tools.push(tool.clone());
            ForcedToolChange::Added(tool)
        }
    }

    pub fn clear_forced_tools(&mut self) -> bool {
        if self.forced_tools.is_empty() {
            false
        } else {
            self.forced_tools.clear();
            true
        }
    }

    pub fn forced_tool_names(&self) -> Vec<String> {
        self.forced_tools.iter().map(|tool| tool.to_string()).collect()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum KeyAction {
    Submit(String),
    Inserted,
    Exit,
    None,
}

#[derive(Debug, PartialEq)]
pub enum ForcedToolChange {
    Added(kittycad::types::MlCopilotTool),
    Removed(kittycad::types::MlCopilotTool),
}

fn slash_commands() -> Vec<String> {
    let mut cmds = vec![
        "/accept".to_string(),
        "/reject".to_string(),
        "/quit".to_string(),
        "/exit".to_string(),
        "/render".to_string(),
    ];

    #[allow(unused_imports)]
    use clap::ValueEnum;

    // Tool control commands. Include base /tool variants and per-tool selectors.
    cmds.push("/tool".to_string());
    cmds.push("/tool clear".to_string());

    // Append all SDK tool commands, eg: /tool edit_kcl_code, auto-updating as enum evolves.
    for tool in kittycad::types::MlCopilotTool::value_variants() {
        cmds.push(format!("/tool {tool}"));
    }

    // Append all SDK system commands as slash commands, eg: /new, /bye, ...
    for v in kittycad::types::MlCopilotSystemCommand::value_variants() {
        if let Some(pv) = v.to_possible_value() {
            cmds.push(format!("/{}", pv.get_name()));
        } else {
            // Fallback to Display as a best-effort
            cmds.push(format!("/{v}"));
        }
    }
    // Dedup in case of overlap (shouldn't happen, but safe)
    cmds.sort();
    cmds.dedup();
    cmds
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
    let matches: Vec<&str> = cmds
        .iter()
        .map(|s| s.as_str())
        .filter(|c| c.starts_with(current))
        .collect();
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
        let first = app.try_submit("one".into(), true).expect("first prompt");
        assert_eq!(first.content, "one");
        assert!(first.forced_tools.is_empty());
        assert!(app.awaiting_response);
        assert!(app.queue.is_empty());
        // Second submit while awaiting -> queued
        assert!(app.try_submit("two".into(), true).is_none());
        assert_eq!(app.queue.len(), 1);
        // EndOfStream -> next queued is released
        let second = app.on_end_of_stream(true).expect("second prompt");
        assert_eq!(second.content, "two");
        assert!(second.forced_tools.is_empty());
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
        assert_eq!(parse_slash_command("/render"), Some(SlashCommand::Render));
        assert_eq!(parse_slash_command("/nope"), None);
        assert_eq!(parse_slash_command("   /accept   "), Some(SlashCommand::Accept));
    }

    #[test]
    fn parse_slash_system_commands() {
        // These come from kittycad::types::MlCopilotSystemCommand and will auto-update
        // when the SDK adds more variants.
        assert_eq!(
            parse_slash_command("/new"),
            Some(SlashCommand::System(kittycad::types::MlCopilotSystemCommand::New))
        );
        assert_eq!(
            parse_slash_command("/bye"),
            Some(SlashCommand::System(kittycad::types::MlCopilotSystemCommand::Bye))
        );
    }

    #[test]
    fn parse_slash_tool_commands() {
        #[allow(unused_imports)]
        use clap::ValueEnum;

        for tool in kittycad::types::MlCopilotTool::value_variants() {
            let underscore = tool.to_string();
            let cmd = format!("/tool {underscore}");
            assert_eq!(
                parse_slash_command(&cmd),
                Some(SlashCommand::ForceTool(tool.clone())),
                "expected {cmd} to map to ForceTool"
            );

            let hyphen = underscore.replace('_', "-");
            if hyphen != underscore {
                let hyphen_cmd = format!("/tool {hyphen}");
                assert_eq!(
                    parse_slash_command(&hyphen_cmd),
                    Some(SlashCommand::ForceTool(tool.clone())),
                    "expected {hyphen_cmd} to map to ForceTool"
                );
            }
        }

        assert_eq!(parse_slash_command("/tool clear"), Some(SlashCommand::ClearForcedTools));
        assert_eq!(parse_slash_command("/tool"), Some(SlashCommand::ShowForcedTools));
        assert_eq!(parse_slash_command("/tool nope"), None);
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
                assert!(text.contains("/render"));
                assert!(text.contains("/tool"));
            }
            other => panic!("expected Info, got {other:?}"),
        }
    }

    #[test]
    fn tab_autocomplete_system_unique_new() {
        let mut app = App::new();
        app.input = "/n".into();
        let _ = app.handle_key_action(key(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.input, "/new");
    }

    #[test]
    fn tab_autocomplete_system_unique_bye() {
        let mut app = App::new();
        app.input = "/b".into();
        let _ = app.handle_key_action(key(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.input, "/bye");
    }

    #[test]
    fn tab_autocomplete_suggestions_include_system() {
        let mut app = App::new();
        app.input = "/".into();
        let _ = app.handle_key_action(key(KeyCode::Tab, KeyModifiers::NONE));
        let last = app.events.last().unwrap();
        match last {
            ChatEvent::Server(kittycad::types::MlCopilotServerMessage::Info { text }) => {
                assert!(text.contains("/new"));
                assert!(text.contains("/bye"));
                assert!(text.contains("/tool"));
            }
            other => panic!("expected Info, got {other:?}"),
        }
    }

    #[test]
    fn toggle_forced_tool_and_snapshot() {
        #[allow(unused_imports)]
        use clap::ValueEnum;

        let mut app = App::new();
        let tool = kittycad::types::MlCopilotTool::value_variants()[0].clone();

        match app.toggle_forced_tool(tool.clone()) {
            ForcedToolChange::Added(t) => assert_eq!(t, tool),
            other => panic!("expected Added, got {other:?}"),
        }
        assert!(app.forced_tools.iter().any(|t| t == &tool));

        let prompt = app.try_submit("hi".into(), true).expect("prompt ready");
        assert_eq!(prompt.content, "hi");
        assert_eq!(prompt.forced_tools, vec![tool.clone()]);

        match app.toggle_forced_tool(tool.clone()) {
            ForcedToolChange::Removed(t) => assert_eq!(t, tool),
            other => panic!("expected Removed, got {other:?}"),
        }
        assert!(app.forced_tools.is_empty());
    }

    #[test]
    fn queued_prompt_retains_forced_tools_across_delay() {
        #[allow(unused_imports)]
        use clap::ValueEnum;

        let mut app = App::new();
        let tool = kittycad::types::MlCopilotTool::value_variants()[0].clone();
        let _ = app.toggle_forced_tool(tool.clone());

        // Files not ready -> queue prompt capturing forced tools snapshot
        assert!(app.try_submit("queued".into(), false).is_none());
        assert_eq!(app.queue.len(), 1);

        // Mutate forced tools before it sends; snapshot should remain unchanged
        let _ = app.toggle_forced_tool(tool.clone()); // remove
        assert!(app.forced_tools.is_empty());

        let queued = app.on_scan_done().expect("queued prompt");
        assert_eq!(queued.content, "queued");
        assert_eq!(queued.forced_tools, vec![tool.clone()]);
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

    #[test]
    fn lifecycle_two_submits_no_carryover() {
        let mut app = App::new();
        // Files not ready yet; first submit queues
        assert!(app.try_submit("first".into(), false).is_none());
        assert_eq!(app.queue.len(), 1);
        // Scan done → send first
        let s1 = app.on_scan_done();
        assert_eq!(s1.as_ref().map(|p| p.content.as_str()), Some("first"));
        assert!(app.awaiting_response);

        // EOS → allow next
        let s_none = app.on_end_of_stream(true);
        assert!(s_none.is_none());
        assert!(!app.awaiting_response);

        // Second submit should return exactly "second"
        let s2 = app.try_submit("second".into(), true);
        assert_eq!(s2.as_ref().map(|p| p.content.as_str()), Some("second"));
        assert!(app.queue.is_empty());
    }
}
