use std::{collections::HashMap, path::Path, str::FromStr, time::Duration};

use anyhow::{Result, anyhow};
use camino::Utf8Path;
use futures::{SinkExt, StreamExt};
use kcl_lib::engine_connection::EngineManager;
use kittycad_modeling_cmds::{
    ModelingCmd, each_cmd as mcmd,
    output::TakeSnapshot,
    websocket::{
        ErrorCode, FailureWebSocketResponse, ModelingCmdReq, ModelingSessionData, OkWebSocketResponseData, RawFile,
        SuccessWebSocketResponse, WebSocketRequest, WebSocketResponse,
    },
};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{Message as WsMsg, protocol::Role},
};

use crate::{
    build_kcl_project::build_kcl_project, cmd_kcl, config::Config, config_file::get_env_var, kcl_error_fmt,
    types::FormatOutput,
};

type DirectWs = WebSocketStream<reqwest::Upgraded>;
type DirectWsRead = futures::stream::SplitStream<DirectWs>;
type DirectWsWrite = futures::stream::SplitSink<DirectWs, WsMsg>;

const ENGINE_EXECUTION_ENV: &str = "ENGINE_EXECUTION";
const WS_RESPONSE_TIMEOUT_SECS: u64 = 600;
const MAX_TRANSIENT_AUTH_MISSING_RESPONSES: usize = 8;

pub struct Context<'a> {
    pub config: &'a mut (dyn Config + Send + Sync + 'a),
    pub io: crate::iostreams::IoStreams,
    pub debug: bool,
    // If set, override the host used when commands don't specify one.
    pub(crate) override_host: Option<String>,
}

/// Result data captured from a KCL program execution.
struct KclProgramRun {
    exec_ctx: kcl_lib::ExecutorContext,
    exec_state: kcl_lib::ExecState,
    session_data: Option<ModelingSessionData>,
}

/// Runs a KCL program and returns state needed for follow-up commands.
async fn run_kcl_program(
    client: &kittycad::Client,
    program: &kcl_lib::Program,
    settings: kcl_lib::ExecutorSettings,
    code: &str,
) -> Result<KclProgramRun> {
    let exec_ctx = kcl_lib::ExecutorContext::new(client, settings).await?;
    let mut exec_state = kcl_lib::ExecState::new(&exec_ctx);
    let session_data = exec_ctx
        .run(program, &mut exec_state)
        .await
        .map_err(|err| kcl_error_fmt::into_miette(err, code))?
        .1;
    Ok(KclProgramRun {
        exec_ctx,
        exec_state,
        session_data,
    })
}

impl<'a> Context<'a> {
    fn resolve_api_host_and_baseurl(&self, hostname: &str) -> Result<(String, String)> {
        let host = if !hostname.is_empty() {
            hostname.to_string()
        } else if let Some(h) = &self.override_host {
            h.clone()
        } else {
            self.config.default_host()?
        };

        let mut baseurl = host.to_string();
        if !host.starts_with("http://") && !host.starts_with("https://") {
            baseurl = format!("https://{host}");
            if host.starts_with("localhost") {
                baseurl = format!("http://{host}")
            }
        }

        Ok((host, baseurl))
    }

    fn http_client_builder(&self) -> reqwest::ClientBuilder {
        let user_agent = concat!(env!("CARGO_PKG_NAME"), ".rs/", env!("CARGO_PKG_VERSION"),);
        reqwest::Client::builder()
            .user_agent(user_agent)
            .timeout(std::time::Duration::from_secs(600))
            .connect_timeout(std::time::Duration::from_secs(60))
    }

    pub fn new(config: &'a mut (dyn Config + Send + Sync)) -> Context<'a> {
        // Let's get our IO streams.
        let io = crate::iostreams::IoStreams::system();

        Context::new_with_io_and_env(config, io, |key| std::env::var(key))
    }

    fn new_with_io_and_env(
        config: &'a mut (dyn Config + Send + Sync),
        mut io: crate::iostreams::IoStreams,
        get_env_var: impl Fn(&str) -> std::result::Result<String, std::env::VarError>,
    ) -> Context<'a> {
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
        if let Ok(zoo_pager) = get_env_var("ZOO_PAGER") {
            io.set_pager(zoo_pager);
        } else {
            if let Ok(pager) = config.get("", "pager")
                && !pager.is_empty()
            {
                io.set_pager(pager);
            }
        }

        // Check if we should force use the tty.
        if let Ok(zoo_force_tty) = get_env_var("ZOO_FORCE_TTY")
            && !zoo_force_tty.is_empty()
        {
            io.force_terminal(&zoo_force_tty);
        }

        Context {
            config,
            io,
            debug: false,
            override_host: None,
        }
    }

    fn api_client_and_token(&self, hostname: &str) -> Result<(kittycad::Client, String)> {
        let (host, baseurl) = self.resolve_api_host_and_baseurl(hostname)?;

        let http_client = self.http_client_builder();
        let ws_client = self
            .http_client_builder()
            // For file conversions we need this to be long.
            .tcp_keepalive(std::time::Duration::from_secs(600))
            .http1_only();

        // Get the token for that host.
        let token = self.config.get(&host, "token")?;

        // Create the client.
        let mut client = kittycad::Client::new_from_reqwest(token.clone(), http_client, ws_client);

        if baseurl != crate::DEFAULT_HOST {
            client.set_base_url(&baseurl);
        }

        Ok((client, token))
    }

    /// This function returns an API client for Zoo that is based on the configured
    /// user.
    pub fn api_client(&self, hostname: &str) -> Result<kittycad::Client> {
        Ok(self.api_client_and_token(hostname)?.0)
    }

    pub fn raw_http_request(
        &self,
        hostname: &str,
        method: reqwest::Method,
        uri: &str,
    ) -> Result<reqwest::RequestBuilder> {
        let (host, baseurl) = self.resolve_api_host_and_baseurl(hostname)?;
        let token = self.config.get(&host, "token")?;
        let client = self.http_client_builder().build()?;
        let url = if uri.starts_with("https://") || uri.starts_with("http://") {
            uri.to_string()
        } else {
            format!("{}/{}", baseurl.trim_end_matches('/'), uri.trim_start_matches('/'))
        };

        Ok(client.request(method, url).bearer_auth(token).header(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json"),
        ))
    }

    /// Return the global host override if set.
    pub fn global_host(&self) -> Option<&str> {
        self.override_host.as_deref()
    }

    pub fn project_cloud_environment_name(&self, hostname: &str) -> Result<String> {
        let (_, baseurl) = self.resolve_api_host_and_baseurl(hostname)?;
        crate::project::project_cloud_environment_name_for_host(&baseurl)
    }

    // Test-only helper for verifying host resolution semantics without creating a client.
    #[cfg(test)]
    pub(crate) fn resolve_host_for_tests(&self, hostname: &str) -> Result<String> {
        if !hostname.is_empty() {
            Ok(hostname.to_string())
        } else if let Some(h) = &self.override_host {
            Ok(h.clone())
        } else {
            self.config.default_host()
        }
    }

    #[allow(dead_code)]
    pub async fn send_single_modeling_cmd(
        &self,
        hostname: &str,
        cmd: ModelingCmd,
        replay: Option<String>,
    ) -> Result<OkWebSocketResponseData> {
        let engine = self.engine(hostname, replay).await?;

        let batch_context = kcl_lib::EngineBatchContext::new();
        let resp = engine
            .send_modeling_cmd(
                &batch_context,
                uuid::Uuid::new_v4(),
                kcl_lib::SourceRange::default(),
                &cmd,
            )
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
        let pr = std::env::var("ZOO_ENGINE_PR").ok().and_then(|s| s.parse().ok());
        let unlocked_framerate = None;
        let video_res_height = None;
        let video_res_width = None;
        let (ws, _headers) = client
            .modeling()
            .commands_ws(kittycad::modeling::CommandsWsParams {
                api_call_id,
                fps,
                order_independent_transparency: Some(false),
                pool,
                post_effect,
                pr,
                replay,
                show_grid,
                unlocked_framerate,
                video_res_height,
                video_res_width,
                webrtc: Some(false),
            })
            .await?;
        Ok(ws)
    }

    pub async fn engine(&self, hostname: &str, replay: Option<String>) -> Result<EngineManager> {
        let ws = self.engine_ws(hostname, replay).await?;

        let engine = EngineManager::new_websocket_transport(ws, Some(cmd_kcl::HEARTBEATS)).await;

        Ok(engine)
    }

    /// Should KCL be executed on the server (true)?
    /// Or locally (false)?
    pub(crate) fn use_server_kcl_execution() -> bool {
        std::env::var(ENGINE_EXECUTION_ENV)
            .map(|value| !value.is_empty())
            .unwrap_or_default()
    }

    async fn engine_ws_with_settings(
        &self,
        hostname: &str,
        settings: &kcl_lib::ExecutorSettings,
    ) -> Result<(reqwest::Upgraded, String)> {
        let (client, token) = self.api_client_and_token(hostname)?;
        let pr = std::env::var("ZOO_ENGINE_PR").ok().and_then(|s| s.parse().ok());
        let (ws, _headers) = client
            .modeling()
            .commands_ws(kittycad::modeling::CommandsWsParams {
                api_call_id: None,
                fps: None,
                order_independent_transparency: None,
                pool: None,
                post_effect: if settings.enable_ssao {
                    Some(kittycad::types::PostEffectType::Ssao)
                } else {
                    None
                },
                pr,
                replay: settings.replay.clone(),
                show_grid: if settings.show_grid { Some(true) } else { None },
                unlocked_framerate: None,
                video_res_height: None,
                video_res_width: None,
                webrtc: Some(false),
            })
            .await?;
        Ok((ws, token))
    }

    /// Run this KCL on the server, then send some followup modeling commands
    /// (e.g. snapshots, exports, physics analysis) and report their results.
    pub(crate) async fn run_server_kcl_then_modeling_cmds(
        &mut self,
        hostname: &str,
        filepath: &Path,
        code: &str,
        cmds: Vec<ModelingCmd>,
        settings: kcl_lib::ExecutorSettings,
        issue_check: kcl_error_fmt::KclIssueCheck,
    ) -> Result<(Vec<OkWebSocketResponseData>, Option<ModelingSessionData>)> {
        let Some(filepath) = Utf8Path::from_path(filepath) else {
            anyhow::bail!("Invalid filepath {} (must be unicode)", filepath.display());
        };
        let project = build_kcl_project(filepath, code)?;
        let (ws, token) = self.engine_ws_with_settings(hostname, &settings).await?;
        let wsconfig = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
            .max_message_size(Some(usize::MAX))
            .max_frame_size(Some(usize::MAX));
        let ws_stream = WebSocketStream::from_raw_socket(ws, Role::Client, Some(wsconfig)).await;
        let (mut write, mut read) = ws_stream.split();
        let mut session_data = None;
        let mut heartbeat =
            tokio::time::interval(Duration::from_secs(settings.heartbeats.unwrap_or(cmd_kcl::HEARTBEATS)));
        let mut auth_missing_grace = MAX_TRANSIENT_AUTH_MISSING_RESPONSES;

        // Some Zoo credentials (including OAuth login tokens) must be forwarded
        // in-band after the websocket upgrade. API tokens can authenticate the
        // HTTP upgrade directly, but sending this request is valid for both
        // credential types and keeps server-side execution consistent with the
        // authentication already resolved by the CLI.
        send_ws_request(&mut write, websocket_auth_request(&token)).await?;

        let exec_request_id = uuid::Uuid::new_v4();
        send_ws_request(
            &mut write,
            WebSocketRequest::ExecKclProject {
                request_id: exec_request_id,
                project,
            },
        )
        .await?;

        // Handle engine responses, looking for KCL execution response.
        loop {
            let resp = read_ws_response_with_heartbeat(&mut read, &mut write, &mut heartbeat)
                .await
                .map_err(|e| anyhow!("During KCL execution, failed to communicate with engine: {e}"))?;
            if let Some(session) = update_session_data(&resp) {
                session_data = Some(session);
                continue;
            }
            if take_transient_auth_token_missing(&resp, &mut auth_missing_grace) {
                continue;
            }

            let success_resp = match resp {
                WebSocketResponse::Success(success) => success,
                WebSocketResponse::Failure(FailureWebSocketResponse { errors, .. }) => {
                    if errors.is_empty() {
                        anyhow::bail!("Failed executing KCL on engine, but the engine returned no error details")
                    } else {
                        let all_errors = errors
                            .into_iter()
                            .map(|error| error.message)
                            .collect::<Vec<_>>()
                            .join("\n");
                        anyhow::bail!("Failed executing KCL on engine, errors: {}", all_errors)
                    }
                }
            };

            if success_resp.request_id != Some(exec_request_id) {
                continue;
            }

            let OkWebSocketResponseData::ExecKclProject { result } = success_resp.resp else {
                anyhow::bail!(
                    "Expected ExecKclProject response, but engine returned {:?}",
                    success_resp.resp
                )
            };

            match result {
                Ok(_) => break,
                Err(err) => {
                    check_server_compilation_issues(&mut self.io.err_out, &err.non_fatal, issue_check)
                        .map_err(|e| anyhow!("KCL execution had errors: {e}"))?;
                    if let Some(error) = err.error {
                        return Err(anyhow!("KCL execution failed: {}", error.get_message()));
                    }
                    break;
                }
            }
        }

        // Send all follow-up commands, looking for each's response.
        let mut responses = Vec::with_capacity(cmds.len());
        for cmd in cmds {
            let cmd_id = uuid::Uuid::new_v4();
            send_ws_request(
                &mut write,
                WebSocketRequest::ModelingCmdReq(ModelingCmdReq {
                    cmd,
                    cmd_id: cmd_id.into(),
                }),
            )
            .await?;

            loop {
                let resp = read_ws_response_with_heartbeat(&mut read, &mut write, &mut heartbeat).await?;
                if let Some(session) = update_session_data(&resp) {
                    session_data = Some(session);
                    continue;
                }

                if response_request_id(&resp) != Some(cmd_id) {
                    continue;
                }

                match resp {
                    WebSocketResponse::Success(SuccessWebSocketResponse { resp, .. }) => {
                        responses.push(resp);
                        break;
                    }
                    WebSocketResponse::Failure(_) => return Err(websocket_failure_to_anyhow(resp)),
                }
            }
        }

        let _ = write.send(WsMsg::Close(None)).await;
        Ok((responses, session_data))
    }

    pub async fn send_kcl_modeling_cmd(
        &mut self,
        hostname: &str,
        filename: &str,
        code: &str,
        cmd: kittycad_modeling_cmds::ModelingCmd,
        settings: kcl_lib::ExecutorSettings,
        issue_check: kcl_error_fmt::KclIssueCheck,
    ) -> Result<(OkWebSocketResponseData, Option<ModelingSessionData>)> {
        if Self::use_server_kcl_execution() {
            let (mut responses, session_data) = self
                .run_server_kcl_then_modeling_cmds(
                    hostname,
                    Path::new(filename),
                    code,
                    vec![
                        ModelingCmd::from(
                            mcmd::ZoomToFit::builder()
                                .animated(false)
                                .object_ids(Default::default())
                                .padding(0.1)
                                .build(),
                        ),
                        cmd,
                    ],
                    settings,
                    issue_check,
                )
                .await?;
            let resp = responses
                .pop()
                .ok_or_else(|| anyhow!("Expected response from engine after executing KCL"))?;
            return Ok((resp, session_data));
        }

        let client = self.api_client(hostname)?;

        let program = kcl_lib::Program::parse_no_errs(code)
            .map_err(|err| kcl_error_fmt::into_miette_for_parse(filename, code, err))?;

        let settings = cmd_kcl::with_heartbeats(settings);
        let run = run_kcl_program(&client, &program, settings, code).await?;

        kcl_error_fmt::check_exec_state_issues(&mut self.io.err_out, filename, code, &run.exec_state, issue_check)?;

        let batch_context = kcl_lib::EngineBatchContext::new();

        // Zoom on the object.
        run.exec_ctx
            .engine
            .send_modeling_cmd(
                &batch_context,
                uuid::Uuid::new_v4(),
                kcl_lib::SourceRange::default(),
                &ModelingCmd::from(
                    mcmd::ZoomToFit::builder()
                        .animated(false)
                        .object_ids(Default::default())
                        .padding(0.1)
                        .build(),
                ),
            )
            .await
            .map_err(|err| kcl_error_fmt::into_miette_for_parse(filename, code, err))?;

        let resp = run
            .exec_ctx
            .engine
            .send_modeling_cmd(
                &batch_context,
                uuid::Uuid::new_v4(),
                kcl_lib::SourceRange::default(),
                &cmd,
            )
            .await
            .map_err(|err| kcl_error_fmt::into_miette_for_parse(filename, code, err))?;
        Ok((resp, run.session_data))
    }

    pub(crate) async fn run_kcl_then_modeling_cmds(
        &mut self,
        hostname: &str,
        filename: &str,
        code: &str,
        cmds: Vec<kittycad_modeling_cmds::ModelingCmd>,
        settings: kcl_lib::ExecutorSettings,
        issue_check: kcl_error_fmt::KclIssueCheck,
    ) -> Result<(Vec<OkWebSocketResponseData>, Option<ModelingSessionData>)> {
        if Self::use_server_kcl_execution() {
            return self
                .run_server_kcl_then_modeling_cmds(hostname, Path::new(filename), code, cmds, settings, issue_check)
                .await;
        }

        let client = self.api_client(hostname)?;

        let program = kcl_lib::Program::parse_no_errs(code)
            .map_err(|err| kcl_error_fmt::into_miette_for_parse(filename, code, err))?;

        let settings = cmd_kcl::with_heartbeats(settings);
        let run = run_kcl_program(&client, &program, settings, code).await?;

        kcl_error_fmt::check_exec_state_issues(&mut self.io.err_out, filename, code, &run.exec_state, issue_check)?;

        let batch_context = kcl_lib::EngineBatchContext::new();
        let mut responses = Vec::with_capacity(cmds.len());
        for cmd in cmds {
            let resp = run
                .exec_ctx
                .engine
                .send_modeling_cmd(
                    &batch_context,
                    uuid::Uuid::new_v4(),
                    kcl_lib::SourceRange::default(),
                    &cmd,
                )
                .await
                .map_err(|err| kcl_error_fmt::into_miette_for_parse(filename, code, err))?;
            responses.push(resp);
        }

        Ok((responses, run.session_data))
    }

    /// Run the given KCL program, then after, run the given extra modeling commands.
    /// If any of those extra modeling commands were TakeSnapshot, return the snapshots.
    pub async fn run_kcl_then_snapshots(
        &mut self,
        hostname: &str,
        filename: &str,
        code: &str,
        snapshot_cmds: Vec<kittycad_modeling_cmds::ModelingCmd>,
        settings: kcl_lib::ExecutorSettings,
        issue_check: kcl_error_fmt::KclIssueCheck,
    ) -> Result<(Vec<TakeSnapshot>, Option<ModelingSessionData>)> {
        if Self::use_server_kcl_execution() {
            let (responses, session_data) = self
                .run_server_kcl_then_modeling_cmds(
                    hostname,
                    Path::new(filename),
                    code,
                    snapshot_cmds,
                    settings,
                    issue_check,
                )
                .await?;
            let mut snapshot_resps = Vec::new();
            for resp in responses {
                if let OkWebSocketResponseData::Modeling {
                    modeling_response: kittycad_modeling_cmds::ok_response::OkModelingCmdResponse::TakeSnapshot(snap),
                } = resp
                {
                    snapshot_resps.push(snap);
                }
            }

            return Ok((snapshot_resps, session_data));
        }

        let client = self.api_client(hostname)?;

        let program = kcl_lib::Program::parse_no_errs(code)
            .map_err(|err| kcl_error_fmt::into_miette_for_parse(filename, code, err))?;

        let settings = cmd_kcl::with_heartbeats(settings);
        let run = run_kcl_program(&client, &program, settings, code).await?;

        kcl_error_fmt::check_exec_state_issues(&mut self.io.err_out, filename, code, &run.exec_state, issue_check)?;

        let batch_context = kcl_lib::EngineBatchContext::new();
        let mut snapshot_resps = Vec::new();
        for snapshot_cmd in snapshot_cmds {
            let resp = run
                .exec_ctx
                .engine
                .send_modeling_cmd(
                    &batch_context,
                    uuid::Uuid::new_v4(),
                    kcl_lib::SourceRange::default(),
                    &snapshot_cmd,
                )
                .await
                .map_err(|err| kcl_error_fmt::into_miette_for_parse(filename, code, err))?;
            if let OkWebSocketResponseData::Modeling {
                modeling_response: kittycad_modeling_cmds::ok_response::OkModelingCmdResponse::TakeSnapshot(snap),
            } = resp
            {
                snapshot_resps.push(snap);
            }
        }

        Ok((snapshot_resps, run.session_data))
    }

    /// Runs KCL, checks execution issues, and exports the resulting files.
    pub(crate) async fn run_kcl_then_export(
        &mut self,
        filename: &str,
        code: &str,
        program: &kcl_lib::Program,
        settings: kcl_lib::ExecutorSettings,
        issue_check: kcl_error_fmt::KclIssueCheck,
        output_format: kittycad_modeling_cmds::format::OutputFormat3d,
    ) -> Result<(Vec<RawFile>, Option<ModelingSessionData>)> {
        let client = self.api_client("")?;
        let run = run_kcl_program(&client, program, settings, code).await?;

        kcl_error_fmt::check_exec_state_issues(&mut self.io.err_out, filename, code, &run.exec_state, issue_check)?;

        let files = run
            .exec_ctx
            .export(output_format)
            .await
            .map_err(|err| kcl_error_fmt::into_miette_for_parse(filename, code, err))?;
        Ok((files, run.session_data))
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
                return Err(anyhow!("An error occurred when opening '{url}': {err}"));
            }
        } else if let Err(err) = open::with(url, &browser) {
            return Err(anyhow!(
                "An error occurred when opening '{url}' with browser '{browser}' configured from '{source}': {err}"
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
            anyhow::bail!("File '{filename}' does not exist.");
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
            if path.to_str().unwrap_or("-") != "-"
                && let Some(ext) = path.extension()
                && ext != "kcl"
            {
                return Err(anyhow::anyhow!("File must have a .kcl extension"));
            }
        }

        let b = self.read_file(path.to_str().unwrap_or("-"))?;
        // Parse the input as a string.
        let code = std::str::from_utf8(&b)?;
        Ok((code.to_string(), path))
    }
}

#[derive(serde::Serialize)]
struct FileReasoningMetadata<'a> {
    action: &'static str,
    file: &'a str,
}

fn file_reasoning_to_markdown(
    title: &str,
    action: &'static str,
    file_name: &str,
    content: Option<(&str, &str)>,
) -> String {
    let metadata = FileReasoningMetadata {
        action,
        file: file_name,
    };
    let Ok(metadata) = serde_json::to_string_pretty(&metadata) else {
        return format!("**{title}**\n\n{action} `{file_name}`");
    };

    let mut markdown = format!("**{title}**\n\n```json\n{metadata}\n```\n");
    if let Some((language, content)) = content.filter(|(_, content)| !content.trim().is_empty()) {
        markdown.push_str(&format!("\n```{language}\n{content}\n```\n"));
    }
    markdown
}

/// Render a ReasoningMessage as Markdown with a bold header and
/// pretty-printed structured content. Intended for Copilot UI rendering.
pub(crate) fn reasoning_to_markdown(reason: &kittycad::types::ReasoningMessage) -> String {
    use serde_json::json;

    match reason {
        kittycad::types::ReasoningMessage::Text { content } => content.trim().to_string(),
        kittycad::types::ReasoningMessage::Markdown { content } => content.trim().to_string(),
        kittycad::types::ReasoningMessage::KclDocs { content } => {
            format!("**KCL Docs**\n\n{}", content.trim())
        }
        kittycad::types::ReasoningMessage::KclCodeExamples { content } => {
            format!("**KCL Examples**\n\n{}", content.trim())
        }
        kittycad::types::ReasoningMessage::FeatureTreeOutline { content } => {
            format!("**Feature Tree**\n\n{}", content.trim())
        }
        kittycad::types::ReasoningMessage::DesignPlan { steps } => {
            let mut md = String::from("**Design Plan**\n");
            for step in steps {
                let obj = json!({
                    "file": step.filepath_to_edit,
                    "edit_instructions": step.edit_instructions,
                });
                let pretty = serde_json::to_string_pretty(&obj).unwrap_or_else(|_| obj.to_string());
                md.push_str("\n```json\n");
                md.push_str(&pretty);
                md.push_str("\n```\n");
            }
            md
        }
        kittycad::types::ReasoningMessage::GeneratedKclCode { code } => {
            // Keep as fenced code for readability; UI flattens to lines.
            let mut md = String::from("**Generated KCL**\n\n");
            md.push_str("```kcl\n");
            md.push_str(code);
            md.push_str("\n```\n");
            md
        }
        kittycad::types::ReasoningMessage::KclCodeError { error } => {
            let mut md = String::from("**KCL Error**\n\n");
            md.push_str("```text\n");
            md.push_str(error.trim());
            md.push_str("\n```\n");
            md
        }
        kittycad::types::ReasoningMessage::CreatedKclFile { file_name, content } => {
            file_reasoning_to_markdown("Created File", "created", file_name, Some(("kcl", content)))
        }
        kittycad::types::ReasoningMessage::UpdatedKclFile { file_name, content } => {
            file_reasoning_to_markdown("Updated File", "updated", file_name, Some(("kcl", content)))
        }
        kittycad::types::ReasoningMessage::DeletedKclFile { file_name } => {
            file_reasoning_to_markdown("Deleted File", "deleted", file_name, None)
        }
        kittycad::types::ReasoningMessage::CreatedProjectFile { file_name, content } => {
            file_reasoning_to_markdown("Created Project File", "created", file_name, Some(("text", content)))
        }
        kittycad::types::ReasoningMessage::UpdatedProjectFile { file_name, content } => {
            file_reasoning_to_markdown("Updated Project File", "updated", file_name, Some(("text", content)))
        }
        kittycad::types::ReasoningMessage::DeletedProjectFile { file_name } => {
            file_reasoning_to_markdown("Deleted Project File", "deleted", file_name, None)
        }
    }
}

fn check_server_compilation_issues(
    err_out: &mut impl std::io::Write,
    issues: &[kcl_error::CompilationIssue],
    issue_check: kcl_error_fmt::KclIssueCheck,
) -> Result<()> {
    if issue_check == kcl_error_fmt::KclIssueCheck::Ignore || issues.is_empty() {
        return Ok(());
    }

    for issue in issues {
        writeln!(err_out, "{:?}: {}", issue.severity, issue.message)?;
    }

    if issue_check == kcl_error_fmt::KclIssueCheck::DenyErrors && issues.iter().any(|issue| issue.is_err()) {
        anyhow::bail!(
            "KCL execution reported errors. Please fix your KCL program before continuing. If you really want to proceed anyway, rerun this command with `--allow-errors`."
        );
    }

    Ok(())
}

async fn send_ws_request(write: &mut DirectWsWrite, request: WebSocketRequest) -> Result<()> {
    let msg = encode_ws_request(&request)?;
    write
        .send(msg)
        .await
        .map_err(|err| anyhow!("could not send request to engine websocket: {err}"))?;
    Ok(())
}

fn encode_ws_request(request: &WebSocketRequest) -> Result<WsMsg> {
    if matches!(request, WebSocketRequest::ExecKclProject { .. }) {
        Ok(WsMsg::Binary(rmp_serde::to_vec_named(request)?.into()))
    } else {
        Ok(WsMsg::Text(serde_json::to_string(request)?.into()))
    }
}

fn websocket_auth_request(token: &str) -> WebSocketRequest {
    let mut headers = HashMap::new();
    headers.insert("Authorization".to_owned(), format!("Bearer {token}"));
    WebSocketRequest::Headers { headers }
}

fn is_transient_auth_token_missing(response: &WebSocketResponse) -> bool {
    matches!(
        response,
        WebSocketResponse::Failure(FailureWebSocketResponse {
            request_id: None,
            errors,
            ..
        }) if !errors.is_empty()
            && errors
                .iter()
                .all(|error| error.error_code == ErrorCode::AuthTokenMissing)
    )
}

fn take_transient_auth_token_missing(response: &WebSocketResponse, remaining: &mut usize) -> bool {
    if *remaining == 0 || !is_transient_auth_token_missing(response) {
        return false;
    }
    *remaining -= 1;
    true
}

async fn read_ws_response_with_heartbeat(
    read: &mut DirectWsRead,
    write: &mut DirectWsWrite,
    heartbeat: &mut tokio::time::Interval,
) -> Result<WebSocketResponse> {
    let timeout = tokio::time::sleep(Duration::from_secs(WS_RESPONSE_TIMEOUT_SECS));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            maybe_msg = read.next() => {
                let Some(msg) = maybe_msg else {
                    anyhow::bail!("engine websocket closed before sending a response");
                };
                return parse_ws_msg(msg?);
            }
            _ = heartbeat.tick() => {
                send_ws_request(write, WebSocketRequest::Ping {}).await?;
            }
            _ = &mut timeout => {
                anyhow::bail!("engine websocket response timed out after {WS_RESPONSE_TIMEOUT_SECS}s");
            }
        }
    }
}

fn parse_ws_msg(msg: WsMsg) -> Result<WebSocketResponse> {
    match msg {
        WsMsg::Text(text) => Ok(serde_json::from_str(&text)?),
        WsMsg::Binary(bin) => Ok(rmp_serde::from_slice(&bin)?),
        other => anyhow::bail!("unexpected engine websocket message: {other}"),
    }
}

fn update_session_data(response: &WebSocketResponse) -> Option<ModelingSessionData> {
    match response {
        WebSocketResponse::Success(SuccessWebSocketResponse {
            resp: OkWebSocketResponseData::ModelingSessionData { session },
            ..
        }) => Some(session.clone()),
        _ => None,
    }
}

fn response_request_id(response: &WebSocketResponse) -> Option<uuid::Uuid> {
    match response {
        WebSocketResponse::Success(SuccessWebSocketResponse { request_id, .. }) => *request_id,
        WebSocketResponse::Failure(FailureWebSocketResponse { request_id, .. }) => *request_id,
    }
}

fn websocket_failure_to_anyhow(response: WebSocketResponse) -> anyhow::Error {
    match response {
        WebSocketResponse::Failure(FailureWebSocketResponse { errors, .. }) => {
            if errors.is_empty() {
                anyhow!("engine websocket request failed with no error details")
            } else {
                anyhow!(
                    "{}",
                    errors
                        .into_iter()
                        .map(|error| error.message)
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            }
        }
        other => anyhow!("unexpected engine websocket response: {other:?}"),
    }
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, sync::Arc};

    use pretty_assertions::assert_eq;

    use super::*;

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

    struct TestEnvConfig<'a> {
        config: &'a mut (dyn crate::config::Config + 'a),
        env: Arc<HashMap<String, String>>,
    }

    impl TestEnvConfig<'_> {
        fn get_env_var(&self, key: &str) -> String {
            self.env.get(key).cloned().unwrap_or_default()
        }
    }

    impl crate::config::Config for TestEnvConfig<'_> {
        fn get(&self, hostname: &str, key: &str) -> Result<String> {
            let (val, _) = self.get_with_source(hostname, key)?;
            Ok(val)
        }

        fn get_with_source(&self, hostname: &str, key: &str) -> Result<(String, String)> {
            if key == "token" {
                let token = self.get_env_var("ZOO_API_TOKEN");
                let token = if token.is_empty() {
                    self.get_env_var("ZOO_TOKEN")
                } else {
                    token
                };
                if !token.is_empty() {
                    return Ok((token, "ZOO_API_TOKEN".to_string()));
                }
            } else {
                let var = format!("ZOO_{}", heck::AsShoutySnakeCase(key));
                let val = self.get_env_var(&var);
                if !val.is_empty() {
                    return Ok((val, var));
                }
            }

            self.config.get_with_source(hostname, key)
        }

        fn set(&mut self, hostname: &str, key: &str, value: Option<&str>) -> Result<()> {
            self.config.set(hostname, key, value)
        }

        fn unset_host(&mut self, key: &str) -> Result<()> {
            self.config.unset_host(key)
        }

        fn hosts(&self) -> Result<Vec<String>> {
            self.config.hosts()
        }

        fn default_host(&self) -> Result<String> {
            let (host, _) = self.default_host_with_source()?;
            Ok(host)
        }

        fn default_host_with_source(&self) -> Result<(String, String)> {
            if let Some(host) = self.env.get("ZOO_HOST") {
                Ok((host.clone(), "ZOO_HOST".to_string()))
            } else {
                self.config.default_host_with_source()
            }
        }

        fn aliases(&mut self) -> Result<crate::config_alias::AliasConfig<'_>> {
            self.config.aliases()
        }

        fn save_aliases(&mut self, aliases: &crate::config_map::ConfigMap) -> Result<()> {
            self.config.save_aliases(aliases)
        }

        fn expand_alias(&mut self, args: Vec<String>) -> Result<(Vec<String>, bool)> {
            self.config.expand_alias(args)
        }

        fn check_writable(&self, hostname: &str, key: &str) -> Result<()> {
            if key == "token" {
                let token = self.get_env_var("ZOO_API_TOKEN");
                let token = if token.is_empty() {
                    self.get_env_var("ZOO_TOKEN")
                } else {
                    token
                };
                if !token.is_empty() {
                    return Err(
                        crate::config_from_env::ReadOnlyEnvVarError::Variable("ZOO_API_TOKEN".to_string()).into(),
                    );
                }
            }

            self.config.check_writable(hostname, key)
        }

        fn write(&self) -> Result<()> {
            self.config.write()
        }

        fn config_to_string(&self) -> Result<String> {
            self.config.config_to_string()
        }

        fn hosts_to_string(&self) -> Result<String> {
            self.config.hosts_to_string()
        }
    }

    #[test]
    fn test_context() {
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
            let mut env = HashMap::new();

            if !t.zoo_pager_env.is_empty() {
                env.insert("ZOO_PAGER".to_string(), t.zoo_pager_env.clone());
            }

            if !t.zoo_force_tty_env.is_empty() {
                env.insert("ZOO_FORCE_TTY".to_string(), t.zoo_force_tty_env.clone());
            }

            let env = Arc::new(env);
            let context_env = Arc::clone(&env);
            let mut c = TestEnvConfig {
                config: &mut config,
                env,
            };

            if !t.pager.is_empty() {
                c.set("", "pager", Some(&t.pager)).unwrap();
            }

            if !t.prompt.is_empty() {
                c.set("", "prompt", Some(&t.prompt)).unwrap();
            }

            let (io, _stdout_path, _stderr_path) = crate::iostreams::IoStreams::test();
            let ctx = Context::new_with_io_and_env(&mut c, io, move |key| {
                context_env.get(key).cloned().ok_or(std::env::VarError::NotPresent)
            });

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
    fn only_uncorrelated_auth_token_missing_is_treated_as_transient() {
        use kittycad_modeling_cmds::websocket::ApiError;

        let missing = ApiError {
            error_code: ErrorCode::AuthTokenMissing,
            message: "send authentication headers".to_owned(),
        };
        let invalid = ApiError {
            error_code: ErrorCode::AuthTokenInvalid,
            message: "invalid authentication token".to_owned(),
        };

        let missing_response = WebSocketResponse::failure(None, vec![missing.clone()]);
        let mut remaining = MAX_TRANSIENT_AUTH_MISSING_RESPONSES;
        for _ in 0..MAX_TRANSIENT_AUTH_MISSING_RESPONSES {
            assert!(take_transient_auth_token_missing(&missing_response, &mut remaining));
        }
        assert!(!take_transient_auth_token_missing(&missing_response, &mut remaining));
        assert!(!is_transient_auth_token_missing(&WebSocketResponse::failure(
            Some(uuid::Uuid::new_v4()),
            vec![missing.clone()],
        )));
        assert!(!is_transient_auth_token_missing(&WebSocketResponse::failure(
            None,
            vec![invalid.clone()],
        )));
        assert!(!is_transient_auth_token_missing(&WebSocketResponse::failure(
            None,
            vec![missing, invalid],
        )));
        assert!(!is_transient_auth_token_missing(&WebSocketResponse::failure(
            None,
            Vec::new(),
        )));
    }

    #[test]
    fn configured_token_becomes_text_websocket_auth_header_without_environment_override() {
        let host = crate::cmd_auth::parse_host(crate::DEFAULT_HOST).unwrap().to_string();
        let mut config = crate::config::new_blank_config().unwrap();
        config.set(&host, "token", Some("configured-oauth-token")).unwrap();
        config.set(&host, "default", Some("true")).unwrap();
        let mut c = TestEnvConfig {
            config: &mut config,
            env: Arc::new(HashMap::new()),
        };
        let (io, _stdout_path, _stderr_path) = crate::iostreams::IoStreams::test();
        let ctx = Context {
            config: &mut c,
            io,
            debug: false,
            override_host: None,
        };

        let (_client, token) = ctx.api_client_and_token("").unwrap();
        let request = websocket_auth_request(&token);
        let WsMsg::Text(encoded) = encode_ws_request(&request).unwrap() else {
            panic!("websocket authentication must be sent as JSON text");
        };

        assert_eq!(
            serde_json::from_str::<serde_json::Value>(encoded.as_ref()).unwrap(),
            serde_json::json!({
                "type": "headers",
                "headers": {
                    "Authorization": "Bearer configured-oauth-token",
                },
            })
        );
    }

    #[test]
    fn exec_kcl_project_is_sent_as_named_messagepack() {
        use kittycad_modeling_cmds::{
            exec_kcl::{KclFile, KclProject},
            shared::safe_filepath::SafeFilepath,
        };

        let entrypoint = SafeFilepath::validate("main.kcl").unwrap();
        let project = KclProject::new(vec![KclFile::new(entrypoint.clone(), b"cube = 1".to_vec())], entrypoint);
        let expected_project = project.clone();
        let expected_request_id = uuid::Uuid::new_v4();
        let request = WebSocketRequest::ExecKclProject {
            request_id: expected_request_id,
            project,
        };

        let WsMsg::Binary(encoded) = encode_ws_request(&request).unwrap() else {
            panic!("ExecKclProject must be sent as MessagePack binary");
        };
        let decoded: WebSocketRequest = rmp_serde::from_slice(encoded.as_ref()).unwrap();

        let WebSocketRequest::ExecKclProject { request_id, project } = decoded else {
            panic!("decoded request was not ExecKclProject");
        };
        assert_eq!(request_id, expected_request_id);
        assert_eq!(project, expected_project);
    }

    #[test]
    fn reasoning_to_markdown_text_has_no_header() {
        let md = super::reasoning_to_markdown(&kittycad::types::ReasoningMessage::Text {
            content: "Hello world".into(),
        });
        assert_eq!(md, "Hello world");
    }

    #[test]
    fn project_file_reasoning_is_rendered() {
        let md = super::reasoning_to_markdown(&kittycad::types::ReasoningMessage::UpdatedProjectFile {
            content: "updated notes".into(),
            file_name: "notes.txt".into(),
        });
        assert!(md.contains("**Updated Project File**"));
        assert!(md.contains("\"file\": \"notes.txt\""));
        assert!(md.contains("```text\nupdated notes\n```"));
    }

    #[test]
    fn resolve_host_prefers_explicit_then_global() {
        let mut config = crate::config::new_blank_config().unwrap();
        let mut c = crate::config_from_env::EnvConfig::inherit_env(&mut config);
        let (io, _stdout_path, _stderr_path) = crate::iostreams::IoStreams::test();
        let mut ctx = Context {
            config: &mut c,
            io,
            debug: false,
            override_host: None,
        };

        // No override: falls back to default host in config (which will be DEFAULT_HOST initially)
        let h = ctx.resolve_host_for_tests("").unwrap();
        assert!(!h.is_empty());

        // Set global override
        ctx.override_host = Some("http://localhost:7777".to_string());
        let h2 = ctx.resolve_host_for_tests("").unwrap();
        assert_eq!(h2, "http://localhost:7777");

        // Explicit arg overrides global
        let h3 = ctx.resolve_host_for_tests("http://foo:1234").unwrap();
        assert_eq!(h3, "http://foo:1234");
    }
}
