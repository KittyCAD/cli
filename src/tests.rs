use pretty_assertions::assert_eq;
use test_context::{test_context, AsyncTestContext};

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct TestItem {
    args: Vec<String>,
    stdin: Option<String>,
    want_out: String,
    want_err: String,
    want_code: i32,
    current_directory: Option<std::path::PathBuf>,
}

struct MainContext {
    test_host: String,
    test_token: String,
    #[allow(dead_code)]
    client: kittycad::Client,
}

#[async_trait::async_trait]
impl AsyncTestContext for MainContext {
    async fn setup() -> Self {
        let test_host = std::env::var("ZOO_TEST_HOST").unwrap_or_default();
        let test_host = crate::cmd_auth::parse_host(&test_host)
            .expect("invalid ZOO_TEST_HOST")
            .to_string();
        let test_token = std::env::var("ZOO_TEST_TOKEN").expect("ZOO_TEST_TOKEN is required");

        let mut zoo = kittycad::Client::new(&test_token);
        if !test_host.is_empty() {
            zoo.set_base_url(&test_host);
        }

        Self {
            test_host,
            test_token,
            client: zoo,
        }
    }

    async fn teardown(self) {}
}

async fn run_test(ctx: &mut MainContext, test: TestItem) {
    // Check if this is the login test - if not, ensure we're logged in first
    if !test.args.contains(&"login".to_string()) {
        let login_test = TestItem {
            args: vec![
                "zoo".to_string(),
                "auth".to_string(),
                "login".to_string(),
                "--host".to_string(),
                ctx.test_host.clone(),
                "--with-token".to_string(),
            ],
            stdin: Some(ctx.test_token.clone()),
            want_out: "✔ Logged in as ".to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        };
        
        // Run login test first
        let (mut login_io, login_stdout_path, login_stderr_path) = crate::iostreams::IoStreams::test();
        login_io.set_stdout_tty(false);
        login_io.set_color_enabled(false);
        login_io.stdin = Box::new(std::io::Cursor::new(ctx.test_token.clone()));
        let mut login_config = crate::config::new_blank_config().unwrap();
        let mut login_c = crate::config_from_env::EnvConfig::inherit_env(&mut login_config);
        let mut login_ctx = crate::context::Context {
            config: &mut login_c,
            io: login_io,
            debug: false,
        };
        
        let login_result = crate::do_main(login_test.args, &mut login_ctx).await;
        let login_stdout = std::fs::read_to_string(login_stdout_path).unwrap_or_default();
        let login_stderr = std::fs::read_to_string(login_stderr_path).unwrap_or_default();
        
        // Verify login succeeded
        if login_result.is_err() || !login_stdout.contains(&login_test.want_out) {
            panic!("Login failed before test execution: result={:?}, stdout={}, stderr={}", login_result, login_stdout, login_stderr);
        }
    }

    let (mut io, stdout_path, stderr_path) = crate::iostreams::IoStreams::test();
    io.set_stdout_tty(false);
    io.set_color_enabled(false);
    if let Some(stdin) = test.stdin {
        io.stdin = Box::new(std::io::Cursor::new(stdin));
    }
    let mut config = crate::config::new_blank_config().unwrap();
    let mut c = crate::config_from_env::EnvConfig::inherit_env(&mut config);
    let mut test_ctx = crate::context::Context {
        config: &mut c,
        io,
        debug: false,
    };

    let old_current_directory = std::env::current_dir().unwrap();
    if let Some(current_directory) = test.current_directory {
        std::env::set_current_dir(&current_directory).unwrap();
    }

    let result = crate::do_main(test.args, &mut test_ctx).await;

    let stdout = std::fs::read_to_string(stdout_path).unwrap_or_default();
    let stderr = std::fs::read_to_string(stderr_path).unwrap_or_default();

    // Reset the cwd.
    std::env::set_current_dir(old_current_directory).unwrap();

    assert!(
        stdout.contains(&test.want_out),
        "stdout mismatch\nActual stdout: {stdout}\nExpected stdout: {}\nActual stderr: {stderr}",
        test.want_out
    );

    match result {
        Ok(code) => {
            assert_eq!(code, test.want_code);
            assert_eq!(
                stdout.is_empty(),
                test.want_out.is_empty(),
                "stdout emptiness mismatch\nActual stdout: {stdout}\nExpected stdout: {}",
                test.want_out
            );
            assert_eq!(
                stderr.is_empty(),
                test.want_err.is_empty(),
                "stderr emptiness mismatch\nActual stderr: {stderr}\nExpected stderr: {}",
                test.want_err
            );
            assert!(
                stderr.contains(&test.want_err),
                r#"stderr content mismatch
Actual stderr: {stderr}
Expected stderr to contain: {}
Actual stdout: {stdout}"#,
                test.want_err
            );
        }
        Err(err) => {
            assert!(
                !test.want_err.is_empty(),
                "unexpected error\nActual error: {err}\nDid not expect any error"
            );
            assert!(
                err.to_string().contains(&test.want_err),
                "error content mismatch\nActual error: {err}\nExpected error to contain: {}",
                test.want_err
            );
            assert!(stderr.is_empty(), "stderr should have been empty, but it was {stderr}");
        }
    }
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_existing_command(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec!["zoo".to_string(), "completion".to_string()],
        want_out: "complete -F _zoo -o nosort -o bashdefault -o default zoo\n".to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_existing_command_with_args(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "completion".to_string(),
            "-s".to_string(),
            "zsh".to_string(),
        ],
        want_out: "_zoo \"$@\"\n".to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_add_an_alias(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "alias".to_string(),
            "set".to_string(),
            "foo".to_string(),
            "completion -s zsh".to_string(),
        ],
        want_out: "- Adding alias for foo: completion -s zsh\n✔ Added alias.".to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;

    // list our aliases
    let test = TestItem {
        args: vec!["zoo".to_string(), "alias".to_string(), "list".to_string()],
        want_out: "\"completion -s zsh\"".to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;

    // call our alias
    let test = TestItem {
        args: vec!["zoo".to_string(), "foo".to_string()],
        want_out: "_zoo \"$@\"\n".to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;

    // call alias with different binary name
    let test = TestItem {
        args: vec!["/bin/thing/zoo".to_string(), "foo".to_string()],
        want_out: "_zoo \"$@\"\n".to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_add_a_shell_alias(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "alias".to_string(),
            "set".to_string(),
            "-s".to_string(),
            "bar".to_string(),
            "which bash".to_string(),
        ],
        want_out: "- Adding alias for bar: !which bash\n✔ Added alias.".to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;

    // call alias
    /*let test = TestItem {
        args: vec!["zoo".to_string(), "bar".to_string()],
        want_out: "/bash".to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;*/
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_login(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "auth".to_string(),
            "login".to_string(),
            "--host".to_string(),
            ctx.test_host.clone(),
            "--with-token".to_string(),
        ],
        stdin: Some(ctx.test_token.clone()),
        want_out: "✔ Logged in as ".to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_api_user(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec!["zoo".to_string(), "api".to_string(), "/user".to_string()],
        want_out: r#""created_at": ""#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_api_user_no_leading_slash(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec!["zoo".to_string(), "api".to_string(), "user".to_string()],
        want_out: r#""created_at": ""#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_api_user_with_header(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "api".to_string(),
            "user".to_string(),
            "-H".to_string(),
            "Origin: https://example.com".to_string(),
        ],
        want_out: r#""created_at": ""#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_api_user_with_headers(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "api".to_string(),
            "user".to_string(),
            "-H".to_string(),
            "Origin: https://example.com".to_string(),
            "-H".to_string(),
            "Another: thing".to_string(),
        ],
        want_out: r#""created_at": ""#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_api_user_with_output_headers(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "api".to_string(),
            "user".to_string(),
            "--include".to_string(),
        ],
        want_out: r#"HTTP/2.0 200 OK"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_api_endpoint_does_not_exist(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec!["zoo".to_string(), "api".to_string(), "foo/bar".to_string()],
        want_out: "".to_string(),
        want_err: "404 Not Found Not Found".to_string(),
        want_code: 1,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_try_to_paginate_over_a_post(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "api".to_string(),
            "organizations".to_string(),
            "--method".to_string(),
            "POST".to_string(),
            "--paginate".to_string(),
        ],
        want_out: "".to_string(),
        want_err: "the `--paginate` option is not supported for non-GET request".to_string(),
        want_code: 1,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_get_your_user(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec!["zoo".to_string(), "user".to_string(), "view".to_string()],
        want_out: "name               |".to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_get_your_user_as_json(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "user".to_string(),
            "view".to_string(),
            "--format=json".to_string(),
        ],
        want_out: r#""created_at": ""#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_convert_a_file(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "file".to_string(),
            "convert".to_string(),
            "assets/in_obj.obj".to_string(),
            "/tmp/".to_string(),
            "--output-format".to_string(),
            "stl".to_string(),
        ],
        want_out: r#"status                | Completed"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_get_the_file_volume(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "file".to_string(),
            "volume".to_string(),
            "assets/in_obj.obj".to_string(),
            "--output-unit".to_string(),
            "cm3".to_string(),
        ],
        want_out: r#"volume       | 0.05360"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_get_the_file_density(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "file".to_string(),
            "density".to_string(),
            "assets/in_obj.obj".to_string(),
            "--output-unit".to_string(),
            "lb-ft3".to_string(),
            "--material-mass-unit".to_string(),
            "g".to_string(),
            "--material-mass".to_string(),
            "1.0".to_string(),
        ],
        want_out: r#"density            | 1164.67"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_get_the_file_mass(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "file".to_string(),
            "mass".to_string(),
            "assets/in_obj.obj".to_string(),
            "--output-unit".to_string(),
            "g".to_string(),
            "--material-density".to_string(),
            "1.0".to_string(),
            "--material-density-unit".to_string(),
            "lb-ft3".to_string(),
        ],
        want_out: r#"mass                  | 0.00085"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_get_the_file_surface_area(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "file".to_string(),
            "surface-area".to_string(),
            "assets/in_obj.obj".to_string(),
            "--output-unit".to_string(),
            "cm2".to_string(),
        ],
        want_out: r#"surface_area | 1.088"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_get_the_file_center_of_mass(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "file".to_string(),
            "center-of-mass".to_string(),
            "assets/in_obj.obj".to_string(),
            "--output-unit".to_string(),
            "cm".to_string(),
        ],
        want_out: r#"center_of_mass | Point3D { x: -0.0133"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_get_the_file_mass_as_json(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "file".to_string(),
            "mass".to_string(),
            "assets/in_obj.obj".to_string(),
            "--format=json".to_string(),
            "--output-unit".to_string(),
            "g".to_string(),
            "--material-density".to_string(),
            "1.0".to_string(),
            "--material-density-unit".to_string(),
            "lb-ft3".to_string(),
        ],
        want_out: r#""mass": 0.000858"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_snapshot_a_kcl_file_as_png(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "kcl".to_string(),
            "snapshot".to_string(),
            "tests/gear.kcl".to_string(),
            "tests/gear.png".to_string(),
        ],
        want_out: r#"Snapshot saved to `tests/gear.png`"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_snapshot_a_kcl_file_with_a_project_toml_as_png(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "kcl".to_string(),
            "snapshot".to_string(),
            "tests/with-settings/gear.kcl".to_string(),
            "tests/with-settings/gear.png".to_string(),
        ],
        want_out: r#"Snapshot saved to `tests/with-settings/gear.png`"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_snapshot_a_kcl_file_with_a_nested_project_toml_as_png(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "kcl".to_string(),
            "snapshot".to_string(),
            "tests/nested-settings/subdir/gear.kcl".to_string(),
            "tests/nested-settings/subdir/gear.png".to_string(),
        ],
        want_out: r#"Snapshot saved to `tests/nested-settings/subdir/gear.png`"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_snapshot_a_kcl_assembly_as_png(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "kcl".to_string(),
            "snapshot".to_string(),
            "tests/walkie-talkie".to_string(),
            "tests/walkie-talkie.png".to_string(),
        ],
        want_out: r#"Snapshot saved to `tests/walkie-talkie.png`"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_snapshot_a_kcl_assembly_as_png_with_dot(ctx: &mut MainContext) {
    let test = TestItem {
        current_directory: Some(std::env::current_dir().unwrap().join("tests/walkie-talkie")),
        args: vec![
            "zoo".to_string(),
            "kcl".to_string(),
            "snapshot".to_string(),
            ".".to_string(),
            "walkie-talkie.png".to_string(),
        ],
        want_out: r#"Snapshot saved to `walkie-talkie.png`"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_get_the_mass_of_a_kcl_file(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "kcl".to_string(),
            "mass".to_string(),
            "tests/gear.kcl".to_string(),
            "--format=json".to_string(),
            "--output-unit".to_string(),
            "g".to_string(),
            "--material-density".to_string(),
            "1.0".to_string(),
            "--material-density-unit".to_string(),
            "lb-ft3".to_string(),
        ],
        want_out: r#"1268.234"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_get_the_mass_of_a_kcl_file_but_use_project_toml(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "kcl".to_string(),
            "mass".to_string(),
            "tests/with-settings/gear.kcl".to_string(),
            "--format=json".to_string(),
            "--output-unit".to_string(),
            "g".to_string(),
            "--material-density".to_string(),
            "1.0".to_string(),
            "--material-density-unit".to_string(),
            "lb-ft3".to_string(),
        ],
        want_out: r#"74.053"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_get_the_mass_of_a_kcl_file_with_nested_dirs_and_a_project_toml(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "kcl".to_string(),
            "mass".to_string(),
            "tests/nested-settings/subdir/gear.kcl".to_string(),
            "--format=json".to_string(),
            "--output-unit".to_string(),
            "g".to_string(),
            "--material-density".to_string(),
            "1.0".to_string(),
            "--material-density-unit".to_string(),
            "lb-ft3".to_string(),
        ],
        want_out: r#"74.053"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_get_the_density_of_a_kcl_file(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "kcl".to_string(),
            "density".to_string(),
            "tests/gear.kcl".to_string(),
            "--output-unit".to_string(),
            "lb-ft3".to_string(),
            "--material-mass-unit".to_string(),
            "g".to_string(),
            "--material-mass".to_string(),
            "1.0".to_string(),
        ],
        want_out: r#"0.0007"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_get_the_volume_of_a_kcl_file(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "kcl".to_string(),
            "volume".to_string(),
            "tests/gear.kcl".to_string(),
            "--output-unit".to_string(),
            "cm3".to_string(),
        ],
        want_out: r#"79173.2958833619"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_get_the_surface_area_of_a_kcl_file(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "kcl".to_string(),
            "surface-area".to_string(),
            "tests/gear.kcl".to_string(),
            "--output-unit".to_string(),
            "cm2".to_string(),
        ],
        want_out: r#"surface_area | 17351.484299764335"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_get_the_center_of_mass_of_a_kcl_file(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "kcl".to_string(),
            "center-of-mass".to_string(),
            "tests/gear.kcl".to_string(),
            "--output-unit".to_string(),
            "cm".to_string(),
        ],
        want_out: r#"center_of_mass | (-0.015537803061306477, 7.619970321655273, -0.00008108330803224817)"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_export_a_kcl_file_as_gltf(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "kcl".to_string(),
            "export".to_string(),
            "--output-format=gltf".to_string(),
            "tests/gear.kcl".to_string(),
            "tests/".to_string(),
        ],
        want_out: r#"Wrote file: tests/output.gltf"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_export_a_kcl_file_as_step_deterministically(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "kcl".to_string(),
            "export".to_string(),
            "--output-format=step".to_string(),
            "--deterministic".to_string(),
            "tests/gear.kcl".to_string(),
            "tests/".to_string(),
        ],
        want_out: r#"Wrote file: tests/output.step"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_export_a_kcl_file_with_a_parse_error(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "kcl".to_string(),
            "export".to_string(),
            "--output-format=gltf".to_string(),
            "tests/parse_error.kcl".to_string(),
            "tests/".to_string(),
        ],
        want_out: r#""#.to_string(),
        want_err: "syntax: Unexpected token".to_string(),
        want_code: 1,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_format_a_kcl_file(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "kcl".to_string(),
            "fmt".to_string(),
            "tests/gear.kcl".to_string(),
        ],
        want_out: r#"startSketchOn(XY)"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_format_a_directory(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "kcl".to_string(),
            "fmt".to_string(),
            "--write".to_string(),
            "tests/walkie-talkie".to_string(),
        ],
        want_out: r#"Formatted directory `tests/walkie-talkie`"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_lint_some_kcl(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "kcl".to_string(),
            "lint".to_string(),
            "tests/gear.kcl".to_string(),
        ],
        want_out: r#""#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_snapshot_a_gltf_with_embedded_buffer(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "file".to_string(),
            "snapshot".to_string(),
            "tests/output-1.gltf".to_string(),
            "tests/output-1.png".to_string(),
        ],
        want_out: r#"Snapshot saved to `tests/output-1.png`"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_snapshot_a_gltf_with_external_buffer(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "file".to_string(),
            "snapshot".to_string(),
            "tests/output-2.gltf".to_string(),
            "tests/output-2.png".to_string(),
        ],
        want_out: r#"Snapshot saved to `tests/output-2.png`"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_snapshot_a_text_to_cad_prompt_as_png(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "ml".to_string(),
            "text-to-cad".to_string(),
            "snapshot".to_string(),
            "--output-dir".to_string(),
            "tests/".to_string(),
            "a".to_string(),
            "2x4".to_string(),
            "lego".to_string(),
            "brick".to_string(),
        ],
        want_out: r#"Snapshot saved to `"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_export_a_text_to_cad_prompt_as_obj(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "ml".to_string(),
            "text-to-cad".to_string(),
            "export".to_string(),
            "--output-format=obj".to_string(),
            "a".to_string(),
            "2x4".to_string(),
            "lego".to_string(),
            "brick".to_string(),
        ],
        want_out: r#"wrote file "#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_export_a_text_to_cad_prompt_as_kcl(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "ml".to_string(),
            "text-to-cad".to_string(),
            "export".to_string(),
            "--output-format=kcl".to_string(),
            "a".to_string(),
            "2x6".to_string(),
            "mounting".to_string(),
            "plate".to_string(),
        ],
        want_out: r#"wrote file "#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_edit_a_kcl_file(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "ml".to_string(),
            "kcl".to_string(),
            "edit".to_string(),
            "tests/assembly-edit".to_string(),
            "make".to_string(),
            "it".to_string(),
            "blue".to_string(),
        ],
        want_out: r#"Wrote to tests/assembly-edit/main.kcl"#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    };
    run_test(ctx, test).await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_view_a_kcl_file_with_multi_file_errors(ctx: &mut MainContext) {
    let test = TestItem {
        args: vec![
            "zoo".to_string(),
            "kcl".to_string(),
            "view".to_string(),
            "tests/parse_file_error".to_string(),
        ],
        want_out: r#""#.to_string(),
        want_err: "lksjndflsskjfnak;jfna##\n        // ·".to_string(),
        want_code: 1,
        ..Default::default()
    };
    run_test(ctx, test).await;
}
