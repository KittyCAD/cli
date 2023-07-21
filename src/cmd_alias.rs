use std::io::Write;

use anyhow::{bail, Result};
use clap::{Command, CommandFactory, Parser};

/// Create command shortcuts.
///
/// Aliases can be used to make shortcuts for `kittycad` commands or to compose multiple commands.
/// Run `kittycad help alias set` to learn more.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdAlias {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug, Clone)]
enum SubCommand {
    Set(CmdAliasSet),
    Delete(CmdAliasDelete),
    List(CmdAliasList),
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdAlias {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            SubCommand::Delete(cmd) => cmd.run(ctx).await,
            SubCommand::Set(cmd) => cmd.run(ctx).await,
            SubCommand::List(cmd) => cmd.run(ctx).await,
        }
    }
}

/// Delete an alias.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdAliasDelete {
    /// The alias to delete.
    #[clap(name = "alias", required = true)]
    pub alias: String,
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdAliasDelete {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let mut alias_config = ctx.config.aliases()?;

        let (expansion, ok) = alias_config.get(&self.alias);
        if !ok {
            bail!("no such alias {}", self.alias);
        }

        match alias_config.delete(&self.alias) {
            Ok(_) => {
                let cs = ctx.io.color_scheme();
                writeln!(
                    ctx.io.out,
                    "{} Deleted alias {}; was {}",
                    cs.success_icon_with_color(ansi_term::Color::Red),
                    self.alias,
                    expansion
                )?;
            }
            Err(e) => {
                bail!("failed to delete alias {}: {}", self.alias, e);
            }
        }

        Ok(())
    }
}

/// Create a shortcut for a `kittycad` command.
///
/// Define a word that will expand to a full `kittycad` command when invoked.
///
/// The expansion may specify additional arguments and flags. If the expansion includes
/// positional placeholders such as "$1", extra arguments that follow the alias will be
/// inserted appropriately. Otherwise, extra arguments will be appended to the expanded
/// command.
///
/// Use "-" as expansion argument to read the expansion string from standard input. This
/// is useful to avoid quoting issues when defining expansions.
///
/// If the expansion starts with "!" or if "--shell" was given, the expansion is a shell
/// expression that will be evaluated through the "sh" interpreter when the alias is
/// invoked. This allows for chaining multiple commands via piping and redirection.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdAliasSet {
    /// The alias to set.
    #[clap(name = "alias", required = true)]
    pub alias: String,

    /// The expansion of the alias.
    #[clap(name = "expansion", required = true)]
    pub expansion: String,

    /// Declare an alias to be passed through a shell interpreter.
    #[clap(short, long)]
    pub shell: bool,
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdAliasSet {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let cs = ctx.io.color_scheme();

        let mut config_aliases = ctx.config.aliases()?;

        match get_expansion(self) {
            Ok(mut expansion) => {
                let mut is_shell = self.shell;
                if is_shell && !expansion.starts_with('!') {
                    expansion = format!("!{expansion}");
                }
                is_shell = expansion.starts_with('!');

                // Check if already exists.
                if valid_command(&self.alias) {
                    bail!("could not create alias: {} is already a kittycad command", self.alias);
                }

                if !is_shell && !valid_command(&expansion) {
                    bail!(
                        "could not create alias: {} does not correspond to a kittycad command",
                        expansion
                    );
                }

                writeln!(
                    ctx.io.out,
                    "- Adding alias for {}: {}",
                    cs.bold(&self.alias),
                    cs.bold(&expansion)
                )?;

                let mut success_msg = format!("{} Added alias.", cs.success_icon());
                let (old_expansion, ok) = config_aliases.get(&self.alias);
                if ok {
                    success_msg = format!(
                        "{} Changed alias {} from {} to {}",
                        cs.success_icon(),
                        cs.bold(&self.alias),
                        cs.bold(&old_expansion),
                        cs.bold(&expansion)
                    );
                }

                match config_aliases.add(&self.alias, &expansion) {
                    Ok(_) => {
                        writeln!(ctx.io.out, "{success_msg}")?;
                    }
                    Err(e) => {
                        bail!("could not create alias: {}", e);
                    }
                }
            }
            Err(e) => {
                bail!("failed to parse expansion {}: {}", self.expansion, e);
            }
        }

        Ok(())
    }
}

/// List your aliases.
///
/// This command prints out all of the aliases `kittycad` is configured to use.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdAliasList {}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdAliasList {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let config_aliases = ctx.config.aliases()?;

        if config_aliases.map.is_empty() {
            writeln!(ctx.io.out, "no aliases configured")?;
            return Ok(());
        }

        let mut tw = tabwriter::TabWriter::new(vec![]);
        for (alias, expansion) in config_aliases.list().iter() {
            writeln!(tw, "{alias}:\t{expansion}")?;
        }
        tw.flush()?;

        let table = String::from_utf8(tw.into_inner()?)?;
        writeln!(ctx.io.out, "{table}")?;

        Ok(())
    }
}

fn get_expansion(cmd: &CmdAliasSet) -> Result<String> {
    if cmd.expansion == "-" {
        let mut expansion = String::new();
        std::io::stdin().read_line(&mut expansion)?;
        Ok(expansion)
    } else {
        Ok(cmd.expansion.to_string())
    }
}

/// Check if a set of arguments is a valid `kittycad` command.
pub fn valid_command(args: &str) -> bool {
    let s = shlex::split(args);
    if s.is_none() {
        return false;
    }

    let args = s.unwrap_or_default();
    if args.is_empty() {
        return false;
    }

    // Convert our opts into a clap app.
    let app: Command = crate::Opts::command();

    // Try to get matches.
    for subcmd in app.get_subcommands() {
        if subcmd.get_name() != args[0] {
            continue;
        }

        match subcmd.clone().try_get_matches_from(args) {
            Ok(_) => {
                // If we get here, we have a valid command.
                return true;
            }
            Err(err) => {
                return match err.kind() {
                    // These come from here: https://docs.rs/clap/latest/clap/enum.ErrorKind.html#variant.DisplayHelp
                    // We basically want to ignore any errors that are valid commands but invalid args.
                    clap::error::ErrorKind::DisplayHelp => true,
                    clap::error::ErrorKind::DisplayVersion => true,
                    clap::error::ErrorKind::MissingRequiredArgument => true,
                    clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => true,
                    _ => {
                        // If we get here, we have an invalid command.
                        false
                    }
                };
            }
        }
    }

    false
}

#[cfg(test)]
mod test {
    use crate::cmd::Command;

    pub struct TestAlias {
        name: String,
        cmd: crate::cmd_alias::SubCommand,
        want_out: String,
        want_err: String,
    }

    pub struct TestValidCommand {
        name: String,
        cmd: String,
        want: bool,
    }

    #[test]
    fn test_valid_command() {
        let tests = vec![
            TestValidCommand {
                name: "empty".to_string(),
                cmd: "".to_string(),
                want: false,
            },
            TestValidCommand {
                name: "single arg valid".to_string(),
                cmd: "completion".to_string(),
                want: true,
            },
            TestValidCommand {
                name: "multiple arg valid".to_string(),
                cmd: "completion -s zsh".to_string(),
                want: true,
            },
            TestValidCommand {
                name: "single arg invalid".to_string(),
                cmd: "foo".to_string(),
                want: false,
            },
            TestValidCommand {
                name: "multiple args invalid".to_string(),
                cmd: "foo -H thing".to_string(),
                want: false,
            },
        ];

        for t in tests {
            let is_valid = crate::cmd_alias::valid_command(&t.cmd);

            assert_eq!(is_valid, t.want, "test {}", t.name);
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[serial_test::serial]
    async fn test_cmd_alias() {
        let tests: Vec<TestAlias> = vec![
            TestAlias {
                name: "list empty".to_string(),
                cmd: crate::cmd_alias::SubCommand::List(crate::cmd_alias::CmdAliasList {}),
                want_out: "no aliases configured\n".to_string(),
                want_err: "".to_string(),
            },
            TestAlias {
                name: "add an alias".to_string(),
                cmd: crate::cmd_alias::SubCommand::Set(crate::cmd_alias::CmdAliasSet {
                    alias: "cs".to_string(),
                    expansion: "config set".to_string(),
                    shell: false,
                }),
                want_out: "- Adding alias for cs: config set\n✔ Added alias.\n".to_string(),
                want_err: "".to_string(),
            },
            TestAlias {
                name: "update an alias".to_string(),
                cmd: crate::cmd_alias::SubCommand::Set(crate::cmd_alias::CmdAliasSet {
                    alias: "cs".to_string(),
                    expansion: "config get".to_string(),
                    shell: false,
                }),
                want_out: "- Adding alias for cs: config get\n✔ Changed alias cs from config set to config get\n"
                    .to_string(),
                want_err: "".to_string(),
            },
            TestAlias {
                name: "add an alias with shell".to_string(),
                cmd: crate::cmd_alias::SubCommand::Set(crate::cmd_alias::CmdAliasSet {
                    alias: "cp".to_string(),
                    expansion: "config list".to_string(),
                    shell: true,
                }),
                want_out: "- Adding alias for cp: !config list\n✔ Added alias.\n".to_string(),
                want_err: "".to_string(),
            },
            TestAlias {
                name: "add an alias with expandable args".to_string(),
                cmd: crate::cmd_alias::SubCommand::Set(crate::cmd_alias::CmdAliasSet {
                    alias: "cs".to_string(),
                    expansion: "config set $1 $2".to_string(),
                    shell: false,
                }),
                want_out:
                    "- Adding alias for cs: config set $1 $2\n✔ Changed alias cs from config get to config set $1 $2"
                        .to_string(),
                want_err: "".to_string(),
            },
            TestAlias {
                name: "add already command -> config".to_string(),
                cmd: crate::cmd_alias::SubCommand::Set(crate::cmd_alias::CmdAliasSet {
                    alias: "config".to_string(),
                    expansion: "alias set".to_string(),
                    shell: false,
                }),
                want_out: "".to_string(),
                want_err: "could not create alias: config is already a kittycad command".to_string(),
            },
            TestAlias {
                name: "add already command -> completion".to_string(),
                cmd: crate::cmd_alias::SubCommand::Set(crate::cmd_alias::CmdAliasSet {
                    alias: "completion".to_string(),
                    expansion: "alias set".to_string(),
                    shell: false,
                }),
                want_out: "".to_string(),
                want_err: "could not create alias: completion is already a kittycad command".to_string(),
            },
            TestAlias {
                name: "add does not exist".to_string(),
                cmd: crate::cmd_alias::SubCommand::Set(crate::cmd_alias::CmdAliasSet {
                    alias: "cp".to_string(),
                    expansion: "dne thing".to_string(),
                    shell: false,
                }),
                want_out: "".to_string(),
                want_err: "could not create alias: dne thing does not correspond to a kittycad command".to_string(),
            },
            TestAlias {
                name: "list all".to_string(),
                cmd: crate::cmd_alias::SubCommand::List(crate::cmd_alias::CmdAliasList {}),
                want_out: "\"!config list\"\n".to_string(),
                want_err: "".to_string(),
            },
            TestAlias {
                name: "delete an alias".to_string(),
                cmd: crate::cmd_alias::SubCommand::Delete(crate::cmd_alias::CmdAliasDelete {
                    alias: "cp".to_string(),
                }),
                want_out: "Deleted alias cp; was !config list".to_string(),
                want_err: "".to_string(),
            },
            TestAlias {
                name: "delete an alias not exist".to_string(),
                cmd: crate::cmd_alias::SubCommand::Delete(crate::cmd_alias::CmdAliasDelete {
                    alias: "thing".to_string(),
                }),
                want_out: "".to_string(),
                want_err: "no such alias thing".to_string(),
            },
            TestAlias {
                name: "list after delete".to_string(),
                cmd: crate::cmd_alias::SubCommand::List(crate::cmd_alias::CmdAliasList {}),
                want_out: "cs:  \"config set $1 $2\"\n".to_string(),
                want_err: "".to_string(),
            },
        ];

        let mut config = crate::config::new_blank_config().unwrap();
        let mut c = crate::config_from_env::EnvConfig::inherit_env(&mut config);

        for t in tests {
            let (mut io, stdout_path, stderr_path) = crate::iostreams::IoStreams::test();
            io.set_stdout_tty(false);
            io.set_color_enabled(false);
            let mut ctx = crate::context::Context {
                config: &mut c,
                io,
                debug: false,
            };

            let cmd_alias = crate::cmd_alias::CmdAlias { subcmd: t.cmd };

            let result = cmd_alias.run(&mut ctx).await;

            let stdout = std::fs::read_to_string(stdout_path).unwrap();
            let stderr = std::fs::read_to_string(stderr_path).unwrap();

            assert!(
                stdout.contains(&t.want_out),
                "test {} ->\nstdout: {}\nwant: {}",
                t.name,
                stdout,
                t.want_out
            );

            match result {
                Ok(()) => {
                    assert!(stdout.is_empty() == t.want_out.is_empty(), "test {}", t.name);
                    assert!(stderr.is_empty(), "test {}", t.name);
                }
                Err(err) => {
                    assert!(
                        err.to_string().contains(&t.want_err),
                        "test {} -> err: {}\nwant_err: {}",
                        t.name,
                        err,
                        t.want_err
                    );
                    assert!(
                        err.to_string().is_empty() == t.want_err.is_empty(),
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
}
