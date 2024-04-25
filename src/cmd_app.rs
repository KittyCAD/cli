use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

/// Open a directory or file in the Zoo Modeling App on your desktop.
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

        writeln!(ctx.io.out, "Opening the Zoo Modeling App at {}", app_path.display())?;

        std::process::Command::new(app_path).arg(&self.path).spawn()?;

        Ok(())
    }
}

/// Get the path to the application on macOS.
fn get_app_path() -> Result<std::path::PathBuf> {
    let paths_to_try = [
        PathBuf::from("/Applications/Zoo Modeling App.app/Contents/MacOS/Zoo Modeling App"),
        PathBuf::from("/Applications/Zoo Modeling.app/Contents/MacOS/Zoo Modeling App"),
        PathBuf::from("/Applications/Zoo.app/Contents/MacOS/Zoo Modeling App"),
        PathBuf::from("/Applications/KittyCAD Modeling.app/Contents/MacOS/Zoo Modeling App"),
    ];

    for path in paths_to_try.iter() {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    anyhow::bail!(
        r#"The Zoo Modeling App is not installed. 
Please download it from https://zoo.dev/modeling-app/download
If you do have the Modeling App installed already, we were 
unable to find it in the standard locations. Please open 
an issue at https://github.com/KittyCAD/cli/issues/new"#
    );
}
