use anyhow::Result;
use clap::Parser;

/// Edit a KCL file with machine learning.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdKcl {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug, Clone)]
enum SubCommand {
    Edit(CmdKclEdit),
    Copilot(CmdKclCopilot),
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdKcl {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            SubCommand::Edit(cmd) => cmd.run(ctx).await,
            SubCommand::Copilot(cmd) => cmd.run(ctx).await,
        }
    }
}

/// Edit `kcl` file(s) with a prompt.
///
///     $ zoo ml kcl edit --prompt "Make it blue"
///
/// This command outputs the edited `kcl` files back to the same location.
/// We do not output to stdout, because for projects with multiple files,
/// it would be difficult to know which file the output corresponds to.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdKclEdit {
    /// The path to the input file or directory containing a main.kcl file.
    /// We will read in the contents of all the project's `kcl` files.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", required = true)]
    pub input: std::path::PathBuf,

    /// Your prompt.
    #[clap(name = "prompt", required = true)]
    pub prompt: Vec<String>,

    /// The source ranges to edit. This is optional.
    /// If you don't pass this, the entire file will be edited.
    #[clap(name = "source_range", long, short = 'r')]
    pub source_range: Option<String>,

    /// Disable streaming reasoning messages (prints by default).
    #[clap(long = "no-reasoning")]
    pub no_reasoning: bool,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdKclEdit {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        // Get the contents of the input file.
        let (files, filepath) = ctx.collect_kcl_files(&self.input).await?;

        let prompt = self.prompt.join(" ");

        if prompt.is_empty() {
            anyhow::bail!("prompt cannot be empty");
        }

        let source_ranges = if let Some(source_range) = &self.source_range {
            Some(vec![kittycad::types::SourceRangePrompt {
                range: convert_to_source_range(source_range)?,
                prompt: prompt.clone(),
                file: Some(filepath.to_string_lossy().to_string()),
            }])
        } else {
            Default::default()
        };

        let body = kittycad::types::TextToCadMultiFileIterationBody {
            prompt: if source_ranges.is_none() { Some(prompt) } else { None },
            source_ranges,
            project_name: None,
            kcl_version: Some(kcl_lib::version().to_owned()),
            conversation_id: None,
        };

        let model = ctx.get_edit_for_prompt("", &body, files, !self.no_reasoning).await?;

        let Some(outputs) = model.outputs else {
            anyhow::bail!("model did not return any outputs");
        };

        // Write the output to each file locally.
        for (file, output) in outputs {
            // We could do these in parallel...
            tokio::fs::write(&file, output).await?;
            writeln!(ctx.io.out, "Wrote to {file}")?;
        }

        Ok(())
    }
}

/// Start an interactive Copilot chat for KCL in the current project directory.
///
///     $ zoo ml kcl copilot
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdKclCopilot {
    /// Optional project name to associate with messages.
    #[clap(long = "project-name")]
    pub project_name: Option<String>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdKclCopilot {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        use futures::{SinkExt, StreamExt};
        use tokio_tungstenite::{
            tungstenite::{protocol::Role, Message},
            WebSocketStream,
        };

        let client = ctx.api_client("")?;

        // Connect to Copilot websocket.
        let (upgraded, _headers) = client.ml().copilot_ws().await?;
        let ws = WebSocketStream::from_raw_socket(upgraded, Role::Client, None).await;
        let (mut write, mut read) = ws.split();

        // Capture project snapshot (files) once at start.
        let files = gather_cwd_files()?;

        // Reader task printing server messages.
        let mut pending_answer = String::new();
        let use_color = ctx.io.color_enabled() && ctx.io.is_stderr_tty();
        let reader = tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                let Ok(msg) = msg else { break };
                if msg.is_text() {
                    let txt = msg.into_text().unwrap_or_default();
                    if let Ok(server_msg) = serde_json::from_str::<kittycad::types::MlCopilotServerMessage>(&txt) {
                        match server_msg {
                            kittycad::types::MlCopilotServerMessage::Delta { delta } => {
                                use std::io::Write as _;
                                pending_answer.push_str(&delta);
                                let _ = write!(std::io::stdout(), "{delta}");
                                let _ = std::io::stdout().flush();
                            }
                            kittycad::types::MlCopilotServerMessage::EndOfStream { .. } => {
                                use std::io::Write as _;
                                let _ = writeln!(std::io::stdout());
                                let _ = std::io::stdout().flush();
                                pending_answer.clear();
                            }
                            kittycad::types::MlCopilotServerMessage::Reasoning(reason) => {
                                crate::context::print_reasoning(reason, use_color);
                            }
                            kittycad::types::MlCopilotServerMessage::Info { text } => {
                                eprintln!("info: {}", text.trim());
                            }
                            kittycad::types::MlCopilotServerMessage::Error { detail } => {
                                eprintln!("ml error: {}", detail.trim());
                            }
                            kittycad::types::MlCopilotServerMessage::ToolOutput { result } => {
                                eprintln!("tool result: {result:#?}");
                            }
                        }
                    }
                } else if msg.is_close() {
                    break;
                }
            }
        });

        // Helper to send a user message.
        // Simple REPL loop.
        loop {
            use std::io::{stdin, Write as _};
            eprint!("You> ");
            let _ = std::io::stderr().flush();

            let mut line = String::new();
            if stdin().read_line(&mut line)? == 0 {
                break; // EOF
            }
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if line == "/quit" || line == "/exit" {
                break;
            }

            let msg = kittycad::types::MlCopilotClientMessage::User {
                content: line.to_string(),
                current_files: Some(files.clone()),
                project_name: self.project_name.clone(),
                source_ranges: None,
            };
            let body = serde_json::to_string(&msg)?;
            write.send(Message::Text(body)).await?;
        }

        // Try to close nicely and stop reader.
        let _ = write.close().await;
        let _ = reader.await;

        Ok(())
    }
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
                // Best-effort read; skip unreadable files.
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

/// Convert from a string like "4:2-4:5" to a source range.
/// Where 4 is the line number and 2 and 5 are the column numbers.
fn convert_to_source_range(source_range: &str) -> Result<kittycad::types::SourceRange> {
    let parts: Vec<&str> = source_range.split('-').collect();
    if parts.len() != 2 {
        anyhow::bail!("source range must be in the format 'line:column-line:column'");
    }

    let inner_parts_start = parts[0].split(':').collect::<Vec<&str>>();
    if inner_parts_start.len() != 2 {
        anyhow::bail!("source range must be in the format 'line:column'");
    }

    let inner_parts_end = parts[1].split(':').collect::<Vec<&str>>();
    if inner_parts_end.len() != 2 {
        anyhow::bail!("source range must be in the format 'line:column'");
    }

    let start = kittycad::types::SourcePosition {
        line: inner_parts_start[0].parse::<u32>()?,
        column: inner_parts_start[1].parse::<u32>()?,
    };
    let end = kittycad::types::SourcePosition {
        line: inner_parts_end[0].parse::<u32>()?,
        column: inner_parts_end[1].parse::<u32>()?,
    };

    Ok(kittycad::types::SourceRange { start, end })
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_convert_to_source_range() {
        let source_range = "4:2-4:5";
        let result = convert_to_source_range(source_range).unwrap();
        assert_eq!(
            result,
            kittycad::types::SourceRange {
                start: kittycad::types::SourcePosition { line: 4, column: 2 },
                end: kittycad::types::SourcePosition { line: 4, column: 5 }
            }
        );
    }

    #[test]
    fn test_convert_to_source_range_invalid() {
        let source_range = "4:2-4";
        let result = convert_to_source_range(source_range);
        assert!(result.is_err());
    }

    #[test]
    fn test_convert_to_source_range_invalid_inner() {
        let source_range = "4:2-4:5:6";
        let result = convert_to_source_range(source_range);
        assert!(result.is_err());
    }

    #[test]
    fn test_convert_to_source_range_bigger() {
        let source_range = "14:12-15:25";
        let result = convert_to_source_range(source_range).unwrap();
        assert_eq!(
            result,
            kittycad::types::SourceRange {
                start: kittycad::types::SourcePosition { line: 14, column: 12 },
                end: kittycad::types::SourcePosition { line: 15, column: 25 }
            }
        );
    }
}
