use anyhow::Result;
use clap::Parser;

/// Prints the version of the program.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdVersion {
    #[doc = "Open the version in the browser."]
    #[clap(short, long)]
    pub web: bool,
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdVersion {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let version = clap::crate_version!();
        let git_hash = git_rev::try_revision_string!();
        let url = changelog_url(version);

        if let Some(gh) = git_hash {
            writeln!(ctx.io.out, "kittycad {version} ({gh})")?;
        } else {
            writeln!(ctx.io.out, "kittycad {version}")?;
        }

        writeln!(ctx.io.out, "{url}")?;

        if self.web {
            ctx.browser("", &url)?;
        }

        Ok(())
    }
}

/// Returns the URL to the changelog for the given version.
pub fn changelog_url(version: &str) -> String {
    format!("https://github.com/KittyCAD/cli/releases/tag/v{version}")
}
