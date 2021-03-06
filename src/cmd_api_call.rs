use anyhow::Result;
use clap::Parser;

/// Perform operations on CAD files.
///
///     # convert a step file to an obj file
///     $ kittycad file convert ./input.step ./output.obj
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdApiCall {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug, Clone)]
enum SubCommand {
    Status(CmdApiCallStatus),
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdApiCall {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            SubCommand::Status(cmd) => cmd.run(ctx).await,
        }
    }
}

/// Perform operations for API calls.
///
///     # get the status of an async API call
///     $ kittycad api-call status <id>
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdApiCallStatus {
    /// The ID of the API call.
    #[clap(name = "id", required = true)]
    pub id: uuid::Uuid,

    /// Command output format.
    #[clap(long, short, arg_enum)]
    pub format: Option<crate::types::FormatOutput>,
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdApiCallStatus {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let client = ctx.api_client("")?;

        let api_call = client.api_calls().get_async_operation(&self.id.to_string()).await?;

        // If it is a file conversion and there is output, we need to save that output to a file
        // for them.
        if let kittycad::types::AsyncApiCallOutput::FileConversion(fc) = &api_call {
            if fc.status == kittycad::types::ApiCallStatus::Completed {
                if let Some(output) = &fc.output {
                    if !output.is_empty() {
                        let path = std::env::current_dir()?;
                        let path = path.join(format!("{}.{}", self.id, fc.output_format));
                        std::fs::write(&path, &output.0)?;

                        // Tell them where we saved the file.
                        writeln!(ctx.io.out, "Saved file conversion output to {}", path.display())?;
                        // Return early.
                        return Ok(());
                    } else {
                        anyhow::bail!("no output was generated for the file conversion! (this is probably a bug in the API) you should report it to support@kittycad.io");
                    }
                }
            }
        }

        // Print the output of the conversion.
        // TODO: make this work as a table.
        ctx.io.write_output(&crate::types::FormatOutput::Json, &api_call)?;

        Ok(())
    }
}
