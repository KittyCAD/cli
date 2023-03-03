use std::collections::HashMap;

use anyhow::{anyhow, Result};
use clap::Parser;
use oauth2::TokenResponse;

/// Login, logout, and get the status of your authentication.
///
/// Manage `kittycad`'s authentication state.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdAuth {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug, Clone)]
enum SubCommand {
    Login(CmdAuthLogin),
    Logout(CmdAuthLogout),
    Status(CmdAuthStatus),
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdAuth {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            SubCommand::Login(cmd) => cmd.run(ctx).await,
            SubCommand::Logout(cmd) => cmd.run(ctx).await,
            SubCommand::Status(cmd) => cmd.run(ctx).await,
        }
    }
}

/// Attempt to parse a given host string as a valid URL.
///
/// http(s) are the only supported schemas. If no schema is specified then https is assumed.
/// The returned URL if successful will be stripped of any path, username, password,
/// fragment or query.
pub fn parse_host(input: &str) -> Result<url::Url> {
    let mut input = input.to_string();
    if input.is_empty() {
        // If they didn't provide a host, set the default.
        input = crate::DEFAULT_HOST.to_string();
    }

    match url::Url::parse(&input) {
        Ok(mut url) => {
            if !url.has_host() {
                // We've successfully parsed a URL with no host.
                // This can happen if input was something like `localhost:8080`
                // where `localhost:` is treated as the scheme (`8080` would be the path).
                // Let's try again by prefixing with `https://`
                return parse_host(&format!("https://{input}"));
            }

            // Make sure scheme is http(s)
            let scheme = url.scheme();
            if scheme != "http" && scheme != "https" {
                anyhow::bail!("non-http(s) scheme given")
            }

            // We're only interested in the scheme, host & port
            // Clear any other component that was set
            url.set_path("");
            let _ = url.set_username("");
            let _ = url.set_password(None);
            url.set_fragment(None);
            url.set_query(None);

            Ok(url)
        }
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            // The input is being interpreted as a relative path meaning the input
            // didn't include a scheme mostly likely. Let's try again by prefixing
            // with `https://`
            parse_host(&format!("https://{input}"))
        }
        Err(err) => anyhow::bail!(err),
    }
}

/// Authenticate with an KittyCAD host.
///
/// Alternatively, pass in a token on standard input by using `--with-token`.
///
///     # start interactive setup
///     $ kittycad auth login
///
///     # authenticate against a specific KittyCAD instance by reading the token from a file
///     $ kittycad auth login --with-token --host kittycad.internal < mytoken.txt
///
///     # authenticate with a specific KittyCAD instance
///     $ kittycad auth login --host kittycad.internal
///
///     # authenticate with an insecure KittyCAD instance (not recommended)
///     $ kittycad auth login --host http://kittycad.internal
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdAuthLogin {
    /// Read token from standard input.
    #[clap(long)]
    pub with_token: bool,

    /// The host of the KittyCAD instance to authenticate with.
    /// By default this is api.kittycad.io.
    /// This assumes the instance is an `https://` url, if not otherwise specified
    /// as `http://`.
    #[clap(short = 'H', long, env = "KITTYCAD_HOST", parse(try_from_str = parse_host))]
    pub host: Option<url::Url>,
    /// Open a browser to authenticate.
    #[clap(short, long)]
    pub web: bool,
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdAuthLogin {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        if !ctx.io.can_prompt() && !self.with_token {
            return Err(anyhow!("--with-token required when not running interactively"));
        }

        let mut token = String::new();

        if self.with_token {
            // Read from stdin.
            ctx.io.stdin.read_to_string(&mut token)?;
        }

        let mut interactive = false;
        if ctx.io.can_prompt() && token.is_empty() {
            interactive = true;
        }

        let default_host = parse_host(crate::DEFAULT_HOST)?;
        let host = if let Some(host) = &self.host {
            host.as_str()
        } else {
            // Set the default.
            default_host.as_str()
        };

        if let Err(err) = ctx.config.check_writable(host, "token") {
            if let Some(crate::config_from_env::ReadOnlyEnvVarError::Variable(var)) = err.downcast_ref() {
                writeln!(
                    ctx.io.err_out,
                    "The value of the {var} environment variable is being used for authentication."
                )?;
                writeln!(
                    ctx.io.err_out,
                    "To have KittyCAD CLI store credentials instead, first clear the value from the environment."
                )?;
                return Err(anyhow!(""));
            }

            return Err(err);
        }

        let cs = ctx.io.color_scheme();

        // Do the login flow if we didn't get a token from stdin.
        if token.is_empty() {
            // We don't want to capture the error here just in case we have no host config
            // for this specific host yet.
            let existing_token = if let Ok(existing_token) = ctx.config.get(host, "token") {
                existing_token
            } else {
                String::new()
            };
            if !existing_token.is_empty() && interactive {
                match dialoguer::Confirm::new()
                    .with_prompt(format!(
                        "You're already logged into {host}. Do you want to re-authenticate?"
                    ))
                    .interact()
                {
                    Ok(true) => {}
                    Ok(false) => {
                        return Ok(());
                    }
                    Err(err) => {
                        return Err(anyhow!("prompt failed: {}", err));
                    }
                }
            }

            // Check the method they would like to login, web or otherwise.
            let mut web = self.web;
            // Only do this if they didn't already select web, and we can run interactively.
            if interactive && !self.web {
                let auth_options = vec!["Login with a web browser", "Paste an authentication token"];
                match dialoguer::Select::with_theme(&dialoguer::theme::ColorfulTheme::default())
                    .with_prompt("How would you like to authenticate KittyCAD CLI?")
                    .items(&auth_options)
                    .default(0)
                    .interact()
                {
                    Ok(index) => {
                        if index == 0 {
                            // They want to authenticate with the web.
                            web = true;
                        }
                    }
                    Err(err) => {
                        return Err(anyhow!("prompt failed: {}", err));
                    }
                }
            }

            token = if web {
                // Do an OAuth 2.0 Device Authorization Grant dance to get a token.
                let device_auth_url = oauth2::DeviceAuthorizationUrl::new(format!("{host}oauth2/device/auth"))?;
                // We can hardcode the client ID.
                // This value is safe to be embedded in version control.
                // This is the client ID of the cli.
                let client_id = "6bd9f64f-0ed6-40c2-ada0-87e1fc699227".to_string();
                let auth_client = oauth2::basic::BasicClient::new(
                    oauth2::ClientId::new(client_id),
                    None,
                    oauth2::AuthUrl::new(format!("{host}authorize"))?,
                    Some(oauth2::TokenUrl::new(format!("{host}oauth2/device/token"))?),
                )
                .set_auth_type(oauth2::AuthType::RequestBody)
                .set_device_authorization_url(device_auth_url);
                writeln!(ctx.io.err_out, "Tip: you can generate an API Token here {host}account")?;

                let details: oauth2::devicecode::StandardDeviceAuthorizationResponse = auth_client
                    .exchange_device_code()?
                    .request_async(oauth2::reqwest::async_http_client)
                    .await?;

                if let Some(uri) = details.verification_uri_complete() {
                    writeln!(
                        ctx.io.out,
                        "Opening {} in your browser.\n\
                     Please verify user code: {}\n",
                        **details.verification_uri(),
                        details.user_code().secret()
                    )?;
                    ctx.browser(host, uri.secret())?;
                } else {
                    writeln!(
                        ctx.io.out,
                        "Open this URL in your browser:\n{}\n\
                     And enter the code: {}\n",
                        **details.verification_uri(),
                        details.user_code().secret()
                    )?;
                }

                auth_client
                    .exchange_device_access_token(&details)
                    .request_async(oauth2::reqwest::async_http_client, tokio::time::sleep, None)
                    .await?
                    .access_token()
                    .secret()
                    .to_string()
            } else {
                writeln!(ctx.io.err_out, "Tip: you can generate an API Token here {host}account")?;

                match dialoguer::Input::<String>::new()
                    .with_prompt("Paste your authentication token")
                    .interact_text()
                {
                    Ok(input) => input,
                    Err(err) => {
                        return Err(anyhow!("prompt failed: {}", err));
                    }
                }
            };
        }

        // Set the token in the config file.
        ctx.config.set(host, "token", &token)?;

        let client = ctx.api_client(host)?;

        // Get the session for the token.
        let session = client.users().get_self().await?;

        // Set the user.
        let email = session
            .email
            .ok_or_else(|| anyhow::anyhow!("user does not have an email"))?;
        ctx.config.set(host, "user", &email)?;

        // Save the config.
        ctx.config.write()?;

        writeln!(ctx.io.out, "{} Logged in as {}", cs.success_icon(), cs.bold(&email))?;

        Ok(())
    }
}

/// Log out of an KittyCAD host.
///
/// This command removes the authentication configuration for a host either specified
/// interactively or via `--host`.
///
///     $ kittycad auth logout
///     # => select what host to log out of via a prompt
///
///     $ kittycad auth logout --host kittycad.internal
///     # => log out of specified host
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdAuthLogout {
    /// The hostname of the KittyCAD instance to log out of.
    #[clap(short = 'H', long, env = "KITTYCAD_HOST", parse(try_from_str = parse_host))]
    pub host: Option<url::Url>,
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdAuthLogout {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        if self.host.is_none() && !ctx.io.can_prompt() {
            return Err(anyhow!("--host required when not running interactively"));
        }

        let candidates = ctx.config.hosts()?;
        if candidates.is_empty() {
            return Err(anyhow!("not logged in to any hosts"));
        }

        let hostname = if self.host.is_none() {
            if candidates.len() == 1 {
                candidates[0].to_string()
            } else {
                let index = dialoguer::Select::with_theme(&dialoguer::theme::ColorfulTheme::default())
                    .with_prompt("What account do you want to log out of?")
                    .default(0)
                    .items(&candidates[..])
                    .interact();

                match index {
                    Ok(i) => candidates[i].to_string(),
                    Err(err) => {
                        return Err(anyhow!("prompt failed: {}", err));
                    }
                }
            }
        } else {
            let hostname = self.host.as_ref().unwrap().to_string();
            let mut found = false;
            for c in candidates {
                if c == hostname {
                    found = true;
                    break;
                }
            }

            if !found {
                return Err(anyhow!("not logged into {}", hostname));
            }

            hostname
        };

        if let Err(err) = ctx.config.check_writable(&hostname, "token") {
            if let Some(crate::config_from_env::ReadOnlyEnvVarError::Variable(var)) = err.downcast_ref() {
                writeln!(
                    ctx.io.err_out,
                    "The value of the {var} environment variable is being used for authentication."
                )?;
                writeln!(
                    ctx.io.err_out,
                    "To erase credentials stored in KittyCAD CLI, first clear the value from the environment."
                )?;
                return Err(anyhow!(""));
            }

            return Err(err);
        }

        let client = ctx.api_client(&hostname)?;

        // Get the current user.
        let session = client.users().get_self().await?;

        let email = session
            .email
            .ok_or_else(|| anyhow::anyhow!("user does not have an email"))?;

        let cs = ctx.io.color_scheme();

        if ctx.io.can_prompt() {
            match dialoguer::Confirm::new()
                .with_prompt(format!(
                    "Are you sure you want to log out of {} as {}?",
                    hostname,
                    cs.bold(&email)
                ))
                .interact()
            {
                Ok(true) => {}
                Ok(false) => {
                    return Ok(());
                }
                Err(err) => {
                    return Err(anyhow!("prompt failed: {}", err));
                }
            }
        }

        // Unset the host.
        ctx.config.unset_host(&hostname)?;

        // Write the changes to the config.
        ctx.config.write()?;

        let cs = ctx.io.color_scheme();
        writeln!(
            ctx.io.out,
            "{} Logged out of {} as {}",
            cs.success_icon(),
            hostname,
            cs.bold(&email)
        )?;

        Ok(())
    }
}

/// Verifies and displays information about your authentication state.
///
/// This command will test your authentication state for each KittyCAD host that `kittycad`
/// knows about and report on any issues.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdAuthStatus {
    /// Display the auth token.
    #[clap(short = 't', long)]
    pub show_token: bool,

    /// Check a specific hostname's auth status.
    #[clap(short = 'H', long, env = "KITTYCAD_HOST", parse(try_from_str = parse_host))]
    pub host: Option<url::Url>,
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdAuthStatus {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let cs = ctx.io.color_scheme();

        let mut status_info: HashMap<String, Vec<String>> = HashMap::new();

        let hostnames = ctx.config.hosts()?;

        if hostnames.is_empty() {
            writeln!(
                ctx.io.out,
                "You are not logged into any KittyCAD hosts. Run `{}` to authenticate.",
                cs.bold("kittycad auth login")
            )?;
            return Ok(());
        }

        let mut failed = false;
        let mut hostname_found = false;

        for hostname in &hostnames {
            if matches!(&self.host, Some(host) if host.as_str() != *hostname) {
                continue;
            }

            hostname_found = true;

            let (token, token_source) = ctx.config.get_with_source(hostname, "token")?;

            let client = ctx.api_client(hostname)?;

            let mut host_status: Vec<String> = vec![];

            match client.users().get_self().await {
                Ok(session) => {
                    let email = session
                        .email
                        .ok_or_else(|| anyhow::anyhow!("user does not have an email"))?;

                    host_status.push(format!(
                        "{} Logged in to {} as {} ({})",
                        cs.success_icon(),
                        hostname,
                        cs.bold(&email),
                        token_source
                    ));
                    let mut token_display = "*******************".to_string();
                    if self.show_token {
                        token_display = token.to_string();
                    }
                    host_status.push(format!("{} Token: {}", cs.success_icon(), token_display));
                }
                Err(err) => {
                    host_status.push(format!("{} {}: api call failed: {}", cs.failure_icon(), hostname, err));
                    failed = true;
                    continue;
                }
            }

            status_info.insert(hostname.to_string(), host_status);
        }

        if !hostname_found {
            writeln!(
                ctx.io.err_out,
                "Hostname {} not found among authenticated KittyCAD hosts",
                self.host.as_ref().unwrap().as_str(),
            )?;
            return Err(anyhow!(""));
        }

        for hostname in hostnames {
            match status_info.get(&hostname) {
                Some(status) => {
                    writeln!(ctx.io.out, "{}", cs.bold(&hostname))?;
                    for line in status {
                        writeln!(ctx.io.out, "{line}")?;
                    }
                }
                None => {
                    writeln!(ctx.io.err_out, "No status information for {hostname}")?;
                }
            }
        }

        if failed {
            return Err(anyhow!(""));
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use crate::cmd::Command;

    pub struct TestItem {
        name: String,
        cmd: crate::cmd_auth::SubCommand,
        stdin: String,
        want_out: String,
        want_err: String,
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[serial_test::serial]
    async fn test_cmd_auth() {
        let test_host = std::env::var("KITTYCAD_TEST_HOST").unwrap_or_default();
        let test_host = crate::cmd_auth::parse_host(&test_host).expect("invalid KITTYCAD_TEST_HOST");

        let test_token = std::env::var("KITTYCAD_TEST_TOKEN").expect("KITTYCAD_TEST_TOKEN is required");

        let tests: Vec<TestItem> = vec![
            TestItem {
                name: "status".to_string(),
                cmd: crate::cmd_auth::SubCommand::Status(crate::cmd_auth::CmdAuthStatus {
                    show_token: false,
                    host: None,
                }),
                stdin: "".to_string(),
                want_out: "".to_string(),
                want_err: "Try authenticating with".to_string(),
            },
            TestItem {
                name: "login --with-token=false".to_string(),
                cmd: crate::cmd_auth::SubCommand::Login(crate::cmd_auth::CmdAuthLogin {
                    host: Some(test_host.clone()),
                    with_token: false,
                    web: false,
                }),
                stdin: test_token.to_string(),
                want_out: "".to_string(),
                want_err: "--with-token required when not running interactively".to_string(),
            },
            TestItem {
                name: "login --with-token=true".to_string(),
                cmd: crate::cmd_auth::SubCommand::Login(crate::cmd_auth::CmdAuthLogin {
                    host: Some(test_host.clone()),
                    with_token: true,
                    web: false,
                }),
                stdin: test_token.to_string(),
                want_out: "✔ Logged in as ".to_string(),
                want_err: "".to_string(),
            },
            TestItem {
                name: "status".to_string(),
                cmd: crate::cmd_auth::SubCommand::Status(crate::cmd_auth::CmdAuthStatus {
                    show_token: false,
                    host: Some(test_host.clone()),
                }),
                stdin: "".to_string(),
                want_out: format!("{test_host}\n✔ Logged in to {test_host} as"),
                want_err: "".to_string(),
            },
            TestItem {
                name: "logout no prompt no host".to_string(),
                cmd: crate::cmd_auth::SubCommand::Logout(crate::cmd_auth::CmdAuthLogout { host: None }),
                stdin: "".to_string(),
                want_out: "".to_string(),
                want_err: "--host required when not running interactively".to_string(),
            },
            TestItem {
                name: "logout no prompt with host".to_string(),
                cmd: crate::cmd_auth::SubCommand::Logout(crate::cmd_auth::CmdAuthLogout {
                    host: Some(test_host.clone()),
                }),
                stdin: "".to_string(),
                want_out: format!("✔ Logged out of {test_host}"),
                want_err: "".to_string(),
            },
        ];

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
            // TODO: we should figure out how to test the prompts.
            io.set_never_prompt(true);
            let mut ctx = crate::context::Context {
                config: &mut c,
                io,
                debug: false,
            };

            let cmd_auth = crate::cmd_auth::CmdAuth { subcmd: t.cmd };
            match cmd_auth.run(&mut ctx).await {
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

    #[test]
    fn test_parse_host() {
        use super::parse_host;

        // TODO: Replace with assert_matches when stable

        // The simple cases where only the host name or IP is passed
        assert!(matches!(
            parse_host("example.com").map(|host| host.to_string()),
            Ok(host) if host == "https://example.com/"
        ));
        assert!(matches!(
            parse_host("localhost").map(|host| host.to_string()),
            Ok(host) if host == "https://localhost/"
        ));
        assert!(matches!(
            parse_host("127.0.0.1").map(|host| host.to_string()),
            Ok(host) if host == "https://127.0.0.1/"
        ));
        assert!(matches!(
            parse_host("[::1]").map(|host| host.to_string()),
            Ok(host) if host == "https://[::1]/"
        ));

        // Explicit port
        assert!(matches!(
            parse_host("example.com:8888").map(|host| host.to_string()),
            Ok(host) if host == "https://example.com:8888/"
        ));
        assert!(matches!(
            parse_host("localhost:8888").map(|host| host.to_string()),
            Ok(host) if host == "https://localhost:8888/"
        ));
        assert!(matches!(
            parse_host("127.0.0.1:8888").map(|host| host.to_string()),
            Ok(host) if host == "https://127.0.0.1:8888/"
        ));
        assert!(matches!(
            parse_host("[::1]:8888").map(|host| host.to_string()),
            Ok(host) if host == "https://[::1]:8888/"
        ));

        // Explicit scheme
        assert!(matches!(
            parse_host("http://example.com:8888").map(|host| host.to_string()),
            Ok(host) if host == "http://example.com:8888/"
        ));
        assert!(matches!(
            parse_host("http://localhost").map(|host| host.to_string()),
            Ok(host) if host == "http://localhost/"
        ));

        // Nonsense scheme
        assert!(matches!(
            parse_host("ftp://localhost").map(|host| host.to_string()),
            Err(_)
        ));

        // Strip out any extraneous pieces we don't need
        assert!(matches!(
            parse_host("http://user:pass@example.com:8888/random/path/?k=v&t=s#fragment=33").map(|host| host.to_string()),
            Ok(host) if host == "http://example.com:8888/"
        ));
    }
}
