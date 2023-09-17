use std::str::FromStr;

use anyhow::Result;
use clap::Parser;

/// Perform actions on `kcl` files.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdKcl {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug, Clone)]
enum SubCommand {
    Export(CmdKclExport),
    Snapshot(CmdKclSnapshot),
    View(CmdKclView),
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdKcl {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            SubCommand::Export(cmd) => cmd.run(ctx).await,
            SubCommand::Snapshot(cmd) => cmd.run(ctx).await,
            SubCommand::View(cmd) => cmd.run(ctx).await,
        }
    }
}

/// Export a `kcl` file as any other supported CAD file format.
///
///     # convert kcl to obj
///     $ kittycad kcl export --output-format=obj my-file.kcl output_dir
///
///     # convert kcl to step
///     $ kittycad kcl export --output-format=step my-obj.kcl .
///
///     # pass a file to convert from stdin
///     $ cat my-obj.kcl | kittycad kcl export --output-format=step - output_dir
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdKclExport {
    /// The path to the input kcl file to export.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", required = true)]
    pub input: std::path::PathBuf,

    /// The path to a directory to output the files.
    #[clap(name = "output-dir", required = true)]
    pub output_dir: std::path::PathBuf,

    /// A valid output file format.
    #[clap(short = 't', long = "output-format", value_enum)]
    output_format: kittycad::types::FileExportFormat,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<crate::types::FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdKclExport {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        // Make sure the output dir is a directory.
        if !self.output_dir.is_dir() {
            anyhow::bail!(
                "output directory `{}` does not exist or is not a directory",
                self.output_dir.to_str().unwrap_or("")
            );
        }

        // Get the contents of the input file.
        let input = ctx.read_file(self.input.to_str().unwrap_or(""))?;
        // Parse the input as a string.
        let input = std::str::from_utf8(&input)?;

        // Spin up websockets and do the conversion.
        // This will not return until there are files.
        let resp = ctx
            .send_modeling_cmd(
                "",
                input,
                kittycad::types::ModelingCmd::Export {
                    entity_ids: vec![],
                    format: get_output_format(&self.output_format),
                },
            )
            .await?;

        if let kittycad::types::OkWebSocketResponseData::Export { files } = resp {
            // Save the files to our export directory.
            for file in files {
                let path = self.output_dir.join(file.name);
                std::fs::write(&path, file.contents)?;
                println!("Wrote file: {}", path.display());
            }
        } else {
            anyhow::bail!("Unexpected response from engine: {:?}", resp);
        }

        Ok(())
    }
}

/// Snapshot a render of a `kcl` file as any supported image format.
///
///     # snapshot as png
///     $ kittycad kcl snapshot my-file.kcl my-file.png
///
///     # pass a file to snapshot from stdin
///     $ cat my-obj.kcl | kittycad kcl snapshot --output-format=png - my-file.png
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdKclSnapshot {
    /// The path to the input kcl file to snapshot.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", required = true)]
    pub input: std::path::PathBuf,

    /// The path to a file to output the image.
    #[clap(name = "output-file", required = true)]
    pub output_file: std::path::PathBuf,

    /// A valid output image format.
    #[clap(short = 't', long = "output-format", value_enum)]
    output_format: Option<kittycad::types::ImageFormat>,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<crate::types::FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdKclSnapshot {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        // Make sure the parent directory is a directory and exists.
        if let Some(parent) = self.output_file.parent() {
            if !parent.is_dir() && parent.to_str().unwrap_or("") != "" {
                anyhow::bail!(
                    "directory `{}` does not exist or is not a directory",
                    parent.to_str().unwrap_or("")
                );
            }
        }

        // Parse the image format.
        let output_format = if let Some(output_format) = &self.output_format {
            output_format.clone()
        } else {
            get_image_format_from_extension(&crate::cmd_file::get_extension(self.output_file.clone()))?
        };

        // Get the contents of the input file.
        let input = ctx.read_file(self.input.to_str().unwrap_or(""))?;
        // Parse the input as a string.
        let input = std::str::from_utf8(&input)?;

        // Spin up websockets and do the conversion.
        // This will not return until there are files.
        let resp = ctx
            .send_modeling_cmd(
                "",
                input,
                kittycad::types::ModelingCmd::TakeSnapshot { format: output_format },
            )
            .await?;

        if let kittycad::types::OkWebSocketResponseData::Modeling {
            modeling_response: kittycad::types::OkModelingCmdResponse::TakeSnapshot { data },
        } = &resp
        {
            // Save the snapshot locally.
            std::fs::write(&self.output_file, &data.contents.0)?;
        } else {
            anyhow::bail!("Unexpected response from engine: {:?}", resp);
        }

        writeln!(
            ctx.io.out,
            "Snapshot saved to `{}`",
            self.output_file.to_str().unwrap_or("")
        )?;

        Ok(())
    }
}

/// View a render of a `kcl` file in your terminal.
///
///     $ kittycad kcl view my-file.kcl
///
///     # pass a file to view from stdin
///     $ cat my-obj.kcl | kittycad kcl view -
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdKclView {
    /// The path to the input kcl file to view.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", required = true)]
    pub input: std::path::PathBuf,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<crate::types::FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdKclView {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        // Get the contents of the input file.
        let input = ctx.read_file(self.input.to_str().unwrap_or(""))?;
        // Parse the input as a string.
        let input = std::str::from_utf8(&input)?;

        // Create a temporary file to write the snapshot to.
        let mut tmp_file = std::env::temp_dir();
        tmp_file.push(&format!("kittycad-kcl-view-{}.png", uuid::Uuid::new_v4()));

        // Spin up websockets and do the conversion.
        // This will not return until there are files.
        let resp = ctx
            .send_modeling_cmd(
                "",
                input,
                kittycad::types::ModelingCmd::TakeSnapshot {
                    format: kittycad::types::ImageFormat::Png,
                },
            )
            .await?;

        if let kittycad::types::OkWebSocketResponseData::Modeling {
            modeling_response: kittycad::types::OkModelingCmdResponse::TakeSnapshot { data },
        } = &resp
        {
            // Save the snapshot locally.
            std::fs::write(&tmp_file, &data.contents.0)?;
        } else {
            anyhow::bail!("Unexpected response from engine: {:?}", resp);
        }

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

/// Get the  image format from the extension.
fn get_image_format_from_extension(ext: &str) -> Result<kittycad::types::ImageFormat> {
    match kittycad::types::ImageFormat::from_str(ext) {
        Ok(format) => Ok(format),
        Err(_) => {
            anyhow::bail!(
                    "unknown source format for file extension: {}. Try setting the `--src-format` flag explicitly or use a valid format.",
                    ext
                )
        }
    }
}

fn get_output_format(format: &kittycad::types::FileExportFormat) -> kittycad::types::OutputFormat {
    // KittyCAD co-ordinate system.
    //
    // * Forward: -Y
    // * Up: +Z
    // * Handedness: Right
    let coords = kittycad::types::System {
        forward: kittycad::types::AxisDirectionPair {
            axis: kittycad::types::Axis::Y,
            direction: kittycad::types::Direction::Negative,
        },
        up: kittycad::types::AxisDirectionPair {
            axis: kittycad::types::Axis::Z,
            direction: kittycad::types::Direction::Positive,
        },
    };

    match format {
        kittycad::types::FileExportFormat::Fbx => kittycad::types::OutputFormat::Fbx {
            storage: kittycad::types::FbxStorage::Binary,
        },
        kittycad::types::FileExportFormat::Glb => kittycad::types::OutputFormat::Gltf {
            storage: kittycad::types::GltfStorage::Binary,
            presentation: kittycad::types::GltfPresentation::Compact,
        },
        kittycad::types::FileExportFormat::Gltf => kittycad::types::OutputFormat::Gltf {
            storage: kittycad::types::GltfStorage::Embedded,
            presentation: kittycad::types::GltfPresentation::Pretty,
        },
        kittycad::types::FileExportFormat::Obj => kittycad::types::OutputFormat::Obj { coords },
        kittycad::types::FileExportFormat::Ply => kittycad::types::OutputFormat::Ply {
            storage: kittycad::types::PlyStorage::Ascii,
            coords,
        },
        kittycad::types::FileExportFormat::Step => kittycad::types::OutputFormat::Step { coords },
        kittycad::types::FileExportFormat::Stl => kittycad::types::OutputFormat::Stl {
            storage: kittycad::types::StlStorage::Ascii,
            coords,
        },
    }
}
