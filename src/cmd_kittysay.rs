use anyhow::Result;
use clap::Parser;

/// Prints your text in a text bubble with KittyCAD as ASCII art
///
///     $ kittycad kittysay
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdKittySay {}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdKittySay {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        writeln!(ctx.io.out, "KITTYSAYSTUFF!");
        ctx.browser("", "https://dl.kittycad.io/drake.jpeg")
    }
}
