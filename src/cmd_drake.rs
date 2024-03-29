use anyhow::Result;
use clap::Parser;

/// Open a drake meme in your web browser.
///
///     $ zoo drake
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdDrake {}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdDrake {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        ctx.browser("", "https://dl.zoo.dev/drake.jpeg")
    }
}
