use pretty_assertions::assert_eq;
use test_context::{test_context, AsyncTestContext};

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct TestItem {
    name: String,
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
        login(test_host.clone(), test_token.clone()).await;

        Self {
            test_host,
            test_token,
            client: zoo,
        }
    }

    async fn teardown(self) {}
}

async fn login(test_host: String, test_token: String) {
    tokio::task::spawn_local(async move {
        run_test(TestItem {
            name: "login".to_string(),
            args: vec![
                "zoo".to_string(),
                "auth".to_string(),
                "login".to_string(),
                "--host".to_string(),
                test_host.clone(),
                "--with-token".to_string(),
            ],
            stdin: Some(test_token),
            want_out: "✔ Logged in as ".to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        })
        .await;
    });
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_existing_command(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "existing command".to_string(),
        args: vec!["zoo".to_string(), "completion".to_string()],
        want_out: "complete -F _zoo -o nosort -o bashdefault -o default zoo\n".to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_existing_command_with_args(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "existing command with args".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_add_an_alias(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "add an alias".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_add_a_shell_alias(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "add a shell alias".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_list_our_aliases(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "list our aliases".to_string(),
        args: vec!["zoo".to_string(), "alias".to_string(), "list".to_string()],
        want_out: "\"completion -s zsh\"".to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_call_alias(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "call alias".to_string(),
        args: vec!["zoo".to_string(), "foo".to_string()],
        want_out: "_zoo \"$@\"\n".to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_call_alias_with_different_binary_name(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "call alias with different binary name".to_string(),
        args: vec!["/bin/thing/zoo".to_string(), "foo".to_string()],
        want_out: "_zoo \"$@\"\n".to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_call_shell_alias(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "call shell alias".to_string(),
        args: vec!["zoo".to_string(), "bar".to_string()],
        want_out: "/bash".to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_version(_ctx: &mut MainContext) {
    let version = clap::crate_version!();
    run_test(TestItem {
        name: "version".to_string(),
        args: vec!["zoo".to_string(), "version".to_string()],
        want_out: format!(
            "zoo {} )({})\n{}",
            version,
            git_rev::revision_string!(),
            crate::cmd_version::changelog_url(version)
        ),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_api_user(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "api /user".to_string(),
        args: vec!["zoo".to_string(), "api".to_string(), "/user".to_string()],
        want_out: r#""created_at": ""#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_api_user_no_leading_(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "api user (no leading /)".to_string(),
        args: vec!["zoo".to_string(), "api".to_string(), "user".to_string()],
        want_out: r#""created_at": ""#.to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_api_user_with_header(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "api user with header".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_api_user_with_headers(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "api user with headers".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_api_user_with_output_headers(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "api user with output headers".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_api_endpoint_does_not_exist(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "api endpoint does not exist".to_string(),
        args: vec!["zoo".to_string(), "api".to_string(), "foo/bar".to_string()],
        want_out: "".to_string(),
        want_err: "404 Not Found Not Found".to_string(),
        want_code: 1,
        ..Default::default()
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_try_to_paginate_over_a_post(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "try to paginate over a post".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_get_your_user(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "get your user".to_string(),
        args: vec!["zoo".to_string(), "user".to_string(), "view".to_string()],
        want_out: "name               |".to_string(),
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_get_your_user_as_json(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "get your user as json".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_convert_a_file(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "convert a file".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_get_the_file_volume(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "get the file volume".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_get_the_file_density(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "get the file density".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_get_the_file_mass(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "get the file mass".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_get_the_file_surface_area(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "get the file surface-area".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_get_the_file_center_of_mass(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "get the file center-of-mass".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_get_the_file_mass_as_json(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "get the file mass as json".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_snapshot_a_kcl_file_as_png(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "snapshot a kcl file as png".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_snapshot_a_kcl_file_with_a_project_dot_toml_as_png(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "snapshot a kcl file with a project.toml as png".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_snapshot_a_kcl_file_with_a_nested_project_toml_as_png(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "snapshot a kcl file with a nested project.toml as png".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_snapshot_a_kcl_assembly_as_png(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "snapshot a kcl assembly as png".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_snapshot_a_kcl_assembly_as_png_with_(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "snapshot a kcl assembly as png with .".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_get_the_mass_of_a_kcl_file(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "get the mass of a kcl file".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_get_the_mass_of_a_kcl_file_but_use_project_toml(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "get the mass of a kcl file but use project.toml".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_get_the_mass_of_a_kcl_file_with_nested_dirs_and_a_project_dot_toml(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "get the mass of a kcl file with nested dirs and a project.toml".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_get_the_density_of_a_kcl_file(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "get the density of a kcl file".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_get_the_volume_of_a_kcl_file(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "get the volume of a kcl file".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_get_the_surface_area_of_a_kcl_file(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "get the surface-area of a kcl file".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_get_the_center_of_mass_of_a_kcl_file(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "get the center-of-mass of a kcl file".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_export_a_kcl_file_as_gltf(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "export a kcl file as gltf".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_export_a_kcl_file_as_step_deterministically(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "export a kcl file as step, deterministically".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_export_a_kcl_file_with_a_parse_error(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "export a kcl file with a parse error".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_format_a_kcl_file(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "format a kcl file".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_format_a_directory(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "format a directory".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_lint_some_kcl(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "lint some kcl".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_snapshot_a_gltf_with_embedded_buffer(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "snapshot a gltf with embedded buffer".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_snapshot_a_gltf_with_external_buffer(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "snapshot a gltf with external buffer".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_snapshot_a_text_to_cad_prompt_as_png(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "snapshot a text-to-cad prompt as png".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_export_a_text_to_cad_prompt_as_obj(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "export a text-to-cad prompt as obj".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_export_a_text_to_cad_prompt_as_kcl(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "export a text-to-cad prompt as kcl".to_string(),
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
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_edit_a_kcl_file(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "edit a kcl file".to_string(),
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
        want_out: r#"Wrote to tests/assembly-edit/main.kcl"#.to_string(), // Make sure it keeps
        // the path.
        want_err: "".to_string(),
        want_code: 0,
        ..Default::default()
    })
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn serial_test_view_a_kcl_file_with_multi_file_errors(_ctx: &mut MainContext) {
    run_test(TestItem {
        name: "view a kcl file with multi-file errors".to_string(),
        args: vec![
            "zoo".to_string(),
            "kcl".to_string(),
            "view".to_string(),
            "tests/parse_file_error".to_string(),
        ],
        want_out: r#""#.to_string(),
        want_err: "lksjndflsskjfnak;jfna##
            }
            
   ·"
        .to_string(),
        want_code: 1,
        ..Default::default()
    })
    .await;
}

// Test goes here
async fn run_test(t: TestItem) {
    let mut config = crate::config::new_blank_config().unwrap();
    let mut c = crate::config_from_env::EnvConfig::inherit_env(&mut config);

    let (mut io, stdout_path, stderr_path) = crate::iostreams::IoStreams::test();
    io.set_stdout_tty(false);
    io.set_color_enabled(false);
    if let Some(stdin) = t.stdin {
        io.stdin = Box::new(std::io::Cursor::new(stdin));
    }
    let mut ctx = crate::context::Context {
        config: &mut c,
        io,
        debug: false,
    };

    let old_current_directory = std::env::current_dir().unwrap();
    if let Some(current_directory) = t.current_directory {
        std::env::set_current_dir(&current_directory).unwrap();
    }

    let result = crate::do_main(t.args, &mut ctx).await;

    let stdout = std::fs::read_to_string(stdout_path).unwrap_or_default();
    let stderr = std::fs::read_to_string(stderr_path).unwrap_or_default();

    // Reset the cwd.
    std::env::set_current_dir(old_current_directory).unwrap();

    assert!(
        stdout.contains(&t.want_out),
        "test {} ->\nstdout: {}\nwant: {}\n\nstderr: {}",
        t.name,
        stdout,
        t.want_out,
        stderr,
    );

    match result {
        Ok(code) => {
            assert_eq!(code, t.want_code, "test {}", t.name);
            assert_eq!(stdout.is_empty(), t.want_out.is_empty(), "test {}", t.name);
            assert_eq!(
                stderr.to_string().is_empty(),
                t.want_err.is_empty(),
                "test {} -> stderr: {}\nwant_err: {}",
                t.name,
                stderr,
                t.want_err
            );
            assert!(
                stderr.contains(&t.want_err),
                "test {} ->\nstderr: {}\nwant: {}\n\nstdout: {}",
                t.name,
                stderr,
                t.want_err,
                stdout,
            );
        }
        Err(err) => {
            assert!(!t.want_err.is_empty(), "test {}", t.name);
            assert!(
                err.to_string().contains(&t.want_err),
                "test {} -> err: {}\nwant_err: {}",
                t.name,
                err,
                t.want_err
            );
            assert_eq!(
                err.to_string().is_empty(),
                t.want_err.is_empty(),
                "test {} -> err: {}\nwant_err: {}",
                t.name,
                err,
                t.want_err
            );
            assert!(stderr.is_empty(), "test {}", t.name);
        }
    }
}
