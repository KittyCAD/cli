#[cfg(target_os = "linux")]
use glob::glob;
#[cfg(not(target_os = "linux"))]
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

const NOT_INSTALLED_ERROR: &str = r#"The Zoo Design Studio is not installed. 
Please download it from https://zoo.dev/design-studio/download
If you do have the Design Studio installed already, we were 
unable to find it in the standard locations. Please open 
an issue at https://github.com/KittyCAD/cli/issues/new"#;

/// Open a directory or file in the Zoo Design Studio on your desktop.
///
/// If you do not have the app installed, you will be prompted to download it.
///
///     $ zoo app .
///
///     $ zoo app main.kcl
///
///     $ zoo app ../main.kcl
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdApp {
    /// The path to the file or directory to open in the app.
    pub path: std::path::PathBuf,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdApp {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let app_path = get_app_path()?;
        let extra_args = get_extra_args()?;

        writeln!(ctx.io.out, "Opening the Zoo Design Studio at {}", app_path.display())?;

        std::process::Command::new(app_path)
            .arg(&self.path)
            .args(&extra_args)
            .spawn()?;

        Ok(())
    }
}

#[cfg(target_os = "linux")]
/// Get the path to the application on linux, assuming .AppImage installation as
/// suggested at https://github.com/KittyCAD/modeling-app/blob/ac23d40e0bc756028d3933060c0c4377e7f6b6a3/INSTALL.md#linux.
// TODO: consider other install locations
fn get_app_path() -> Result<std::path::PathBuf> {
    match dirs::home_dir() {
        Some(home) => {
            let path = home
                .join("Applications")
                .join("Zoo Design Studio-*-arm64-linux.AppImage");
            for entry in glob(&path.to_string_lossy()).expect("Failed to read glob pattern") {
                match entry {
                    Ok(path) => return Ok(path),
                    Err(e) => println!("{:?}", e),
                }
            }
            anyhow::bail!(NOT_INSTALLED_ERROR);
        }
        None => {
            anyhow::bail!("Could not determine home directory");
        }
    }
}

#[cfg(target_os = "linux")]
/// Get the extra args for the application on linux.
fn get_extra_args() -> Result<Vec<String>> {
    let args = vec!["--no-sandbox".into()];
    Ok(args)
}

#[cfg(target_os = "macos")]
/// Get the path to the application on macOS.
fn get_app_path() -> Result<std::path::PathBuf> {
    let paths_to_try = [PathBuf::from(
        "/Applications/Zoo Design Studio.app/Contents/MacOS/Zoo Design Studio",
    )];

    for path in paths_to_try.iter() {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    anyhow::bail!(NOT_INSTALLED_ERROR);
}

#[cfg(target_os = "macos")]
/// Get the extra args for the application on macos.
fn get_extra_args() -> Result<Vec<String>> {
    let args = vec![];
    Ok(args)
}

#[cfg(target_os = "windows")]
/// Get the path to the application on windows.
fn get_app_path() -> Result<std::path::PathBuf> {
    let paths_to_try = [PathBuf::from(
        r#"C:\Program Files\Zoo Design Studio\Zoo Design Studio.exe"#,
    )];

    for path in paths_to_try.iter() {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    anyhow::bail!(NOT_INSTALLED_ERROR);
}

#[cfg(target_os = "macos")]
/// Get the extra args for the application on windows.
fn get_extra_args() -> Result<Vec<String>> {
    let args = vec![];
    Ok(args)
}
