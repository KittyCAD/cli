use std::{net::SocketAddr, path::Path, str::FromStr};

use anyhow::Result;
use clap::Parser;
use image::{DynamicImage, ImageReader};
use kcl_lib::{ToLspRange, TypedPath};
use kcmc::format::OutputFormat3d as OutputFormat;
use kittycad::types as kt;
use kittycad_modeling_cmds::{self as kcmc, units::UnitLength};
use url::Url;

use crate::{iostreams::IoStreams, kcl_error_fmt, types::CameraView};

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
    #[clap(alias = "fmt")]
    Format(CmdKclFormat),
    Snapshot(CmdKclSnapshot),
    View(CmdKclView),
    Volume(CmdKclVolume),
    Mass(CmdKclMass),
    CenterOfMass(CmdKclCenterOfMass),
    Density(CmdKclDensity),
    SurfaceArea(CmdKclSurfaceArea),
    Lint(CmdKclLint),
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdKcl {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            SubCommand::Export(cmd) => cmd.run(ctx).await,
            SubCommand::Format(cmd) => cmd.run(ctx).await,
            SubCommand::Snapshot(cmd) => cmd.run(ctx).await,
            SubCommand::View(cmd) => cmd.run(ctx).await,
            SubCommand::Volume(cmd) => cmd.run(ctx).await,
            SubCommand::Mass(cmd) => cmd.run(ctx).await,
            SubCommand::CenterOfMass(cmd) => cmd.run(ctx).await,
            SubCommand::Density(cmd) => cmd.run(ctx).await,
            SubCommand::SurfaceArea(cmd) => cmd.run(ctx).await,
            SubCommand::Lint(cmd) => cmd.run(ctx).await,
        }
    }
}

/// Export a `kcl` file as any other supported CAD file format.
///
///     # convert kcl to obj
///     $ zoo kcl export --output-format=obj my-file.kcl output_dir
///
///     # convert kcl to step
///     $ zoo kcl export --output-format=step my-obj.kcl .
///
///     # pass a file to convert from stdin
///     $ cat my-obj.kcl | zoo kcl export --output-format=step - output_dir
///
/// By default, this will search the input path for a `project.toml` file to determine any specific execution settings.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdKclExport {
    /// The path to the input kcl file to export.
    /// This can also be the path to a directory containing a main.kcl file.
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

    /// If true, print a link to this request's tracing data.
    #[clap(long, default_value = "false")]
    pub show_trace: bool,

    /// If true, the output file should be deterministic, meaning any date or time information
    /// will be replaced with a fixed value.
    /// This is useful for when pushing to version control.
    #[clap(long, default_value = "false")]
    pub deterministic: bool,
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
        let (code, filepath) = ctx.get_code_and_file_path(&self.input).await?;

        // Get the modeling settings from the project.toml if exists.
        let settings = get_modeling_settings_from_project_toml(&filepath)?;

        let program = kcl_lib::Program::parse_no_errs(&code)
            .map_err(|err| kcl_error_fmt::into_miette_for_parse(&filepath.display().to_string(), &code, err))?;
        let meta_settings = program.meta_settings()?.unwrap_or_default();
        let units: UnitLength = meta_settings.default_length_units;

        let client = ctx.api_client("")?;
        let ectx = kcl_lib::ExecutorContext::new(&client, settings).await?;
        let mut state = kcl_lib::ExecState::new(&ectx);
        let session_data = ectx
            .run(&program, &mut state)
            .await
            .map_err(|err| kcl_error_fmt::into_miette(err, &code))?
            .1;

        let files = ectx
            .export(get_output_format(&self.output_format, units, self.deterministic))
            .await?;

        // Save the files to our export directory.
        for file in files {
            let path = self.output_dir.join(file.name);
            std::fs::write(&path, file.contents)?;

            writeln!(ctx.io.out, "Wrote file: {}", path.display())?;
        }

        if self.show_trace {
            print_trace_link(&mut ctx.io, &session_data.map(kt::ModelingSessionData::from))
        }

        Ok(())
    }
}

/// Format a `kcl` file.
///
///     # Output to stdout by default
///     $ zoo kcl fmt my-file.kcl
///
///     # Overwrite the file
///     $ zoo kcl fmt -w my-file.kcl
///
///     # Pass a file to format from stdin
///     $ cat my-obj.kcl | zoo kcl fmt
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdKclFormat {
    /// The path to the input kcl file to format.
    /// This can also be the path to a directory containing a main.kcl file.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", required = true)]
    pub input: std::path::PathBuf,

    /// Write the output back to the original file.
    /// This will fail if the input is from stdin.
    #[clap(short, long)]
    pub write: bool,

    /// Size of a tab in spaces.
    #[clap(long, short, default_value = "2")]
    pub tab_size: usize,

    /// Prefer tabs over spaces.
    #[clap(long, default_value = "false")]
    pub use_tabs: bool,

    /// How to handle the final newline in the file. If true, ensure file ends with a newline. If false, ensure file does not end with a newline.
    #[clap(long, default_value = "true")]
    pub insert_final_newline: bool,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<crate::types::FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdKclFormat {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let options = kcl_lib::FormatOptions {
            tab_size: self.tab_size,
            use_tabs: self.use_tabs,
            insert_final_newline: self.insert_final_newline,
        };

        // Check if input is a directory.
        if self.input.is_dir() && self.write {
            // Recurisvely format all files in the directory.
            kcl_lib::recast_dir(&self.input, &options).await?;

            writeln!(ctx.io.out, "Formatted directory `{}`", self.input.display())?;

            // return early if we are not writing to a file.
            return Ok(());
        }

        let (code, _) = ctx.get_code_and_file_path(&self.input).await?;

        // Parse the file.
        let program = kcl_lib::Program::parse_no_errs(&code)?;

        // Recast the program to a string.
        let formatted = program.recast_with_options(&options);

        if self.write {
            if self.input.to_str().unwrap_or("-") == "-" {
                anyhow::bail!("cannot write to stdin");
            }

            // Write the formatted file back to the original file.
            std::fs::write(&self.input, formatted)?;
        } else if let Some(format) = &self.format {
            if format == &crate::types::FormatOutput::Json {
                // Print the formatted file to stdout as json.
                writeln!(ctx.io.out, "{}", serde_json::to_string_pretty(&program)?)?;
            }
        } else {
            // Print the formatted file to stdout.
            writeln!(ctx.io.out, "{formatted}")?;
        }

        Ok(())
    }
}

/// Snapshot a render of a `kcl` file as any supported image format.
///
///     # snapshot as png
///     $ zoo kcl snapshot my-file.kcl my-file.png
///
///     # pass a file to snapshot from stdin
///     $ cat my-obj.kcl | zoo kcl snapshot --output-format=png - my-file.png
///
/// By default, this will search the input path for a `project.toml` file to determine any specific execution settings.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdKclSnapshot {
    /// The path to the input kcl file to snapshot.
    /// This can also be the path to a directory containing a main.kcl file.
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

    /// If given, this command will reuse an existing KittyCAD modeling session.
    /// You can start the session via `zoo session-start --listen-on 0.0.0.0:3333` in this CLI.
    #[clap(long, default_value = None)]
    pub session: Option<SocketAddr>,

    /// If true, print a link to this request's tracing data.
    #[clap(long, default_value = "false")]
    pub show_trace: bool,

    /// If true, tell engine to store a replay.
    #[clap(long, default_value = "false")]
    pub replay: bool,

    /// Which angle to take the snapshot from.
    /// Defaults to "front".
    #[clap(long, value_enum)]
    pub angle: Option<CameraView>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdKclSnapshot {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        // Make sure the parent directory is a directory and exists.
        if let Some(parent) = self.output_file.parent() {
            if !parent.is_dir() && !parent.to_str().unwrap_or("").is_empty() {
                anyhow::bail!(
                    "directory `{}` does not exist or is not a directory",
                    parent.to_str().unwrap_or("")
                );
            }
        }

        // Parse the image format.
        let output_format = if let Some(output_format) = &self.output_format {
            match output_format {
                kittycad::types::ImageFormat::Png => kittycad_modeling_cmds::ImageFormat::Png,
                kittycad::types::ImageFormat::Jpeg => kittycad_modeling_cmds::ImageFormat::Jpeg,
            }
        } else {
            get_image_format_from_extension(&crate::cmd_file::get_extension(self.output_file.clone()))?
        };

        // Get the contents of the input file.
        let (code, filepath) = ctx.get_code_and_file_path(&self.input).await?;

        // Get the modeling settings from the project.toml if exists.
        let mut executor_settings = get_modeling_settings_from_project_toml(&filepath)?;
        executor_settings.replay = self.replay.then_some(filepath.to_string_lossy().to_string());

        let (many_pngs, session_data) = match self.session {
            Some(addr) => {
                // TODO
                let client = reqwest::ClientBuilder::new().build()?;
                let url = Url::parse(&format!("http://{addr}"))?;
                let resp = client
                    .post(url)
                    .body(serde_json::to_vec(&kcl_lib::test_server::RequestBody {
                        kcl_program: code,
                        test_name: self.input.display().to_string(),
                    })?)
                    .send()
                    .await?;
                let status = resp.status();
                if status.is_success() {
                    (vec![resp.bytes().await?.to_vec()], Default::default())
                } else {
                    let err_msg = resp.text().await?;
                    anyhow::bail!("{status}: {err_msg}")
                }
            }
            None => {
                match self.angle.unwrap_or_default() {
                    CameraView::Front => {
                        // Spin up websockets and do the conversion.
                        // This will not return until there are files.
                        let (resp, session_data) = ctx
                            .send_kcl_modeling_cmd(
                                "",
                                &filepath.display().to_string(),
                                &code,
                                kittycad_modeling_cmds::ModelingCmd::TakeSnapshot(
                                    kittycad_modeling_cmds::TakeSnapshot { format: output_format },
                                ),
                                executor_settings,
                            )
                            .await?;

                        if let kittycad_modeling_cmds::websocket::OkWebSocketResponseData::Modeling {
                            modeling_response:
                                kittycad_modeling_cmds::ok_response::OkModelingCmdResponse::TakeSnapshot(data),
                        } = &resp
                        {
                            (vec![data.contents.0.clone()], session_data)
                        } else {
                            anyhow::bail!("Unexpected response from engine: {resp:?}");
                        }
                    }
                    CameraView::FourWays => {
                        let (responses, session_data) = ctx
                            .run_kcl_then_snapshots(
                                "",
                                &filepath.display().to_string(),
                                &code,
                                four_sides_view(),
                                executor_settings,
                            )
                            .await?;
                        (
                            responses.into_iter().map(|resp| resp.contents.0).collect(),
                            session_data,
                        )
                    }
                }
            }
        };
        let output_file_display = self.output_file.display().to_string();

        // Is there just 1 PNG?
        match <[_; 1]>::try_from(many_pngs) {
            Ok([single]) => {
                std::fs::write(&self.output_file, single)?;
                writeln!(ctx.io.out, "Snapshot saved to `{output_file_display}`")?;
            }
            // If not, maybe there's 4 PNGs?
            Err(output_file_contents) => match <[_; 4]>::try_from(output_file_contents) {
                Ok([a, b, c, d]) => {
                    let [a, b, c, d] = four_png_readers(a, b, c, d);
                    combine_quadrants(
                        &a.decode()?,
                        &b.decode()?,
                        &c.decode()?,
                        &d.decode()?,
                        &self.output_file,
                    )?;
                    writeln!(ctx.io.out, "Snapshot saved to `{output_file_display}`")?;
                }
                // If not 4, error.
                Err(vec) => {
                    anyhow::bail!("Can only handle 1 or 4 images but received {}", vec.len());
                }
            },
        };

        if self.show_trace {
            print_trace_link(&mut ctx.io, &session_data.map(kt::ModelingSessionData::from))
        }

        Ok(())
    }
}

fn four_png_readers(a: Vec<u8>, b: Vec<u8>, c: Vec<u8>, d: Vec<u8>) -> [ImageReader<std::io::Cursor<Vec<u8>>>; 4] {
    use std::io::Cursor;
    let mut a = ImageReader::new(Cursor::new(a));
    a.set_format(image::ImageFormat::Png);
    let mut b = ImageReader::new(Cursor::new(b));
    b.set_format(image::ImageFormat::Png);
    let mut c = ImageReader::new(Cursor::new(c));
    c.set_format(image::ImageFormat::Png);
    let mut d = ImageReader::new(Cursor::new(d));
    d.set_format(image::ImageFormat::Png);
    [a, b, c, d]
}

/// View a render of a `kcl` file in your terminal.
///
///     $ zoo kcl view my-file.kcl
///
///     # pass a file to view from stdin
///     $ cat my-obj.kcl | zoo kcl view -
///
/// By default, this will search the input path for a `project.toml` file to determine any specific execution settings.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdKclView {
    /// The path to the input kcl file to view.
    /// This can also be the path to a directory containing a main.kcl file.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", required = true)]
    pub input: std::path::PathBuf,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<crate::types::FormatOutput>,

    /// Which angle to take the snapshot from.
    /// Defaults to "front".
    #[clap(long, value_enum)]
    pub angle: Option<CameraView>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdKclView {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        // Get the contents of the input file.
        let (code, filepath) = ctx.get_code_and_file_path(&self.input).await?;

        // Get the modeling settings from the project.toml if exists.
        let executor_settings = get_modeling_settings_from_project_toml(&filepath)?;

        // Create a temporary file to write the snapshot to.
        let mut tmp_file = std::env::temp_dir();
        tmp_file.push(format!("zoo-kcl-view-{}.png", uuid::Uuid::new_v4()));

        match self.angle.unwrap_or_default() {
            CameraView::Front => {
                // Spin up websockets and do the conversion.
                // This will not return until there are files.
                let (resp, _session_data) = ctx
                    .send_kcl_modeling_cmd(
                        "",
                        &filepath.display().to_string(),
                        &code,
                        kittycad_modeling_cmds::ModelingCmd::TakeSnapshot(kittycad_modeling_cmds::TakeSnapshot {
                            format: kittycad_modeling_cmds::ImageFormat::Png,
                        }),
                        executor_settings,
                    )
                    .await?;

                // Get the PNG out.
                let png_bytes = if let kittycad_modeling_cmds::websocket::OkWebSocketResponseData::Modeling {
                    modeling_response: kittycad_modeling_cmds::ok_response::OkModelingCmdResponse::TakeSnapshot(data),
                } = resp
                {
                    data.contents.0
                } else {
                    anyhow::bail!("Unexpected response from engine: {resp:?}");
                };
                // Save the snapshot locally.
                std::fs::write(&tmp_file, &png_bytes)?;
            }
            CameraView::FourWays => {
                let (responses, _session_data) = ctx
                    .run_kcl_then_snapshots(
                        "",
                        &filepath.display().to_string(),
                        &code,
                        four_sides_view(),
                        executor_settings,
                    )
                    .await?;
                let [a, b, c, d] = responses
                    .into_iter()
                    .map(|resp| resp.contents.0)
                    .collect::<Vec<_>>()
                    .try_into()
                    .map_err(|snaps: Vec<_>| {
                        anyhow::anyhow!("Expected 4 images from the 4-way view, but only found {}", snaps.len())
                    })?;
                let [a, b, c, d] = four_png_readers(a, b, c, d);
                combine_quadrants(&a.decode()?, &b.decode()?, &c.decode()?, &d.decode()?, &tmp_file)?;
            }
        };

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
pub fn get_image_format_from_extension(ext: &str) -> Result<kittycad_modeling_cmds::ImageFormat> {
    match kittycad_modeling_cmds::ImageFormat::from_str(ext) {
        Ok(format) => Ok(format),
        Err(_) => {
            anyhow::bail!(
                    "unknown source format for file extension: {ext}. Try setting the `--src-format` flag explicitly or use a valid format."
                )
        }
    }
}

fn get_output_format(
    format: &kittycad::types::FileExportFormat,
    src_unit: kittycad_modeling_cmds::units::UnitLength,
    deterministic: bool,
) -> OutputFormat {
    // Zoo co-ordinate system.
    //
    // * Forward: -Y
    // * Up: +Z
    // * Handedness: Right
    let coords = kcmc::coord::System {
        forward: kcmc::coord::AxisDirectionPair {
            axis: kcmc::coord::Axis::Y,
            direction: kcmc::coord::Direction::Negative,
        },
        up: kcmc::coord::AxisDirectionPair {
            axis: kcmc::coord::Axis::Z,
            direction: kcmc::coord::Direction::Positive,
        },
    };

    match format {
        kt::FileExportFormat::Fbx => OutputFormat::Fbx(kcmc::format::fbx::export::Options {
            storage: kcmc::format::fbx::export::Storage::Binary,
            created: if deterministic {
                Some("1970-01-01T00:00:00Z".parse().unwrap())
            } else {
                None
            },
        }),
        kt::FileExportFormat::Glb => OutputFormat::Gltf(kcmc::format::gltf::export::Options {
            storage: kcmc::format::gltf::export::Storage::Binary,
            presentation: kcmc::format::gltf::export::Presentation::Compact,
        }),
        kt::FileExportFormat::Gltf => OutputFormat::Gltf(kcmc::format::gltf::export::Options {
            storage: kcmc::format::gltf::export::Storage::Embedded,
            presentation: kcmc::format::gltf::export::Presentation::Pretty,
        }),
        kt::FileExportFormat::Obj => OutputFormat::Obj(kcmc::format::obj::export::Options {
            coords,
            units: src_unit,
        }),
        kt::FileExportFormat::Ply => OutputFormat::Ply(kcmc::format::ply::export::Options {
            storage: kcmc::format::ply::export::Storage::Ascii,
            coords,
            selection: kcmc::format::Selection::DefaultScene,
            units: src_unit,
        }),
        kt::FileExportFormat::Step => OutputFormat::Step(kcmc::format::step::export::Options {
            coords,
            created: if deterministic {
                Some("1970-01-01T00:00:00Z".parse().unwrap())
            } else {
                None
            },
        }),
        kt::FileExportFormat::Stl => OutputFormat::Stl(kcmc::format::stl::export::Options {
            storage: kcmc::format::stl::export::Storage::Ascii,
            coords,
            units: src_unit,
            selection: kcmc::format::Selection::DefaultScene,
        }),
    }
}

/// Get the volume of an object in a kcl file.
///
///     # get the volume of a file
///     $ zoo kcl volume my-file.kcl
///
///     # pass a file from stdin
///     $ cat my-file.kcl | zoo kcl volume
///
/// By default, this will search the input path for a `project.toml` file to determine any specific execution settings.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdKclVolume {
    /// The path to the input file.
    /// This can also be the path to a directory containing a main.kcl file.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", required = true)]
    pub input: std::path::PathBuf,

    /// Output format.
    #[clap(long, short, value_enum)]
    pub format: Option<crate::types::FormatOutput>,

    /// Output unit.
    #[clap(long = "output-unit", short = 'u', value_enum)]
    pub output_unit: kittycad::types::UnitVolume,

    /// If true, print a link to this request's tracing data.
    #[clap(long, default_value = "false")]
    pub show_trace: bool,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdKclVolume {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        // Get the contents of the input file.
        let (code, filepath) = ctx.get_code_and_file_path(&self.input).await?;

        // Get the modeling settings from the project.toml if exists.
        let executor_settings = get_modeling_settings_from_project_toml(&filepath)?;

        // Spin up websockets and do the conversion.
        // This will not return until there are files.
        let (resp, session_data) = ctx
            .send_kcl_modeling_cmd(
                "",
                &filepath.display().to_string(),
                &code,
                kittycad_modeling_cmds::ModelingCmd::Volume(kittycad_modeling_cmds::Volume {
                    entity_ids: vec![], // get whole model
                    output_unit: self.output_unit.clone().into(),
                }),
                executor_settings,
            )
            .await?;

        if let kittycad_modeling_cmds::websocket::OkWebSocketResponseData::Modeling {
            modeling_response: kittycad_modeling_cmds::ok_response::OkModelingCmdResponse::Volume(data),
        } = &resp
        {
            // Print the output.
            let format = ctx.format(&self.format)?;
            ctx.io.write_output(&format, &data)?;
        } else {
            anyhow::bail!("Unexpected response from engine: {resp:?}");
        }

        if self.show_trace {
            print_trace_link(&mut ctx.io, &session_data.map(kt::ModelingSessionData::from))
        }
        Ok(())
    }
}

/// Get the mass of objects in a kcl file.
///
///     # get the mass of a file
///     $ zoo kcl mass my-file.kcl
///
///     # pass a file from stdin
///     $ cat my-file.kcl | zoo kcl mass
///
/// By default, this will search the input path for a `project.toml` file to determine any specific execution settings.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdKclMass {
    /// The path to the input file.
    /// This can also be the path to a directory containing a main.kcl file.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", required = true)]
    pub input: std::path::PathBuf,

    /// Material density.
    #[clap(short = 'm', long = "material-density")]
    material_density: f32,

    /// Material density unit.
    #[clap(long = "material-density-unit", value_enum)]
    material_density_unit: kittycad::types::UnitDensity,

    /// Output format.
    #[clap(long, short, value_enum)]
    pub format: Option<crate::types::FormatOutput>,

    /// Output unit.
    #[clap(long = "output-unit", short = 'u', value_enum)]
    pub output_unit: kittycad::types::UnitMass,

    /// If true, print a link to this request's tracing data.
    #[clap(long, default_value = "false")]
    pub show_trace: bool,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdKclMass {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        if self.material_density == 0.0 {
            anyhow::bail!("`--material-density` must not be 0.0");
        }

        // Get the contents of the input file.
        let (code, filepath) = ctx.get_code_and_file_path(&self.input).await?;

        // Get the modeling settings from the project.toml if exists.
        let executor_settings = get_modeling_settings_from_project_toml(&filepath)?;

        // Spin up websockets and do the conversion.
        // This will not return until there are files.
        let (resp, session_data) = ctx
            .send_kcl_modeling_cmd(
                "",
                &filepath.display().to_string(),
                &code,
                kittycad_modeling_cmds::ModelingCmd::Mass(kittycad_modeling_cmds::Mass {
                    entity_ids: vec![], // get whole model
                    material_density: self.material_density.into(),
                    material_density_unit: self.material_density_unit.clone().into(),
                    output_unit: self.output_unit.clone().into(),
                }),
                executor_settings,
            )
            .await?;

        if let kittycad_modeling_cmds::websocket::OkWebSocketResponseData::Modeling {
            modeling_response: kittycad_modeling_cmds::ok_response::OkModelingCmdResponse::Mass(data),
        } = &resp
        {
            // Print the output.
            let format = ctx.format(&self.format)?;
            ctx.io.write_output(&format, &data)?;
        } else {
            anyhow::bail!("Unexpected response from engine: {resp:?}");
        }

        if self.show_trace {
            print_trace_link(&mut ctx.io, &session_data.map(kt::ModelingSessionData::from))
        }
        Ok(())
    }
}

/// Get the center of mass of objects in a kcl file.
///
///     # get the mass of a file
///     $ zoo kcl center-of-mass my-file.kcl
///
///     # pass a file from stdin
///     $ cat my-file.kcl | zoo kcl center-of-mass
///
/// By default, this will search the input path for a `project.toml` file to determine any specific execution settings.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdKclCenterOfMass {
    /// The path to the input file.
    /// This can also be the path to a directory containing a main.kcl file.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", required = true)]
    pub input: std::path::PathBuf,

    /// Output format.
    #[clap(long, short, value_enum)]
    pub format: Option<crate::types::FormatOutput>,

    /// Output unit.
    #[clap(long = "output-unit", short = 'u', value_enum)]
    pub output_unit: kittycad::types::UnitLength,

    /// If true, print a link to this request's tracing data.
    #[clap(long, default_value = "false")]
    pub show_trace: bool,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdKclCenterOfMass {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        // Get the contents of the input file.
        let (code, filepath) = ctx.get_code_and_file_path(&self.input).await?;

        // Get the modeling settings from the project.toml if exists.
        let executor_settings = get_modeling_settings_from_project_toml(&filepath)?;

        // Spin up websockets and do the conversion.
        // This will not return until there are files.
        let (resp, session_data) = ctx
            .send_kcl_modeling_cmd(
                "",
                &filepath.display().to_string(),
                &code,
                kittycad_modeling_cmds::ModelingCmd::CenterOfMass(kittycad_modeling_cmds::CenterOfMass {
                    entity_ids: vec![], // get whole model
                    output_unit: self.output_unit.clone().into(),
                }),
                executor_settings,
            )
            .await?;

        if let kittycad_modeling_cmds::websocket::OkWebSocketResponseData::Modeling {
            modeling_response: kittycad_modeling_cmds::ok_response::OkModelingCmdResponse::CenterOfMass(data),
        } = &resp
        {
            // Print the output.
            let format = ctx.format(&self.format)?;
            ctx.io.write_output(&format, &data)?;
        } else {
            anyhow::bail!("Unexpected response from engine: {resp:?}");
        }

        if self.show_trace {
            print_trace_link(&mut ctx.io, &session_data.map(kt::ModelingSessionData::from))
        }
        Ok(())
    }
}

/// Get the density of objects in a kcl file.
///
///     # get the density of a file
///     $ zoo kcl density my-file.kcl
///
///     # pass a file from stdin
///     $ cat my-file.kcl | zoo kcl density
///
/// By default, this will search the input path for a `project.toml` file to determine any specific execution settings.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdKclDensity {
    /// The path to the input file.
    /// This can also be the path to a directory containing a main.kcl file.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", required = true)]
    pub input: std::path::PathBuf,

    /// Material mass.
    #[clap(short = 'm', long = "material-mass")]
    material_mass: f32,

    /// The unit of the material mass.
    #[clap(long = "material-mass-unit", value_enum)]
    material_mass_unit: kittycad::types::UnitMass,

    /// Output format.
    #[clap(long, short, value_enum)]
    pub format: Option<crate::types::FormatOutput>,

    /// Output unit.
    #[clap(long = "output-unit", short = 'u', value_enum)]
    pub output_unit: kittycad::types::UnitDensity,

    /// If true, print a link to this request's tracing data.
    #[clap(long, default_value = "false")]
    pub show_trace: bool,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdKclDensity {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        if self.material_mass == 0.0 {
            anyhow::bail!("`--material-mass` must not be 0.0");
        }

        // Get the contents of the input file.
        let (code, filepath) = ctx.get_code_and_file_path(&self.input).await?;

        // Get the modeling settings from the project.toml if exists.
        let executor_settings = get_modeling_settings_from_project_toml(&filepath)?;

        // Spin up websockets and do the conversion.
        // This will not return until there are files.
        let (resp, session_data) = ctx
            .send_kcl_modeling_cmd(
                "",
                &filepath.display().to_string(),
                &code,
                kittycad_modeling_cmds::ModelingCmd::Density(kittycad_modeling_cmds::Density {
                    entity_ids: vec![], // get whole model
                    material_mass: self.material_mass.into(),
                    material_mass_unit: self.material_mass_unit.clone().into(),
                    output_unit: self.output_unit.clone().into(),
                }),
                executor_settings,
            )
            .await?;

        if let kittycad_modeling_cmds::websocket::OkWebSocketResponseData::Modeling {
            modeling_response: kittycad_modeling_cmds::ok_response::OkModelingCmdResponse::Density(data),
        } = &resp
        {
            // Print the output.
            let format = ctx.format(&self.format)?;
            ctx.io.write_output(&format, &data)?;
        } else {
            anyhow::bail!("Unexpected response from engine: {resp:?}");
        }

        if self.show_trace {
            print_trace_link(&mut ctx.io, &session_data.map(kt::ModelingSessionData::from))
        }
        Ok(())
    }
}

/// Get the surface area of objects in a kcl file.
///
///     # get the surface-area of a file
///     $ zoo kcl surface-area my-file.kcl
///
///     # pass a file from stdin
///     $ cat my-file.kcl | zoo kcl surface-area
///
/// By default, this will search the input path for a `project.toml` file to determine any specific execution settings.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdKclSurfaceArea {
    /// The path to the input file.
    /// This can also be the path to a directory containing a main.kcl file.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", required = true)]
    pub input: std::path::PathBuf,

    /// Output format.
    #[clap(long, short, value_enum)]
    pub format: Option<crate::types::FormatOutput>,

    /// Output unit.
    #[clap(long = "output-unit", short = 'u', value_enum)]
    pub output_unit: kittycad::types::UnitArea,

    /// If true, print a link to this request's tracing data.
    #[clap(long, default_value = "false")]
    pub show_trace: bool,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdKclSurfaceArea {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        // Get the contents of the input file.
        let (code, filepath) = ctx.get_code_and_file_path(&self.input).await?;

        // Get the modeling settings from the project.toml if exists.
        let executor_settings = get_modeling_settings_from_project_toml(&filepath)?;

        // Spin up websockets and do the conversion.
        // This will not return until there are files.
        let (resp, session_data) = ctx
            .send_kcl_modeling_cmd(
                "",
                &filepath.display().to_string(),
                &code,
                kittycad_modeling_cmds::ModelingCmd::SurfaceArea(kittycad_modeling_cmds::SurfaceArea {
                    entity_ids: vec![], // get whole model
                    output_unit: self.output_unit.clone().into(),
                }),
                executor_settings,
            )
            .await?;

        if let kittycad_modeling_cmds::websocket::OkWebSocketResponseData::Modeling {
            modeling_response: kittycad_modeling_cmds::ok_response::OkModelingCmdResponse::SurfaceArea(data),
        } = &resp
        {
            // Print the output.
            let format = ctx.format(&self.format)?;
            ctx.io.write_output(&format, &data)?;
        } else {
            anyhow::bail!("Unexpected response from engine: {resp:?}");
        }

        if self.show_trace {
            print_trace_link(&mut ctx.io, &session_data.map(kt::ModelingSessionData::from))
        }
        Ok(())
    }
}

/// Lint a KCL file for style issues.
///
///     # check a file for issues
///     $ zoo kcl lint my-file.kcl
///
///     # pass a file from stdin
///     $ cat my-file.kcl | zoo kcl lint -
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdKclLint {
    /// The path to the input file.
    /// This can also be the path to a directory containing a main.kcl file.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", required = true)]
    pub input: std::path::PathBuf,

    /// Print a long-form description of what the issue is, and the rational
    /// behind why.
    #[clap(long, default_value = "false")]
    pub descriptions: bool,

    /// Show where the offending KCL source code is.
    #[clap(long, short, default_value = "false")]
    pub show_code: bool,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdKclLint {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let path = self.input.to_str().unwrap_or("");
        // Get the contents of the input file.
        let (code, _) = ctx.get_code_and_file_path(&self.input).await?;

        // Parse the file.
        let program = kcl_lib::Program::parse_no_errs(&code)?;

        for discovered_finding in program.lint_all()? {
            let finding_range = discovered_finding.pos.to_lsp_range(&code);
            let start = finding_range.start;
            let end = finding_range.end;

            let title = if discovered_finding.description.is_empty() {
                discovered_finding.finding.title.to_owned()
            } else {
                format!(
                    "{} ({})",
                    discovered_finding.finding.title, discovered_finding.description
                )
            };

            println!(
                "{}:{}:{}: [{}] {}",
                path,
                start.line + 1,
                start.character + 1,
                discovered_finding.finding.code,
                title,
            );

            if self.descriptions {
                println!("\n{}", discovered_finding.finding.description);
            }

            if self.show_code {
                if start.line != end.line {
                    unimplemented!()
                }
                let printable_line = code.lines().collect::<Vec<&str>>()[start.line as usize];
                println!(
                    "\n\x1b[38;5;248m{}\x1b[38;5;208;1m{}\x1b[38;5;248m{}\x1b[0m",
                    &printable_line[..(start.character as usize)],
                    &printable_line[(start.character as usize)..(end.character as usize)],
                    &printable_line[(end.character as usize)..],
                );
                println!(
                    "{}{} ↖ right here",
                    " ".repeat(start.character as usize),
                    "▔".repeat((end.character - start.character) as usize)
                );
                println!("\n");
            }
        }

        Ok(())
    }
}

/// Get the extension for a path buffer.
pub fn get_extension(path: std::path::PathBuf) -> String {
    path.into_boxed_path()
        .extension()
        .unwrap_or_default()
        .to_str()
        .unwrap_or("")
        .to_string()
}

fn print_trace_link(io: &mut IoStreams, session_data: &Option<kittycad::types::ModelingSessionData>) {
    let Some(data) = session_data else {
        return;
    };
    let api_call_id = &data.api_call_id;
    let link = format!("https://ui.honeycomb.io/kittycad/environments/prod/datasets/api-deux?query=%7B%22time_range%22%3A7200%2C%22granularity%22%3A0%2C%22calculations%22%3A%5B%7B%22op%22%3A%22COUNT%22%7D%5D%2C%22filters%22%3A%5B%7B%22column%22%3A%22api_call.id%22%2C%22op%22%3A%22%3D%22%2C%22value%22%3A%22{api_call_id}%22%7D%5D%2C%22filter_combination%22%3A%22AND%22%2C%22limit%22%3A1000%7D");
    let _ = writeln!(
        io.out,
        "Was this request slow? Send a Zoo employee this link:\n----\n{link}"
    );
}

/// Look for a `project.toml` file the same directory as the input file.
/// Use that for the engine settings.
fn get_modeling_settings_from_project_toml(input: &std::path::Path) -> Result<kcl_lib::ExecutorSettings> {
    // Create the default settings from the src unit if given.
    let mut default_settings: kcl_lib::ExecutorSettings = Default::default();
    let typed_path = TypedPath::from(input.display().to_string().as_str());
    default_settings.with_current_file(typed_path);

    // Check if the path was stdin.
    if input.to_str() == Some("-") {
        return Ok(default_settings);
    }

    // Make it a path.
    let input = std::path::Path::new(input);
    // Ensure the path exists.
    if !input.exists() {
        let input_display = input.display().to_string();
        anyhow::bail!("file `{input_display}` does not exist");
    }
    // Get the directory if we don't already have one.
    let dir = if input.is_dir() {
        input.to_path_buf()
    } else {
        input
            .parent()
            .ok_or_else(|| {
                let input_display = input.display().to_string();
                anyhow::anyhow!("could not get parent directory of `{input_display}`")
            })?
            .to_path_buf()
    };

    // Look for a `project.toml` file in the directory.
    let project_toml = find_project_toml(&dir)?;
    if let Some(project_toml) = project_toml {
        let project_toml = std::fs::read_to_string(&project_toml)?;
        let project_toml: kcl_lib::ProjectConfiguration = toml::from_str(&project_toml)?;
        let mut settings: kcl_lib::ExecutorSettings = project_toml.into();
        let typed_path = TypedPath::from(input.display().to_string().as_str());
        settings.with_current_file(typed_path);
        Ok(settings)
    } else {
        Ok(default_settings)
    }
}

/// Search recursively for a project.toml in parents.
pub fn find_project_toml(path: &std::path::Path) -> Result<Option<std::path::PathBuf>> {
    let mut path = path.to_path_buf();
    loop {
        let project_toml = path.join("project.toml");
        if project_toml.exists() {
            return Ok(Some(project_toml));
        }
        if !path.pop() {
            return Ok(None);
        }
    }
}

/// Make the exported file have a deterministic date for git and version control etc.
pub fn write_deterministic_export(file_path: &std::path::Path, file_contents: &[u8]) -> Result<()> {
    if let Ok(contents) = std::str::from_utf8(file_contents) {
        let mut content = contents.to_string();

        // Create a regex pattern for finding the date.
        let re = regex::Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d+\+\d{2}:\d{2}")?;

        // Replace all occurrences.
        content = re.replace_all(&content, "1970-01-01T00:00:00.0+00:00").to_string();

        // Write the modified content back to the file.
        std::fs::write(file_path, content)?;
    } else {
        // Write the content back to the file.
        std::fs::write(file_path, file_contents)?;
    }

    Ok(())
}

/// Generate snapshots from 4 perspectives: front/side/top/isometric.
fn four_sides_view() -> Vec<kcmc::ModelingCmd> {
    use kcmc::shared::Point3d;
    let center = Point3d::default();

    let snap = kcmc::ModelingCmd::TakeSnapshot(kcmc::TakeSnapshot {
        format: kittycad_modeling_cmds::ImageFormat::Png,
    });

    let front = kcmc::ModelingCmd::DefaultCameraLookAt(kcmc::DefaultCameraLookAt {
        up: Point3d { x: 0.0, y: 0.0, z: 1.0 },
        vantage: Point3d {
            x: 0.0,
            y: -1.0,
            z: 0.0,
        },
        center,
        sequence: None,
    });

    let side = kcmc::ModelingCmd::DefaultCameraLookAt(kcmc::DefaultCameraLookAt {
        up: Point3d { x: 0.0, y: 0.0, z: 1.0 },
        vantage: Point3d { x: 1.0, y: 0.0, z: 0.0 },
        center,
        sequence: None,
    });

    let top = kcmc::ModelingCmd::DefaultCameraLookAt(kcmc::DefaultCameraLookAt {
        up: Point3d { x: 0.0, y: 1.0, z: 0.0 },
        vantage: Point3d { x: 0.0, y: 0.0, z: 1.0 },
        center,
        sequence: None,
    });

    let iso = kcmc::ModelingCmd::ViewIsometric(kcmc::ViewIsometric { padding: 0.0 });

    let zoom = kcmc::ModelingCmd::ZoomToFit(kcmc::ZoomToFit {
        animated: false,
        object_ids: Default::default(),
        padding: 0.1,
    });
    vec![
        front,
        zoom.clone(),
        snap.clone(),
        side,
        zoom.clone(),
        snap.clone(),
        top,
        zoom.clone(),
        snap.clone(),
        iso,
        zoom.clone(),
        snap,
    ]
}

fn combine_quadrants(
    top_left: &DynamicImage,
    top_right: &DynamicImage,
    bottom_left: &DynamicImage,
    bottom_right: &DynamicImage,
    output_path: &Path,
) -> Result<()> {
    use image::{GenericImage, GenericImageView, ImageBuffer, Rgba};
    let (w, h) = top_left.dimensions();

    // Sanity checks
    for img in [top_right, bottom_left, bottom_right] {
        assert_eq!(img.dimensions(), (w, h));
    }

    // Create output image (2w × 2h)
    let mut out: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(w * 2, h * 2);

    // Copy pixels into each quadrant
    out.copy_from(top_left, 0, 0).unwrap();
    out.copy_from(top_right, w, 0).unwrap();
    out.copy_from(bottom_left, 0, h).unwrap();
    out.copy_from(bottom_right, w, h).unwrap();

    out.save(output_path)?;
    Ok(())
}
