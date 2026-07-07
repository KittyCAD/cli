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

    use crate::cmd::Command;

    pub struct TestItem {
        name: String,
        cmd: crate::cmd_user::SubCommand,
        stdin: String,
        want_out: String,
        want_err: String,
    }

    /// Same-process unit test for paths that do not require dependencies to
    /// read real process env.
    ///
    /// Tests that need `kcl_lib` or other dependencies to read `ZOO_API_TOKEN`
    /// must use the child-process integration test harness in
    /// [`tests/kcl_process.rs`](../../tests/kcl_process.rs).
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_cmd_user() {
        let tests: Vec<TestItem> = vec![TestItem {
            name: "volume: input file does not exist".to_string(),
            cmd: crate::cmd_user::SubCommand::Edit(crate::cmd_user::CmdUserEdit {
                new_is_onboarded: Default::default(),
                new_company: Default::default(),
                new_discord: Default::default(),
                new_username: Default::default(),
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
                config: &mut config,
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
