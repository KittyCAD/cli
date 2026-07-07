//! Child-process integration tests for KCL commands that need real process
//! environment variables.
//!
//! These tests execute the compiled `zoo` binary instead of calling `do_main`
//! in-process. This is intentional: some KCL paths depend on `kcl_lib`, and
//! `kcl_lib` reads authentication details such as `ZOO_API_TOKEN` directly from
//! the process environment. In Rust 2024, mutating the parent test process
//! environment with `std::env::set_var` or `std::env::remove_var` is not a safe
//! way to test that behavior on every platform. Passing environment variables to
//! a child process with `Command::env` gives each test its own real environment
//! without mutating global state in the test process.
//!
//! Use this harness for tests that must verify behavior of `kcl snapshot`,
//! `kcl export`, `kcl analyze`, or other commands where dependencies need to
//! observe real environment variables. Use ordinary same-process unit tests for
//! pure CLI/config behavior that can be tested with injected values or in-memory
//! config.

use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

const ANALYZE_CUBE_KCL: &str = r#"@settings(kclVersion = 2.0)

rectangleSketch = sketch(on = XY) {
  line1 = line(start = [var 2.47mm, var 2.96mm], end = [var 3.47mm, var 2.96mm])
  line2 = line(start = [var 3.47mm, var 2.96mm], end = [var 3.47mm, var 3.96mm])
  line3 = line(start = [var 3.47mm, var 3.96mm], end = [var 2.47mm, var 3.96mm])
  line4 = line(start = [var 2.47mm, var 3.96mm], end = [var 2.47mm, var 2.96mm])
  coincident([line1.end, line2.start])
  coincident([line2.end, line3.start])
  coincident([line3.end, line4.start])
  coincident([line4.end, line1.start])
  parallel([line2, line4])
  parallel([line3, line1])
  perpendicular([line1, line2])
  horizontal(line3)
  distance([line4.end, line2.start]) == 1mm
  distance([line2.start, line3.start]) == 1mm
}
hidden001 = hide(rectangleSketch)
region001 = region(segments = [
  rectangleSketch.line4,
  rectangleSketch.line1
])
cube1 = extrude(region001, length = 2mm)
"#;

fn zoo_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_zoo"))
}

fn apply_test_auth_env(cmd: &mut Command) {
    match std::env::var("ZOO_TEST_TOKEN") {
        Ok(token) => {
            cmd.env("ZOO_API_TOKEN", token);
        }
        Err(_) => {
            eprintln!("WARNING: ZOO_TEST_TOKEN is not set; kcl child-process test may fail");
        }
    }

    if let Ok(host) = std::env::var("ZOO_TEST_HOST") {
        if !host.is_empty() {
            cmd.env("ZOO_HOST", host);
        }
    }
}

fn run_zoo(args: &[&str], current_dir: &Path, config_dir: &Path) -> Output {
    let mut cmd = Command::new(zoo_bin());
    cmd.args(args)
        .current_dir(current_dir)
        .env("ZOO_CONFIG_DIR", config_dir);
    apply_test_auth_env(&mut cmd);
    cmd.output()
        .unwrap_or_else(|err| panic!("failed to run zoo with args {args:?}: {err}"))
}

fn assert_success(output: &Output, args: &[&str]) {
    if !output.status.success() {
        panic!(
            "zoo {args:?} failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn kcl_analyze_child_process_returns_expected_json_sections() {
    let project_dir = tempfile::tempdir().expect("create project temp dir");
    let config_dir = tempfile::tempdir().expect("create config temp dir");
    std::fs::write(project_dir.path().join("main.kcl"), ANALYZE_CUBE_KCL).expect("write main.kcl");

    let args = ["kcl", "analyze", "main.kcl", "--format", "json"];
    let output = run_zoo(&args, project_dir.path(), config_dir.path());
    assert_success(&output, &args);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|err| {
        panic!(
            "zoo {args:?} did not write valid JSON\nerror: {err}\nstdout:\n{stdout}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
    });

    for key in [
        "volume",
        "mass",
        "density",
        "surface_area",
        "center_of_mass",
        "bounding_box",
    ] {
        assert!(
            json.get(key).is_some(),
            "zoo {args:?} JSON output missing `{key}`\nstdout:\n{stdout}"
        );
    }
}
