use std::hash::{DefaultHasher, Hash, Hasher};
use std::str::FromStr;

use anyhow::Result;
use base64::prelude::*;
use clap::Parser;
use kcl_lib::engine::EngineManager;

/// Perform operations on CAD files.
///
///     # convert a step file to an obj file
///     $ zoo file convert --output-format=obj ./input.step ./
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdFile {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug, Clone)]
enum SubCommand {
    Convert(CmdFileConvert),
    Snapshot(CmdFileSnapshot),
    Volume(CmdFileVolume),
    Mass(CmdFileMass),
    CenterOfMass(CmdFileCenterOfMass),
    Density(CmdFileDensity),
    SurfaceArea(CmdFileSurfaceArea),
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdFile {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            SubCommand::Convert(cmd) => cmd.run(ctx).await,
            SubCommand::Snapshot(cmd) => cmd.run(ctx).await,
            SubCommand::Volume(cmd) => cmd.run(ctx).await,
            SubCommand::Mass(cmd) => cmd.run(ctx).await,
            SubCommand::CenterOfMass(cmd) => cmd.run(ctx).await,
            SubCommand::Density(cmd) => cmd.run(ctx).await,
            SubCommand::SurfaceArea(cmd) => cmd.run(ctx).await,
        }
    }
}

/// Convert a CAD file from one format to another.
///
/// If the file being converted is larger than a certain size it will be
/// performed asynchronously, you can then check its status with the
/// `zoo api-call status <id_of_your_operation>` command.
///
///     # convert step to obj
///     $ zoo file convert --output-format=obj my-file.step output_dir
///
///     # convert obj to step
///     $ zoo file convert --output-format=step my-obj.obj .
///
///     # pass a file to convert from stdin
///     # when converting from stdin, the original file type is required
///     $ cat my-obj.obj | zoo file convert --output-format=step - output_dir
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdFileConvert {
    /// The path to the input file to convert.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", required = true)]
    pub input: std::path::PathBuf,

    /// The path to a directory to output the files.
    #[clap(name = "output-dir", required = true)]
    pub output_dir: std::path::PathBuf,

    /// A valid source file format.
    #[clap(short = 's', long = "src-format", value_enum)]
    src_format: Option<kittycad::types::FileImportFormat>,

    /// A valid output file format.
    #[clap(short = 't', long = "output-format", value_enum)]
    output_format: kittycad::types::FileExportFormat,

    /// Command output format.
    #[clap(long, short, value_enum)]
    pub format: Option<crate::types::FormatOutput>,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdFileConvert {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        // Make sure the output dir is a directory.
        if !self.output_dir.is_dir() {
            anyhow::bail!(
                "output directory `{}` does not exist or is not a directory",
                self.output_dir.to_str().unwrap_or("")
            );
        }

        // Parse the source format.
        let src_format = if let Some(src_format) = &self.src_format {
            src_format.clone()
        } else {
            get_import_format_from_extension(&get_extension(self.input.clone()))?
        };

        // Get the contents of the input file.
        let input = ctx.read_file(self.input.to_str().unwrap_or(""))?;

        // Do the conversion.
        let client = ctx.api_client("")?;

        // Create the file conversion.
        let mut file_conversion = client
            .file()
            .create_conversion(self.output_format.clone(), src_format, &input.into())
            .await?;

        // If they specified an output file, save the output to that file.
        if file_conversion.status == kittycad::types::ApiCallStatus::Completed {
            if let Some(outputs) = file_conversion.outputs {
                // Write the contents of the files to the output directory.
                for (filename, data) in outputs.iter() {
                    let path = self.output_dir.clone().join(filename);
                    std::fs::write(&path, data)?;
                    writeln!(
                        ctx.io.out,
                        "wrote file `{}` to {}",
                        filename,
                        path.to_str().unwrap_or("")
                    )?;
                }
            } else {
                anyhow::bail!("no output was generated! (this is probably a bug in the API) you should report it to support@zoo.dev");
            }
        }

        // Reset the outputs field of the file conversion.
        // Otherwise what we print will be crazy big.
        file_conversion.outputs = None;

        // Print the output of the conversion.
        let format = ctx.format(&self.format)?;
        ctx.io.write_output(&format, &file_conversion)?;

        Ok(())
    }
}

/// Snapshot a render of a CAD file as any supported image format.
///
///     # snapshot as png
///     $ zoo file snapshot my-file.obj my-file.png
///
///     # pass a file to snapshot from stdin
///     $ cat my-obj.obj | zoo file snapshot --output-format=png - my-file.png
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdFileSnapshot {
    /// The path to the input file to snapshot.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", required = true)]
    pub input: std::path::PathBuf,

    /// A valid source file format.
    #[clap(short = 's', long = "src-format", value_enum)]
    src_format: Option<kittycad::types::FileImportFormat>,

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
impl crate::cmd::Command for CmdFileSnapshot {
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
            crate::cmd_kcl::get_image_format_from_extension(&crate::cmd_file::get_extension(self.output_file.clone()))?
        };
        // Parse the source format.
        let src_format = if let Some(src_format) = &self.src_format {
            src_format.clone()
        } else {
            get_import_format_from_extension(&get_extension(self.input.clone()))?
        };

        // TODO: let user choose the units.
        let src_format = get_input_format(src_format, kittycad::types::UnitLength::Mm)?;

        // Get the contents of the input file.
        let file_location_str = self.input.to_str().unwrap_or_default();
        let input = ctx.read_file(file_location_str)?;
        let filename = self.input.file_name().unwrap_or_default().to_str().unwrap_or("");

        // gltf with "standard" storage is an oddball in the KittyCAD system.
        // In order for the program to know it's dealing with this type, an
        // attempt to parse as json is made, then we check for the buffers
        // property which describes what external files are needed.
        let mut files: Vec<kittycad::types::ImportFile> = vec![kittycad::types::ImportFile {
            path: filename.to_string(),
            data: input.clone(),
        }];

        if let kittycad::types::InputFormat::Gltf {} = src_format {
            if let Ok(str) = std::str::from_utf8(&input) {
                if let Ok(json) = serde_json::from_str::<crate::types::GltfStandardJsonLite>(str) {
                    // Use the path of the control file as the prefix path of
                    // the relative file name.

                    for buffer in json.buffers {
                        if is_data_uri(&buffer.uri) {
                            // Using the whole data URI would create massive
                            // path properties. Use a hash instead.
                            let mut hasher = DefaultHasher::new();
                            buffer.uri.hash(&mut hasher);
                            let hash_u64 = hasher.finish();

                            if let Some(buf_base64) = buffer.uri.split(',').nth(1) {
                                files.push(kittycad::types::ImportFile {
                                    path: hash_u64.to_string(),
                                    data: BASE64_STANDARD.decode(buf_base64)?,
                                });
                            } else {
                                anyhow::bail!("invalid data uri in gltf.buffers.uri property");
                            }
                        } else {
                            let path_ = self
                                .input
                                .parent()
                                .unwrap_or(std::path::Path::new(""))
                                .join(std::path::Path::new(&buffer.uri));
                            let path = path_.to_str().unwrap_or_default();
                            let data = ctx.read_file(path)?;
                            files.push(kittycad::types::ImportFile {
                                path: path_.file_name().unwrap_or_default().to_str().unwrap_or("").to_string(),
                                data,
                            });
                        }
                    }
                }
            }
        }

        let client = ctx.api_client("")?;
        let ws = client
            .modeling()
            .commands_ws(None, None, None, None, None, Some(false))
            .await?;

        let engine = kcl_lib::engine::conn::EngineConnection::new(ws).await?;

        // Send an import request to the engine.
        let resp = engine
            .send_modeling_cmd(
                false,
                uuid::Uuid::new_v4(),
                kcl_lib::executor::SourceRange::default(),
                kittycad::types::ModelingCmd::ImportFiles {
                    files,
                    format: src_format,
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
                false,
                uuid::Uuid::new_v4(),
                kcl_lib::executor::SourceRange::default(),
                kittycad::types::ModelingCmd::DefaultCameraFocusOn { uuid: object_id },
            )
            .await?;

        // Spin up websockets and do the conversion.
        // This will not return until there are files.
        let resp = engine
            .send_modeling_cmd(
                false,
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

/// Get the volume of an object in a CAD file.
///
/// If the input file is larger than a certain size it will be
/// performed asynchronously, you can then check the status with the
/// `zoo api-call status <id_of_your_operation>` command.
///
///     # get the volume of a file
///     $ zoo file volume my-file.step
///
///     # pass a file from stdin, the original file type is required
///     $ cat my-obj.obj | zoo file volume - --src-format=obj
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdFileVolume {
    /// The path to the input file.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", required = true)]
    pub input: std::path::PathBuf,

    /// A valid source file format.
    #[clap(short = 's', long = "src-format", value_enum)]
    src_format: Option<kittycad::types::FileImportFormat>,

    /// Output format.
    #[clap(long, short, value_enum)]
    pub format: Option<crate::types::FormatOutput>,

    /// Output unit.
    #[clap(long = "output-unit", short = 'u', value_enum)]
    pub output_unit: kittycad::types::UnitVolume,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdFileVolume {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        // Parse the source format.
        let src_format = if let Some(src_format) = &self.src_format {
            src_format.clone()
        } else {
            get_import_format_from_extension(&get_extension(self.input.clone()))?
        };

        // Get the contents of the input file.
        let input = ctx.read_file(self.input.to_str().unwrap_or(""))?;

        // Do the operation.
        let client = ctx.api_client("")?;

        let file_volume = client
            .file()
            .create_volume(Some(self.output_unit.clone()), src_format, &input.into())
            .await?;

        // Print the output of the conversion.
        let format = ctx.format(&self.format)?;
        ctx.io.write_output(&format, &file_volume)?;

        Ok(())
    }
}

/// Get the mass of an object in a CAD file.
///
/// If the input file is larger than a certain size it will be
/// performed asynchronously, you can then check the status with the
/// `zoo api-call status <id_of_your_operation>` command.
///
///     # get the mass of a file
///     $ zoo file mass my-file.step
///
///     # pass a file from stdin, the original file type is required
///     $ cat my-obj.obj | zoo file mass - --src-format=obj
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdFileMass {
    /// The path to the input file.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", required = true)]
    pub input: std::path::PathBuf,

    /// A valid source file format.
    #[clap(short = 's', long = "src-format", value_enum)]
    src_format: Option<kittycad::types::FileImportFormat>,

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
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdFileMass {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        if self.material_density == 0.0 {
            anyhow::bail!("`--material-density` must not be 0.0");
        }

        // Parse the source format.
        let src_format = if let Some(src_format) = &self.src_format {
            src_format.clone()
        } else {
            get_import_format_from_extension(&get_extension(self.input.clone()))?
        };

        // Get the contents of the input file.
        let input = ctx.read_file(self.input.to_str().unwrap_or(""))?;

        // Do the operation.
        let client = ctx.api_client("")?;

        let file_mass = client
            .file()
            .create_mass(
                self.material_density.into(),
                Some(self.material_density_unit.clone()),
                Some(self.output_unit.clone()),
                src_format,
                &input.into(),
            )
            .await?;

        // Print the output of the conversion.
        let format = ctx.format(&self.format)?;
        ctx.io.write_output(&format, &file_mass)?;

        Ok(())
    }
}

/// Get the center of mass of an object in a CAD file.
///
/// If the input file is larger than a certain size it will be
/// performed asynchronously, you can then check the status with the
/// `zoo api-call status <id_of_your_operation>` command.
///
///     # get the mass of a file
///     $ zoo file center-of-mass my-file.step
///
///     # pass a file from stdin, the original file type is required
///     $ cat my-obj.obj | zoo file center-of-mass - --src-format=obj
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdFileCenterOfMass {
    /// The path to the input file.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", required = true)]
    pub input: std::path::PathBuf,

    /// A valid source file format.
    #[clap(short = 's', long = "src-format", value_enum)]
    src_format: Option<kittycad::types::FileImportFormat>,

    /// Output format.
    #[clap(long, short, value_enum)]
    pub format: Option<crate::types::FormatOutput>,

    /// Output unit.
    #[clap(long = "output-unit", short = 'u', value_enum)]
    pub output_unit: kittycad::types::UnitLength,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdFileCenterOfMass {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        // Parse the source format.
        let src_format = if let Some(src_format) = &self.src_format {
            src_format.clone()
        } else {
            get_import_format_from_extension(&get_extension(self.input.clone()))?
        };

        // Get the contents of the input file.
        let input = ctx.read_file(self.input.to_str().unwrap_or(""))?;

        // Do the operation.
        let client = ctx.api_client("")?;

        let file_center_of_mass = client
            .file()
            .create_center_of_mass(Some(self.output_unit.clone()), src_format, &input.into())
            .await?;

        // Print the output of the conversion.
        let format = ctx.format(&self.format)?;
        ctx.io.write_output(&format, &file_center_of_mass)?;

        Ok(())
    }
}

/// Get the density of an object in a CAD file.
///
/// If the input file is larger than a certain size it will be
/// performed asynchronously, you can then check the status with the
/// `zoo api-call status <id_of_your_operation>` command.
///
///     # get the density of a file
///     $ zoo file density my-file.step
///
///     # pass a file from stdin, the original file type is required
///     $ cat my-obj.obj | zoo file density - --src-format=obj
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdFileDensity {
    /// The path to the input file.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", required = true)]
    pub input: std::path::PathBuf,

    /// A valid source file format.
    #[clap(short = 's', long = "src-format")]
    src_format: Option<kittycad::types::FileImportFormat>,

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
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdFileDensity {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        if self.material_mass == 0.0 {
            anyhow::bail!("`--material-mass` must not be 0.0");
        }

        // Parse the source format.
        let src_format = if let Some(src_format) = &self.src_format {
            src_format.clone()
        } else {
            get_import_format_from_extension(&get_extension(self.input.clone()))?
        };

        // Get the contents of the input file.
        let input = ctx.read_file(self.input.to_str().unwrap_or(""))?;

        // Do the operation.
        let client = ctx.api_client("")?;

        let file_density = client
            .file()
            .create_density(
                self.material_mass.into(),
                Some(self.material_mass_unit.clone()),
                Some(self.output_unit.clone()),
                src_format,
                &input.into(),
            )
            .await?;

        // Print the output of the conversion.
        let format = ctx.format(&self.format)?;
        ctx.io.write_output(&format, &file_density)?;

        Ok(())
    }
}

/// Get the surface area of an object in a CAD file.
///
/// If the input file is larger than a certain size it will be
/// performed asynchronously, you can then check the status with the
/// `zoo api-call status <id_of_your_operation>` command.
///
///     # get the surface-area of a file
///     $ zoo file surface-area my-file.step
///
///     # pass a file from stdin, the original file type is required
///     $ cat my-obj.obj | zoo file surface-area - --src-format=obj
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdFileSurfaceArea {
    /// The path to the input file.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", required = true)]
    pub input: std::path::PathBuf,

    /// A valid source file format.
    #[clap(short = 's', long = "src-format")]
    src_format: Option<kittycad::types::FileImportFormat>,

    /// Output format.
    #[clap(long, short, value_enum)]
    pub format: Option<crate::types::FormatOutput>,

    /// Output unit.
    #[clap(long = "output-unit", short = 'u', value_enum)]
    pub output_unit: kittycad::types::UnitArea,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdFileSurfaceArea {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        // Parse the source format.
        let src_format = if let Some(src_format) = &self.src_format {
            src_format.clone()
        } else {
            get_import_format_from_extension(&get_extension(self.input.clone()))?
        };

        // Get the contents of the input file.
        let input = ctx.read_file(self.input.to_str().unwrap_or(""))?;

        // Do the operation.
        let client = ctx.api_client("")?;

        let file_surface_area = client
            .file()
            .create_surface_area(Some(self.output_unit.clone()), src_format, &input.into())
            .await?;

        // Print the output of the conversion.
        let format = ctx.format(&self.format)?;
        ctx.io.write_output(&format, &file_surface_area)?;

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

/// Get the source format from the extension.
fn get_import_format_from_extension(ext: &str) -> Result<kittycad::types::FileImportFormat> {
    match kittycad::types::FileImportFormat::from_str(ext) {
        Ok(format) => Ok(format),
        Err(_) => {
            if ext == "stp" {
                Ok(kittycad::types::FileImportFormat::Step)
            } else if ext == "glb" {
                Ok(kittycad::types::FileImportFormat::Gltf)
            } else {
                anyhow::bail!(
                    "unknown source format for file extension: {}. Try setting the `--src-format` flag explicitly or use a valid format.",
                    ext
                )
            }
        }
    }
}

/// Get the source format from the extension.
fn get_input_format(
    format: kittycad::types::FileImportFormat,
    ul: kittycad::types::UnitLength,
) -> Result<kittycad::types::InputFormat> {
    // Zoo co-ordinate system.
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
        kittycad::types::FileImportFormat::Step => Ok(kittycad::types::InputFormat::Step {}),
        kittycad::types::FileImportFormat::Stl => Ok(kittycad::types::InputFormat::Stl { coords, units: ul }),
        kittycad::types::FileImportFormat::Obj => Ok(kittycad::types::InputFormat::Obj { coords, units: ul }),
        kittycad::types::FileImportFormat::Gltf => Ok(kittycad::types::InputFormat::Gltf {}),
        kittycad::types::FileImportFormat::Ply => Ok(kittycad::types::InputFormat::Ply { coords, units: ul }),
        kittycad::types::FileImportFormat::Fbx => Ok(kittycad::types::InputFormat::Fbx {}),
        kittycad::types::FileImportFormat::Sldprt => Ok(kittycad::types::InputFormat::Sldprt {}),
    }
}

/// Determine if buffers[].buffer.uri is a data uri.
fn is_data_uri(s: &str) -> bool {
    matches!(s.split(':').next(), Some("data"))
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use crate::cmd::Command;

    pub struct TestItem {
        name: String,
        cmd: crate::cmd_file::SubCommand,
        stdin: String,
        want_out: String,
        want_err: String,
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[serial_test::serial]
    async fn test_cmd_file() {
        let tests: Vec<TestItem> = vec![
            TestItem {
                    name: "convert input with bad ext".to_string(),
                    cmd: crate::cmd_file::SubCommand::Convert(crate::cmd_file::CmdFileConvert {
                        input: std::path::PathBuf::from("test/bad_ext.bad_ext"),
                        output_dir: std::path::PathBuf::from("tests/"),
                        output_format: kittycad::types::FileExportFormat::Obj,
                        src_format: None,
                        format: None,

                    }),
                    stdin: "".to_string(),
                    want_out: "".to_string(),
                    want_err: "unknown source format for file extension: bad_ext. Try setting the `--src-format` flag explicitly or use a valid format.".to_string(),
                },
                TestItem {
                    name: "convert: input file does not exist".to_string(),
                    cmd: crate::cmd_file::SubCommand::Convert(crate::cmd_file::CmdFileConvert {
                        input: std::path::PathBuf::from("test/bad_ext.stp"),
                        output_dir: std::path::PathBuf::from("tests/"),
                        output_format: kittycad::types::FileExportFormat::Obj,
                        src_format: None,
                        format: None,
                    }),
                    stdin: "".to_string(),
                    want_out: "".to_string(),
                    want_err: "File 'test/bad_ext.stp' does not exist.".to_string(),
                },
                TestItem {
                    name: "volume with bad ext".to_string(),
                    cmd: crate::cmd_file::SubCommand::Volume(crate::cmd_file::CmdFileVolume {
                        input: std::path::PathBuf::from("tests/bad_ext.bad_ext"),
                        src_format: None,
                        format: None,
                        output_unit: kittycad::types::UnitVolume::Cm3,
                    }),
                    stdin: "".to_string(),
                    want_out: "".to_string(),
                    want_err: "unknown source format for file extension: bad_ext. Try setting the `--src-format` flag explicitly or use a valid format.".to_string(),
                },
                TestItem {
                    name: "volume: input file does not exist".to_string(),
                    cmd: crate::cmd_file::SubCommand::Volume(crate::cmd_file::CmdFileVolume {
                        input: std::path::PathBuf::from("test/bad_ext.stp"),
                        src_format: None,
                        format: None,
                        output_unit: kittycad::types::UnitVolume::Cm3,
                    }),
                    stdin: "".to_string(),
                    want_out: "".to_string(),
                    want_err: "File 'test/bad_ext.stp' does not exist.".to_string(),
                }
                ];

        let mut config = crate::config::new_blank_config().unwrap();
        let mut c = crate::config_from_env::EnvConfig::inherit_env(&mut config);

        for t in tests {
            let (mut io, stdout_path, stderr_path) = crate::iostreams::IoStreams::test();
            if !t.stdin.is_empty() {
                io.stdin = Box::new(std::io::Cursor::new(t.stdin));
            }
            // We need to also turn off the fancy terminal colors.
            // This ensures it also works in GitHub actions/any CI.
            io.set_color_enabled(false);
            // TODO: we should figure out how to test the prompts.
            io.set_never_prompt(true);
            let mut ctx = crate::context::Context {
                config: &mut c,
                io,
                debug: false,
            };

            let cmd_file = crate::cmd_file::CmdFile { subcmd: t.cmd };
            match cmd_file.run(&mut ctx).await {
                Ok(()) => {
                    let stdout = std::fs::read_to_string(stdout_path).unwrap();
                    let stderr = std::fs::read_to_string(stderr_path).unwrap();
                    assert!(stderr.is_empty(), "test {}: {}", t.name, stderr);
                    if !stdout.contains(&t.want_out) {
                        assert_eq!(stdout, t.want_out, "test {}: stdout mismatch", t.name);
                    }
                }
                Err(err) => {
                    let stdout = std::fs::read_to_string(stdout_path).unwrap();
                    let stderr = std::fs::read_to_string(stderr_path).unwrap();
                    assert_eq!(stdout, t.want_out, "test {}", t.name);
                    if !err.to_string().contains(&t.want_err) {
                        assert_eq!(err.to_string(), t.want_err, "test {}: err mismatch", t.name);
                    }
                    assert!(stderr.is_empty(), "test {}: {}", t.name, stderr);
                }
            }
        }
    }
}
