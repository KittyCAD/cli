use std::io::Write;

use anyhow::Result;
use clap::Parser;
use cli_macro::crud_gen;

/// Edit and view your user.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdUser {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[crud_gen {
    tag = "users",
}]
#[derive(Parser, Debug, Clone)]
enum SubCommand {}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdUser {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            SubCommand::Edit(cmd) => cmd.run(ctx).await,
            SubCommand::View(cmd) => cmd.run(ctx).await,
            SubCommand::Delete(cmd) => cmd.run(ctx).await,
        }
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use test_context::{test_context, AsyncTestContext};

    use crate::cmd::Command;

    pub struct TestItem {
        name: String,
        cmd: crate::cmd_user::SubCommand,
        stdin: String,
        want_out: String,
        want_err: String,
    }

    struct TContext {
        orig_zoo_host: Result<String, std::env::VarError>,
        orig_zoo_token: Result<String, std::env::VarError>,
    }

    #[async_trait::async_trait]
    impl AsyncTestContext for TContext {
        async fn setup() -> TContext {
            let orig = TContext {
                orig_zoo_host: std::env::var("ZOO_HOST"),
                orig_zoo_token: std::env::var("ZOO_TOKEN"),
            };

            // Set our test values.
            let test_host = std::env::var("ZOO_TEST_HOST").unwrap_or_default();

            let test_token = std::env::var("ZOO_TEST_TOKEN").expect("ZOO_TEST_TOKEN is required");
            std::env::set_var("ZOO_HOST", test_host);
            std::env::set_var("ZOO_TOKEN", test_token);

            orig
        }

        async fn teardown(self) {
            // Put the original env var back.
            if let Ok(ref val) = self.orig_zoo_host {
                std::env::set_var("ZOO_HOST", val);
            } else {
                std::env::remove_var("ZOO_HOST");
            }

            if let Ok(ref val) = self.orig_zoo_token {
                std::env::set_var("ZOO_TOKEN", val);
            } else {
                std::env::remove_var("ZOO_TOKEN");
            }
        }
    }

    #[test_context(TContext)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[serial_test::serial]
    async fn test_cmd_user(_ctx: &mut TContext) {
        let tests: Vec<TestItem> = vec![TestItem {
            name: "volume: input file does not exist".to_string(),
            cmd: crate::cmd_user::SubCommand::Edit(crate::cmd_user::CmdUserEdit {
                new_is_onboarded: Default::default(),
                new_company: Default::default(),
                new_discord: Default::default(),
                new_phone: Default::default(),
                new_last_name: Default::default(),
                new_first_name: Default::default(),
                new_github: Default::default(),
                new_image: Default::default(),
            }),
            stdin: "".to_string(),
            want_out: "".to_string(),
            want_err: "nothing to edit".to_string(),
        }];

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
            io.set_never_prompt(true);
            let mut ctx = crate::context::Context {
                config: &mut c,
                io,
                debug: false,
                override_host: None,
            };

            let cmd_user = crate::cmd_user::CmdUser { subcmd: t.cmd };
            match cmd_user.run(&mut ctx).await {
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
