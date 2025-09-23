use anyhow::{bail, Result};
use clap::Parser;

use crate::config::{ConfigOption, CONFIG_OPTIONS};

// TODO: make this doc a function that parses from the config the options so it's not hardcoded
/// Manage configuration for `zoo`.
///
/// Current respected settings:
/// - editor: the text editor program to use for authoring text
/// - prompt: toggle interactive prompting in the terminal (default: "enabled")
/// - pager: the terminal pager program to send standard output to
/// - browser: the web browser to use for opening URLs
/// - format: the formatting style for command output
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdConfig {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug, Clone)]
enum SubCommand {
    Set(CmdConfigSet),
    List(CmdConfigList),
    Get(CmdConfigGet),
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdConfig {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            SubCommand::Get(cmd) => cmd.run(ctx).await,
            SubCommand::Set(cmd) => cmd.run(ctx).await,
            SubCommand::List(cmd) => cmd.run(ctx).await,
        }
    }
}

/// Print the value of a given configuration key.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdConfigGet {
    /// The key to get the value of.
    #[clap(name = "key", required = true)]
    pub key: String,

    /// Get per-host setting.
    #[clap(short = 'H', long, default_value = "")]
    pub host: String,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdConfigGet {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match ctx.config.get(&self.host, &self.key) {
            Ok(value) => writeln!(ctx.io.out, "{value}")?,
            Err(err) => {
                bail!("{err}");
            }
        }

        Ok(())
    }
}

/// Update configuration with a value for the given key.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdConfigSet {
    /// The key to set the value of.
    #[clap(name = "key", required = true)]
    pub key: String,

    /// The value to set.
    #[clap(name = "value", required = true)]
    pub value: String,

    /// Set per-host setting.
    #[clap(short = 'H', long, default_value = "")]
    pub host: String,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdConfigSet {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        crate::config::validate_key(&self.key)?;
        crate::config::validate_value(&self.key, &self.value)?;

        // Set the value. If self.host is empty it will be top-level set.
        if let Err(err) = ctx.config.set(&self.host, &self.key, Some(&self.value)) {
            bail!("{err}");
        }

        // Unset the option in all other hosts if it's a mutually exclusive option.
        if !self.host.is_empty() {
            for option in CONFIG_OPTIONS {
                if let &ConfigOption::HostLevel {
                    key,
                    mutually_exclusive,
                    ..
                } = option
                {
                    if key != self.key || !mutually_exclusive {
                        continue;
                    }

                    for host in ctx.config.hosts()? {
                        // Skip the host that was the original value target
                        if host == self.host {
                            continue;
                        }
                        if let Err(err) = ctx.config.set(&host, &self.key, None) {
                            bail!("{err}");
                        }
                    }
                }
            }
        }

        // Write the config file.
        if let Err(err) = ctx.config.write() {
            bail!("{err}");
        }

        Ok(())
    }
}

/// Print a list of configuration keys and values.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdConfigList {
    /// Get per-host configuration.
    #[clap(short = 'H', long, default_value = "")]
    pub host: String,
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdConfigList {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let host = if self.host.is_empty() {
            // We don't want to do the default host here since we want to show the default's for
            // all hosts, even if ZOO_HOST is set.
            // TODO: in this case we should print all the hosts configs, not just the default.
            "".to_string()
        } else {
            self.host.to_string()
        };

        for option in CONFIG_OPTIONS {
            if let &ConfigOption::TopLevel { key, description, .. } = option {
                match ctx.config.get(&host, key) {
                    Ok(value) => writeln!(ctx.io.out, "{description}\n{key}={value}\n")?,
                    Err(err) => {
                        if host.is_empty() {
                            // Only bail if the host is empty, since some hosts may not have
                            // all the options.
                            bail!("{err}");
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use crate::{cmd::Command, config, config::Config};

    pub struct TestItem {
        name: String,
        cmd: crate::cmd_config::SubCommand,
        want_out: String,
        want_err: String,
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_cmd_config() {
        let tests: Vec<TestItem> = vec![
            TestItem {
                name: "list empty".to_string(),
                cmd: crate::cmd_config::SubCommand::List(crate::cmd_config::CmdConfigList { host: "".to_string() }),
                want_out: "the text editor program to use for authoring text\neditor=\n\ntoggle interactive prompting in the terminal\nprompt=enabled\n\nthe terminal pager program to send standard output to\npager=\n\nthe web browser to use for opening URLs\nbrowser=\n\nthe formatting style for command output\nformat=table\n\n".to_string(),
                want_err: "".to_string(),
            },
            TestItem {
                name: "set a key unknown".to_string(),
                cmd: crate::cmd_config::SubCommand::Set(crate::cmd_config::CmdConfigSet {
                    key: "foo".to_string(),
                    value: "bar".to_string(),
                    host: "".to_string(),
                }),
                want_out: "".to_string(),
                want_err: "invalid key: foo".to_string(),
            },
            TestItem {
                name: "set a key".to_string(),
                cmd: crate::cmd_config::SubCommand::Set(crate::cmd_config::CmdConfigSet {
                    key: "browser".to_string(),
                    value: "bar".to_string(),
                    host: "".to_string(),
                }),
                want_out: "".to_string(),
                want_err: "".to_string(),
            },
            TestItem {
                name: "set a key with host".to_string(),
                cmd: crate::cmd_config::SubCommand::Set(crate::cmd_config::CmdConfigSet {
                    key: "prompt".to_string(),
                    value: "disabled".to_string(),
                    host: "example.org".to_string(),
                }),
                want_out: "".to_string(),
                want_err: "".to_string(),
            },
            TestItem {
                name: "get a key we set".to_string(),
                cmd: crate::cmd_config::SubCommand::Get(crate::cmd_config::CmdConfigGet {
                    key: "browser".to_string(),
                    host: "".to_string(),
                }),
                want_out: "bar\n".to_string(),
                want_err: "".to_string(),
            },
            TestItem {
                name: "get a key we set with host".to_string(),
                cmd: crate::cmd_config::SubCommand::Get(crate::cmd_config::CmdConfigGet {
                    key: "prompt".to_string(),
                    host: "example.org".to_string(),
                }),
                want_out: "disabled\n".to_string(),
                want_err: "".to_string(),
            },
            TestItem {
                name: "get a non existent key".to_string(),
                cmd: crate::cmd_config::SubCommand::Get(crate::cmd_config::CmdConfigGet {
                    key: "blah".to_string(),
                    host: "".to_string(),
                }),
                want_out: "".to_string(),
                want_err: "Key 'blah' not found".to_string(),
            },
            TestItem {
                name: "list all default".to_string(),
                cmd: crate::cmd_config::SubCommand::List(crate::cmd_config::CmdConfigList { host: "".to_string() }),
                // want_out: "editor=\nprompt=enabled\npager=\nbrowser=bar\nformat=table\n".to_string(),
                want_out: "the text editor program to use for authoring text\neditor=\n\ntoggle interactive prompting in the terminal\nprompt=enabled\n\nthe terminal pager program to send standard output to\npager=\n\nthe web browser to use for opening URLs\nbrowser=bar\n\nthe formatting style for command output\nformat=table\n\n".to_string(),
                want_err: "".to_string(),
            },
        ];

        let mut config = crate::config::new_blank_config().unwrap();
        let mut c = crate::config_from_env::EnvConfig::inherit_env(&mut config);

        for t in tests {
            let (io, stdout_path, stderr_path) = crate::iostreams::IoStreams::test();
            let mut ctx = crate::context::Context {
                config: &mut c,
                io,
                debug: false,
                override_host: None,
            };

            let cmd_config = crate::cmd_config::CmdConfig { subcmd: t.cmd };
            match cmd_config.run(&mut ctx).await {
                Ok(()) => {
                    let stdout = std::fs::read_to_string(stdout_path).unwrap();
                    let stderr = std::fs::read_to_string(stderr_path).unwrap();
                    assert_eq!(stdout, t.want_out, "test {}", t.name);
                    assert!(stderr.is_empty(), "test {}", t.name);
                }
                Err(err) => {
                    let stdout = std::fs::read_to_string(stdout_path).unwrap();
                    let stderr = std::fs::read_to_string(stderr_path).unwrap();
                    assert_eq!(stdout, t.want_out, "test {}", t.name);
                    assert_eq!(&err.to_string(), &t.want_err, "test {}", t.name);
                    assert!(stderr.is_empty(), "test {}", t.name);
                }
            }
        }
    }

    #[tokio::test]
    async fn test_hosts_default_mutually_exclusive() -> Result<(), anyhow::Error> {
        let mut config = config::new_blank_config().unwrap();
        assert!(config.set("example.com", "token", Some("abcdef")).is_ok());
        assert!(config.set("zoo.computer", "token", Some("ghijkl")).is_ok());
        assert_eq!(config.hosts()?.len(), 2);

        let mut c = crate::config_from_env::EnvConfig::inherit_env(&mut config);

        let (io, _stdout_path, _stderr_path) = crate::iostreams::IoStreams::test();
        let mut ctx = crate::context::Context {
            config: &mut c,
            io,
            debug: false,
            override_host: None,
        };

        let mut cmd_config = crate::cmd_config::CmdConfig {
            subcmd: crate::cmd_config::SubCommand::Set(crate::cmd_config::CmdConfigSet {
                key: "default".to_string(),
                value: "true".to_string(),
                host: "example.com".to_string(),
            }),
        };
        cmd_config.run(&mut ctx).await?;

        cmd_config = crate::cmd_config::CmdConfig {
            subcmd: crate::cmd_config::SubCommand::Set(crate::cmd_config::CmdConfigSet {
                key: "default".to_string(),
                value: "true".to_string(),
                host: "zoo.computer".to_string(),
            }),
        };
        cmd_config.run(&mut ctx).await?;

        let config_text = config.hosts_to_string().unwrap();
        assert_eq!(
            config_text,
            r#"["example.com"]
token = "abcdef"

["zoo.computer"]
token = "ghijkl"
default = true"#
        );

        Ok(())
    }
}
