use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use tokio::{process::Command, time::sleep};
use url::Url;

#[cfg_attr(not(target_os = "windows"), ignore)]
#[tokio::test(flavor = "current_thread")]
async fn win_ca_cli_smoke() -> Result<()> {
    if !should_run() {
        eprintln!("WIN_CA_SMOKE not set; skipping Windows CA smoke test");
        return Ok(());
    }

    let binary = find_cli_binary()?;
    let config_dir = tempfile::tempdir().context("creating temporary config dir")?;

    let target_raw = std::env::var("SMOKE_URL").unwrap_or_else(|_| "https://127.0.0.1:4443/".to_string());
    let target_url = Url::parse(&target_raw).context("parsing SMOKE_URL")?;
    let host = host_from_url(&target_url)?;
    let endpoint = match std::env::var("SMOKE_ENDPOINT") {
        Ok(value) => value,
        Err(_) => {
            let path = target_url.path();
            if path.is_empty() {
                "/".to_string()
            } else {
                path.to_string()
            }
        }
    };

    let expected_key = std::env::var("SMOKE_EXPECTED_KEY").unwrap_or_else(|_| "status".to_string());
    let expected_value = std::env::var("SMOKE_EXPECTED_VALUE").unwrap_or_else(|_| "ok".to_string());
    let token = std::env::var("SMOKE_TOKEN")
        .or_else(|_| std::env::var("ZOO_TOKEN"))
        .or_else(|_| std::env::var("KITTYCAD_TOKEN"))
        .expect("SMOKE_TOKEN, ZOO_TOKEN, or KITTYCAD_TOKEN must be set for smoke test");

    let attempts = env_u32("SMOKE_ATTEMPTS").unwrap_or(60);
    let delay = Duration::from_millis(env_u64("SMOKE_DELAY_MS").unwrap_or(500));

    let mut last_error: Option<String> = None;

    for attempt in 0..attempts {
        let mut cmd = Command::new(&binary);
        cmd.arg("--host").arg(&host);
        cmd.arg("api").arg(&endpoint);
        cmd.env("ZOO_TOKEN", &token);
        cmd.env("ZOO_NO_UPDATE_NOTIFIER", "1");
        cmd.env("NO_COLOR", "1");
        cmd.env("ZOO_PAGER", "cat");
        cmd.env("ZOO_CONFIG_DIR", config_dir.path());
        cmd.env("CI", "true");
        if let Ok(extra) = std::env::var("NODE_EXTRA_CA_CERTS") {
            cmd.env("NODE_EXTRA_CA_CERTS", extra);
        }
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        match cmd.output().await {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8(output.stdout).context("stdout was not valid UTF-8")?;
                let json: serde_json::Value =
                    serde_json::from_str(&stdout).context("CLI response was not valid JSON")?;
                let actual = json.get(&expected_key).and_then(|val| val.as_str()).unwrap_or_default();
                if actual == expected_value {
                    return Ok(());
                } else {
                    last_error = Some(format!(
                        "CLI succeeded but response missing expected pair {expected_key:?}={expected_value:?}. Full JSON: {json}"));
                    break;
                }
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                last_error = Some(format!(
                    "Attempt {attempt} failed with status {:?}: stderr: {stderr}; stdout: {stdout}",
                    output.status.code()
                ));
            }
            Err(err) => {
                last_error = Some(format!("Attempt {attempt} failed to launch zoo: {err}"));
            }
        }

        sleep(delay).await;
    }

    Err(anyhow!(
        "Failed to reach CLI smoke target after {attempts} attempts: {}",
        last_error.unwrap_or_else(|| "no output captured".to_string())
    ))
}

fn should_run() -> bool {
    match std::env::var("WIN_CA_SMOKE") {
        Ok(val) => {
            let lower = val.to_ascii_lowercase();
            !(lower.is_empty() || lower == "0" || lower == "false")
        }
        Err(_) => false,
    }
}

fn env_u32(key: &str) -> Option<u32> {
    std::env::var(key).ok()?.parse().ok()
}

fn env_u64(key: &str) -> Option<u64> {
    std::env::var(key).ok()?.parse().ok()
}

fn find_cli_binary() -> Result<String> {
    if let Ok(path) = std::env::var("WIN_CA_ZOO_BIN") {
        return Ok(path);
    }

    if let Some(path) = option_env!("CARGO_BIN_EXE_zoo") {
        return Ok(path.to_string());
    }

    Err(anyhow!(
        "CARGO_BIN_EXE_zoo not set; build the zoo binary with cargo test"
    ))
}

fn host_from_url(url: &Url) -> Result<String> {
    let scheme = url.scheme();
    let host = url.host_str().ok_or_else(|| anyhow!("SMOKE_URL missing host"))?;
    let host_port = match url.port() {
        Some(port) => format!("{scheme}://{host}:{port}"),
        None => format!("{scheme}://{host}"),
    };
    Ok(host_port)
}
