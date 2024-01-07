use anyhow::Result;
use clap::Parser;
use itertools::Itertools;

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

#[async_trait::async_trait(?Send)]
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
    #[clap(long, short, value_enum)]
    pub format: Option<crate::types::FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdApiCallStatus {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let client = ctx.api_client("")?;

        let api_call = client.api_calls().get_async_operation(&self.id.to_string()).await?;

        // If it is a file conversion and there is output, we need to save that output to a file
        // for them.
        if let kittycad::types::AsyncApiCallOutput::FileConversion {
            completed_at: _,
            created_at: _,
            error: _,
            id: _,
            output_format: _,
            output_format_options: _,
            outputs,
            src_format: _,
            src_format_options: _,
            started_at: _,
            status,
            updated_at: _,
            user_id: _,
        } = &api_call
        {
            if *status == kittycad::types::ApiCallStatus::Completed {
                if let Some(outputs) = &outputs {
                    let path = std::env::current_dir()?;
                    for (name, output) in outputs {
                        if output.is_empty() {
                            anyhow::bail!("no output was generated for the file conversion! (this is probably a bug in the API) you should report it to support@zoo.dev");
                        }
                        let path = path.join(name);
                        std::fs::write(&path, &output.0)?;
                    }

                    let paths = outputs
                        .keys()
                        .map(|k| path.join(k))
                        .map(|p| p.to_string_lossy().to_string())
                        .collect_vec();
                    // Tell them where we saved the file.
                    writeln!(ctx.io.out, "Saved file conversion output(s) to: {}", paths.join(", "))?;

                    // Return early.
                    return Ok(());
                }
            }
        }

        // Print the output of the conversion.
        // TODO: make this work as a table.
        ctx.io.write_output(&crate::types::FormatOutput::Json, &api_call)?;

        Ok(())
    }
}
