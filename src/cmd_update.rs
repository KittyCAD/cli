use anyhow::Result;
use clap::Parser;

/// Update the current running binary to the latest version.
///
/// This function will return an error if the current binary is under Homebrew or if
/// the running version is already the latest version.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdUpdate {}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdUpdate {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        if crate::update::is_under_homebrew()? {
            anyhow::bail!("You are running under Homebrew. Please run `brew update && brew upgrade zoo` instead.");
        }

        // Get the latest release.
        let latest_release = crate::update::get_latest_release_info().await?;
        let current_version = clap::crate_version!();

        if !crate::update::version_greater_then(&latest_release.version, current_version)? {
            anyhow::bail!(
                "You are already running the latest version ({}) of `zoo`.",
                current_version
            );
        }

        let current_binary_path = std::env::current_exe()?;

        let cs = ctx.io.color_scheme();

        writeln!(
            ctx.io.out,
            "Updating from v{} to {}...",
            current_version, latest_release.version
        )?;

        // Download the latest release.
        let temp_latest_binary_path = crate::update::download_binary_to_temp_file(&latest_release.version).await?;

        // Rename the file to that of the current running exe.
        std::fs::rename(temp_latest_binary_path, current_binary_path)?;

        writeln!(
            ctx.io.out,
            "{} Updated to v{}!",
            cs.success_icon(),
            latest_release.version.trim_start_matches('v')
        )?;

        Ok(())
    }
}
