//! The Zoo command line tool.
#![deny(missing_docs)]

// Always export the cmd_* modules as public so that it tells us when we are
// missing docs.

mod cmd;
/// The alias command.
pub mod cmd_alias;
/// The api command.
pub mod cmd_api;
/// The api call command.
pub mod cmd_api_call;
/// The app command.
pub mod cmd_app;
/// The auth command.
pub mod cmd_auth;
/// The completion command.
pub mod cmd_completion;
/// The config command.
pub mod cmd_config;
/// The drake command.
pub mod cmd_drake;
/// The file command.
pub mod cmd_file;
/// The generate command.
pub mod cmd_generate;
/// The kcl command.
pub mod cmd_kcl;
/// The ml command.
pub mod cmd_ml;
/// The open command.
pub mod cmd_open;
/// The say command.
pub mod cmd_say;
/// The update command.
pub mod cmd_update;
/// The user command.
pub mod cmd_user;
/// The version command.
pub mod cmd_version;
/// Formatting for `kcl` errors.
pub mod kcl_error_fmt;

// Use of a mod or pub mod is not actually necessary.
mod built_info {
    // The file has been placed there by the build script.
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

mod colors;
mod config;
mod config_alias;
mod config_file;
mod config_from_env;
mod config_from_file;
mod config_map;
mod context;
mod docs_man;
mod docs_markdown;
mod iostreams;
mod prompt_ext;
mod types;

#[cfg(test)]
mod tests;

mod update;

use std::io::{Read, Write};

use anyhow::Result;
use clap::Parser;
use slog::Drain;

/// The default host for the Zoo API.
pub const DEFAULT_HOST: &str = "https://api.zoo.dev";

/// Work seamlessly with Zoo from the command line.
///
/// You've never CAD it so good.
///
/// Environment variables that can be used with `zoo`.
///
/// ZOO_TOKEN: an authentication token for Zoo API requests. Setting this
/// avoids being prompted to authenticate and takes precedence over previously
/// stored credentials.
///
/// ZOO_HOST: specify the Zoo hostname for commands that would otherwise assume
/// the "api.zoo.dev" host.
///
/// ZOO_BROWSER, BROWSER (in order of precedence): the web browser to use for opening
/// links.
///
/// DEBUG: set to any value to enable verbose output to standard error.
///
/// ZOO_PAGER, PAGER (in order of precedence): a terminal paging program to send
/// standard output to, e.g. "less".
///
/// NO_COLOR: set to any value to avoid printing ANSI escape sequences for color output.
///
/// CLICOLOR: set to "0" to disable printing ANSI colors in output.
///
/// CLICOLOR_FORCE: set to a value other than "0" to keep ANSI colors in output
/// even when the output is piped.
///
/// ZOO_FORCE_TTY: set to any value to force terminal-style output even when the
/// output is redirected. When the value is a number, it is interpreted as the number of
/// columns available in the viewport. When the value is a percentage, it will be applied
/// against the number of columns available in the current viewport.
///
/// ZOO_NO_UPDATE_NOTIFIER: set to any value to disable update notifications. By
/// default, `zoo` checks for new releases once every 24 hours and displays an upgrade
/// notice on standard error if a newer version was found.
///
/// ZOO_CONFIG_DIR: the directory where `zoo` will store configuration files.
/// Default: `$XDG_CONFIG_HOME/zoo` or `$HOME/.config/zoo`.
#[derive(Parser, Debug, Clone)]
#[clap(version = clap::crate_version!(), author = clap::crate_authors!("\n"))]
struct Opts {
    /// Print debug info
    #[clap(short, long, global = true, env)]
    debug: bool,

    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug, Clone)]
enum SubCommand {
    #[clap(alias = "aliases")]
    Alias(cmd_alias::CmdAlias),
    Api(cmd_api::CmdApi),
    ApiCall(cmd_api_call::CmdApiCall),
    App(cmd_app::CmdApp),
    Auth(cmd_auth::CmdAuth),
    Completion(cmd_completion::CmdCompletion),
    Config(cmd_config::CmdConfig),
    Drake(cmd_drake::CmdDrake),
    File(cmd_file::CmdFile),
    Generate(cmd_generate::CmdGenerate),
    Kcl(cmd_kcl::CmdKcl),
    Ml(cmd_ml::CmdMl),
    Say(cmd_say::CmdSay),
    Open(cmd_open::CmdOpen),
    Update(cmd_update::CmdUpdate),
    User(cmd_user::CmdUser),
    Version(cmd_version::CmdVersion),
}

#[tokio::main]
async fn main() -> Result<(), ()> {
    let build_version = clap::crate_version!();
    // Check for updates to the cli.
    // We don't await here since we don't want to block the main thread.
    // We'll check again before we exit.
    let update = crate::update::check_for_update(build_version, false);

    // Let's get our configuration.
    let mut c = crate::config_file::parse_default_config().unwrap();
    let mut config = crate::config_from_env::EnvConfig::inherit_env(&mut c);
    let mut ctx = crate::context::Context::new(&mut config);

    // Let's grab all our args.
    let args: Vec<String> = std::env::args().collect();
    let result = do_main(args, &mut ctx).await;

    // If we have an update, let's print it.
    handle_update(&mut ctx, update.await.unwrap_or_default(), build_version).unwrap();

    if let Err(err) = result {
        eprintln!("{err}");
        std::process::exit(1);
    }

    std::process::exit(result.unwrap_or(0));
}

async fn do_main(mut args: Vec<String>, ctx: &mut crate::context::Context<'_>) -> Result<i32> {
    let original_args = args.clone();

    // Remove the first argument, which is the program name, and can change depending on how
    // they are calling it.
    args.remove(0);

    let args_str = shlex::try_join(args.iter().map(|s| s.as_str()).collect::<Vec<&str>>())?;

    // Check if the user is passing in an alias.
    if !crate::cmd_alias::valid_command(&args_str) {
        // Let's validate if it is an alias.
        // It is okay to check the error here because we will not error out if the
        // alias does not exist. We will just return the expanded args.
        let (mut expanded_args, is_shell) = ctx.config.expand_alias(original_args)?;

        if is_shell {
            // Remove the first argument, since thats our `sh`.
            expanded_args.remove(0);

            let mut external_cmd = std::process::Command::new("sh")
                .args(expanded_args)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()?;

            let ecode = external_cmd.wait()?;

            // Pipe the output to the terminal.
            if let Some(stdout_rd) = external_cmd.stdout.as_mut() {
                let mut stdout = Vec::new();
                stdout_rd.read_to_end(&mut stdout)?;
                ctx.io.out.write_all(&stdout)?;
            }

            if let Some(mut stderr_rd) = external_cmd.stderr {
                let mut stderr = Vec::new();
                stderr_rd.read_to_end(&mut stderr)?;
                ctx.io.err_out.write_all(&stderr)?;
            }

            return Ok(ecode.code().unwrap_or(0));
        }

        // So we handled if the alias was a shell.
        // We can now parse our options from the extended args.
        args = expanded_args;
    } else {
        args = original_args;
    }

    // Parse the command line arguments.
    let opts: Opts = Opts::parse_from(args);

    // Set our debug flag.
    ctx.debug = opts.debug;

    // Setup our logger. This is mainly for debug purposes.
    // And getting debug logs from other libraries we consume, like even Zoo.
    if ctx.debug {
        let decorator = slog_term::TermDecorator::new().build();
        let drain = slog_term::FullFormat::new(decorator).build().fuse();
        let drain = slog_async::Async::new(drain).build().fuse();

        let logger = slog::Logger::root(drain, slog::o!());

        let scope_guard = slog_scope::set_global_logger(logger);
        scope_guard.cancel_reset();

        slog_stdlog::init_with_level(log::Level::Debug).unwrap();
    }

    match opts.subcmd {
        SubCommand::Alias(cmd) => run_cmd(&cmd, ctx).await,
        SubCommand::Api(cmd) => run_cmd(&cmd, ctx).await,
        SubCommand::ApiCall(cmd) => run_cmd(&cmd, ctx).await,
        SubCommand::App(cmd) => run_cmd(&cmd, ctx).await,
        SubCommand::Auth(cmd) => run_cmd(&cmd, ctx).await,
        SubCommand::Completion(cmd) => run_cmd(&cmd, ctx).await,
        SubCommand::Config(cmd) => run_cmd(&cmd, ctx).await,
        SubCommand::Drake(cmd) => run_cmd(&cmd, ctx).await,
        SubCommand::File(cmd) => run_cmd(&cmd, ctx).await,
        SubCommand::Generate(cmd) => run_cmd(&cmd, ctx).await,
        SubCommand::Kcl(cmd) => run_cmd(&cmd, ctx).await,
        SubCommand::Ml(cmd) => run_cmd(&cmd, ctx).await,
        SubCommand::Say(cmd) => run_cmd(&cmd, ctx).await,
        SubCommand::Open(cmd) => run_cmd(&cmd, ctx).await,
        SubCommand::Update(cmd) => run_cmd(&cmd, ctx).await,
        SubCommand::User(cmd) => run_cmd(&cmd, ctx).await,
        SubCommand::Version(cmd) => run_cmd(&cmd, ctx).await,
    }
}

async fn run_cmd(cmd: &impl crate::cmd::Command, ctx: &mut context::Context<'_>) -> Result<i32> {
    let cs = ctx.io.color_scheme();

    if let Err(err) = cmd.run(ctx).await {
        // If the error was from the API, let's handle it better for each type of error.
        match err.downcast::<kittycad::types::error::Error>() {
            Ok(err) => {
                if err.status() == Some(http::StatusCode::FORBIDDEN) {
                    writeln!(
                        ctx.io.err_out,
                        "{} You are not authorized to perform this action",
                        cs.failure_icon(),
                    )?;
                } else if err.status() == Some(http::StatusCode::UNAUTHORIZED) {
                    writeln!(ctx.io.err_out, "{} You are not authenticated.", cs.failure_icon())?;

                    writeln!(ctx.io.err_out, "Try authenticating with: `zoo auth login`")?;
                } else if let kittycad::types::error::Error::UnexpectedResponse(resp) = err {
                    let body = resp.text().await?;
                    writeln!(ctx.io.err_out, "zoo.dev api error: {}", body)?;
                } else {
                    writeln!(ctx.io.err_out, "{err}")?;
                }
            }
            Err(err) => {
                writeln!(ctx.io.err_out, "{err}")?;
            }
        }
        return Ok(1);
    }

    Ok(0)
}

fn handle_update(
    ctx: &mut crate::context::Context,
    update: Option<crate::update::ReleaseInfo>,
    build_version: &str,
) -> Result<()> {
    if let Some(latest_release) = update {
        // do not notify Homebrew users before the version bump had a chance to get merged into homebrew-core
        let is_homebrew = crate::update::is_under_homebrew()?;

        if !(is_homebrew && crate::update::is_recent_release(latest_release.published_at)) {
            let cs = ctx.io.color_scheme();

            writeln!(
                ctx.io.err_out,
                "\n\n{} {} → {}\n",
                cs.yellow("A new release of zoo is available:"),
                cs.cyan(build_version),
                cs.purple(&latest_release.version)
            )?;

            if is_homebrew {
                writeln!(ctx.io.err_out, "To upgrade, run: `brew update && brew upgrade zoo`")?;
            } else {
                writeln!(ctx.io.err_out, "To upgrade, run: `zoo update`")?;
            }

            writeln!(ctx.io.err_out, "{}\n\n", cs.yellow(&latest_release.url))?;
        }
    }

    Ok(())
}
