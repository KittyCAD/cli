//! This module reads a KCL project from disk into memory, so it can be sent over the network.
use std::env::current_dir;

use anyhow::{Result, anyhow};
use camino::{Utf8Path, Utf8PathBuf};
use kittycad_modeling_cmds::{
    exec_kcl::{KclFile, KclProject},
    shared::safe_filepath::SafeFilepath,
};

const FILES_TO_SEND_TO_ENGINE: [&str; 18] = [
    "kcl", "fbx", "glb", "gltf", "obj", "ply", "step", "stl", "sat", "sab", "model", "catpart", "ipt", "prt", "xpr",
    "x_t", "x_t", "sldprt",
];

/// If the user passed '.' or '-' then the KCL project's entrypoint
/// is assumed to be 'main.kcl'.
/// If filepath is '-' then the code should be given in `code`. Otherwise that argument is not used.
pub fn build_kcl_project(filepath: &Utf8Path, code: &str) -> Result<KclProject> {
    let cwd = current_dir().map_err(|e| anyhow!("Could not get current working directory: {e}"))?;
    let cwd = Utf8PathBuf::from_path_buf(cwd)
        .map_err(|cwd| anyhow!("expected cwd to be unicode but found: {}", cwd.display()))?;
    // Single-file case, where file comes from stdin.
    if filepath == "-" {
        let entrypoint = safe_filepath("main.kcl")?;
        let file = KclFile::new(entrypoint.clone(), code.as_bytes().to_vec());
        return Ok(KclProject::new(vec![file], entrypoint));
    }

    let (project_root, entrypoint) = find_kcl_directory(&cwd, filepath);

    let entrypoint = entrypoint.strip_prefix(&project_root)?;

    build_kcl_project_from(entrypoint, &project_root)
}

/// Returns the project root, and the entrypoint (i.e. first KCL file to start executing)
fn find_kcl_directory(cwd: &Utf8Path, filepath: &Utf8Path) -> (Utf8PathBuf, Utf8PathBuf) {
    // Get the project's root directory and entrypoint.
    if filepath == "." {
        // If user passed '.', then assume the entrypoint is main.kcl
        let root = cwd.to_path_buf();
        let mut entrypoint = root.clone();
        entrypoint.push("main.kcl");
        (root, entrypoint)
    } else {
        let root = filepath
            .parent()
            .map(|parent| parent.to_path_buf())
            .unwrap_or(Utf8PathBuf::from("."));
        if root == "" {
            (Utf8PathBuf::from("."), Utf8PathBuf::from(format!("./{filepath}")))
        } else {
            (root, filepath.to_owned())
        }
    }
}

fn build_kcl_project_from(entrypoint: &Utf8Path, project_root: &Utf8Path) -> Result<KclProject> {
    // Find all relevant files in the KCL project.
    let mut files = Vec::new();
    for entry in ignore::WalkBuilder::new(project_root).build() {
        let entry = entry.map_err(|e| anyhow!("could not read entry: {e}"))?;
        // We only care about files here, the Walker takes care of directories.
        if !entry.file_type().is_some_and(|file_type| file_type.is_file()) {
            continue;
        }
        let Some(extension) = entry.path().extension().map(|s| s.display().to_string()) else {
            continue;
        };
        if !FILES_TO_SEND_TO_ENGINE.contains(&extension.as_str()) {
            continue;
        }

        let contents = std::fs::read(entry.path())?;

        let path = Utf8Path::from_path(entry.path())
            .ok_or(anyhow!("invalid path {}", entry.path().display()))?
            .to_path_buf();
        let relative_path = path.strip_prefix(project_root)?;

        files.push(KclFile::new(safe_filepath(relative_path.as_str())?, contents));
    }

    let entrypoint = safe_filepath(entrypoint.as_str())?;
    Ok(KclProject::new(files, entrypoint))
}

fn safe_filepath(path: &str) -> Result<SafeFilepath> {
    path.parse::<SafeFilepath>()
        .map_err(|err| anyhow!("invalid KCL project path `{path}`: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Changes working directory when acquired,
    /// changes it back to original when dropped.
    struct CurrentDirGuard {
        original: std::path::PathBuf,
    }

    impl CurrentDirGuard {
        fn set_to(path: &std::path::Path) -> Self {
            let original = std::env::current_dir().unwrap();
            std::env::set_current_dir(path).unwrap();
            Self { original }
        }
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            std::env::set_current_dir(&self.original).unwrap();
        }
    }

    /// List files in the project, in alphabetical order.
    fn project_file_paths(project: &KclProject) -> Vec<String> {
        let mut paths = project
            .files
            .iter()
            .map(|file| file.path.to_string())
            .collect::<Vec<_>>();
        paths.sort();
        paths
    }

    /// Read this file from this project. Panics if file not found.
    fn project_file_contents(project: &KclProject, path: &str) -> String {
        let file = project
            .files
            .iter()
            .find(|file| file.path.to_string() == path)
            .unwrap_or_else(|| panic!("missing project file `{path}`"));
        String::from_utf8(file.contents.clone()).unwrap()
    }

    const MAIN_DOT_KCL: &str = "main.kcl";
    const EXAMPLE_FILE_CONTENTS: &str = "cube(1)\n";

    /// Test building a KCL project from - i.e. reading main.kcl from stdin
    #[test]
    fn build_kcl_project_from_stdin_uses_main_kcl() {
        let contents = "cube(1)\n";
        let project = build_kcl_project(Utf8Path::new("-"), contents).unwrap();

        // Projects built from - should have one file, main.kcl.
        assert_eq!(project.entrypoint.to_string(), MAIN_DOT_KCL);
        assert_eq!(project_file_paths(&project), vec![MAIN_DOT_KCL]);
        assert_eq!(project_file_contents(&project, MAIN_DOT_KCL), contents);
    }

    /// Test that passing a single file as the KCL project will use that file properly.
    #[test]
    #[serial_test::serial]
    fn build_kcl_project_from_bare_file_uses_file_as_entrypoint() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("cube.kcl"), EXAMPLE_FILE_CONTENTS).unwrap();
        let _guard = CurrentDirGuard::set_to(tmp.path());

        let project = build_kcl_project(Utf8Path::new("cube.kcl"), "should_be_ignored()").unwrap();

        // Only that file should be included.
        assert_eq!(project.entrypoint.to_string(), "cube.kcl");
        assert_eq!(project_file_paths(&project), vec!["cube.kcl"]);
        assert_eq!(project_file_contents(&project, "cube.kcl"), EXAMPLE_FILE_CONTENTS);
    }

    /// Test that all KCL files get included when using . (i.e. the cwd)
    #[test]
    #[serial_test::serial]
    fn build_kcl_project_from_dot_uses_main_kcl_entrypoint() {
        let tmp = tempfile::tempdir().unwrap();
        // Put two files in the cwd.
        std::fs::write(tmp.path().join(MAIN_DOT_KCL), EXAMPLE_FILE_CONTENTS).unwrap();
        std::fs::write(tmp.path().join("part.kcl"), "sphere(1)\n").unwrap();
        let _guard = CurrentDirGuard::set_to(tmp.path());

        let project = build_kcl_project(Utf8Path::new("."), "cube(2)\n").unwrap();

        // Both files should be in the project.
        assert_eq!(project.entrypoint.to_string(), MAIN_DOT_KCL);
        assert_eq!(project_file_paths(&project), vec![MAIN_DOT_KCL, "part.kcl"]);
    }

    /// Test that when you give the project entrypoint as some file nested in
    /// a directory, the files in that directory are all included.
    #[test]
    #[serial_test::serial]
    fn build_kcl_project_from_nested_main_uses_paths_relative_to_project_root() {
        // Write two files into the cube/ directory.
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("cube");
        std::fs::create_dir(&project_dir).unwrap();
        std::fs::write(project_dir.join(MAIN_DOT_KCL), "import \"part.kcl\"\n").unwrap();
        std::fs::write(project_dir.join("part.kcl"), "sphere(1)\n").unwrap();
        let _guard = CurrentDirGuard::set_to(tmp.path());

        let project = build_kcl_project(Utf8Path::new("cube/main.kcl"), "cube(2)\n").unwrap();

        // Directory name should be stripped,
        // i.e. we should see main.kcl, not cube/main.kcl
        assert_eq!(project.entrypoint.to_string(), MAIN_DOT_KCL);
        assert_eq!(project_file_paths(&project), vec![MAIN_DOT_KCL, "part.kcl"]);
    }

    /// Test that relevant files (e.g. kcl, step) are included,
    /// but not irrelevant files (e.g. txt)
    #[test]
    #[serial_test::serial]
    fn build_kcl_project_includes_supported_assets_and_skips_unrelated_files() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("assembly");
        std::fs::create_dir(&project_dir).unwrap();
        std::fs::write(project_dir.join(MAIN_DOT_KCL), "import \"part.kcl\"\n").unwrap();
        std::fs::write(project_dir.join("part.kcl"), "sphere(1)\n").unwrap();
        std::fs::write(project_dir.join("shape.step"), b"step bytes").unwrap();
        std::fs::write(project_dir.join("notes.txt"), "not sent\n").unwrap();
        std::fs::write(project_dir.join("no_extension"), "not sent\n").unwrap();
        let _guard = CurrentDirGuard::set_to(tmp.path());

        let project = build_kcl_project(Utf8Path::new("assembly/main.kcl"), "cube(2)\n").unwrap();

        assert_eq!(project.entrypoint.to_string(), MAIN_DOT_KCL);
        assert_eq!(
            project_file_paths(&project),
            vec![MAIN_DOT_KCL, "part.kcl", "shape.step"]
        );
        assert_eq!(project_file_contents(&project, "shape.step"), "step bytes");
    }
}
