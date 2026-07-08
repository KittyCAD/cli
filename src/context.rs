use std::{io::Write, path::Path, str::FromStr, time::Duration};

use anyhow::{Result, anyhow};
use camino::Utf8Path;
use futures::{SinkExt, StreamExt};
use kcl_lib::engine_connection::EngineManager;
use kittycad::types::{ApiCallStatus, AsyncApiCallOutput, TextToCad, TextToCadCreateBody, TextToCadMultiFileIteration};
use kittycad_modeling_cmds::{
    ModelingCmd, each_cmd as mcmd,
    output::TakeSnapshot,
    websocket::{
        FailureWebSocketResponse, ModelingCmdReq, ModelingSessionData, OkWebSocketResponseData, RawFile,
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

pub struct Context<'a> {
    pub config: &'a mut (dyn Config + Send + Sync + 'a),
    pub io: crate::iostreams::IoStreams,
    pub debug: bool,
    // If set, override the host used when commands don't specify one.
    pub(crate) override_host: Option<String>,
    /// Overrides retry behavior for KCL execution tests.
    #[cfg(test)]
    pub(crate) kcl_retry_config: Option<RetryConfig>,
}

/// Controls how retryable KCL execution failures are retried.
#[derive(Debug, Clone)]
pub(crate) struct RetryConfig {
    retries: usize,
    print_retries: bool,
}

impl RetryConfig {
    /// Returns retry settings that make each KCL execution attempt run only
    /// once.
    fn no_retries() -> Self {
        Self {
            retries: 0,
            print_retries: false,
        }
    }
}

#[cfg(test)]
impl Default for RetryConfig {
    /// Returns the retry settings used by tests that exercise retry handling.
    fn default() -> Self {
        Self {
            retries: 2,
            print_retries: true,
        }
    }
}

/// Result data captured from a single KCL program execution attempt.
struct KclProgramRun {
    exec_ctx: kcl_lib::ExecutorContext,
    exec_state: kcl_lib::ExecState,
    session_data: Option<ModelingSessionData>,
}

/// Error type used to preserve KCL diagnostics across retry attempts.
enum KclExecError {
    /// A KCL error that should be formatted with filename and source context.
    Err(Box<kcl_lib::KclError>),
    /// A KCL execution error that already includes command outputs.
    WithOutputs(Box<kcl_lib::KclErrorWithOutputs>),
    /// Any non-KCL error from setup or issue checking.
    Other(anyhow::Error),
}

impl KclExecError {
    /// Converts the stored error into the diagnostic form expected by callers.
    fn into_anyhow(self, filename: &str, code: &str) -> anyhow::Error {
        match self {
            Self::Err(err) => kcl_error_fmt::into_miette_for_parse(filename, code, *err),
            Self::WithOutputs(err) => kcl_error_fmt::into_miette(*err, code),
            Self::Other(err) => err,
        }
    }
}

impl std::fmt::Display for KclExecError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Err(err) => write!(formatter, "{err}"),
            Self::WithOutputs(err) => write!(formatter, "{err}"),
            Self::Other(err) => write!(formatter, "{err}"),
        }
    }
}

impl kcl_lib::IsRetryable for KclExecError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Err(err) => kcl_lib::IsRetryable::is_retryable(err.as_ref()),
            Self::WithOutputs(err) => kcl_lib::IsRetryable::is_retryable(err.as_ref()),
            Self::Other(_) => false,
        }
    }
}

/// Runs an async operation until it succeeds, fails fatally, or exhausts
/// retries.
#[cfg(test)]
async fn execute_with_retries<F, Fut, T, E>(config: &RetryConfig, mut execute: F) -> std::result::Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = std::result::Result<T, E>>,
    E: kcl_lib::IsRetryable + std::fmt::Display,
{
    let mut retries_remaining = config.retries;
    loop {
        let exec_result = execute().await;

        if should_retry_kcl_attempt(config, &mut retries_remaining, &exec_result) {
            continue;
        }

        return exec_result;
    }
}

/// Adjusts the retries remaining and returns whether a failed KCL attempt
/// should be retried. This handles all the book-keeping of retrying.
fn should_retry_kcl_attempt<T, E>(
    config: &RetryConfig,
    retries_remaining: &mut usize,
    result: &std::result::Result<T, E>,
) -> bool
where
    E: kcl_lib::IsRetryable + std::fmt::Display,
{
    if *retries_remaining > 0
        && let Err(error) = result
        && kcl_lib::IsRetryable::is_retryable(error)
    {
        if config.print_retries {
            eprintln!("Execute got {error}; retrying...");
        }
        *retries_remaining -= 1;
        true
    } else {
        false
    }
}

/// Runs one KCL execution attempt and returns state needed for follow-up
/// commands.
async fn run_kcl_program_once_with_client(
    client: &kittycad::Client,
    program: &kcl_lib::Program,
    settings: kcl_lib::ExecutorSettings,
) -> std::result::Result<KclProgramRun, KclExecError> {
    let ctx = kcl_lib::ExecutorContext::new(client, settings)
        .await
        .map_err(KclExecError::Other)?;
    let mut exec_state = kcl_lib::ExecState::new(&ctx);
    let session_data = ctx
        .run(program, &mut exec_state)
        .await
        .map_err(|err| KclExecError::WithOutputs(Box::new(err)))?
        .1;
    Ok(KclProgramRun {
        exec_ctx: ctx,
        exec_state,
        session_data,
    })
}

impl<'a> Context<'a> {
    /// Returns the retry settings to use for local KCL execution.
    fn kcl_retry_config(&self) -> RetryConfig {
        #[cfg(test)]
        {
            self.kcl_retry_config.clone().unwrap_or_else(RetryConfig::no_retries)
        }
        #[cfg(not(test))]
        {
            RetryConfig::no_retries()
        }
    }

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
            #[cfg(test)]
            kcl_retry_config: None,
        }
    }

    /// This function returns an API client for Zoo that is based on the configured
    /// user.
    pub fn api_client(&self, hostname: &str) -> Result<kittycad::Client> {
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
        let mut client = kittycad::Client::new_from_reqwest(token, http_client, ws_client);

        if baseurl != crate::DEFAULT_HOST {
            client.set_base_url(&baseurl);
        }

        Ok(client)
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
    ) -> Result<reqwest::Upgraded> {
        let client = self.api_client(hostname)?;
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
        Ok(ws)
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
        let ws = self.engine_ws_with_settings(hostname, &settings).await?;
        let wsconfig = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
            .max_message_size(Some(usize::MAX))
            .max_frame_size(Some(usize::MAX));
        let ws_stream = WebSocketStream::from_raw_socket(ws, Role::Client, Some(wsconfig)).await;
        let (mut write, mut read) = ws_stream.split();
        let mut session_data = None;
        let mut heartbeat =
            tokio::time::interval(Duration::from_secs(settings.heartbeats.unwrap_or(cmd_kcl::HEARTBEATS)));

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
        let retry_config = self.kcl_retry_config();
        let mut retries_remaining = retry_config.retries;
        loop {
            let result: std::result::Result<(OkWebSocketResponseData, Option<ModelingSessionData>), KclExecError> =
                async {
                    let run = run_kcl_program_once_with_client(&client, &program, settings.clone()).await?;

                    kcl_error_fmt::check_exec_state_issues(
                        &mut self.io.err_out,
                        filename,
                        code,
                        &run.exec_state,
                        issue_check,
                    )
                    .map_err(KclExecError::Other)?;

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
                        .map_err(|err| KclExecError::Err(Box::new(err)))?;

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
                        .map_err(|err| KclExecError::Err(Box::new(err)))?;
                    Ok((resp, run.session_data))
                }
                .await;

            if should_retry_kcl_attempt(&retry_config, &mut retries_remaining, &result) {
                continue;
            }

            return result.map_err(|err| err.into_anyhow(filename, code));
        }
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
        let retry_config = self.kcl_retry_config();
        let mut retries_remaining = retry_config.retries;
        loop {
            let result: std::result::Result<(Vec<OkWebSocketResponseData>, Option<ModelingSessionData>), KclExecError> =
                async {
                    let run = run_kcl_program_once_with_client(&client, &program, settings.clone()).await?;

                    kcl_error_fmt::check_exec_state_issues(
                        &mut self.io.err_out,
                        filename,
                        code,
                        &run.exec_state,
                        issue_check,
                    )
                    .map_err(KclExecError::Other)?;

                    let batch_context = kcl_lib::EngineBatchContext::new();
                    let mut responses = Vec::with_capacity(cmds.len());
                    for cmd in cmds.clone() {
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
                            .map_err(|err| KclExecError::Err(Box::new(err)))?;
                        responses.push(resp);
                    }

                    Ok((responses, run.session_data))
                }
                .await;

            if should_retry_kcl_attempt(&retry_config, &mut retries_remaining, &result) {
                continue;
            }

            return result.map_err(|err| err.into_anyhow(filename, code));
        }
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
        let retry_config = self.kcl_retry_config();
        let mut retries_remaining = retry_config.retries;
        loop {
            let result: std::result::Result<(Vec<TakeSnapshot>, Option<ModelingSessionData>), KclExecError> = async {
                let run = run_kcl_program_once_with_client(&client, &program, settings.clone()).await?;

                kcl_error_fmt::check_exec_state_issues(
                    &mut self.io.err_out,
                    filename,
                    code,
                    &run.exec_state,
                    issue_check,
                )
                .map_err(KclExecError::Other)?;

                let batch_context = kcl_lib::EngineBatchContext::new();
                let mut snapshot_resps = Vec::new();
                for snapshot_cmd in snapshot_cmds.clone() {
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
                        .map_err(|err| KclExecError::Err(Box::new(err)))?;
                    if let OkWebSocketResponseData::Modeling {
                        modeling_response:
                            kittycad_modeling_cmds::ok_response::OkModelingCmdResponse::TakeSnapshot(snap),
                    } = resp
                    {
                        snapshot_resps.push(snap);
                    }
                }

                Ok((snapshot_resps, run.session_data))
            }
            .await;

            if should_retry_kcl_attempt(&retry_config, &mut retries_remaining, &result) {
                continue;
            }

            return result.map_err(|err| err.into_anyhow(filename, code));
        }
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
        let retry_config = self.kcl_retry_config();
        let mut retries_remaining = retry_config.retries;
        loop {
            let result: std::result::Result<(Vec<RawFile>, Option<ModelingSessionData>), KclExecError> = async {
                let run = run_kcl_program_once_with_client(&client, program, settings.clone()).await?;

                kcl_error_fmt::check_exec_state_issues(
                    &mut self.io.err_out,
                    filename,
                    code,
                    &run.exec_state,
                    issue_check,
                )
                .map_err(KclExecError::Other)?;

                let files = run
                    .exec_ctx
                    .export(output_format.clone())
                    .await
                    .map_err(|err| KclExecError::Err(Box::new(err)))?;
                Ok((files, run.session_data))
            }
            .await;

            if should_retry_kcl_attempt(&retry_config, &mut retries_remaining, &result) {
                continue;
            }

            return result.map_err(|err| err.into_anyhow(filename, code));
        }
    }

    /// Create and poll a plain text-to-CAD job.
    ///
    /// `show_reasoning` is accepted to keep the command-facing API aligned with
    /// KCL edit generation, but this endpoint currently does not emit reasoning
    /// websocket messages or an `EndOfStream` marker. The parameter is ignored so
    /// text-to-CAD commands do not hang while waiting for reasoning output.
    pub async fn get_model_for_prompt(
        &mut self,
        hostname: &str,
        prompt: &str,
        kcl: bool,
        format: kittycad::types::FileExportFormat,
        _show_reasoning: bool,
    ) -> Result<TextToCad> {
        let client = self.api_client(hostname)?;

        // Create the text-to-cad request.
        let mut gen_model: TextToCad = client
            .ml()
            .create_text_to_cad(
                Some(kcl),
                format,
                &TextToCadCreateBody {
                    prompt: prompt.to_string(),
                    kcl_version: Some(kcl_lib::version().to_owned()),
                    project_name: None,
                    model_version: None,
                },
            )
            .await?;

        // Plain text-to-CAD generation does not currently emit reasoning websocket
        // messages or an EndOfStream marker. KCL edits still stream reasoning through
        // get_edit_for_prompt.

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
                anyhow::bail!("Unexpected response type: {result:?}");
            }

            status = gen_model.status.clone();

            // Wait for a bit before polling again.
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }

        // If the model failed we will want to tell the user.
        if gen_model.status == ApiCallStatus::Failed {
            if let Some(error) = gen_model.error {
                anyhow::bail!("Your prompt returned an error: ```\n{error}\n```");
            } else {
                anyhow::bail!("Your prompt returned an error, but no error message. :(");
            }
        }

        if gen_model.status != ApiCallStatus::Completed {
            anyhow::bail!("Your prompt timed out");
        }

        Ok(gen_model)
    }

    pub async fn get_edit_for_prompt(
        &mut self,
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
        let mut reasoning_guard = self
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
                anyhow::bail!("Unexpected response type: {result:?}");
            }

            reasoning_guard.drain(&mut self.io.err_out)?;
            status = gen_model.status.clone();

            // Wait for a bit before polling again.
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            reasoning_guard.drain(&mut self.io.err_out)?;
        }

        // Flush reasoning output before returning success or surfacing a terminal error.
        reasoning_guard.finish(&mut self.io.err_out).await?;

        // If the model failed we will want to tell the user.
        if gen_model.status == ApiCallStatus::Failed {
            if let Some(error) = gen_model.error {
                anyhow::bail!("Your prompt returned an error: ```\n{error}\n```");
            } else {
                anyhow::bail!("Your prompt returned an error, but no error message. :(");
            }
        }

        if gen_model.status != ApiCallStatus::Completed {
            anyhow::bail!("Your prompt timed out");
        }

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

        // Walk the containing directory and collect all the sibling kcl files. For a
        // relative input like `gear.kcl`, `parent()` is `Some("")`, which needs to be
        // treated as the current directory rather than an invalid path.
        let project_root = filepath.parent().ok_or_else(|| {
            let filepath_display = filepath.display().to_string();
            anyhow!("Could not get parent directory to: `{filepath_display}`")
        })?;
        let project_root = if project_root.as_os_str().is_empty() {
            std::path::PathBuf::from(".")
        } else {
            project_root.to_path_buf()
        };
        let walked_kcl = kcl_lib::walk_dir(&project_root).await?;
        let canonical_filepath = std::fs::canonicalize(&filepath).unwrap_or_else(|_| filepath.clone());

        // Get all the attachements async.
        let futures = walked_kcl
            .into_iter()
            .filter(|file| std::fs::canonicalize(file).unwrap_or_else(|_| file.clone()) != canonical_filepath)
            .map(|file| {
                tokio::spawn(async move {
                    let path_display = file.display().to_string();
                    let contents = tokio::fs::read(&file)
                        .await
                        .map_err(|err| anyhow!("Failed to read file `{path_display}`: {err:?}"))?;

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
                    errors.push(anyhow::anyhow!("Failed to join future: {err:?}"));
                }
            }
        }

        if !errors.is_empty() {
            anyhow::bail!("Failed to walk some kcl files: {errors:?}");
        }

        Ok((files, filepath))
    }
}

impl Context<'_> {
    async fn spawn_reasoning_ws_task(&self, client: &kittycad::Client, id: uuid::Uuid, enable: bool) -> ReasoningGuard {
        if !enable {
            return ReasoningGuard::default();
        }

        match client.ml().reasoning_ws(id).await {
            Ok((upgraded, _headers)) => {
                let use_color = self.io.color_enabled() && self.io.is_stderr_tty();
                let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
                let handle = tokio::spawn(async move {
                    let mut ws = WebSocketStream::from_raw_socket(upgraded, Role::Client, None).await;
                    while let Some(msg) = ws.next().await {
                        let Ok(msg) = msg else { break };
                        if msg.is_text() {
                            let txt = msg.into_text().unwrap_or_default();
                            if let Ok(server_msg) =
                                serde_json::from_str::<kittycad::types::MlCopilotServerMessage>(&txt)
                            {
                                match server_msg {
                                    kittycad::types::MlCopilotServerMessage::Reasoning(reason) => {
                                        for line in format_reasoning(reason, use_color) {
                                            if sender.send(line).is_err() {
                                                break;
                                            }
                                        }
                                    }
                                    kittycad::types::MlCopilotServerMessage::Error { detail } => {
                                        let _ = sender.send(format_copilot_error(&detail, use_color));
                                        // Do not break: errors may be non-fatal; keep streaming.
                                    }
                                    kittycad::types::MlCopilotServerMessage::EndOfStream { .. } => {
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                        } else if msg.is_close() {
                            break;
                        }
                    }
                    let _ = ws.close(None).await;
                });
                ReasoningGuard::new(handle, receiver)
            }
            Err(err) => {
                let _ = err; // suppress unused warning; intentionally silent
                ReasoningGuard::default()
            }
        }
    }
}

// RAII guard to ensure the reasoning websocket task is cancelled and joined.
#[derive(Default)]
struct ReasoningGuard {
    handle: Option<tokio::task::JoinHandle<()>>,
    receiver: Option<tokio::sync::mpsc::UnboundedReceiver<String>>,
}

impl ReasoningGuard {
    fn new(handle: tokio::task::JoinHandle<()>, receiver: tokio::sync::mpsc::UnboundedReceiver<String>) -> Self {
        Self {
            handle: Some(handle),
            receiver: Some(receiver),
        }
    }

    fn drain(&mut self, err_out: &mut dyn Write) -> Result<()> {
        let Some(receiver) = &mut self.receiver else {
            return Ok(());
        };
        while let Ok(line) = receiver.try_recv() {
            writeln!(err_out, "{line}")?;
        }
        Ok(())
    }

    async fn finish(mut self, err_out: &mut dyn Write) -> Result<()> {
        self.drain(err_out)?;
        if let Some(mut handle) = self.handle.take() {
            // KCL edit streams should finish by sending `EndOfStream`, so give
            // the task a short chance to drain final messages. Text-to-CAD
            // generation showed that not every endpoint closes the reasoning
            // stream, so this remains a brief best-effort grace period.
            if tokio::time::timeout(Duration::from_secs(1), &mut handle).await.is_err() {
                self.drain(err_out)?;
                handle.abort();
                let _ = handle.await;
            }
        }
        self.drain(err_out)
    }
}

impl Drop for ReasoningGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
            // Ensure the task is polled to completion in the background.
            tokio::spawn(async move {
                let _ = handle.await;
            });
        }
    }
}

pub(crate) fn format_reasoning(reason: kittycad::types::ReasoningMessage, use_color: bool) -> Vec<String> {
    use nu_ansi_term::Color;
    let lbl = |plain: &str, color: Color| -> String {
        if use_color {
            color.paint(plain).to_string()
        } else {
            plain.to_string()
        }
    };
    match reason {
        kittycad::types::ReasoningMessage::Text { content } => {
            vec![format!("{} {}", lbl("reasoning:", Color::Cyan), content.trim())]
        }
        kittycad::types::ReasoningMessage::Markdown { content } => {
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
                let n = if use_color {
                    Color::Green.paint(format!("{:>2}.", idx + 1)).to_string()
                } else {
                    format!("{:>2}.", idx + 1)
                };
                let file = if use_color {
                    Color::Yellow.paint(&step.filepath_to_edit).to_string()
                } else {
                    step.filepath_to_edit.clone()
                };
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
            if !content.trim().is_empty() {
                v.push(indent_block(&content));
            }
            v
        }
        kittycad::types::ReasoningMessage::UpdatedKclFile { file_name, content } => {
            let mut v = vec![format!("{} {}", lbl("updated file:", Color::Yellow), file_name)];
            if !content.trim().is_empty() {
                v.push(indent_block(&content));
            }
            v
        }
        kittycad::types::ReasoningMessage::DeletedKclFile { file_name } => {
            vec![format!("{} {}", lbl("deleted file:", Color::Red), file_name)]
        }
    }
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
            let meta = json!({ "action": "created", "file": file_name });
            let mut md = String::from("**Created File**\n\n");
            md.push_str("```json\n");
            md.push_str(&serde_json::to_string_pretty(&meta).unwrap_or_else(|_| meta.to_string()));
            md.push_str("\n```\n");
            if !content.trim().is_empty() {
                md.push_str("\n```kcl\n");
                md.push_str(content);
                md.push_str("\n```\n");
            }
            md
        }
        kittycad::types::ReasoningMessage::UpdatedKclFile { file_name, content } => {
            let meta = json!({ "action": "updated", "file": file_name });
            let mut md = String::from("**Updated File**\n\n");
            md.push_str("```json\n");
            md.push_str(&serde_json::to_string_pretty(&meta).unwrap_or_else(|_| meta.to_string()));
            md.push_str("\n```\n");
            if !content.trim().is_empty() {
                md.push_str("\n```kcl\n");
                md.push_str(content);
                md.push_str("\n```\n");
            }
            md
        }
        kittycad::types::ReasoningMessage::DeletedKclFile { file_name } => {
            let meta = json!({ "action": "deleted", "file": file_name });
            let mut md = String::from("**Deleted File**\n\n");
            md.push_str("```json\n");
            md.push_str(&serde_json::to_string_pretty(&meta).unwrap_or_else(|_| meta.to_string()));
            md.push_str("\n```\n");
            md
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
    let msg = serde_json::to_string(&request)?;
    write
        .send(WsMsg::Text(msg.into()))
        .await
        .map_err(|err| anyhow!("could not send request to engine websocket: {err}"))?;
    Ok(())
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

fn indent_block(s: &str) -> String {
    let mut out = String::new();
    for line in s.lines() {
        out.push_str("    ");
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn format_copilot_error(detail: &str, use_color: bool) -> String {
    use nu_ansi_term::Color;
    if use_color {
        format!("{} {}", Color::Red.paint("ml error:"), detail.trim())
    } else {
        format!("ml error: {}", detail.trim())
    }
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, sync::Arc};

    use pretty_assertions::assert_eq;

    use super::*;

    /// Test error used to verify retryable and fatal retry paths.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum RetryTestError {
        /// Error variant that should be retried.
        Retryable,
        /// Error variant that should stop retrying.
        Fatal,
    }

    impl std::fmt::Display for RetryTestError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Retryable => write!(formatter, "retryable"),
                Self::Fatal => write!(formatter, "fatal"),
            }
        }
    }

    impl kcl_lib::IsRetryable for RetryTestError {
        fn is_retryable(&self) -> bool {
            matches!(self, Self::Retryable)
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
    fn reasoning_to_markdown_text_has_no_header() {
        let md = super::reasoning_to_markdown(&kittycad::types::ReasoningMessage::Text {
            content: "Hello world".into(),
        });
        assert_eq!(md, "Hello world");
    }

    /// Verifies that retryable failures are retried until the operation
    /// succeeds.
    #[tokio::test]
    async fn execute_with_retries_retries_retryable_errors() {
        let retry_config = RetryConfig {
            retries: 2,
            print_retries: false,
        };
        let mut attempts = 0;

        let result: std::result::Result<&str, RetryTestError> = execute_with_retries(&retry_config, || {
            attempts += 1;
            let attempt = attempts;
            async move {
                if attempt < 3 {
                    Err(RetryTestError::Retryable)
                } else {
                    Ok("ok")
                }
            }
        })
        .await;

        assert_eq!(result, Ok("ok"));
        assert_eq!(attempts, 3);
    }

    /// Verifies that fatal failures are returned without retrying.
    #[tokio::test]
    async fn execute_with_retries_does_not_retry_fatal_errors() {
        let retry_config = RetryConfig {
            retries: 2,
            print_retries: false,
        };
        let mut attempts = 0;

        let result: std::result::Result<(), RetryTestError> = execute_with_retries(&retry_config, || {
            attempts += 1;
            async { Err(RetryTestError::Fatal) }
        })
        .await;

        assert_eq!(result, Err(RetryTestError::Fatal));
        assert_eq!(attempts, 1);
    }

    /// Verifies that retryable failures are not retried when retries are
    /// disabled.
    #[tokio::test]
    async fn execute_with_retries_no_retries_does_not_retry_retryable_errors() {
        let retry_config = RetryConfig::no_retries();
        let mut attempts = 0;

        let result: std::result::Result<(), RetryTestError> = execute_with_retries(&retry_config, || {
            attempts += 1;
            async { Err(RetryTestError::Retryable) }
        })
        .await;

        assert_eq!(result, Err(RetryTestError::Retryable));
        assert_eq!(attempts, 1);
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
            kcl_retry_config: None,
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

    #[tokio::test]
    #[serial_test::serial]
    async fn collect_kcl_files_uses_current_directory_for_relative_file_inputs() {
        let tmp = tempfile::tempdir().expect("failed to create temp dir");
        std::fs::write(tmp.path().join("gear.kcl"), "cube(1)\n").expect("write gear.kcl");

        let old_current_directory = std::env::current_dir().expect("current dir");
        std::env::set_current_dir(tmp.path()).expect("set current dir");

        let mut config = crate::config::new_blank_config().unwrap();
        let mut c = crate::config_from_env::EnvConfig::inherit_env(&mut config);
        let (io, _stdout_path, _stderr_path) = crate::iostreams::IoStreams::test();
        let mut ctx = Context {
            config: &mut c,
            io,
            debug: false,
            override_host: None,
            kcl_retry_config: None,
        };

        let (files, filepath) = ctx
            .collect_kcl_files(std::path::Path::new("gear.kcl"))
            .await
            .expect("collect relative project files");

        std::env::set_current_dir(old_current_directory).expect("restore current dir");

        assert_eq!(filepath, std::path::PathBuf::from("gear.kcl"));
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "gear.kcl");
        assert_eq!(files[0].filepath.as_deref(), Some(std::path::Path::new("gear.kcl")));
    }

    #[test]
    fn test_format_reasoning_plain() {
        let lines = format_reasoning(
            kittycad::types::ReasoningMessage::Text {
                content: "hello world".into(),
            },
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
        assert!(
            lines[0].contains("\u{1b}["),
            "expected ANSI color codes in colored output"
        );
    }
}
