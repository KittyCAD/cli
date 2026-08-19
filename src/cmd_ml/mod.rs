use anyhow::Result;
use clap::Parser;

/// Kcl commands.
mod cmd_kcl;

/// Perform machine learning (ML-Ephant) commands.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdMl {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug, Clone)]
enum SubCommand {
    Kcl(crate::cmd_ml::cmd_kcl::CmdKcl),
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdMl {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            SubCommand::Kcl(cmd) => cmd.run(ctx).await,
        }
    }
}
