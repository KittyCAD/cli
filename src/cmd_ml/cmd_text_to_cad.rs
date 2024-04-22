use anyhow::Result;
use clap::Parser;
use kcl_lib::engine::EngineManager;

/// Perform Text-to-CAD commands.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdTextToCad {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug, Clone)]
enum SubCommand {
    Export(CmdTextToCadExport),
    Snapshot(CmdTextToCadSnapshot),
    View(CmdTextToCadView),
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdTextToCad {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            SubCommand::Export(cmd) => cmd.run(ctx).await,
            SubCommand::Snapshot(cmd) => cmd.run(ctx).await,
            SubCommand::View(cmd) => cmd.run(ctx).await,
        }
    }
}

/// Run a Text-to-CAD prompt and export it as any other supported CAD file format.
///
///     $ zoo ml text-to-cad export --output-format=obj A 2x4 lego brick
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdTextToCadExport {
    /// Your prompt.
    #[clap(name = "prompt", required = true)]
    pub prompt: Vec<String>,

    /// The path to a directory to output the files.
    /// If not set this will be the current directory.
    #[clap(long, name = "output-dir")]
    pub output_dir: Option<std::path::PathBuf>,

    /// A valid output file format.
    #[clap(short = 't', long = "output-format", value_enum)]
    output_format: kittycad::types::FileExportFormat,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<crate::types::FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdTextToCadExport {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let output_dir = if let Some(output_dir) = &self.output_dir {
            output_dir.clone()
        } else {
            std::env::current_dir()?
        };

        // Make sure the output dir is a directory.
        if !output_dir.is_dir() {
            anyhow::bail!(
                "output directory `{}` does not exist or is not a directory",
                output_dir.to_str().unwrap_or("")
            );
        }

        let prompt = self.prompt.join(" ");

        if prompt.is_empty() {
            anyhow::bail!("prompt cannot be empty");
        }

        let mut model = ctx
            .get_model_for_prompt("", &prompt, self.output_format.clone())
            .await?;

        if let Some(outputs) = model.outputs {
            // Write the contents of the files to the output directory.
            for (filename, data) in outputs.iter() {
                let path = output_dir.clone().join(filename);
                std::fs::write(&path, data)?;
                writeln!(
                    ctx.io.out,
                    "wrote file `{}` to {}",
                    filename,
                    path.to_str().unwrap_or("")
                )?;
            }
        } else {
            anyhow::bail!(
                "no output was generated! (this is probably a bug in the API) you should report it to support@zoo.dev"
            );
        }

        // Reset the outputs field of the model.
        // Otherwise what we print will be crazy big.
        model.outputs = None;

        // Print the output of the conversion.
        let format = ctx.format(&self.format)?;
        ctx.io.write_output(&format, &model)?;

        Ok(())
    }
}

/// Snapshot a render of a Text-to-CAD prompt as any supported image format.
///
///     # snapshot as png
///     $ zoo ml text-to-cad snapshot A 2x4 lego brick
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdTextToCadSnapshot {
    /// Your prompt.
    #[clap(name = "prompt", required = true)]
    pub prompt: Vec<String>,

    /// The path to a directory to output the files.
    /// If not set this will be the current directory.
    #[clap(long, name = "output-dir")]
    pub output_dir: Option<std::path::PathBuf>,

    /// A valid output image format.
    #[clap(short = 't', long = "output-format", value_enum, default_value = "png")]
    output_format: kittycad::types::ImageFormat,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<crate::types::FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdTextToCadSnapshot {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let output_dir = if let Some(output_dir) = &self.output_dir {
            output_dir.clone()
        } else {
            std::env::current_dir()?
        };

        // Make sure the output dir is a directory.
        if !output_dir.is_dir() {
            anyhow::bail!(
                "output directory `{}` does not exist or is not a directory",
                output_dir.to_str().unwrap_or("")
            );
        }

        let prompt = self.prompt.join(" ");

        if prompt.is_empty() {
            anyhow::bail!("prompt cannot be empty");
        }

        let model = ctx
            .get_model_for_prompt("", &prompt, kittycad::types::FileExportFormat::Gltf)
            .await?;

        // Get the gltf bytes.
        let mut gltf_bytes = vec![];
        if let Some(outputs) = &model.outputs {
            for (key, value) in outputs {
                if key.ends_with(".gltf") {
                    gltf_bytes = value.0.clone();
                    break;
                }
            }
        } else {
            anyhow::bail!("Your design completed, but no gltf outputs were found");
        }

        let output_file = prompt.replace(' ', "_").to_lowercase() + "." + &self.output_format.to_string();
        let output_file_path = output_dir.join(&output_file);

        let image_bytes = get_image_bytes(ctx, &gltf_bytes, self.output_format.clone()).await?;
        // Save the snapshot locally.
        std::fs::write(&output_file_path, image_bytes)?;

        writeln!(
            ctx.io.out,
            "Snapshot saved to `{}`",
            output_file_path.to_str().unwrap_or("")
        )?;

        Ok(())
    }
}

/// View a render of a Text-to-CAD prompt in your terminal.
///
///     $ zoo ml text-to-cad view A 2x4 lego brick
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdTextToCadView {
    /// Your prompt.
    #[clap(name = "prompt", required = true)]
    pub prompt: Vec<String>,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<crate::types::FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdTextToCadView {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let prompt = self.prompt.join(" ");

        if prompt.is_empty() {
            anyhow::bail!("prompt cannot be empty");
        }

        let model = ctx
            .get_model_for_prompt("", &prompt, kittycad::types::FileExportFormat::Gltf)
            .await?;

        // Get the gltf bytes.
        let mut gltf_bytes = vec![];
        if let Some(outputs) = &model.outputs {
            for (key, value) in outputs {
                if key.ends_with(".gltf") {
                    gltf_bytes = value.0.clone();
                    break;
                }
            }
        } else {
            anyhow::bail!("Your design completed, but no gltf outputs were found");
        }

        // Create a temporary file to write the snapshot to.
        let mut tmp_file = std::env::temp_dir();
        tmp_file.push(&format!("zoo-text-to-cad-view-{}.png", uuid::Uuid::new_v4()));

        let image_bytes = get_image_bytes(ctx, &gltf_bytes, kittycad::types::ImageFormat::Png).await?;

        // Save the snapshot locally.
        std::fs::write(&tmp_file, image_bytes)?;

        let (width, height) = (ctx.io.tty_size)()?;

        let offset_x = 0;
        let offset_y = 0;
        // Now we setup the terminal viewer.
        let image_conf = viuer::Config {
            // set offset
            x: offset_x,
            y: offset_y,
            // set dimensions
            width: Some(width as u32 - (offset_x * 2) as u32),
            // Make sure to leave the last row at the bottom for the prompt.
            // Which is what the +1 is.
            height: Some(height as u32 - ((offset_y * 2) + 1) as u32),
            ..Default::default()
        };
        viuer::print_from_file(&tmp_file, &image_conf)?;

        // Remove the temporary file.
        std::fs::remove_file(&tmp_file)?;

        Ok(())
    }
}

async fn get_image_bytes(
    ctx: &mut crate::context::Context<'_>,
    gltf_bytes: &[u8],
    output_format: kittycad::types::ImageFormat,
) -> Result<Vec<u8>> {
    let engine = ctx.engine("").await?;

    // Send an import request to the engine.
    let resp = engine
        .send_modeling_cmd(
            uuid::Uuid::new_v4(),
            kcl_lib::executor::SourceRange::default(),
            kittycad::types::ModelingCmd::ImportFiles {
                files: vec![kittycad::types::ImportFile {
                    path: "model.gltf".to_string(),
                    data: gltf_bytes.to_vec(),
                }],
                format: kittycad::types::InputFormat::Gltf {},
            },
        )
        .await?;

    let kittycad::types::OkWebSocketResponseData::Modeling {
        modeling_response: kittycad::types::OkModelingCmdResponse::ImportFiles { data },
    } = &resp
    else {
        anyhow::bail!("Unexpected response from engine import: {:?}", resp);
    };

    let object_id = data.object_id;

    // Zoom on the object.
    engine
        .send_modeling_cmd(
            uuid::Uuid::new_v4(),
            kcl_lib::executor::SourceRange::default(),
            kittycad::types::ModelingCmd::DefaultCameraFocusOn { uuid: object_id },
        )
        .await?;

    // Spin up websockets and do the conversion.
    // This will not return until there are files.
    let resp = engine
        .send_modeling_cmd(
            uuid::Uuid::new_v4(),
            kcl_lib::executor::SourceRange::default(),
            kittycad::types::ModelingCmd::TakeSnapshot { format: output_format },
        )
        .await?;

    if let kittycad::types::OkWebSocketResponseData::Modeling {
        modeling_response: kittycad::types::OkModelingCmdResponse::TakeSnapshot { data },
    } = &resp
    {
        // Save the snapshot locally.
        Ok(data.contents.0.clone())
    } else {
        anyhow::bail!("Unexpected response from engine: {:?}", resp);
    }
}
