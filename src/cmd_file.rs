use std::str::FromStr;

use anyhow::Result;
use clap::Parser;
use kittycad::traits::Base64Ops;

/// Perform operations on CAD files.
///
///     # convert a step file to an obj file
///     $ kittycad file convert ./input.step --output-format=obj
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdFile {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug, Clone)]
enum SubCommand {
    Convert(CmdFileConvert),
    Volume(CmdFileVolume),
    Mass(CmdFileMass),
    Density(CmdFileDensity),
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdFile {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            SubCommand::Convert(cmd) => cmd.run(ctx).await,
            SubCommand::Volume(cmd) => cmd.run(ctx).await,
            SubCommand::Mass(cmd) => cmd.run(ctx).await,
            SubCommand::Density(cmd) => cmd.run(ctx).await,
        }
    }
}

/// Convert a CAD file from one format to another.
///
/// If the file being converted is larger than a certain size it will be
/// performed asynchronously, you can then check its status with the
/// `kittycad api-call status <id_of_your_operation>` command.
///
///     # convert step to obj and save to file
///     $ kittycad file convert my-file.step my-file.obj
///
///     # convert obj to step and print to stdout
///     $ kittycad file convert my-obj.obj --to step
///
///     # convert step to obj and print to stdout
///     $ kittycad file convert my-step.step -t obj
///
///     # pass a file to convert from stdin and print to stdout
///     # when converting from stdin, the original file type is required
///     $ cat my-obj.obj | kittycad file convert - --to step --from obj
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdFileConvert {
    /// The path to the input file to convert.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", parse(from_os_str), required = true)]
    pub input: std::path::PathBuf,

    /// The path to an output file, this is not necessary but if given it will
    /// save the output of the conversion to the given path.
    #[clap(name = "output")]
    pub output: Option<std::path::PathBuf>,

    /// A valid source file format.
    #[clap(short = 's', long = "src-format")]
    src_format: Option<kittycad::types::FileSourceFormat>,
    /// A valid output file format.
    #[clap(short = 't', long = "output-format")]
    output_format: Option<kittycad::types::FileOutputFormat>,

    /// Command output format.
    #[clap(long, short)]
    pub format: Option<crate::types::FormatOutput>,
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdFileConvert {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        // Parse the source format.
        let src_format = if let Some(src_format) = &self.src_format {
            src_format.clone()
        } else {
            get_source_format_from_extension(&get_extension(self.input.clone()))?
        };

        if self.output_format.is_none() && self.output.is_none() {
            anyhow::bail!("`--output-format` or `<output>` is required");
        }

        // Parse the output format.
        let output_format = if let Some(output_format) = &self.output_format {
            output_format.clone()
        } else {
            get_output_format_from_extension(&get_extension(self.input.clone()))?
        };

        // Get the contents of the input file.
        let input = ctx.read_file(self.input.to_str().unwrap_or(""))?;

        // Do the conversion.
        let client = ctx.api_client("")?;

        // Create the file conversion.
        let (file_conversion, contents) = client
            .file()
            .create_conversion_with_decode(output_format, src_format, input)
            .await?;

        // If they specified an output file, save the output to that file.
        if let Some(output) = &self.output {
            if file_conversion.status == kittycad::types::ApiCallStatus::Completed {
                if !contents.is_empty() {
                    std::fs::write(output, contents)?;
                } else {
                    anyhow::bail!("no output was generated! (this is probably a bug in the API) you should report it to support@kittycad.io");
                }
            }
        }

        // Print the output of the conversion.
        let format = ctx.format(&self.format)?;
        ctx.io.write_output(&format, &file_conversion)?;

        Ok(())
    }
}

/// Get the volume of an object in a CAD file.
///
/// If the input file is larger than a certain size it will be
/// performed asynchronously, you can then check the status with the
/// `kittycad api-call status <id_of_your_operation>` command.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdFileVolume {
    /// The path to the input file.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", parse(from_os_str), required = true)]
    pub input: std::path::PathBuf,

    /// A valid source file format.
    #[clap(short = 's', long = "src-format")]
    src_format: Option<kittycad::types::FileSourceFormat>,

    /// Output format.
    #[clap(long, short)]
    pub format: Option<crate::types::FormatOutput>,
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdFileVolume {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        // Parse the source format.
        let src_format = if let Some(src_format) = &self.src_format {
            src_format.clone()
        } else {
            get_source_format_from_extension(&get_extension(self.input.clone()))?
        };

        // Get the contents of the input file.
        let input = ctx.read_file(self.input.to_str().unwrap_or(""))?;

        // Do the operation.
        let client = ctx.api_client("")?;

        // Create the file conversion.
        let file_volume = client.file().create_volume(src_format, input).await?;

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
/// `kittycad api-call status <id_of_your_operation>` command.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdFileMass {
    /// The path to the input file.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", parse(from_os_str), required = true)]
    pub input: std::path::PathBuf,

    /// A valid source file format.
    #[clap(short = 's', long = "src-format")]
    src_format: Option<kittycad::types::FileSourceFormat>,

    /// Material density.
    #[clap(short = 'm', long = "material-density", default_value = "1.0")]
    material_density: f32,

    /// Output format.
    #[clap(long, short)]
    pub format: Option<crate::types::FormatOutput>,
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdFileMass {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        if self.material_density == 0.0 {
            anyhow::bail!("`--material-density` must not be 0.0");
        }

        // Parse the source format.
        let src_format = if let Some(src_format) = &self.src_format {
            src_format.clone()
        } else {
            get_source_format_from_extension(&get_extension(self.input.clone()))?
        };

        // Get the contents of the input file.
        let input = ctx.read_file(self.input.to_str().unwrap_or(""))?;

        // Do the operation.
        let client = ctx.api_client("")?;

        // Create the file conversion.
        let file_mass = client
            .file()
            .create_mass(self.material_density.into(), src_format, input)
            .await?;

        // Print the output of the conversion.
        let format = ctx.format(&self.format)?;
        ctx.io.write_output(&format, &file_mass)?;

        Ok(())
    }
}

/// Get the density of an object in a CAD file.
///
/// If the input file is larger than a certain size it will be
/// performed asynchronously, you can then check the status with the
/// `kittycad api-call status <id_of_your_operation>` command.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdFileDensity {
    /// The path to the input file.
    /// If you pass `-` as the path, the file will be read from stdin.
    #[clap(name = "input", parse(from_os_str), required = true)]
    pub input: std::path::PathBuf,

    /// A valid source file format.
    #[clap(short = 's', long = "src-format")]
    src_format: Option<kittycad::types::FileSourceFormat>,

    /// Material mass.
    #[clap(short = 'm', long = "material-mass", default_value = "1.0")]
    material_mass: f32,

    /// Output format.
    #[clap(long, short)]
    pub format: Option<crate::types::FormatOutput>,
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdFileDensity {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        if self.material_mass == 0.0 {
            anyhow::bail!("`--material-mass` must not be 0.0");
        }

        // Parse the source format.
        let src_format = if let Some(src_format) = &self.src_format {
            src_format.clone()
        } else {
            get_source_format_from_extension(&get_extension(self.input.clone()))?
        };

        // Get the contents of the input file.
        let input = ctx.read_file(self.input.to_str().unwrap_or(""))?;

        // Do the operation.
        let client = ctx.api_client("")?;

        // Create the file conversion.
        let file_density = client
            .file()
            .create_density(self.material_mass.into(), src_format, input)
            .await?;

        // Print the output of the conversion.
        let format = ctx.format(&self.format)?;
        ctx.io.write_output(&format, &file_density)?;

        Ok(())
    }
}

/// Get the extension for a path buffer.
fn get_extension(path: std::path::PathBuf) -> String {
    path.into_boxed_path()
        .extension()
        .unwrap_or_default()
        .to_str()
        .unwrap_or("")
        .to_string()
}

/// Get the source format from the extension.
fn get_source_format_from_extension(ext: &str) -> Result<kittycad::types::FileSourceFormat> {
    match kittycad::types::FileSourceFormat::from_str(ext) {
        Ok(format) => Ok(format),
        Err(_) => {
            if ext == "stp" {
                Ok(kittycad::types::FileSourceFormat::Step)
            } else {
                anyhow::bail!(
                    "unknown source format for file extension: {}. Try setting the `--src-format` flag explicitly or use a valid format.",
                    ext
                )
            }
        }
    }
}

/// Get the output format from the extension.
fn get_output_format_from_extension(ext: &str) -> Result<kittycad::types::FileOutputFormat> {
    match kittycad::types::FileOutputFormat::from_str(ext) {
        Ok(format) => Ok(format),
        Err(_) => {
            if ext == "stp" {
                Ok(kittycad::types::FileOutputFormat::Step)
            } else {
                anyhow::bail!(
                    "unknown source format for file extension: {}. Try setting the `--output-format` flag explicitly or use a valid format.",
                    ext
                )
            }
        }
    }
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
        let tests: Vec<TestItem> = vec![TestItem {
                    name: "convert with bad ext".to_string(),
                    cmd: crate::cmd_file::SubCommand::Convert(crate::cmd_file::CmdFileConvert {
                        input: std::path::PathBuf::from("test/bad_ext.bad_ext"),
                        output: None,
                        output_format: None,
                        src_format: None,
                        format: None,
                    }),
                    stdin: "".to_string(),
                    want_out: "".to_string(),
                    want_err: "unknown source format for file extension: bad_ext. Try setting the `--src-format` flag explicitly or use a valid format.".to_string(),
                },
        TestItem {
                    name: "convert with no output_format".to_string(),
                    cmd: crate::cmd_file::SubCommand::Convert(crate::cmd_file::CmdFileConvert {
                        input: std::path::PathBuf::from("test/bad_ext.stp"),
                        output: None,
                        output_format: None,
                        src_format: None,
                        format: None,
                    }),
                    stdin: "".to_string(),
                    want_out: "".to_string(),
                    want_err: "`--output-format` or `<output>` is required".to_string(),
                },
                TestItem {
                    name: "convert: input file does not exist".to_string(),
                    cmd: crate::cmd_file::SubCommand::Convert(crate::cmd_file::CmdFileConvert {
                        input: std::path::PathBuf::from("test/bad_ext.stp"),
                        output: None,
                        output_format: Some(kittycad::types::FileOutputFormat::Obj),
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
                        input: std::path::PathBuf::from("test/bad_ext.bad_ext"),
                        src_format: None,
                        format: None,
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
