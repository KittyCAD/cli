use anyhow::Result;
use clap::Parser;

/// Text-to-CAD commands.
mod cmd_text_to_cad;

/// Perform machine learning (ML-Ephant) commands.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdMl {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug, Clone)]
enum SubCommand {
    #[clap(name = "text-to-cad")]
    TextToCad(crate::cmd_ml::cmd_text_to_cad::CmdTextToCad),
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdMl {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            SubCommand::TextToCad(cmd) => cmd.run(ctx).await,
        }
    }
}
