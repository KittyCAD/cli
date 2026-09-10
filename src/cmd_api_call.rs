use anyhow::Result;
use clap::Parser;
use itertools::Itertools;

/// Perform operations on CAD files.
///
///     # convert a step file to an obj file
///     $ zoo file convert ./input.step ./output.obj
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
///    `$ zoo api-call status <id>`
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
        let format = ctx.format(&self.format)?;
        let client = ctx.api_client("")?;

        let mut api_call = client.api_calls().get_async_operation(self.id).await?;

        // If it is a file conversion and there is output, we need to save that output to a file
        // for them.
        if let kittycad::types::AsyncApiCallOutput::FileConversion { outputs, status, .. } = &mut api_call
            && *status == kittycad::types::ApiCallStatus::Completed
            && let Some(files) = outputs
        {
            let path = std::env::current_dir()?;
            for (name, output) in files.iter() {
                if output.is_empty() {
                    anyhow::bail!(
                        "no output was generated for the file conversion! (this is probably a bug in the API) you should report it to support@zoo.dev"
                    );
                }
                let path = path.join(name);
                std::fs::write(&path, &output.0)?;
            }

            let paths = files
                .keys()
                .map(|k| path.join(k))
                .map(|p| p.to_string_lossy().to_string())
                .collect_vec();
            // Tell them where we saved the file.
            ctx.io.write_status(
                &format,
                format_args!("Saved file conversion output(s) to: {}", paths.join(", ")),
            )?;

            // The files are on disk; avoid printing their base64 contents as well.
            *outputs = None;
        }

        match format {
            crate::types::FormatOutput::Json => ctx.io.write_output_json(&serde_json::to_value(&api_call)?)?,
            crate::types::FormatOutput::Yaml => ctx.io.write_output_yaml(&api_call)?,
            crate::types::FormatOutput::Table => {
                let serde_json::Value::Object(fields) = serde_json::to_value(&api_call)? else {
                    anyhow::bail!("Expected an object for the API call status");
                };
                ctx.io
                    .write_output_table_for_vec(fields.into_iter().map(|(property, value)| ApiCallStatusRow {
                        property,
                        value: match value {
                            serde_json::Value::String(value) => value,
                            value => value.to_string(),
                        },
                    }))?;
            }
        }

        Ok(())
    }
}

#[derive(tabled::Tabled)]
#[tabled(rename_all = "PascalCase")]
struct ApiCallStatusRow {
    property: String,
    value: String,
}
