use std::str::FromStr;

use anyhow::{anyhow, Result};
use kcl_lib::engine::EngineManager;
use kittycad::types::OkWebSocketResponseData;

use crate::{config::Config, config_file::get_env_var, kcl_error_fmt, types::FormatOutput};

pub struct Context<'a> {
    pub config: &'a mut (dyn Config + Send + Sync + 'a),
    pub io: crate::iostreams::IoStreams,
    pub debug: bool,
}

impl Context<'_> {
    pub fn new(config: &mut (dyn Config + Send + Sync)) -> Context {
        // Let's get our IO streams.
        let mut io = crate::iostreams::IoStreams::system();

        // Set the prompt.
        let prompt = config.get("", "prompt").unwrap();
        if prompt == "disabled" {
            io.set_never_prompt(true)
        }

        // Set the pager.
        // Pager precedence
        // 1. ZOO_PAGER
        // 2. pager from config
        // 3. PAGER
        if let Ok(zoo_pager) = std::env::var("ZOO_PAGER") {
            io.set_pager(zoo_pager);
        } else if let Ok(pager) = config.get("", "pager") {
            if !pager.is_empty() {
                io.set_pager(pager);
            }
        }

        // Check if we should force use the tty.
        if let Ok(zoo_force_tty) = std::env::var("ZOO_FORCE_TTY") {
            if !zoo_force_tty.is_empty() {
                io.force_terminal(&zoo_force_tty);
            }
        }

        Context {
            config,
            io,
            debug: false,
        }
    }

    /// This function returns an API client for Zoo that is based on the configured
    /// user.
    pub fn api_client(&self, hostname: &str) -> Result<kittycad::Client> {
        // Use the host passed in if it's set.
        // Otherwise, use the default host.
        let host = if hostname.is_empty() {
            self.config.default_host()?
        } else {
            hostname.to_string()
        };

        // Change the baseURL to the one we want.
        let mut baseurl = host.to_string();
        if !host.starts_with("http://") && !host.starts_with("https://") {
            baseurl = format!("https://{host}");
            if host.starts_with("localhost") {
                baseurl = format!("http://{host}")
            }
        }

        let user_agent = concat!(env!("CARGO_PKG_NAME"), ".rs/", env!("CARGO_PKG_VERSION"),);
        let http_client = reqwest::Client::builder()
            .user_agent(user_agent)
            // For file conversions we need this to be long.
            .timeout(std::time::Duration::from_secs(600))
            .connect_timeout(std::time::Duration::from_secs(60));
        let ws_client = reqwest::Client::builder()
            .user_agent(user_agent)
            // For file conversions we need this to be long.
            .timeout(std::time::Duration::from_secs(600))
            .connect_timeout(std::time::Duration::from_secs(60))
            .tcp_keepalive(std::time::Duration::from_secs(600))
            .http1_only();

        // Get the token for that host.
        let token = self.config.get(&host, "token")?;

        // Create the client.
        let mut client = kittycad::Client::new_from_reqwest(token, http_client, ws_client);

        if baseurl != crate::DEFAULT_HOST {
            client.set_base_url(&baseurl);
        }

        Ok(client)
    }

    #[allow(dead_code)]
    pub async fn send_single_modeling_cmd(
        &self,
        hostname: &str,
        cmd: kittycad::types::ModelingCmd,
    ) -> Result<OkWebSocketResponseData> {
        let engine = self.engine(hostname).await?;

        let resp = engine
            .send_modeling_cmd(uuid::Uuid::new_v4(), kcl_lib::executor::SourceRange::default(), cmd)
            .await?;
        Ok(resp)
    }

    async fn engine_ws(&self, hostname: &str) -> Result<reqwest::Upgraded> {
        let client = self.api_client(hostname)?;
        let ws = client
            .modeling()
            .commands_ws(None, None, None, None, None, None, None, Some(false))
            .await?;
        Ok(ws)
    }

    pub async fn engine(&self, hostname: &str) -> Result<kcl_lib::engine::conn::EngineConnection> {
        let ws = self.engine_ws(hostname).await?;

        let engine = kcl_lib::engine::conn::EngineConnection::new(ws).await?;

        Ok(engine)
    }

    pub async fn send_kcl_modeling_cmd(
        &self,
        hostname: &str,
        code: &str,
        cmd: kittycad::types::ModelingCmd,
        units: kittycad::types::UnitLength,
    ) -> Result<OkWebSocketResponseData> {
        let client = self.api_client(hostname)?;

        let tokens = kcl_lib::token::lexer(code)?;
        let parser = kcl_lib::parser::Parser::new(tokens);
        let program = parser
            .ast()
            .map_err(|err| kcl_error_fmt::KclError::new(code.to_string(), err))?;

        let ctx = kcl_lib::executor::ExecutorContext::new(
            &client,
            kcl_lib::executor::ExecutorSettings {
                units: units.into(),
                ..Default::default()
            },
        )
        .await?;
        let _ = ctx
            .run(&program, None)
            .await
            .map_err(|err| kcl_error_fmt::KclError::new(code.to_string(), err))?;

        // Zoom on the object.
        ctx.engine
            .send_modeling_cmd(
                uuid::Uuid::new_v4(),
                kcl_lib::executor::SourceRange::default(),
                kittycad::types::ModelingCmd::ZoomToFit {
                    object_ids: Default::default(),
                    padding: 0.1,
                },
            )
            .await?;

        let resp = ctx
            .engine
            .send_modeling_cmd(uuid::Uuid::new_v4(), kcl_lib::executor::SourceRange::default(), cmd)
            .await
            .map_err(|err| kcl_error_fmt::KclError::new(code.to_string(), err))?;
        Ok(resp)
    }

    pub async fn get_model_for_prompt(
        &self,
        hostname: &str,
        prompt: &str,
        format: kittycad::types::FileExportFormat,
    ) -> Result<kittycad::types::TextToCad> {
        let client = self.api_client(hostname)?;

        // Create the text-to-cad request.
        let mut gen_model: kittycad::types::TextToCad = client
            .ml()
            .create_text_to_cad(
                None,
                format,
                &kittycad::types::TextToCadCreateBody {
                    prompt: prompt.to_string(),
                },
            )
            .await?;

        // Poll until the model is ready.
        let mut status = gen_model.status.clone();
        // Get the current time.
        let start = std::time::Instant::now();
        // Give it 5 minutes to complete. That should be way
        // more than enough!
        while status != kittycad::types::ApiCallStatus::Completed
            && status != kittycad::types::ApiCallStatus::Failed
            && start.elapsed().as_secs() < 60 * 5
        {
            // Poll for the status.
            let result = client.api_calls().get_async_operation(gen_model.id).await?;

            if let kittycad::types::AsyncApiCallOutput::TextToCad {
                completed_at,
                created_at,
                error,
                feedback,
                id,
                model_version,
                output_format,
                outputs,
                prompt,
                started_at,
                status,
                updated_at,
                user_id,
                code,
                model,
            } = result
            {
                gen_model = kittycad::types::TextToCad {
                    completed_at,
                    created_at,
                    error,
                    feedback,
                    id,
                    model_version,
                    output_format,
                    outputs,
                    prompt,
                    started_at,
                    status,
                    updated_at,
                    user_id,
                    code,
                    model,
                };
            } else {
                anyhow::bail!("Unexpected response type: {:?}", result);
            }

            status = gen_model.status.clone();

            // Wait for a bit before polling again.
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }

        // If the model failed we will want to tell the user.
        if gen_model.status == kittycad::types::ApiCallStatus::Failed {
            if let Some(error) = gen_model.error {
                anyhow::bail!("Your prompt returned an error: ```\n{}\n```", error);
            } else {
                anyhow::bail!("Your prompt returned an error, but no error message. :(");
            }
        }

        if gen_model.status != kittycad::types::ApiCallStatus::Completed {
            anyhow::bail!("Your prompt timed out");
        }

        // Okay, we successfully got a model!
        Ok(gen_model)
    }

    /// This function opens a browser that is based on the configured
    /// environment to the specified path.
    ///
    /// Browser precedence:
    /// 1. ZOO_BROWSER
    /// 2. BROWSER
    /// 3. browser from config
    pub fn browser(&self, hostname: &str, url: &str) -> Result<()> {
        let source: String;
        let browser = if !get_env_var("ZOO_BROWSER").is_empty() {
            source = "ZOO_BROWSER".to_string();
            get_env_var("ZOO_BROWSER")
        } else if !get_env_var("BROWSER").is_empty() {
            source = "BROWSER".to_string();
            get_env_var("BROWSER")
        } else {
            source = crate::config_file::config_file()?;
            self.config.get(hostname, "browser").unwrap_or_else(|_| "".to_string())
        };

        if browser.is_empty() {
            if let Err(err) = open::that(url) {
                return Err(anyhow!("An error occurred when opening '{}': {}", url, err));
            }
        } else if let Err(err) = open::with(url, &browser) {
            return Err(anyhow!(
                "An error occurred when opening '{}' with browser '{}' configured from '{}': {}",
                url,
                browser,
                source,
                err
            ));
        }

        Ok(())
    }

    /// Return the configured output format or override the default with the value passed in,
    /// if it is some.
    pub fn format(&self, format: &Option<FormatOutput>) -> Result<FormatOutput> {
        if let Some(format) = format {
            Ok(format.clone())
        } else {
            let value = self.config.get("", "format")?;
            Ok(FormatOutput::from_str(&value).unwrap_or_default())
        }
    }

    /// Read the file at the given path and returns the contents.
    /// If "-" is given, read from stdin.
    pub fn read_file(&mut self, filename: &str) -> Result<Vec<u8>> {
        if filename.is_empty() {
            anyhow::bail!("File path cannot be empty.");
        }

        if filename == "-" {
            let mut buffer = Vec::new();

            // Read everything from stdin.
            self.io.stdin.read_to_end(&mut buffer)?;

            return Ok(buffer);
        }

        if !std::path::Path::new(filename).exists() {
            anyhow::bail!("File '{}' does not exist.", filename);
        }

        std::fs::read(filename).map_err(Into::into)
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use test_context::{test_context, TestContext};

    use super::*;

    struct TContext {
        orig_zoo_pager_env: Result<String, std::env::VarError>,
        orig_zoo_force_tty_env: Result<String, std::env::VarError>,
    }

    impl TestContext for TContext {
        fn setup() -> TContext {
            TContext {
                orig_zoo_pager_env: std::env::var("ZOO_PAGER"),
                orig_zoo_force_tty_env: std::env::var("ZOO_FORCE_TTY"),
            }
        }

        fn teardown(self) {
            // Put the original env var back.
            if let Ok(ref val) = self.orig_zoo_pager_env {
                std::env::set_var("ZOO_PAGER", val);
            } else {
                std::env::remove_var("ZOO_PAGER");
            }

            if let Ok(ref val) = self.orig_zoo_force_tty_env {
                std::env::set_var("ZOO_FORCE_TTY", val);
            } else {
                std::env::remove_var("ZOO_FORCE_TTY");
            }
        }
    }

    pub struct TestItem {
        name: String,
        zoo_pager_env: String,
        zoo_force_tty_env: String,
        pager: String,
        prompt: String,
        want_pager: String,
        want_prompt: String,
        want_terminal_width_override: i32,
    }

    #[test_context(TContext)]
    #[test]
    #[serial_test::serial]
    fn test_context(_ctx: &mut TContext) {
        let tests = vec![
            TestItem {
                name: "ZOO_PAGER env".to_string(),
                zoo_pager_env: "more".to_string(),
                zoo_force_tty_env: "".to_string(),
                prompt: "".to_string(),
                pager: "".to_string(),
                want_pager: "more".to_string(),
                want_prompt: "enabled".to_string(),
                want_terminal_width_override: 0,
            },
            TestItem {
                name: "ZOO_PAGER env override".to_string(),
                zoo_pager_env: "more".to_string(),
                zoo_force_tty_env: "".to_string(),
                prompt: "".to_string(),
                pager: "less".to_string(),
                want_pager: "more".to_string(),
                want_prompt: "enabled".to_string(),
                want_terminal_width_override: 0,
            },
            TestItem {
                name: "config pager".to_string(),
                zoo_pager_env: "".to_string(),
                zoo_force_tty_env: "".to_string(),
                prompt: "".to_string(),
                pager: "less".to_string(),
                want_pager: "less".to_string(),
                want_prompt: "enabled".to_string(),
                want_terminal_width_override: 0,
            },
            TestItem {
                name: "config prompt".to_string(),
                zoo_pager_env: "".to_string(),
                zoo_force_tty_env: "".to_string(),
                prompt: "disabled".to_string(),
                pager: "less".to_string(),
                want_pager: "less".to_string(),
                want_prompt: "disabled".to_string(),
                want_terminal_width_override: 0,
            },
            TestItem {
                name: "ZOO_FORCE_TTY env".to_string(),
                zoo_pager_env: "".to_string(),
                zoo_force_tty_env: "120".to_string(),
                prompt: "disabled".to_string(),
                pager: "less".to_string(),
                want_pager: "less".to_string(),
                want_prompt: "disabled".to_string(),
                want_terminal_width_override: 120,
            },
        ];

        for t in tests {
            let mut config = crate::config::new_blank_config().unwrap();
            let mut c = crate::config_from_env::EnvConfig::inherit_env(&mut config);

            if !t.pager.is_empty() {
                c.set("", "pager", &t.pager).unwrap();
            }

            if !t.prompt.is_empty() {
                c.set("", "prompt", &t.prompt).unwrap();
            }

            if !t.zoo_pager_env.is_empty() {
                std::env::set_var("ZOO_PAGER", t.zoo_pager_env.clone());
            } else {
                std::env::remove_var("ZOO_PAGER");
            }

            if !t.zoo_force_tty_env.is_empty() {
                std::env::set_var("ZOO_FORCE_TTY", t.zoo_force_tty_env.clone());
            } else {
                std::env::remove_var("ZOO_FORCE_TTY");
            }

            let ctx = Context::new(&mut c);

            assert_eq!(ctx.io.get_pager(), t.want_pager, "test: {}", t.name);

            assert_eq!(
                ctx.io.get_never_prompt(),
                t.want_prompt == "disabled",
                "test {}",
                t.name
            );

            assert_eq!(ctx.config.get("", "pager").unwrap(), t.want_pager, "test: {}", t.name);
            assert_eq!(ctx.config.get("", "prompt").unwrap(), t.want_prompt, "test: {}", t.name);

            if t.want_terminal_width_override > 0 {
                assert_eq!(
                    ctx.io.terminal_width(),
                    t.want_terminal_width_override,
                    "test: {}",
                    t.name
                );
            }
        }
    }
}
