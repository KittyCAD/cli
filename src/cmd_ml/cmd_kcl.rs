use anyhow::{Context, Result};
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
/// Requires the current directory to contain a `main.kcl` file.
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
        // Only allow starting the copilot in a directory containing a main.kcl.
        // Reuse existing project discovery logic to keep error messages consistent.
        let (_, main_path) = ctx.get_code_and_file_path(&std::path::PathBuf::from(".")).await?;

        let project_root = main_path.parent().context("main.kcl is missing a parent directory")?;
        enforce_project_size(project_root)?;

        let host = ctx.global_host().unwrap_or("").to_string();
        crate::ml::copilot::run::run_copilot_tui(ctx, self.project_name.clone(), host).await
    }
}

const COPILOT_PROJECT_ENTRY_LIMIT: usize = 25;

fn enforce_project_size(root: &std::path::Path) -> Result<()> {
    let mut stack = vec![root.to_path_buf()];
    let mut entries = 0usize;

    while let Some(dir) = stack.pop() {
        let meta = std::fs::symlink_metadata(&dir).with_context(|| format!("failed to inspect `{}`", dir.display()))?;
        if !meta.is_dir() {
            continue;
        }

        for entry in std::fs::read_dir(&dir).with_context(|| format!("failed to read directory `{}`", dir.display()))? {
            let entry = entry.with_context(|| format!("failed to read entry under `{}`", dir.display()))?;
            let file_type = entry
                .file_type()
                .with_context(|| format!("failed to inspect `{}`", entry.path().display()))?;

            entries += 1;
            if entries > COPILOT_PROJECT_ENTRY_LIMIT {
                anyhow::bail!(
                    "Project contains more than {COPILOT_PROJECT_ENTRY_LIMIT} files or directories; Copilot needs a smaller project"
                );
            }

            if file_type.is_dir() && !file_type.is_symlink() {
                stack.push(entry.path());
            }
        }
    }

    Ok(())
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

    #[test]
    fn enforce_project_size_allows_small_projects() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        std::fs::write(tmp.path().join("main.kcl"), "cube(1)\n").expect("write main");
        std::fs::write(tmp.path().join("extra.kcl"), "cube(2)\n").expect("write extra");

        enforce_project_size(tmp.path()).expect("project within limit");
    }

    #[test]
    fn enforce_project_size_rejects_large_projects() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        std::fs::write(tmp.path().join("main.kcl"), "cube(1)\n").expect("write main");

        for idx in 0..=(COPILOT_PROJECT_ENTRY_LIMIT) {
            std::fs::write(tmp.path().join(format!("extra-{idx}.kcl")), "cube(2)\n").expect("write extra");
        }

        let err = enforce_project_size(tmp.path()).expect_err("project should be rejected");
        assert!(
            err.to_string().contains("Copilot needs a smaller project"),
            "unexpected error: {err}"
        );
    }
}
