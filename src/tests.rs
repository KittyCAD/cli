use std::path::{Path, PathBuf};

use anyhow::Result;
use test_context::{test_context, AsyncTestContext};

use crate::config::Config;

type SetupFn = fn(&mut TestConfig, &MainContext) -> Result<()>;

macro_rules! svec {
    ($($item:expr),* $(,)?) => {
        vec![$($item.to_string()),*]
    };
}

macro_rules! cli_tests {
    ($($name:ident($ctx:ident) => $body:block)+) => {
        $(
            #[test_context(MainContext)]
            #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
            #[serial_test::serial]
            async fn $name($ctx: &mut MainContext) {
                let test = $body;
                run_test_item($ctx, test).await;
            }
        )+
    };
}

struct TestItem {
    name: &'static str,
    args: Vec<String>,
    stdin: Option<String>,
    want_out: String,
    want_err: String,
    want_code: i32,
    current_directory: Option<PathBuf>,
    setup: Option<SetupFn>,
}

impl TestItem {
    fn new(name: &'static str, args: Vec<String>) -> Self {
        Self {
            name,
            args,
            stdin: None,
            want_out: String::new(),
            want_err: String::new(),
            want_code: 0,
            current_directory: None,
            setup: None,
        }
    }

    fn stdin(mut self, stdin: impl Into<String>) -> Self {
        self.stdin = Some(stdin.into());
        self
    }

    fn stdout_contains(mut self, want_out: impl Into<String>) -> Self {
        self.want_out = want_out.into();
        self
    }

    fn stderr_contains(mut self, want_err: impl Into<String>) -> Self {
        self.want_err = want_err.into();
        self
    }

    fn exit_code(mut self, want_code: i32) -> Self {
        self.want_code = want_code;
        self
    }

    fn current_directory(mut self, current_directory: impl Into<PathBuf>) -> Self {
        self.current_directory = Some(current_directory.into());
        self
    }

    fn setup(mut self, setup: SetupFn) -> Self {
        self.setup = Some(setup);
        self
    }
}

struct MainContext {
    test_host: String,
    test_token: String,
}

impl AsyncTestContext for MainContext {
    async fn setup() -> Self {
        let test_host = std::env::var("ZOO_TEST_HOST").unwrap_or_default();
        let test_host = crate::cmd_auth::parse_host(&test_host)
            .expect("invalid ZOO_TEST_HOST")
            .to_string();
        let test_token = std::env::var("ZOO_TEST_TOKEN").expect("ZOO_TEST_TOKEN is required");

        Self { test_host, test_token }
    }

    async fn teardown(self) {}
}

#[derive(Debug)]
struct TestConfig {
    inner: crate::config_from_file::FileConfig,
}

impl TestConfig {
    fn new() -> Result<Self> {
        let root = crate::config::new_blank_root()?;
        Ok(Self {
            inner: crate::config_from_file::FileConfig {
                map: crate::config_map::ConfigMap {
                    root: root.as_table().clone(),
                },
            },
        })
    }

    fn aliases_table(&self) -> Result<toml_edit::Table> {
        match self.inner.map.find_entry("aliases") {
            Ok(aliases) => match aliases.as_table() {
                Some(table) => Ok(table.clone()),
                None => anyhow::bail!("aliases is not a table"),
            },
            Err(err) => {
                if err.to_string().contains("not found") {
                    Ok(toml_edit::Table::new())
                } else {
                    anyhow::bail!("Error reading aliases table: {err}")
                }
            }
        }
    }
}

impl crate::config::Config for TestConfig {
    fn get(&self, hostname: &str, key: &str) -> Result<String> {
        self.inner.get(hostname, key)
    }

    fn get_with_source(&self, hostname: &str, key: &str) -> Result<(String, String)> {
        self.inner.get_with_source(hostname, key)
    }

    fn set(&mut self, hostname: &str, key: &str, value: Option<&str>) -> Result<()> {
        self.inner.set(hostname, key, value)
    }

    fn unset_host(&mut self, key: &str) -> Result<()> {
        self.inner.unset_host(key)
    }

    fn hosts(&self) -> Result<Vec<String>> {
        self.inner.hosts()
    }

    fn default_host(&self) -> Result<String> {
        self.inner.default_host()
    }

    fn default_host_with_source(&self) -> Result<(String, String)> {
        self.inner.default_host_with_source()
    }

    fn aliases(&mut self) -> Result<crate::config_alias::AliasConfig<'_>> {
        let aliases_table = self.aliases_table()?;

        Ok(crate::config_alias::AliasConfig {
            map: crate::config_map::ConfigMap { root: aliases_table },
            parent: self,
        })
    }

    fn save_aliases(&mut self, aliases: &crate::config_map::ConfigMap) -> Result<()> {
        self.inner.save_aliases(aliases)
    }

    fn expand_alias(&mut self, args: Vec<String>) -> Result<(Vec<String>, bool)> {
        self.inner.expand_alias(args)
    }

    fn check_writable(&self, hostname: &str, key: &str) -> Result<()> {
        self.inner.check_writable(hostname, key)
    }

    fn write(&self) -> Result<()> {
        Ok(())
    }

    fn config_to_string(&self) -> Result<String> {
        self.inner.config_to_string()
    }

    fn hosts_to_string(&self) -> Result<String> {
        self.inner.hosts_to_string()
    }
}

struct CurrentDirGuard {
    original_directory: PathBuf,
}

impl CurrentDirGuard {
    fn change_to(path: Option<&Path>) -> Result<Self> {
        let original_directory = std::env::current_dir()?;
        if let Some(path) = path {
            std::env::set_current_dir(path)?;
        }

        Ok(Self { original_directory })
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.original_directory);
    }
}

fn setup_authenticated(config: &mut TestConfig, ctx: &MainContext) -> Result<()> {
    config.set(&ctx.test_host, "token", Some(&ctx.test_token))?;
    config.set(&ctx.test_host, "default", Some("true"))?;
    Ok(())
}

fn setup_alias_completion(config: &mut TestConfig, _ctx: &MainContext) -> Result<()> {
    let mut aliases = config.aliases()?;
    aliases.add("foo", "completion -s zsh")?;
    Ok(())
}

fn setup_alias_shell(config: &mut TestConfig, _ctx: &MainContext) -> Result<()> {
    let mut aliases = config.aliases()?;
    aliases.add("bar", "!which bash")?;
    Ok(())
}

fn setup_aliases(config: &mut TestConfig, ctx: &MainContext) -> Result<()> {
    setup_alias_completion(config, ctx)?;
    setup_alias_shell(config, ctx)
}

fn make_single_file_edit_project() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::copy("tests/gear.kcl", tmp.path().join("gear.kcl")).expect("copy gear.kcl");
    tmp
}

fn make_multi_file_edit_project() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::create_dir_all(tmp.path().join("subdir")).expect("create subdir");
    std::fs::write(tmp.path().join("main.kcl"), "// Glorious cube\n\nsideLength = 10\n").expect("write main.kcl");
    std::fs::write(
        tmp.path().join("subdir/main.kcl"),
        "// Glorious cylinder\n\nheight = 20\n",
    )
    .expect("write subdir/main.kcl");
    tmp
}

fn make_large_copilot_project() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(tmp.path().join("main.kcl"), "cube(1)\n").expect("write main.kcl");
    for idx in 0..26 {
        std::fs::write(tmp.path().join(format!("extra-{idx}.kcl")), "cube(1)\n").expect("write extra file");
    }
    tmp
}

async fn run_test_item(ctx: &mut MainContext, item: TestItem) {
    let mut config = TestConfig::new().expect("failed to create blank test config");
    if let Some(setup) = item.setup {
        setup(&mut config, ctx).unwrap_or_else(|err| panic!("setup for '{}' failed: {err}", item.name));
    }

    let (mut io, stdout_path, stderr_path) = crate::iostreams::IoStreams::test();
    io.set_stdout_tty(false);
    io.set_color_enabled(false);
    if let Some(stdin) = item.stdin {
        io.stdin = Box::new(std::io::Cursor::new(stdin));
    }

    let mut command_ctx = crate::context::Context {
        config: &mut config,
        io,
        debug: false,
        override_host: None,
    };

    let _cwd_guard = CurrentDirGuard::change_to(item.current_directory.as_deref())
        .unwrap_or_else(|err| panic!("failed to set cwd for '{}': {err}", item.name));

    let result = crate::do_main(item.args, &mut command_ctx).await;

    let stdout = std::fs::read_to_string(stdout_path).unwrap_or_default();
    let stderr = std::fs::read_to_string(stderr_path).unwrap_or_default();

    match result {
        Ok(code) => {
            assert_eq!(
                code, item.want_code,
                "test '{}': unexpected exit code\nactual stdout: {stdout}\nactual stderr: {stderr}",
                item.name
            );
            assert_eq!(
                stdout.is_empty(),
                item.want_out.is_empty(),
                "test '{}': stdout mismatch\nactual stdout: {stdout}\nexpected stdout to contain: {}",
                item.name,
                item.want_out
            );
            assert_eq!(
                stderr.is_empty(),
                item.want_err.is_empty(),
                "test '{}': stderr mismatch\nactual stderr: {stderr}\nexpected stderr to contain: {}",
                item.name,
                item.want_err
            );
            if !item.want_out.is_empty() {
                assert!(
                    stdout.contains(&item.want_out),
                    "test '{}': stdout mismatch\nactual stdout: {stdout}\nexpected stdout to contain: {}\nactual stderr: {stderr}",
                    item.name,
                    item.want_out
                );
            }
            if !item.want_err.is_empty() {
                assert!(
                    stderr.contains(&item.want_err),
                    "test '{}': stderr mismatch\nactual stderr: {stderr}\nexpected stderr to contain: {}\nactual stdout: {stdout}",
                    item.name,
                    item.want_err
                );
            }
        }
        Err(err) => {
            assert!(
                !item.want_err.is_empty(),
                "test '{}': actual error: {err}\ndid not expect any error",
                item.name
            );
            assert!(
                err.to_string().contains(&item.want_err),
                "test '{}': actual error: {err}\nexpected error to contain: {}",
                item.name,
                item.want_err
            );
            assert!(
                stderr.is_empty(),
                "test '{}': stderr should have been empty, but it was {stderr}",
                item.name
            );
        }
    }
}

cli_tests! {
    existing_command(_ctx) => {
        TestItem::new("existing command", svec!["zoo", "completion"])
            .stdout_contains("complete -F _zoo -o nosort -o bashdefault -o default zoo\n")
    }

    existing_command_with_args(_ctx) => {
        TestItem::new("existing command with args", svec!["zoo", "completion", "-s", "zsh"])
            .stdout_contains("_zoo \"$@\"\n")
    }

    ml_text_to_cad_export_reasoning_on(_ctx) => {
        TestItem::new(
            "ml text-to-cad export reasoning on",
            svec![
                "zoo",
                "ml",
                "text-to-cad",
                "export",
                "-t",
                "obj",
                "--output-dir",
                "/tmp",
                "A",
                "2x4",
                "lego",
                "brick",
            ],
        )
        .setup(setup_authenticated)
        .stdout_contains("Completed")
        .stderr_contains("reasoning:")
    }

    ml_text_to_cad_export_no_reasoning(_ctx) => {
        TestItem::new(
            "ml text-to-cad export no reasoning",
            svec![
                "zoo",
                "ml",
                "text-to-cad",
                "export",
                "-t",
                "obj",
                "--output-dir",
                "/tmp",
                "--no-reasoning",
                "A",
                "2x4",
                "lego",
                "brick",
            ],
        )
        .setup(setup_authenticated)
        .stdout_contains("Completed")
    }

    ml_kcl_copilot_requires_main_kcl(_ctx) => {
        TestItem::new(
            "ml kcl copilot requires main.kcl",
            svec!["zoo", "ml", "kcl", "copilot"],
        )
        .stderr_contains("does not contain a main.kcl file")
        .exit_code(1)
    }

    add_an_alias(_ctx) => {
        TestItem::new(
            "add an alias",
            svec!["zoo", "alias", "set", "foo", "completion -s zsh"],
        )
        .stdout_contains("- Adding alias for foo: completion -s zsh\n✔ Added alias.")
    }

    add_a_shell_alias(_ctx) => {
        TestItem::new(
            "add a shell alias",
            svec!["zoo", "alias", "set", "-s", "bar", "which bash"],
        )
        .stdout_contains("- Adding alias for bar: !which bash\n✔ Added alias.")
    }

    list_our_aliases(_ctx) => {
        TestItem::new("list our aliases", svec!["zoo", "alias", "list"])
            .setup(setup_aliases)
            .stdout_contains("\"completion -s zsh\"")
    }

    call_alias(_ctx) => {
        TestItem::new("call alias", svec!["zoo", "foo"])
            .setup(setup_alias_completion)
            .stdout_contains("_zoo \"$@\"\n")
    }

    call_alias_with_different_binary_name(_ctx) => {
        TestItem::new("call alias with different binary name", svec!["/bin/thing/zoo", "foo"])
            .setup(setup_alias_completion)
            .stdout_contains("_zoo \"$@\"\n")
    }

    call_shell_alias(_ctx) => {
        TestItem::new("call shell alias", svec!["zoo", "bar"])
            .setup(setup_alias_shell)
            .stdout_contains("/bash")
    }

    version(_ctx) => {
        let version = clap::crate_version!();
        TestItem::new("version", svec!["zoo", "version"]).stdout_contains(format!(
            "zoo {} ({})\n{}",
            version,
            git_rev::revision_string!(),
            crate::cmd_version::changelog_url(version)
        ))
    }

    login(ctx) => {
        TestItem::new(
            "login",
            svec![
                "zoo",
                "--host",
                ctx.test_host.clone(),
                "auth",
                "login",
                "--with-token",
            ],
        )
        .stdin(ctx.test_token.clone())
        .stdout_contains("✔ Logged in as ")
    }

    api_user_with_leading_slash(_ctx) => {
        TestItem::new("api /user", svec!["zoo", "api", "/user"])
            .setup(setup_authenticated)
            .stdout_contains(r#""created_at": ""#)
    }

    api_user_without_leading_slash(_ctx) => {
        TestItem::new("api user (no leading /)", svec!["zoo", "api", "user"])
            .setup(setup_authenticated)
            .stdout_contains(r#""created_at": ""#)
    }

    api_user_with_header(_ctx) => {
        TestItem::new(
            "api user with header",
            svec!["zoo", "api", "user", "-H", "Origin: https://example.com"],
        )
        .setup(setup_authenticated)
        .stdout_contains(r#""created_at": ""#)
    }

    api_user_with_headers(_ctx) => {
        TestItem::new(
            "api user with headers",
            svec![
                "zoo",
                "api",
                "user",
                "-H",
                "Origin: https://example.com",
                "-H",
                "Another: thing",
            ],
        )
        .setup(setup_authenticated)
        .stdout_contains(r#""created_at": ""#)
    }

    api_user_with_output_headers(_ctx) => {
        TestItem::new(
            "api user with output headers",
            svec!["zoo", "api", "user", "--include"],
        )
        .setup(setup_authenticated)
        .stdout_contains("HTTP/2.0 200 OK")
    }

    api_endpoint_does_not_exist(_ctx) => {
        TestItem::new(
            "api endpoint does not exist",
            svec!["zoo", "api", "foo/bar"],
        )
        .setup(setup_authenticated)
        .stderr_contains("404 Not Found Not Found")
        .exit_code(1)
    }

    try_to_paginate_over_a_post(_ctx) => {
        TestItem::new(
            "try to paginate over a post",
            svec![
                "zoo",
                "api",
                "organizations",
                "--method",
                "POST",
                "--paginate",
            ],
        )
        .setup(setup_authenticated)
        .stderr_contains("the `--paginate` option is not supported for non-GET request")
        .exit_code(1)
    }

    get_your_user(_ctx) => {
        TestItem::new("get your user", svec!["zoo", "user", "view"])
            .setup(setup_authenticated)
            .stdout_contains("name")
    }

    get_your_user_as_json(_ctx) => {
        TestItem::new(
            "get your user as json",
            svec!["zoo", "user", "view", "--format=json"],
        )
        .setup(setup_authenticated)
        .stdout_contains(r#""created_at": ""#)
    }

    convert_a_file(_ctx) => {
        TestItem::new(
            "convert a file",
            svec![
                "zoo",
                "file",
                "convert",
                "assets/in_obj.obj",
                "/tmp/",
                "--output-format",
                "stl",
            ],
        )
        .setup(setup_authenticated)
        .stdout_contains("Completed")
    }

    get_the_file_volume(_ctx) => {
        TestItem::new(
            "get the file volume",
            svec![
                "zoo",
                "file",
                "volume",
                "assets/in_obj.obj",
                "--output-unit",
                "cm3",
            ],
        )
        .setup(setup_authenticated)
        .stdout_contains("0.05360")
    }

    get_the_file_density(_ctx) => {
        TestItem::new(
            "get the file density",
            svec![
                "zoo",
                "file",
                "density",
                "assets/in_obj.obj",
                "--output-unit",
                "lb-ft3",
                "--material-mass-unit",
                "g",
                "--material-mass",
                "1.0",
            ],
        )
        .setup(setup_authenticated)
        .stdout_contains("1164.67")
    }

    get_the_file_mass(_ctx) => {
        TestItem::new(
            "get the file mass",
            svec![
                "zoo",
                "file",
                "mass",
                "assets/in_obj.obj",
                "--output-unit",
                "g",
                "--material-density",
                "1.0",
                "--material-density-unit",
                "lb-ft3",
            ],
        )
        .setup(setup_authenticated)
        .stdout_contains("0.00085")
    }

    get_the_file_surface_area(_ctx) => {
        TestItem::new(
            "get the file surface-area",
            svec![
                "zoo",
                "file",
                "surface-area",
                "assets/in_obj.obj",
                "--output-unit",
                "cm2",
            ],
        )
        .setup(setup_authenticated)
        .stdout_contains("1.088")
    }

    get_the_file_center_of_mass(_ctx) => {
        TestItem::new(
            "get the file center-of-mass",
            svec![
                "zoo",
                "file",
                "center-of-mass",
                "assets/in_obj.obj",
                "--output-unit",
                "cm",
            ],
        )
        .setup(setup_authenticated)
        .stdout_contains("Point3D { x: -0.0133")
    }

    get_the_file_mass_as_json(_ctx) => {
        TestItem::new(
            "get the file mass as json",
            svec![
                "zoo",
                "file",
                "mass",
                "assets/in_obj.obj",
                "--format=json",
                "--output-unit",
                "g",
                "--material-density",
                "1.0",
                "--material-density-unit",
                "lb-ft3",
            ],
        )
        .setup(setup_authenticated)
        .stdout_contains(r#""mass": 0.000858"#)
    }

    snapshot_a_kcl_file_as_png(_ctx) => {
        TestItem::new(
            "snapshot a kcl file as png",
            svec!["zoo", "kcl", "snapshot", "tests/gear.kcl", "tests/gear.png"],
        )
        .setup(setup_authenticated)
        .stdout_contains("Snapshot saved to `tests/gear.png`")
    }

    snapshot_a_kcl_file_with_a_project_toml_as_png(_ctx) => {
        TestItem::new(
            "snapshot a kcl file with a project.toml as png",
            svec![
                "zoo",
                "kcl",
                "snapshot",
                "tests/with-settings/gear.kcl",
                "tests/with-settings/gear.png",
            ],
        )
        .setup(setup_authenticated)
        .stdout_contains("Snapshot saved to `tests/with-settings/gear.png`")
    }

    snapshot_a_kcl_file_with_a_nested_project_toml_as_png(_ctx) => {
        TestItem::new(
            "snapshot a kcl file with a nested project.toml as png",
            svec![
                "zoo",
                "kcl",
                "snapshot",
                "tests/nested-settings/subdir/gear.kcl",
                "tests/nested-settings/subdir/gear.png",
            ],
        )
        .setup(setup_authenticated)
        .stdout_contains("Snapshot saved to `tests/nested-settings/subdir/gear.png`")
    }

    snapshot_a_kcl_assembly_as_png(_ctx) => {
        TestItem::new(
            "snapshot a kcl assembly as png",
            svec!["zoo", "kcl", "snapshot", "tests/walkie-talkie", "tests/walkie-talkie.png"],
        )
        .setup(setup_authenticated)
        .stdout_contains("Snapshot saved to `tests/walkie-talkie.png`")
    }

    snapshot_a_kcl_assembly_as_png_with_dot(_ctx) => {
        TestItem::new(
            "snapshot a kcl assembly as png with .",
            svec!["zoo", "kcl", "snapshot", ".", "walkie-talkie.png"],
        )
        .setup(setup_authenticated)
        .current_directory(std::env::current_dir().unwrap().join("tests/walkie-talkie"))
        .stdout_contains("Snapshot saved to `walkie-talkie.png`")
    }

    get_the_mass_of_a_kcl_file(_ctx) => {
        TestItem::new(
            "get the mass of a kcl file",
            svec![
                "zoo",
                "kcl",
                "mass",
                "tests/gear.kcl",
                "--format=json",
                "--output-unit",
                "g",
                "--material-density",
                "1.0",
                "--material-density-unit",
                "lb-ft3",
            ],
        )
        .setup(setup_authenticated)
        .stdout_contains("1268.234")
    }

    get_the_mass_of_a_kcl_file_but_use_project_toml(_ctx) => {
        TestItem::new(
            "get the mass of a kcl file but use project.toml",
            svec![
                "zoo",
                "kcl",
                "mass",
                "tests/with-settings/gear.kcl",
                "--format=json",
                "--output-unit",
                "g",
                "--material-density",
                "1.0",
                "--material-density-unit",
                "lb-ft3",
            ],
        )
        .setup(setup_authenticated)
        .stdout_contains("74.052")
    }

    get_the_mass_of_a_kcl_file_with_nested_dirs_and_a_project_toml(_ctx) => {
        TestItem::new(
            "get the mass of a kcl file with nested dirs and a project.toml",
            svec![
                "zoo",
                "kcl",
                "mass",
                "tests/nested-settings/subdir/gear.kcl",
                "--format=json",
                "--output-unit",
                "g",
                "--material-density",
                "1.0",
                "--material-density-unit",
                "lb-ft3",
            ],
        )
        .setup(setup_authenticated)
        .stdout_contains("74.052")
    }

    analyze_a_kcl_file_as_table(_ctx) => {
        TestItem::new(
            "analyze a kcl file as table",
            svec![
                "zoo",
                "kcl",
                "analyze",
                "tests/gear.kcl",
                "--volume-output-unit",
                "cm3",
                "--mass-output-unit",
                "g",
                "--surface-area-output-unit",
                "cm2",
                "--material-density",
                "1.0",
                "--material-density-unit",
                "lb-ft3",
            ],
        )
        .setup(setup_authenticated)
        .stdout_contains("center_of_mass")
    }

    analyze_a_kcl_file_as_json(_ctx) => {
        TestItem::new(
            "analyze a kcl file as json",
            svec![
                "zoo",
                "kcl",
                "analyze",
                "tests/gear.kcl",
                "--format=json",
                "--volume-output-unit",
                "cm3",
                "--mass-output-unit",
                "g",
                "--surface-area-output-unit",
                "cm2",
                "--material-density",
                "1.0",
                "--material-density-unit",
                "lb-ft3",
            ],
        )
        .setup(setup_authenticated)
        .stdout_contains(r#""center_of_mass""#)
    }

    analyze_a_kcl_file_as_json_with_default_metric_units(_ctx) => {
        TestItem::new(
            "analyze a kcl file as json with default metric units",
            svec![
                "zoo",
                "kcl",
                "analyze",
                "tests/gear.kcl",
                "--format=json",
                "--material-density",
                "1.0",
            ],
        )
        .setup(setup_authenticated)
        .stdout_contains(r#""output_unit": "kg:m3""#)
    }

    analyze_a_kcl_file_and_use_project_toml(_ctx) => {
        TestItem::new(
            "analyze a kcl file and use project.toml",
            svec![
                "zoo",
                "kcl",
                "analyze",
                "tests/with-settings/gear.kcl",
                "--format=json",
                "--volume-output-unit",
                "cm3",
                "--mass-output-unit",
                "g",
                "--surface-area-output-unit",
                "cm2",
                "--material-density",
                "1.0",
                "--material-density-unit",
                "lb-ft3",
            ],
        )
        .setup(setup_authenticated)
        .stdout_contains(r#""mass""#)
    }

    analyze_a_kcl_file_with_invalid_density(_ctx) => {
        TestItem::new(
            "analyze a kcl file with invalid density",
            svec![
                "zoo",
                "kcl",
                "analyze",
                "tests/gear.kcl",
                "--volume-output-unit",
                "cm3",
                "--mass-output-unit",
                "g",
                "--surface-area-output-unit",
                "cm2",
                "--material-density",
                "0.0",
                "--material-density-unit",
                "lb-ft3",
            ],
        )
        .setup(setup_authenticated)
        .stderr_contains("`--material-density` must not be 0.0")
        .exit_code(1)
    }

    get_the_density_of_a_kcl_file(_ctx) => {
        TestItem::new(
            "get the density of a kcl file",
            svec![
                "zoo",
                "kcl",
                "density",
                "tests/gear.kcl",
                "--output-unit",
                "lb-ft3",
                "--material-mass-unit",
                "g",
                "--material-mass",
                "1.0",
            ],
        )
        .setup(setup_authenticated)
        .stdout_contains("0.0007")
    }
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn ml_kcl_edit_reasoning_on(ctx: &mut MainContext) {
    let tmp = make_single_file_edit_project();
    run_test_item(
        ctx,
        TestItem::new(
            "ml kcl edit reasoning on",
            svec!["zoo", "ml", "kcl", "edit", "gear.kcl", "Make", "it", "blue",],
        )
        .setup(setup_authenticated)
        .current_directory(tmp.path().to_path_buf())
        .stdout_contains("gear.kcl")
        .stderr_contains("reasoning:"),
    )
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn ml_kcl_edit_no_reasoning(ctx: &mut MainContext) {
    let tmp = make_single_file_edit_project();
    run_test_item(
        ctx,
        TestItem::new(
            "ml kcl edit no reasoning",
            svec![
                "zoo",
                "ml",
                "kcl",
                "edit",
                "--no-reasoning",
                "gear.kcl",
                "Make",
                "it",
                "blue",
            ],
        )
        .setup(setup_authenticated)
        .current_directory(tmp.path().to_path_buf())
        .stdout_contains("gear.kcl"),
    )
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn ml_kcl_edit_multi_file_root(ctx: &mut MainContext) {
    let tmp = make_multi_file_edit_project();
    run_test_item(
        ctx,
        TestItem::new(
            "ml kcl edit multi-file (root)",
            svec![
                "zoo",
                "ml",
                "kcl",
                "edit",
                "--no-reasoning",
                ".",
                "Add",
                "a",
                "simple",
                "cube",
                "to",
                "main.kcl",
                "and",
                "a",
                "cylinder",
                "to",
                "subdir/main.kcl",
            ],
        )
        .setup(setup_authenticated)
        .current_directory(tmp.path().to_path_buf())
        .stdout_contains("main.kcl"),
    )
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn ml_kcl_edit_multi_file_subdir(ctx: &mut MainContext) {
    let tmp = make_multi_file_edit_project();
    run_test_item(
        ctx,
        TestItem::new(
            "ml kcl edit multi-file (subdir)",
            svec![
                "zoo",
                "ml",
                "kcl",
                "edit",
                "--no-reasoning",
                ".",
                "Add",
                "a",
                "simple",
                "cube",
                "to",
                "main.kcl",
                "and",
                "a",
                "cylinder",
                "to",
                "subdir/main.kcl",
            ],
        )
        .setup(setup_authenticated)
        .current_directory(tmp.path().to_path_buf())
        .stdout_contains("subdir/main.kcl"),
    )
    .await;
}

#[test_context(MainContext)]
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
#[serial_test::serial]
async fn ml_kcl_copilot_rejects_large_project(ctx: &mut MainContext) {
    let tmp = make_large_copilot_project();
    run_test_item(
        ctx,
        TestItem::new(
            "ml kcl copilot rejects large project",
            svec!["zoo", "ml", "kcl", "copilot"],
        )
        .setup(setup_authenticated)
        .current_directory(tmp.path().to_path_buf())
        .stderr_contains("Copilot needs a smaller project")
        .exit_code(1),
    )
    .await;
}
