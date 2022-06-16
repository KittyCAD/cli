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
}

struct MainContext {
    test_host: String,
    test_token: String,
    client: kittycad::Client,
}

#[async_trait::async_trait]
impl AsyncTestContext for MainContext {
    async fn setup() -> Self {
        let test_host = std::env::var("KITTYCAD_TEST_HOST").unwrap_or_default();
        let test_host = crate::cmd_auth::parse_host(&test_host)
            .expect("invalid KITTYCAD_TEST_HOST")
            .to_string();
        let test_token = std::env::var("KITTYCAD_TEST_TOKEN").expect("KITTYCAD_TEST_TOKEN is required");

        let mut kittycad = kittycad::Client::new(&test_token);
        if !test_host.is_empty() {
            kittycad.set_host(&test_host);
        }

        Self {
            test_host,
            test_token,
            client: kittycad,
        }
    }

    async fn teardown(self) {}
}

#[test_context(MainContext)]
#[tokio::test]
#[serial_test::serial]
async fn test_main(ctx: &mut MainContext) {
    let version = clap::crate_version!();

    let tests: Vec<TestItem> = vec![
        TestItem {
            name: "existing command".to_string(),
            args: vec!["kittycad".to_string(), "completion".to_string()],
            want_out: "complete -F _kittycad -o bashdefault -o default kittycad\n".to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
            name: "existing command with args".to_string(),
            args: vec![
                "kittycad".to_string(),
                "completion".to_string(),
                "-s".to_string(),
                "zsh".to_string(),
            ],
            want_out: "_kittycad \"$@\"\n".to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
            name: "add an alias".to_string(),
            args: vec![
                "kittycad".to_string(),
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
                "kittycad".to_string(),
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
            args: vec!["kittycad".to_string(), "alias".to_string(), "list".to_string()],
            want_out: "\"completion -s zsh\"".to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
            name: "call alias".to_string(),
            args: vec!["kittycad".to_string(), "foo".to_string()],
            want_out: "_kittycad \"$@\"\n".to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
            name: "call alias with different binary name".to_string(),
            args: vec!["/bin/thing/kittycad".to_string(), "foo".to_string()],
            want_out: "_kittycad \"$@\"\n".to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
            name: "call shell alias".to_string(),
            args: vec!["kittycad".to_string(), "bar".to_string()],
            want_out: "/bash".to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
            name: "version".to_string(),
            args: vec!["kittycad".to_string(), "version".to_string()],
            want_out: format!(
                "kittycad {} ({})\n{}",
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
                "kittycad".to_string(),
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
        },
        TestItem {
            name: "api /user".to_string(),
            args: vec!["kittycad".to_string(), "api".to_string(), "/user".to_string()],
            want_out: r#""created_at": ""#.to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
            name: "api user (no leading /)".to_string(),
            args: vec!["kittycad".to_string(), "api".to_string(), "user".to_string()],
            want_out: r#""created_at": ""#.to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
            name: "api user with header".to_string(),
            args: vec![
                "kittycad".to_string(),
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
                "kittycad".to_string(),
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
                "kittycad".to_string(),
                "api".to_string(),
                "user".to_string(),
                "--include".to_string(),
            ],
            want_out: r#"HTTP/2.0 200 OK
access-control-allow-credentials:  """#
                .to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
            name: "api endpoint does not exist".to_string(),
            args: vec!["kittycad".to_string(), "api".to_string(), "foo/bar".to_string()],
            want_out: "".to_string(),
            want_err: "404 Not Found Not Found".to_string(),
            want_code: 1,
            ..Default::default()
        },
        TestItem {
            name: "try to paginate over a post".to_string(),
            args: vec![
                "kittycad".to_string(),
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
            args: vec!["kittycad".to_string(), "user".to_string(), "view".to_string()],
            want_out: "name           |".to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
        TestItem {
            name: "get your user as json".to_string(),
            args: vec![
                "kittycad".to_string(),
                "user".to_string(),
                "view".to_string(),
                "--format=json".to_string(),
            ],
            want_out: r#""created_at": ""#.to_string(),
            want_err: "".to_string(),
            want_code: 0,
            ..Default::default()
        },
    ];

    let mut config = crate::config::new_blank_config().unwrap();
    let mut c = crate::config_from_env::EnvConfig::inherit_env(&mut config);

    for t in tests {
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

        let result = crate::do_main(t.args, &mut ctx).await;

        let stdout = std::fs::read_to_string(stdout_path).unwrap_or_default();
        let stderr = std::fs::read_to_string(stderr_path).unwrap_or_default();

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
}
