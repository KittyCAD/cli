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

        Self {
            test_host,
            test_token,
            client: zoo,
        }
    }

    async fn teardown(self) {}
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn test_main(ctx: &mut MainContext) {
    let version = clap::crate_version!();

    let mut tests: Vec<TestItem> = vec![
        TestItem {
            name: "existing command".to_string(),
            args: vec!["zoo".to_string(), "completion".to_string()],
            want_out: "complete -F _zoo -o nosort -o bashdefault -o default zoo\n".to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
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
        },
        // ML: text-to-cad export streams reasoning to stderr by default.
        TestItem {
            name: "ml text-to-cad export reasoning on".to_string(),
            args: vec![
                "zoo".to_string(),
                "ml".to_string(),
                "text-to-cad".to_string(),
                "export".to_string(),
                "-t".to_string(),
                "obj".to_string(),
                "--output-dir".to_string(),
                "/tmp".to_string(),
                "A".to_string(),
                "2x4".to_string(),
                "lego".to_string(),
                "brick".to_string(),
            ],
            // Just assert completion appears in stdout table.
            want_out: "Completed".to_string(),
            // Look for explicit reasoning output label in stderr.
            want_err: "reasoning:".to_string(),
            want_code: 0,
            ..Default::default()
        },
        // ML: text-to-cad export does not stream when disabled.
        TestItem {
            name: "ml text-to-cad export no reasoning".to_string(),
            args: vec![
                "zoo".to_string(),
                "ml".to_string(),
                "text-to-cad".to_string(),
                "export".to_string(),
                "-t".to_string(),
                "obj".to_string(),
                "--output-dir".to_string(),
                "/tmp".to_string(),
                "--no-reasoning".to_string(),
                "A".to_string(),
                "2x4".to_string(),
                "lego".to_string(),
                "brick".to_string(),
            ],
            want_out: "Completed".to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        // ML: kcl copilot should only start within a project directory containing main.kcl.
        TestItem {
            name: "ml kcl copilot requires main.kcl".to_string(),
            args: vec![
                "zoo".to_string(),
                "ml".to_string(),
                "kcl".to_string(),
                "copilot".to_string(),
            ],
            // No stdout expected; assert error message substring.
            want_out: "".to_string(),
            want_err: "does not contain a main.kcl file".to_string(),
            want_code: 1,
            ..Default::default()
        },
        TestItem {
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
        },
        TestItem {
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
        },
        TestItem {
            name: "list our aliases".to_string(),
            args: vec!["zoo".to_string(), "alias".to_string(), "list".to_string()],
            want_out: "\"completion -s zsh\"".to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
            name: "call alias".to_string(),
            args: vec!["zoo".to_string(), "foo".to_string()],
            want_out: "_zoo \"$@\"\n".to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
            name: "call alias with different binary name".to_string(),
            args: vec!["/bin/thing/zoo".to_string(), "foo".to_string()],
            want_out: "_zoo \"$@\"\n".to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
            name: "call shell alias".to_string(),
            args: vec!["zoo".to_string(), "bar".to_string()],
            want_out: "/bash".to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
            name: "version".to_string(),
            args: vec!["zoo".to_string(), "version".to_string()],
            want_out: format!(
                "zoo {} ({})\n{}",
                version,
                git_rev::revision_string!(),
                crate::cmd_version::changelog_url(version)
            ),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
            name: "login".to_string(),
            args: vec![
                "zoo".to_string(),
                "--host".to_string(),
                ctx.test_host.clone(),
                "auth".to_string(),
                "login".to_string(),
                "--with-token".to_string(),
            ],
            stdin: Some(ctx.test_token.clone()),
            want_out: "✔ Logged in as ".to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
            name: "api /user".to_string(),
            args: vec!["zoo".to_string(), "api".to_string(), "/user".to_string()],
            want_out: r#""created_at": ""#.to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
            name: "api user (no leading /)".to_string(),
            args: vec!["zoo".to_string(), "api".to_string(), "user".to_string()],
            want_out: r#""created_at": ""#.to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
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
        },
        TestItem {
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
        },
        TestItem {
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
        },
        TestItem {
            name: "api endpoint does not exist".to_string(),
            args: vec!["zoo".to_string(), "api".to_string(), "foo/bar".to_string()],
            want_out: "".to_string(),
            want_err: "404 Not Found Not Found".to_string(),
            want_code: 1,
            ..Default::default()
        },
        TestItem {
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
        },
        TestItem {
            name: "get your user".to_string(),
            args: vec!["zoo".to_string(), "user".to_string(), "view".to_string()],
            want_out: "name               |".to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
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
        },
        TestItem {
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
        },
        TestItem {
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
        },
        TestItem {
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
        },
        TestItem {
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
        },
        TestItem {
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
        },
        TestItem {
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
        },
        TestItem {
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
        },
        TestItem {
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
        },
        TestItem {
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
        },
        TestItem {
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
        },
        TestItem {
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
        },
        TestItem {
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
        },
        TestItem {
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
        },
        TestItem {
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
        },
        TestItem {
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
        },
        TestItem {
            name: "analyze a kcl file as table".to_string(),
            args: vec![
                "zoo".to_string(),
                "kcl".to_string(),
                "analyze".to_string(),
                "tests/gear.kcl".to_string(),
                "--volume-output-unit".to_string(),
                "cm3".to_string(),
                "--mass-output-unit".to_string(),
                "g".to_string(),
                "--surface-area-output-unit".to_string(),
                "cm2".to_string(),
                "--material-density".to_string(),
                "1.0".to_string(),
                "--material-density-unit".to_string(),
                "lb-ft3".to_string(),
            ],
            want_out: r#"center_of_mass"#.to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
            name: "analyze a kcl file as json".to_string(),
            args: vec![
                "zoo".to_string(),
                "kcl".to_string(),
                "analyze".to_string(),
                "tests/gear.kcl".to_string(),
                "--format=json".to_string(),
                "--volume-output-unit".to_string(),
                "cm3".to_string(),
                "--mass-output-unit".to_string(),
                "g".to_string(),
                "--surface-area-output-unit".to_string(),
                "cm2".to_string(),
                "--material-density".to_string(),
                "1.0".to_string(),
                "--material-density-unit".to_string(),
                "lb-ft3".to_string(),
            ],
            want_out: r#""center_of_mass""#.to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
            name: "analyze a kcl file as json with default metric units".to_string(),
            args: vec![
                "zoo".to_string(),
                "kcl".to_string(),
                "analyze".to_string(),
                "tests/gear.kcl".to_string(),
                "--format=json".to_string(),
                "--material-density".to_string(),
                "1.0".to_string(),
            ],
            want_out: r#""output_unit": "kg:m3""#.to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
            name: "analyze a kcl file and use project.toml".to_string(),
            args: vec![
                "zoo".to_string(),
                "kcl".to_string(),
                "analyze".to_string(),
                "tests/with-settings/gear.kcl".to_string(),
                "--format=json".to_string(),
                "--volume-output-unit".to_string(),
                "cm3".to_string(),
                "--mass-output-unit".to_string(),
                "g".to_string(),
                "--surface-area-output-unit".to_string(),
                "cm2".to_string(),
                "--material-density".to_string(),
                "1.0".to_string(),
                "--material-density-unit".to_string(),
                "lb-ft3".to_string(),
            ],
            want_out: r#""mass""#.to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
            name: "analyze a kcl file with invalid density".to_string(),
            args: vec![
                "zoo".to_string(),
                "kcl".to_string(),
                "analyze".to_string(),
                "tests/gear.kcl".to_string(),
                "--volume-output-unit".to_string(),
                "cm3".to_string(),
                "--mass-output-unit".to_string(),
                "g".to_string(),
                "--surface-area-output-unit".to_string(),
                "cm2".to_string(),
                "--material-density".to_string(),
                "0.0".to_string(),
                "--material-density-unit".to_string(),
                "lb-ft3".to_string(),
            ],
            want_out: r#""#.to_string(),
            want_err: r#"`--material-density` must not be 0.0"#.to_string(),
            want_code: 1,
            ..Default::default()
        },
        TestItem {
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
        },
        //      TestItem {
        //          name: "get the volume of a kcl file".to_string(),
        //          args: vec![
        //              "zoo".to_string(),
        //              "kcl".to_string(),
        //              "volume".to_string(),
        //              "tests/gear.kcl".to_string(),
        //              "--output-unit".to_string(),
        //              "cm3".to_string(),
        //          ],
        //          want_out: r#"79173.2958833619"#.to_string(),
        //          want_err: "".to_string(),
        //          want_code: 0,
        //          ..Default::default()
        //      },
        //      TestItem {
        //          name: "get the surface-area of a kcl file".to_string(),
        //          args: vec![
        //              "zoo".to_string(),
        //              "kcl".to_string(),
        //              "surface-area".to_string(),
        //              "tests/gear.kcl".to_string(),
        //              "--output-unit".to_string(),
        //              "cm2".to_string(),
        //          ],
        //          want_out: r#"surface_area | 17351.484299764335"#.to_string(),
        //          want_err: "".to_string(),
        //          want_code: 0,
        //          ..Default::default()
        //      },
        //      TestItem {
        //          name: "get the center-of-mass of a kcl file".to_string(),
        //          args: vec![
        //              "zoo".to_string(),
        //              "kcl".to_string(),
        //              "center-of-mass".to_string(),
        //              "tests/gear.kcl".to_string(),
        //              "--output-unit".to_string(),
        //              "cm".to_string(),
        //          ],
        //          want_out: r#"center_of_mass | (-0.015537803061306477, 7.619970321655273, -0.00008108330803224817)"#
        //              .to_string(),
        //          want_err: "".to_string(),
        //          want_code: 0,
        //          ..Default::default()
        //      },
        //      TestItem {
        //          name: "export a kcl file as gltf".to_string(),
        //          args: vec![
        //              "zoo".to_string(),
        //              "kcl".to_string(),
        //              "export".to_string(),
        //              "--output-format=gltf".to_string(),
        //              "tests/gear.kcl".to_string(),
        //              "tests/".to_string(),
        //          ],
        //          want_out: r#"Wrote file: tests/output.gltf"#.to_string(),
        //          want_err: "".to_string(),
        //          want_code: 0,
        //          ..Default::default()
        //      },
        //      TestItem {
        //          name: "export a kcl file as step, deterministically".to_string(),
        //          args: vec![
        //              "zoo".to_string(),
        //              "kcl".to_string(),
        //              "export".to_string(),
        //              "--output-format=step".to_string(),
        //              "--deterministic".to_string(),
        //              "tests/gear.kcl".to_string(),
        //              "tests/".to_string(),
        //          ],
        //          want_out: r#"Wrote file: tests/output.step"#.to_string(),
        //          want_err: "".to_string(),
        //          want_code: 0,
        //          ..Default::default()
        //      },
        //      TestItem {
        //          name: "export a kcl file with a parse error".to_string(),
        //          args: vec![
        //              "zoo".to_string(),
        //              "kcl".to_string(),
        //              "export".to_string(),
        //              "--output-format=gltf".to_string(),
        //              "tests/parse_error.kcl".to_string(),
        //              "tests/".to_string(),
        //          ],
        //          want_out: r#""#.to_string(),
        //          want_err: "syntax: Unexpected token".to_string(),
        //          want_code: 1,
        //          ..Default::default()
        //      },
        //      TestItem {
        //          name: "format a kcl file".to_string(),
        //          args: vec![
        //              "zoo".to_string(),
        //              "kcl".to_string(),
        //              "fmt".to_string(),
        //              "tests/gear.kcl".to_string(),
        //          ],
        //          want_out: r#"startSketchOn(XY)"#.to_string(),
        //          want_err: "".to_string(),
        //          want_code: 0,
        //          ..Default::default()
        //      },
        //      TestItem {
        //          name: "format a directory".to_string(),
        //          args: vec![
        //              "zoo".to_string(),
        //              "kcl".to_string(),
        //              "fmt".to_string(),
        //              "--write".to_string(),
        //              "tests/walkie-talkie".to_string(),
        //          ],
        //          want_out: r#"Formatted directory `tests/walkie-talkie`"#.to_string(),
        //          want_err: "".to_string(),
        //          want_code: 0,
        //          ..Default::default()
        //      },
        //      TestItem {
        //          name: "lint some kcl".to_string(),
        //          args: vec![
        //              "zoo".to_string(),
        //              "kcl".to_string(),
        //              "lint".to_string(),
        //              "tests/gear.kcl".to_string(),
        //          ],
        //          want_out: r#""#.to_string(),
        //          want_err: "".to_string(),
        //          want_code: 0,
        //          ..Default::default()
        //      },
        //      TestItem {
        //          name: "snapshot a gltf with embedded buffer".to_string(),
        //          args: vec![
        //              "zoo".to_string(),
        //              "file".to_string(),
        //              "snapshot".to_string(),
        //              "tests/output-1.gltf".to_string(),
        //              "tests/output-1.png".to_string(),
        //          ],
        //          want_out: r#"Snapshot saved to `tests/output-1.png`"#.to_string(),
        //          want_err: "".to_string(),
        //          want_code: 0,
        //          ..Default::default()
        //      },
        //      TestItem {
        //          name: "snapshot a gltf with external buffer".to_string(),
        //          args: vec![
        //              "zoo".to_string(),
        //              "file".to_string(),
        //              "snapshot".to_string(),
        //              "tests/output-2.gltf".to_string(),
        //              "tests/output-2.png".to_string(),
        //          ],
        //          want_out: r#"Snapshot saved to `tests/output-2.png`"#.to_string(),
        //          want_err: "".to_string(),
        //          want_code: 0,
        //          ..Default::default()
        //      },
        //      TestItem {
        //          name: "snapshot a text-to-cad prompt as png".to_string(),
        //          args: vec![
        //              "zoo".to_string(),
        //              "ml".to_string(),
        //              "text-to-cad".to_string(),
        //              "snapshot".to_string(),
        //              "--output-dir".to_string(),
        //              "tests/".to_string(),
        //              "a".to_string(),
        //              "2x4".to_string(),
        //              "lego".to_string(),
        //              "brick".to_string(),
        //          ],
        //          want_out: r#"Snapshot saved to `"#.to_string(),
        //          want_err: "".to_string(),
        //          want_code: 0,
        //          ..Default::default()
        //      },
        //      TestItem {
        //          name: "export a text-to-cad prompt as obj".to_string(),
        //          args: vec![
        //              "zoo".to_string(),
        //              "ml".to_string(),
        //              "text-to-cad".to_string(),
        //              "export".to_string(),
        //              "--output-format=obj".to_string(),
        //              "a".to_string(),
        //              "2x4".to_string(),
        //              "lego".to_string(),
        //              "brick".to_string(),
        //          ],
        //          want_out: r#"wrote file "#.to_string(),
        //          want_err: "".to_string(),
        //          want_code: 0,
        //          ..Default::default()
        //      },
        //      TestItem {
        //          name: "export a text-to-cad prompt as kcl".to_string(),
        //          args: vec![
        //              "zoo".to_string(),
        //              "ml".to_string(),
        //              "text-to-cad".to_string(),
        //              "export".to_string(),
        //              "--output-format=kcl".to_string(),
        //              "a".to_string(),
        //              "2x6".to_string(),
        //              "mounting".to_string(),
        //              "plate".to_string(),
        //          ],
        //          want_out: r#"wrote file "#.to_string(),
        //          want_err: "".to_string(),
        //          want_code: 0,
        //          ..Default::default()
        //      },
        //      TestItem {
        //          name: "edit a kcl file".to_string(),
        //          args: vec![
        //              "zoo".to_string(),
        //              "ml".to_string(), "kcl".to_string(),
        //              "edit".to_string(),
        //              "tests/assembly-edit".to_string(),
        //              "make".to_string(),
        //              "it".to_string(),
        //              "blue".to_string(),
        //          ],
        //          want_out: r#"Wrote to tests/assembly-edit/main.kcl"#.to_string(), // Make sure it keeps
        //          // the path.
        //          want_err: "".to_string(),
        //          want_code: 0,
        //          ..Default::default()
        //      },
        //      TestItem {
        //          name: "view a kcl file with multi-file errors".to_string(),
        //          args: vec![
        //              "zoo".to_string(),
        //              "kcl".to_string(),
        //              "view".to_string(),
        //              "tests/parse_file_error".to_string(),
        //          ],
        //          want_out: r#""#.to_string(),
        //          want_err: "lksjndflsskjfnak;jfna##
        // ·"
        //          .to_string(),
        //          want_code: 1,
        //          ..Default::default()
        //      },
    ];

    // Add e2e tests for `ml kcl edit` using a temp project to avoid modifying repo files.
    let mut temp_projects: Vec<tempfile::TempDir> = Vec::new();
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let tmp_path = tmp.path().to_path_buf();
    std::fs::copy("tests/gear.kcl", tmp_path.join("gear.kcl")).expect("copy gear.kcl");
    // Hold the dir open for the duration of the test run.
    temp_projects.push(tmp);

    // Temp project for multi-file edit: root main.kcl and subdir/main.kcl.
    let tmp_multi = tempfile::tempdir().expect("failed to create temp dir");
    let tmp_multi_path = tmp_multi.path().to_path_buf();
    std::fs::create_dir_all(tmp_multi_path.join("subdir")).expect("create subdir");
    std::fs::write(tmp_multi_path.join("main.kcl"), "// Glorious cube\n\nsideLength = 10\n").expect("write main.kcl");
    std::fs::write(
        tmp_multi_path.join("subdir/main.kcl"),
        "// Glorious cylinder\n\nheight = 20\n",
    )
    .expect("write subdir/main.kcl");
    temp_projects.push(tmp_multi);

    // Temp project exceeding the Copilot size limit (> 25 entries).
    let tmp_large = tempfile::tempdir().expect("failed to create temp dir");
    let tmp_large_path = tmp_large.path().to_path_buf();
    std::fs::write(tmp_large_path.join("main.kcl"), "cube(1)\n").expect("write main.kcl");
    for idx in 0..26 {
        std::fs::write(tmp_large_path.join(format!("extra-{idx}.kcl")), "cube(1)\n").expect("write extra file");
    }
    temp_projects.push(tmp_large);

    tests.push(TestItem {
        name: "ml kcl edit reasoning on".to_string(),
        args: vec![
            "zoo".to_string(),
            "ml".to_string(),
            "kcl".to_string(),
            "edit".to_string(),
            "gear.kcl".to_string(),
            "Make".to_string(),
            "it".to_string(),
            "blue".to_string(),
        ],
        // Do not match on the "Wrote to" phrase; match on filename presence.
        want_out: "gear.kcl".to_string(),
        // Look for explicit reasoning output label in stderr.
        want_err: "reasoning:".to_string(),
        want_code: 0,
        current_directory: Some(tmp_path.clone()),
        ..Default::default()
    });

    tests.push(TestItem {
        name: "ml kcl edit no reasoning".to_string(),
        args: vec![
            "zoo".to_string(),
            "ml".to_string(),
            "kcl".to_string(),
            "edit".to_string(),
            "--no-reasoning".to_string(),
            "gear.kcl".to_string(),
            "Make".to_string(),
            "it".to_string(),
            "blue".to_string(),
        ],
        // Do not match on the "Wrote to" phrase; match on filename presence.
        want_out: "gear.kcl".to_string(),
        want_err: "".to_string(),
        want_code: 0,
        current_directory: Some(tmp_path.clone()),
        ..Default::default()
    });

    // Multi-file project: ensure both root and subdir files are edited.
    tests.push(TestItem {
        name: "ml kcl edit multi-file (root)".to_string(),
        args: vec![
            "zoo".to_string(),
            "ml".to_string(),
            "kcl".to_string(),
            "edit".to_string(),
            "--no-reasoning".to_string(),
            ".".to_string(),
            "Add".to_string(),
            "a".to_string(),
            "simple".to_string(),
            "cube".to_string(),
            "to".to_string(),
            "main.kcl".to_string(),
            "and".to_string(),
            "a".to_string(),
            "cylinder".to_string(),
            "to".to_string(),
            "subdir/main.kcl".to_string(),
        ],
        // Only assert presence of the root file path in stdout.
        want_out: "main.kcl".to_string(),
        want_err: "".to_string(),
        want_code: 0,
        current_directory: Some(tmp_multi_path.clone()),
        ..Default::default()
    });
    tests.push(TestItem {
        name: "ml kcl edit multi-file (subdir)".to_string(),
        args: vec![
            "zoo".to_string(),
            "ml".to_string(),
            "kcl".to_string(),
            "edit".to_string(),
            "--no-reasoning".to_string(),
            ".".to_string(),
            "Add".to_string(),
            "a".to_string(),
            "simple".to_string(),
            "cube".to_string(),
            "to".to_string(),
            "main.kcl".to_string(),
            "and".to_string(),
            "a".to_string(),
            "cylinder".to_string(),
            "to".to_string(),
            "subdir/main.kcl".to_string(),
        ],
        // Only assert presence of the subdir file path in stdout.
        want_out: "subdir/main.kcl".to_string(),
        want_err: "".to_string(),
        want_code: 0,
        current_directory: Some(tmp_multi_path.clone()),
        ..Default::default()
    });

    tests.push(TestItem {
        name: "ml kcl copilot rejects large project".to_string(),
        args: vec![
            "zoo".to_string(),
            "ml".to_string(),
            "kcl".to_string(),
            "copilot".to_string(),
        ],
        want_out: "".to_string(),
        want_err: "Copilot needs a smaller project".to_string(),
        want_code: 1,
        current_directory: Some(tmp_large_path.clone()),
        ..Default::default()
    });

    let mut config = crate::config::new_blank_config().unwrap();
    let mut c = crate::config_from_env::EnvConfig::inherit_env(&mut config);

    struct TestError {
        name: String,
        reason: String,
    }

    let mut test_errors: Vec<TestError> = Vec::new();
    let num_tests = tests.len();
    let mut num_tests_run = 0;
    let test_filter = std::env::var("ZOO_CLI_TEST_NAME").ok();
    for t in tests {
        if let Some(filter) = &test_filter {
            if !t.name.contains(filter) {
                continue;
            }
        }
        num_tests_run += 1;
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
            override_host: None,
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

        if !stdout.contains(&t.want_out) {
            test_errors.push(TestError {
                name: t.name.clone(),
                reason: format!(
                    "Actual stdout: {}\nExpected stdout: {}\nActual stderror: {}",
                    stdout, t.want_out, stderr
                ),
            });
            continue;
        };

        match result {
            Ok(code) => {
                // assert_eq!(code, t.want_code, "test {}", t.name);
                if code != t.want_code {
                    test_errors.push(TestError {
                        name: t.name,
                        reason: format!("Actual error code: {code}\nExpected error code: {}", t.want_code),
                    });
                    continue;
                }
                if stdout.is_empty() != t.want_out.is_empty() {
                    test_errors.push(TestError {
                        name: t.name,
                        reason: format!("Actual stdout: {}\nExpected stdout: {}", stdout, t.want_out),
                    });
                    continue;
                }
                if stderr.to_string().is_empty() != t.want_err.is_empty() {
                    test_errors.push(TestError {
                        name: t.name,
                        reason: format!("Actual stderr: {}\nExpected stderr: {}", stderr, t.want_err),
                    });
                    continue;
                }
                if !stderr.contains(&t.want_err) {
                    test_errors.push(TestError {
                        name: t.name,
                        reason: format!(
                            "Actual stderr: {}\nExpected stderr to contain: {}\nActual stdout: {}",
                            stderr, t.want_err, stdout
                        ),
                    });
                    continue;
                }
            }
            Err(err) => {
                if t.want_err.is_empty() {
                    test_errors.push(TestError {
                        name: t.name,
                        reason: format!("Actual error: {err}\nDid not expect any error"),
                    });
                    continue;
                }
                if !err.to_string().contains(&t.want_err) {
                    test_errors.push(TestError {
                        name: t.name,
                        reason: format!("Actual error: {}\nExpected error to contain: {}", err, t.want_err),
                    });
                    continue;
                }
                if err.to_string().is_empty() != t.want_err.is_empty() {
                    test_errors.push(TestError {
                        name: t.name,
                        reason: format!("Actual error: {}\nExpected error to contain: {}", err, t.want_err),
                    });
                    continue;
                }
                if !stderr.is_empty() {
                    test_errors.push(TestError {
                        name: t.name,
                        reason: format!("Stderr should have been empty, but it was {stderr}"),
                    });
                    continue;
                }
            }
        }
    }

    let failed = test_errors.len();
    let passed = num_tests_run - failed;
    let skipped = num_tests - num_tests_run;
    assert_eq!(num_tests, failed + passed + skipped);
    eprintln!("Failed {failed} tests, passed {passed}, skipped {skipped}");
    for test_error in test_errors {
        eprintln!("===");
        eprintln!("Test '{}' failed:\n{}", test_error.name, test_error.reason);
    }
    eprintln!("Failed {failed} tests, passed {passed}, skipped {skipped}");
}
