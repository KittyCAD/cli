use std::{path::Path, process::Command};

fn run_config(config_dir: &Path, args: &[&str], host: Option<&str>) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_zoo"));
    cmd.args(args)
        .env("ZOO_CONFIG_DIR", config_dir)
        .env("ZOO_API_TOKEN", "test-token-from-environment")
        .env("ZOO_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1")
        .env_remove("DEBUG")
        .env_remove("ZOO_HOST")
        .env_remove("ZOO_BROWSER");
    if let Some(host) = host {
        cmd.env("ZOO_HOST", host);
    }
    let output = cmd.output().expect("run zoo");
    assert!(
        output.status.success(),
        "zoo {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("UTF-8 output")
}

#[test]
fn config_get_reads_environment_without_panicking() {
    let config = tempfile::tempdir().unwrap();
    assert_eq!(
        run_config(config.path(), &["config", "get", "host"], Some("https://example.com")),
        "https://example.com\n"
    );
    for host in [None, Some("https://example.com")] {
        assert_eq!(
            run_config(config.path(), &["config", "get", "token"], host),
            "test-token-from-environment\n"
        );
        assert_eq!(
            run_config(
                config.path(),
                &["--host", "api.example.org", "config", "get", "token"],
                host,
            ),
            "test-token-from-environment\n"
        );
    }
}

#[test]
fn config_commands_preserve_host_keys_without_panicking() {
    let config = tempfile::tempdir().unwrap();
    for host_env in [None, Some("https://api.example.com")] {
        run_config(config.path(), &["config", "list"], host_env);
        for host_flag in ["--host", "-H"] {
            run_config(
                config.path(),
                &["config", "set", "browser", "test-browser", host_flag, "example.org"],
                host_env,
            );
            assert_eq!(
                run_config(
                    config.path(),
                    &["config", "get", "browser", host_flag, "example.org"],
                    host_env,
                ),
                "test-browser\n"
            );
            assert!(
                run_config(config.path(), &["config", "list", host_flag, "example.org"], host_env)
                    .contains("browser=test-browser")
            );
        }
    }
}
