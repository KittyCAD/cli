use std::str::FromStr;

use anyhow::{anyhow, Result};
use kcl_lib::{native_engine::EngineConnection, EngineManager};
use kcmc::{each_cmd as mcmd, websocket::OkWebSocketResponseData};
use kittycad::types::{ApiCallStatus, AsyncApiCallOutput, TextToCad, TextToCadCreateBody, TextToCadMultiFileIteration};
use kittycad_modeling_cmds::{self as kcmc, shared::FileExportFormat, websocket::ModelingSessionData, ModelingCmd};
use tokio_tungstenite::{tungstenite::protocol::Role, WebSocketStream};
use futures::StreamExt;

use crate::{config::Config, config_file::get_env_var, kcl_error_fmt, types::FormatOutput};

pub struct Context<'a> {
    pub config: &'a mut (dyn Config + Send + Sync + 'a),
    pub io: crate::iostreams::IoStreams,
    pub debug: bool,
}

impl Context<'_> {
    pub fn new(config: &mut (dyn Config + Send + Sync)) -> Context<'_> {
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
        cmd: ModelingCmd,
        replay: Option<String>,
    ) -> Result<OkWebSocketResponseData> {
        let engine = self.engine(hostname, replay).await?;

        let resp = engine
            .send_modeling_cmd(uuid::Uuid::new_v4(), kcl_lib::SourceRange::default(), &cmd)
            .await?;
        Ok(resp)
    }

    async fn engine_ws(&self, hostname: &str, replay: Option<String>) -> Result<reqwest::Upgraded> {
        let client = self.api_client(hostname)?;
        let api_call_id = None;
        let fps = None;
        let pool = None;
        let post_effect = None;
        let show_grid = None;
        let unlocked_framerate = None;
        let video_res_height = None;
        let video_res_width = None;
        let (ws, _headers) = client
            .modeling()
            .commands_ws(
                api_call_id,
                fps,
                pool,
                post_effect,
                replay,
                show_grid,
                unlocked_framerate,
                video_res_height,
                video_res_width,
                Some(false),
            )
            .await?;
        Ok(ws)
    }

    pub async fn engine(&self, hostname: &str, replay: Option<String>) -> Result<EngineConnection> {
        let ws = self.engine_ws(hostname, replay).await?;

        let engine = EngineConnection::new(ws).await?;

        Ok(engine)
    }

    pub async fn send_kcl_modeling_cmd(
        &self,
        hostname: &str,
        filename: &str,
        code: &str,
        cmd: kittycad_modeling_cmds::ModelingCmd,
        settings: kcl_lib::ExecutorSettings,
    ) -> Result<(OkWebSocketResponseData, Option<ModelingSessionData>)> {
        let client = self.api_client(hostname)?;

        let program = kcl_lib::Program::parse_no_errs(code)
            .map_err(|err| kcl_error_fmt::into_miette_for_parse(filename, code, err))?;

        let ctx = kcl_lib::ExecutorContext::new(&client, settings).await?;
        let mut state = kcl_lib::ExecState::new(&ctx);
        let session_data = ctx
            .run(&program, &mut state)
            .await
            .map_err(|err| kcl_error_fmt::into_miette(err, code))?
            .1;

        // Zoom on the object.
        ctx.engine
            .send_modeling_cmd(
                uuid::Uuid::new_v4(),
                kcl_lib::SourceRange::default(),
                &ModelingCmd::from(mcmd::ZoomToFit {
                    animated: false,
                    object_ids: Default::default(),
                    padding: 0.1,
                }),
            )
            .await?;

        let resp = ctx
            .engine
            .send_modeling_cmd(uuid::Uuid::new_v4(), kcl_lib::SourceRange::default(), &cmd)
            .await
            .map_err(|err| kcl_error_fmt::into_miette_for_parse(filename, code, err))?;
        Ok((resp, session_data))
    }

    pub async fn get_model_for_prompt(
        &self,
        hostname: &str,
        prompt: &str,
        kcl: bool,
        format: kittycad::types::FileExportFormat,
        show_reasoning: bool,
    ) -> Result<TextToCad> {
        let client = self.api_client(hostname)?;

        let format = match format {
            kittycad::types::FileExportFormat::Fbx => FileExportFormat::Fbx,
            kittycad::types::FileExportFormat::Glb => FileExportFormat::Glb,
            kittycad::types::FileExportFormat::Obj => FileExportFormat::Obj,
            kittycad::types::FileExportFormat::Ply => FileExportFormat::Ply,
            kittycad::types::FileExportFormat::Stl => FileExportFormat::Stl,
            kittycad::types::FileExportFormat::Gltf => FileExportFormat::Gltf,
            kittycad::types::FileExportFormat::Step => FileExportFormat::Step,
        };

        // Create the text-to-cad request.
        let mut gen_model: TextToCad = client
            .ml()
            .create_text_to_cad(
                Some(kcl),
                format.into(),
                &TextToCadCreateBody {
                    prompt: prompt.to_string(),
                    kcl_version: Some(kcl_lib::version().to_owned()),
                    project_name: None,
                },
            )
            .await?;

        // Start reasoning websocket to stream reasoning messages for this generation.
        let reasoning_task = self.spawn_reasoning_ws_task(&client, gen_model.id, show_reasoning).await;

        // Poll until the model is ready.
        let mut status = gen_model.status.clone();
        // Get the current time.
        let start = std::time::Instant::now();
        // Give it 5 minutes to complete. That should be way
        // more than enough!
        while status != ApiCallStatus::Completed
            && status != ApiCallStatus::Failed
            && start.elapsed().as_secs() < 60 * 5
        {
            // Poll for the status.
            let result = client.api_calls().get_async_operation(gen_model.id).await?;

            if let AsyncApiCallOutput::TextToCad {
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
                kcl_version,
                conversation_id,
            } = result
            {
                gen_model = TextToCad {
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
                    kcl_version,
                    conversation_id,
                };
            } else {
                anyhow::bail!("Unexpected response type: {:?}", result);
            }

            status = gen_model.status.clone();

            // Wait for a bit before polling again.
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }

        // If the model failed we will want to tell the user.
        if gen_model.status == ApiCallStatus::Failed {
            if let Some(error) = gen_model.error {
                anyhow::bail!("Your prompt returned an error: ```\n{}\n```", error);
            } else {
                anyhow::bail!("Your prompt returned an error, but no error message. :(");
            }
        }

        if gen_model.status != ApiCallStatus::Completed {
            anyhow::bail!("Your prompt timed out");
        }

        // Okay, we successfully got a model!
        if let Some(handle) = reasoning_task { handle.abort(); }
        Ok(gen_model)
    }

    pub async fn get_edit_for_prompt(
        &self,
        hostname: &str,
        body: &kittycad::types::TextToCadMultiFileIterationBody,
        files: Vec<kittycad::types::multipart::Attachment>,
        show_reasoning: bool,
    ) -> Result<TextToCadMultiFileIteration> {
        let client = self.api_client(hostname)?;

        // Create the text-to-cad request.
        let mut gen_model = client.ml().create_text_to_cad_multi_file_iteration(files, body).await?;

        // Start reasoning websocket to stream reasoning messages for this edit.
        // default to showing reasoning for edits as well; caller can pass false by wrapping here if needed later
        let reasoning_task = self
            .spawn_reasoning_ws_task(&client, gen_model.id, show_reasoning)
            .await;

        // Poll until the model is ready.
        let mut status = gen_model.status.clone();
        // Get the current time.
        let start = std::time::Instant::now();
        // Give it 5 minutes to complete. That should be way
        // more than enough!
        while status != ApiCallStatus::Completed
            && status != ApiCallStatus::Failed
            && start.elapsed().as_secs() < 60 * 5
        {
            // Poll for the status.
            let result = client.api_calls().get_async_operation(gen_model.id).await?;

            if let AsyncApiCallOutput::TextToCadMultiFileIteration {
                completed_at,
                created_at,
                error,
                feedback,
                id,
                model_version,
                prompt,
                started_at,
                status,
                updated_at,
                user_id,
                model,
                source_ranges,
                outputs,
                kcl_version,
                project_name,
                conversation_id,
            } = result
            {
                gen_model = TextToCadMultiFileIteration {
                    completed_at,
                    created_at,
                    error,
                    feedback,
                    id,
                    model_version,
                    prompt,
                    started_at,
                    status,
                    updated_at,
                    user_id,
                    model,
                    source_ranges,
                    outputs,
                    kcl_version,
                    project_name,
                    conversation_id,
                };
            } else {
                anyhow::bail!("Unexpected response type: {:?}", result);
            }

            status = gen_model.status.clone();

            // Wait for a bit before polling again.
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }

        // If the model failed we will want to tell the user.
        if gen_model.status == ApiCallStatus::Failed {
            if let Some(error) = gen_model.error {
                anyhow::bail!("Your prompt returned an error: ```\n{}\n```", error);
            } else {
                anyhow::bail!("Your prompt returned an error, but no error message. :(");
            }
        }

        if gen_model.status != ApiCallStatus::Completed {
            anyhow::bail!("Your prompt timed out");
        }

        // Okay, we successfully got a model!
        if let Some(handle) = reasoning_task { handle.abort(); }
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

    /// Get the path to the current file from the path given, and read the code.
    pub async fn get_code_and_file_path(&mut self, path: &std::path::Path) -> Result<(String, std::path::PathBuf)> {
        // If the user passes in ".", use the current working directory.
        // This is useful for running commands from the current directory.
        let mut path = path.to_path_buf();
        if path.to_str().unwrap_or("-") == "." {
            path = std::env::current_dir()?;
        }

        // Check if the path is a directory, if so we want to look for a main.kcl inside.
        if path.is_dir() {
            path = path.join("main.kcl");
            if !path.exists() {
                return Err(anyhow::anyhow!(
                    "Directory `{}` does not contain a main.kcl file",
                    path.display()
                ));
            }
        } else {
            // Otherwise be sure we have a kcl file.
            if path.to_str().unwrap_or("-") != "-" {
                if let Some(ext) = path.extension() {
                    if ext != "kcl" {
                        return Err(anyhow::anyhow!("File must have a .kcl extension"));
                    }
                }
            }
        }

        let b = self.read_file(path.to_str().unwrap_or("-"))?;
        // Parse the input as a string.
        let code = std::str::from_utf8(&b)?;
        Ok((code.to_string(), path))
    }

    /// Collect all the kcl files in the given directory or parent directory to the given path.
    pub async fn collect_kcl_files(
        &mut self,
        path: &std::path::Path,
        ) -> Result<(Vec<kittycad::types::multipart::Attachment>, std::path::PathBuf)> {
        let mut files = Vec::new();

        let (code, filepath) = self.get_code_and_file_path(path).await?;
        files.push(kittycad::types::multipart::Attachment {
            name: filepath.to_string_lossy().to_string(),
            filepath: Some(filepath.clone()),
            content_type: Some("text/plain".parse()?),
            data: code.as_bytes().to_vec(),
        });

        // Walk the directory and collect all the kcl files.
        let parent = filepath
            .parent()
            .ok_or_else(|| anyhow!("Could not get parent directory to: `{}`", filepath.display()))?;
        let walked_kcl = kcl_lib::walk_dir(&parent.to_path_buf()).await?;

        // Get all the attachements async.
        let futures = walked_kcl
            .into_iter()
            .filter(|file| *file != filepath)
            .map(|file| {
                tokio::spawn(async move {
                    let contents = tokio::fs::read(&file)
                        .await
                        .map_err(|err| anyhow::anyhow!("Failed to read file `{}`: {:?}", file.display(), err))?;

                    Ok::<kittycad::types::multipart::Attachment, anyhow::Error>(
                        kittycad::types::multipart::Attachment {
                            name: file.to_string_lossy().to_string(),
                            filepath: Some(file),
                            content_type: Some("text/plain".parse()?),
                            data: contents,
                        },
                    )
                })
            })
            .collect::<Vec<_>>();

        // Join all futures and await their completion
        let results = futures::future::join_all(futures).await;

        // Check if any of the futures failed.
        let mut errors = Vec::new();
        for result in results {
            match result {
                Ok(Ok(attachment)) => {
                    files.push(attachment);
                }
                Ok(Err(err)) => {
                    errors.push(err);
                }
                Err(err) => {
                    errors.push(anyhow::anyhow!("Failed to join future: {:?}", err));
                }
            }
        }

        if !errors.is_empty() {
            anyhow::bail!("Failed to walk some kcl files: {:?}", errors);
        }

        Ok((files, filepath))
    }
}

impl Context<'_> {
    async fn spawn_reasoning_ws_task(
        &self,
        client: &kittycad::Client,
        id: uuid::Uuid,
        enable: bool,
    ) -> Option<tokio::task::JoinHandle<()>> {
        if !enable {
            return None;
        }

        match client.ml().reasoning_ws(id).await {
            Ok((upgraded, _headers)) => {
                let use_color = self.io.color_enabled() && self.io.is_stderr_tty();
                Some(tokio::spawn(async move {
                let mut ws = WebSocketStream::from_raw_socket(upgraded, Role::Client, None).await;
                while let Some(msg) = ws.next().await {
                    let Ok(msg) = msg else { break };
                    if msg.is_text() {
                        let txt = msg.into_text().unwrap_or_default();
                        if let Ok(server_msg) = serde_json::from_str::<kittycad::types::MlCopilotServerMessage>(&txt) {
                            if let kittycad::types::MlCopilotServerMessage::Reasoning(reason) = server_msg {
                                print_reasoning(reason, use_color);
                            }
                        }
                    }
                }
            }))
            }
            Err(err) => {
                if self.debug {
                    eprintln!("reasoning ws failed to connect for {id}: {err}");
                }
                None
            }
        }
    }
}

// Print only reasoning messages in a friendly, concise CLI format.
fn print_reasoning(reason: kittycad::types::ReasoningMessage, use_color: bool) {
    for line in format_reasoning(reason, use_color) {
        eprintln!("{}", line);
    }
}

fn format_reasoning(reason: kittycad::types::ReasoningMessage, use_color: bool) -> Vec<String> {
    use nu_ansi_term::Color;
    let lbl = |plain: &str, color: Color| -> String {
        if use_color { color.paint(plain).to_string() } else { plain.to_string() }
    };
    match reason {
        kittycad::types::ReasoningMessage::Text { content } => {
            vec![format!("{} {}", lbl("reasoning:", Color::Cyan), content.trim())]
        }
        kittycad::types::ReasoningMessage::KclDocs { content } => {
            vec![format!("{} {}", lbl("kcl docs:", Color::Purple), content.trim())]
        }
        kittycad::types::ReasoningMessage::KclCodeExamples { content } => {
            vec![format!("{} {}", lbl("kcl examples:", Color::Purple), content.trim())]
        }
        kittycad::types::ReasoningMessage::FeatureTreeOutline { content } => {
            vec![format!("{} {}", lbl("feature tree:", Color::Blue), content.trim())]
        }
        kittycad::types::ReasoningMessage::DesignPlan { steps } => {
            let mut v = vec![lbl("design plan:", Color::Cyan)];
            for (idx, step) in steps.iter().enumerate() {
                let n = if use_color { Color::Green.paint(format!("{:>2}.", idx + 1)).to_string() } else { format!("{:>2}.", idx + 1) };
                let file = if use_color { Color::Yellow.paint(&step.filepath_to_edit).to_string() } else { step.filepath_to_edit.clone() };
                v.push(format!("  {} {} {}", n, file, step.edit_instructions));
            }
            v
        }
        kittycad::types::ReasoningMessage::GeneratedKclCode { code } => {
            vec![lbl("generated kcl:", Color::Purple), indent_block(&code)]
        }
        kittycad::types::ReasoningMessage::KclCodeError { error } => {
            vec![format!("{} {}", lbl("kcl error:", Color::Red), error.trim())]
        }
        kittycad::types::ReasoningMessage::CreatedKclFile { file_name, content } => {
            let mut v = vec![format!("{} {}", lbl("created file:", Color::Green), file_name)];
            if !content.trim().is_empty() { v.push(indent_block(&content)); }
            v
        }
        kittycad::types::ReasoningMessage::UpdatedKclFile { file_name, content } => {
            let mut v = vec![format!("{} {}", lbl("updated file:", Color::Yellow), file_name)];
            if !content.trim().is_empty() { v.push(indent_block(&content)); }
            v
        }
        kittycad::types::ReasoningMessage::DeletedKclFile { file_name } => {
            vec![format!("{} {}", lbl("deleted file:", Color::Red), file_name)]
        }
    }
}

fn indent_block(s: &str) -> String {
    let mut out = String::new();
    for line in s.lines() {
        out.push_str("    ");
        out.push_str(line);
        out.push('\n');
    }
    out
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
                c.set("", "pager", Some(&t.pager)).unwrap();
            }

            if !t.prompt.is_empty() {
                c.set("", "prompt", Some(&t.prompt)).unwrap();
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

    #[test]
    fn test_format_reasoning_plain() {
        let lines = format_reasoning(
            kittycad::types::ReasoningMessage::Text { content: "hello world".into() },
            false,
        );
        assert_eq!(lines[0], "reasoning: hello world");

        let steps = vec![kittycad::types::PlanStep {
            edit_instructions: "add fillet".into(),
            filepath_to_edit: "main.kcl".into(),
        }];
        let lines = format_reasoning(kittycad::types::ReasoningMessage::DesignPlan { steps }, false);
        assert_eq!(lines[0], "design plan:");
        assert!(lines[1].contains("1."));
        assert!(lines[1].contains("main.kcl"));
        assert!(lines[1].contains("add fillet"));

        let lines = format_reasoning(
            kittycad::types::ReasoningMessage::GeneratedKclCode { code: "cube(1)".into() },
            false,
        );
        assert_eq!(lines[0], "generated kcl:");
        assert!(lines[1].starts_with("    "));
        assert!(lines[1].contains("cube(1)"));
    }

    #[test]
    fn test_format_reasoning_color() {
        // We don't assert exact ANSI sequences; just ensure something wraps when enabled.
        let lines = format_reasoning(
            kittycad::types::ReasoningMessage::KclCodeError { error: "boom".into() },
            true,
        );
        assert!(lines[0].contains("boom"));
        assert!(lines[0].contains("\u{1b}["), "expected ANSI color codes in colored output");
    }
}
