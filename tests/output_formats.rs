//! Exercise output selection through the CLI with local HTTP/WebSocket servers.

use std::{path::Path, process::Output, time::Duration};

use futures::{SinkExt, StreamExt};
use kittycad_modeling_cmds::{
    ModelingCmd,
    exec_kcl::ExecKclProjectOk,
    websocket::{OkWebSocketResponseData, SuccessWebSocketResponse, WebSocketRequest},
};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    process::Command,
};
use tokio_tungstenite::tungstenite::Message;

const CALL_ID: &str = "8fea640b-1257-4b44-94b3-66061dd2f3dc";
const TIMEOUT: Duration = Duration::from_secs(15);

async fn run_zoo(dir: &Path, args: &[&str], host: &str, configured: Option<&str>) -> Output {
    let config = tempfile::tempdir().unwrap();
    if let Some(format) = configured {
        std::fs::write(
            config.path().join("config.toml"),
            format!("format = {format:?}\nprompt = \"disabled\"\n"),
        )
        .unwrap();
    }
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_zoo"));
    cmd.args(args)
        .current_dir(dir)
        .env("ZOO_CONFIG_DIR", config.path())
        .env("ZOO_API_TOKEN", "local-test-token")
        .env("ZOO_HOST", host)
        .env("ENGINE_EXECUTION", "1")
        .env("ZOO_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1")
        .env_remove("DEBUG")
        .env_remove("ZOO_FORMAT")
        .env_remove("ZOO_FORCE_TTY")
        .kill_on_drop(true);
    tokio::time::timeout(TIMEOUT, cmd.output()).await.unwrap().unwrap()
}

fn assert_success(output: &Output) {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

fn assert_formatted(output: &Output, format: &str, expected: &Value, table_marker: &str) {
    assert_success(output);
    match format {
        "json" => assert_eq!(serde_json::from_slice::<Value>(&output.stdout).unwrap(), *expected),
        "yaml" => {
            assert_eq!(serde_yaml::from_slice::<Value>(&output.stdout).unwrap(), *expected);
            // JSON is also valid YAML, so parsing alone would miss hardcoded JSON output.
            assert!(serde_json::from_slice::<Value>(&output.stdout).is_err());
        }
        "table" => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(stdout.contains(table_marker), "{stdout}");
            assert!(serde_json::from_slice::<Value>(&output.stdout).is_err());
        }
        _ => panic!("unexpected format"),
    }
}

const FORMATS: &[(Option<&str>, Option<&str>, &str)] = &[
    (None, None, "table"),
    (None, Some("json"), "json"),
    (None, Some("yaml"), "yaml"),
    (Some("json"), None, "json"),
    (Some("yaml"), None, "yaml"),
    (Some("yaml"), Some("json"), "json"),
    (Some("json"), Some("table"), "table"),
];

async fn serve_bounding_box(listener: TcpListener) {
    let (stream, _) = listener.accept().await.unwrap();
    let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
    while let Some(msg) = ws.next().await {
        let msg = msg.unwrap();
        if msg.is_close() {
            break;
        }
        let request: WebSocketRequest = serde_json::from_str(msg.to_text().unwrap()).unwrap();
        let (request_id, resp) = match request {
            WebSocketRequest::Ping {} => continue,
            WebSocketRequest::ExecKclProject { request_id, project } => {
                assert_eq!(project.files.len(), 1);
                (
                    request_id,
                    OkWebSocketResponseData::ExecKclProject {
                        result: Ok(ExecKclProjectOk::builder().artifact_graph(Default::default()).build()),
                    },
                )
            }
            WebSocketRequest::ModelingCmdReq(request) => {
                let response = match request.cmd {
                    ModelingCmd::ZoomToFit(_) => json!({
                        "type": "zoom_to_fit", "data": {"settings": {
                            "pos": {"x": 0, "y": -100, "z": 0},
                            "center": {"x": 0, "y": 0, "z": 0},
                            "up": {"x": 0, "y": 0, "z": 1},
                            "orientation": {"x": 0, "y": 0, "z": 0, "w": 1},
                            "ortho": true
                        }}
                    }),
                    ModelingCmd::BoundingBox(_) => json!({
                        "type": "bounding_box", "data": {
                            "center": {"x": -10, "y": 20, "z": 30},
                            "dimensions": {"x": 40, "y": 50, "z": 60}
                        }
                    }),
                    other => panic!("unexpected modeling command: {other:?}"),
                };
                (
                    request.cmd_id.into(),
                    OkWebSocketResponseData::Modeling {
                        modeling_response: serde_json::from_value(response).unwrap(),
                    },
                )
            }
            other => panic!("unexpected websocket request: {other:?}"),
        };
        let response = SuccessWebSocketResponse {
            success: true,
            request_id: Some(request_id),
            resp,
        };
        ws.send(Message::Text(serde_json::to_string(&response).unwrap().into()))
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn bounding_box_honors_format_and_converts_units() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("main.kcl"), "cube(1)\n").unwrap();
    let expected = json!([
        {"property": "Center", "axis": "x", "value": -1.0},
        {"property": "Center", "axis": "y", "value": 2.0},
        {"property": "Center", "axis": "z", "value": 3.0},
        {"property": "Distance", "axis": "x", "value": 4.0},
        {"property": "Distance", "axis": "y", "value": 5.0},
        {"property": "Distance", "axis": "z", "value": 6.0}
    ]);
    for &(configured, explicit, format) in FORMATS {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let host = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(serve_bounding_box(listener));
        let mut args = vec!["kcl", "bounding-box", "main.kcl", "--output-unit", "cm"];
        if let Some(explicit) = explicit {
            args.extend(["--format", explicit]);
        }
        let output = run_zoo(project.path(), &args, &host, configured).await;
        assert_success(&output);
        tokio::time::timeout(TIMEOUT, server).await.unwrap().unwrap();
        assert_formatted(&output, format, &expected, "Property");
    }
}

async fn serve_http(listener: TcpListener, body: Vec<u8>) -> String {
    let (mut stream, _) = listener.accept().await.unwrap();
    let mut request = Vec::new();
    loop {
        let mut buf = [0; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        assert!(n > 0);
        request.extend_from_slice(&buf[..n]);
        if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&request[..end]);
            let length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if request.len() >= end + 4 + length {
                break;
            }
        }
    }
    let headers = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(headers.as_bytes()).await.unwrap();
    stream.write_all(&body).await.unwrap();
    String::from_utf8(request).unwrap()
}

#[tokio::test]
async fn api_call_status_honors_format_including_downloads() {
    for completed in [false, true] {
        let mut response = json!({
            "type": "file_conversion", "id": CALL_ID,
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z",
            "status": if completed { "completed" } else { "queued" },
            "src_format": "step", "output_format": "obj",
            "user_id": "00000000-0000-0000-0000-000000000001"
        });
        if completed {
            response["outputs"] = json!({"part.obj": "bWVzaA=="});
        }
        let mut expected = serde_json::to_value(
            serde_json::from_value::<kittycad::types::AsyncApiCallOutput>(response.clone()).unwrap(),
        )
        .unwrap();
        if completed {
            expected.as_object_mut().unwrap().remove("outputs");
        }
        for &(configured, explicit, format) in FORMATS {
            let dir = tempfile::tempdir().unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let host = format!("http://{}", listener.local_addr().unwrap());
            let server = tokio::spawn(serve_http(listener, serde_json::to_vec(&response).unwrap()));
            let mut args = vec!["api-call", "status", CALL_ID];
            if let Some(explicit) = explicit {
                args.extend(["--format", explicit]);
            }
            let output = run_zoo(dir.path(), &args, &host, configured).await;
            assert_success(&output);
            let request = tokio::time::timeout(TIMEOUT, server).await.unwrap().unwrap();
            assert!(
                request.starts_with(&format!("GET /async/operations/{CALL_ID} ")),
                "{request}"
            );
            assert_formatted(&output, format, &expected, "file_conversion");
            if format == "table" {
                let stdout = String::from_utf8_lossy(&output.stdout);
                assert!(stdout.contains(CALL_ID), "{stdout}");
                assert!(
                    stdout.contains(if completed { "completed" } else { "queued" }),
                    "{stdout}"
                );
            }
            if completed {
                assert_eq!(std::fs::read(dir.path().join("part.obj")).unwrap(), b"mesh");
                let status = if format == "table" {
                    &output.stdout
                } else {
                    &output.stderr
                };
                assert!(String::from_utf8_lossy(status).contains("Saved file conversion"));
            }
        }
    }
}

#[tokio::test]
async fn file_and_image_commands_reject_unused_format() {
    let dir = tempfile::tempdir().unwrap();
    for mut args in [
        vec!["kcl", "export", "missing.kcl", ".", "-t", "obj"],
        vec!["kcl", "snapshot", "missing.kcl", "part.png"],
        vec!["kcl", "view", "missing.kcl"],
    ] {
        args.extend(["--format", "json"]);
        let output = run_zoo(dir.path(), &args, "http://127.0.0.1:1", None).await;
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("unexpected argument '--format'"));
    }
}

#[tokio::test]
async fn session_snapshot_rejects_unsupported_rendering_flags() {
    let dir = tempfile::tempdir().unwrap();
    for flags in [
        vec!["--angle", "top"],
        vec!["--camera-style", "perspective"],
        vec!["--camera-style", "ortho"],
        vec!["--camera-padding", "0.2"],
        vec!["--camera-padding", "0.1"],
        vec!["--replay"],
        vec!["--allow-errors"],
        vec!["--show-trace"],
    ] {
        let mut args = vec!["kcl", "snapshot", "missing.kcl", "part.png", "--session", "127.0.0.1:1"];
        args.extend(&flags);
        let output = run_zoo(dir.path(), &args, "http://127.0.0.1:1", None).await;
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("cannot be used with") && stderr.contains(flags[0]),
            "{stderr}"
        );
        assert!(!dir.path().join("part.png").exists());
    }
}

#[tokio::test]
async fn session_snapshot_rejects_explicit_and_inferred_jpeg_before_reading_input() {
    let dir = tempfile::tempdir().unwrap();
    for (file, flags) in [("part.jpeg", vec![]), ("part.png", vec!["-t", "jpeg"])] {
        let mut args = vec!["kcl", "snapshot", "missing.kcl", file, "--session", "127.0.0.1:1"];
        args.extend(flags);
        let output = run_zoo(dir.path(), &args, "http://127.0.0.1:1", None).await;
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("--session only supports PNG"));
        assert!(!dir.path().join(file).exists());
    }
}

#[tokio::test]
async fn session_snapshot_still_saves_png() {
    let dir = tempfile::tempdir().unwrap();
    let code = "cube(1)\n";
    std::fs::write(dir.path().join("main.kcl"), code).unwrap();
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(1, 1)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let png = png.into_inner();
    for flags in [vec![], vec!["-t", "png"]] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let server = tokio::spawn(serve_http(listener, png.clone()));
        let mut args = vec!["kcl", "snapshot", "main.kcl", "part.png", "--session", &addr];
        args.extend(flags);
        let output = run_zoo(dir.path(), &args, "http://127.0.0.1:1", None).await;
        let request = tokio::time::timeout(TIMEOUT, server).await.unwrap().unwrap();
        assert_success(&output);
        let (_, body) = request.split_once("\r\n\r\n").unwrap();
        assert_eq!(serde_json::from_str::<Value>(body).unwrap()["kcl_program"], code);
        assert_eq!(std::fs::read(dir.path().join("part.png")).unwrap(), png);
    }
}
