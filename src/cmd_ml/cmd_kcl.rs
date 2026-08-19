use anyhow::{Context, Result};
use clap::Parser;

/// KCL machine learning commands.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdKcl {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug, Clone)]
enum SubCommand {
    Copilot(CmdKclCopilot),
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdKcl {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            SubCommand::Copilot(cmd) => cmd.run(ctx).await,
        }
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

#[cfg(test)]
mod test {
    use super::*;

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

        for idx in 0..COPILOT_PROJECT_ENTRY_LIMIT {
            std::fs::write(tmp.path().join(format!("extra-{idx}.kcl")), "cube(2)\n").expect("write extra");
        }

        let err = enforce_project_size(tmp.path()).expect_err("project should be rejected");
        assert!(
            err.to_string().contains("Copilot needs a smaller project"),
            "unexpected error: {err}"
        );
    }
}
