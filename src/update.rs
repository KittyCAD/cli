#[cfg(target_family = "unix")]
use std::os::unix::fs::PermissionsExt;
use std::{fs, io::Write};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::config_file::get_env_var;

/// ReleaseInfo stores information about a release.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReleaseInfo {
    #[serde(rename = "tag_name")]
    pub version: String,
    pub url: String,
    pub published_at: chrono::DateTime<chrono::Utc>,
}

/// StateEntry stores information about a state.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StateEntry {
    pub checked_for_update_at: chrono::DateTime<chrono::Utc>,
    pub latest_release: ReleaseInfo,
}

/// Check for updates to the cli.
///
/// Returns the latest version of the cli, or none if there is not a new
/// update or we shouldn't update.
pub async fn check_for_update(current_version: &str, force: bool) -> Result<Option<ReleaseInfo>> {
    if !should_check_for_update() && !force {
        return Ok(None);
    }

    let state_file = crate::config_file::state_file()?;

    // Get our current state.
    if std::path::Path::new(&state_file).exists() {
        let state = get_state_entry(&state_file)?;

        if !force {
            let duration_since_last_check = chrono::Utc::now() - state.checked_for_update_at;
            // TODO: After we make a mjor release of v1 we should bump this to like 6/12 hours.
            if duration_since_last_check < chrono::Duration::hours(1) {
                // If we've checked for updates in the last 1 hour, don't check again.
                return Ok(None);
            }
        }
    }

    // Get the latest release.
    let latest_release = get_latest_release_info().await?;

    // Update our state.
    set_state_entry(&state_file, chrono::Utc::now(), latest_release.clone())?;

    if version_greater_then(&latest_release.version, current_version)? {
        return Ok(Some(latest_release));
    }

    Ok(None)
}

/// If we should check for an update to the cli.
fn should_check_for_update() -> bool {
    if !get_env_var("KITTYCAD_NO_UPDATE_NOTIFIER").is_empty() {
        return false;
    }

    !is_ci() && atty::is(atty::Stream::Stdout) && atty::is(atty::Stream::Stderr)
}

/// If we are running in a CI environment.
fn is_ci() -> bool {
    !get_env_var("CI").is_empty() || // GitHub Actions, Travis CI, CircleCI, Cirrus CI, GitLab CI, CommandVeyor, CodeShip, dsari
		!get_env_var("BUILD_NUMBER").is_empty() || // Jenkins, TeamCity
		!get_env_var("RUN_ID").is_empty() // TaskCluster, dsari
}

/// Get the information about the latest version of the cli.
pub async fn get_latest_release_info() -> Result<ReleaseInfo> {
    // If the user has a GITHUB_TOKEN environment variable, use it to get the latest release.
    // This allows us to test this while the repo is still private.
    // We might want to remove this in the future.
    let github_token = crate::config_file::get_env_var("GITHUB_TOKEN");

    let url = "https://api.github.com/repos/KittyCAD/cli/releases/latest";

    let mut req = reqwest::Client::new().get(url);

    // Set the user agent.
    req = req.header("User-Agent", format!("kittycad/{}", clap::crate_version!()));

    if !github_token.is_empty() {
        req = req.bearer_auth(github_token);
    }

    let resp = req.send().await?;
    let text = resp.text().await?;

    let latest_release: ReleaseInfo = match serde_json::from_str(&text) {
        Ok(release_info) => release_info,
        Err(err) => {
            return Err(anyhow!(
                "Failed to parse response from GitHub: {}\ntext:\n{}",
                err.to_string(),
                text
            ));
        }
    };

    Ok(latest_release)
}

/// Get an entry in the state file.
fn get_state_entry(filepath: &str) -> Result<StateEntry> {
    let file_content = fs::read_to_string(filepath)?;
    let state_entry: StateEntry = toml::from_str(&file_content)?;

    Ok(state_entry)
}

/// Set an entry in the state file.
fn set_state_entry(filename: &str, t: chrono::DateTime<chrono::Utc>, r: ReleaseInfo) -> Result<()> {
    let data = StateEntry {
        checked_for_update_at: t,
        latest_release: r,
    };

    let content = toml::to_string(&data)?;

    // Make sure we have a parent directory.
    let path = std::path::Path::new(&filename);
    let parent = path.parent().unwrap();
    fs::create_dir_all(parent).with_context(|| format!("failed to create directory {}", parent.display()))?;

    // Write the file.
    fs::write(filename, content).with_context(|| format!("failed to write file {filename}"))?;

    Ok(())
}

/// Return is one version is greater than another.
pub fn version_greater_then(v: &str, w: &str) -> Result<bool> {
    match version_compare::compare(v, w) {
        Ok(cmp) => Ok(cmp == version_compare::Cmp::Gt),
        Err(_) => Err(anyhow!("failed to compare versions: {} {}", v, w)),
    }
}

/// Returns if the release was published in the last 24 hours.
pub fn is_recent_release(published_at: chrono::DateTime<chrono::Utc>) -> bool {
    let duration = chrono::Utc::now() - published_at;

    duration.num_days() < 1
}

/// Check whether the `kittycad` binary was found under the Homebrew prefix.
pub fn is_under_homebrew() -> Result<bool> {
    let binary_path = std::env::current_exe()?;
    let binary_path_str = binary_path.to_str().unwrap();

    match std::process::Command::new("brew").args(vec!["--prefix"]).output() {
        Ok(output) => {
            let homebrew_prefix = String::from_utf8(output.stdout)?;
            let brew_bin_prefix = std::path::Path::new(homebrew_prefix.trim()).join("bin");
            Ok(binary_path_str.starts_with(brew_bin_prefix.to_str().unwrap()))
        }
        Err(e) => {
            // brew wasn't found, so cli can't be running under homebrew.
            if e.kind() == std::io::ErrorKind::NotFound {
                Ok(false)
            } else {
                Err(e.into())
            }
        }
    }
}

/// Takes a version string and returns the URL to download the latest release.
fn get_exe_download_url(version: &str) -> String {
    // Make sure the version starts with a v.
    let version = if !version.starts_with('v') {
        format!("v{version}")
    } else {
        version.to_string()
    };

    format!(
        "https://dl.kittycad.io/releases/cli/{}/kittycad-{}",
        version,
        crate::built_info::TARGET
    )
}

/// Takes a version string and downloads the latest binary to a temp file.
/// This also checks the SHA256 hash of the file.
pub async fn download_binary_to_temp_file(version: &str) -> Result<String> {
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join("kittycad");

    let url = get_exe_download_url(version);

    // Get the contents of the binary.
    let resp = reqwest::get(&url).await?;
    let bin_body = resp.bytes().await?;

    // Get the contents of the sha256sum.
    let resp = reqwest::get(&format!("{url}.sha256")).await?;
    let sha256_body = resp.text().await?;
    let sha256_parts = sha256_body.split(' ').collect::<Vec<&str>>();
    let sha256_hash = sha256_parts[0];

    // Verify the sha256 hash of the binary.
    let bin_hash = sha256_digest(bin_body.as_ref())?;
    if bin_hash != sha256_hash {
        anyhow::bail!("SHA256 hash mismatch: local ({}) != remote ({})", bin_hash, sha256_hash);
    }

    // Write the body to the file.
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(&temp_file)?;
    f.write_all(&bin_body)?;
    f.flush()?;

    let temp_file_path = temp_file
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("failed to convert temp file path to string"))?;

    // Set the file permissions to be correct, executable, write (so we can update).
    // TODO: make this also work on windows.
    #[cfg(target_family = "unix")]
    std::fs::set_permissions(temp_file_path, std::fs::Permissions::from_mode(0o755))?;

    Ok(temp_file_path.to_string())
}

/// Calculates the SHA256 hash of a reader.
fn sha256_digest<R: std::io::Read>(mut reader: R) -> Result<String> {
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buffer = [0; 1024];

    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        context.update(&buffer[..count]);
    }

    let digest = context.finish();

    Ok(data_encoding::HEXLOWER.encode(digest.as_ref()))
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    #[tokio::test]
    async fn test_download_binary_to_temp_file() {
        if crate::update::is_ci() {
            return;
        }

        let file = super::download_binary_to_temp_file("v0.1.0").await.unwrap();

        assert_eq!(
            file,
            format!(
                "{}/kittycad",
                std::env::temp_dir().to_str().unwrap().trim_end_matches('/')
            )
        );
    }

    #[test]
    fn test_get_exe_download_url() {
        let url = super::get_exe_download_url("0.1.0");
        assert_eq!(
            url,
            format!(
                "https://dl.kittycad.io/releases/cli/v0.1.0/kittycad-{}",
                crate::built_info::TARGET
            )
        );

        let url = super::get_exe_download_url("v0.2.0");
        assert_eq!(
            url,
            format!(
                "https://dl.kittycad.io/releases/cli/v0.2.0/kittycad-{}",
                crate::built_info::TARGET
            )
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_check_for_update() {
        let result = super::check_for_update("0.0.1", true).await.unwrap();
        assert_eq!(result.is_some(), true);

        let latest_release = result.unwrap();

        let gh_latest_release = super::get_latest_release_info().await.unwrap();

        assert_eq!(latest_release.version, gh_latest_release.version);
    }

    pub struct TestItem {
        name: String,
        current_version: String,
        latest_version: String,
        want_result: bool,
    }

    #[test]
    fn test_update() {
        let tests: Vec<TestItem> = vec![
            TestItem {
                name: "latest is newer".to_string(),
                current_version: "v0.0.1".to_string(),
                latest_version: "v1.0.0".to_string(),
                want_result: true,
            },
            TestItem {
                name: "current is prerelease".to_string(),
                current_version: "v1.0.0-pre.1".to_string(),
                latest_version: "v1.0.0".to_string(),
                want_result: true,
            },
            TestItem {
                name: "current is built from source".to_string(),
                current_version: "v1.2.3-123-gdeadbeef".to_string(),
                latest_version: "v1.2.3".to_string(),
                want_result: false,
            },
            TestItem {
                name: "current is built from source after a prerelease".to_string(),
                current_version: "v1.2.3-rc.1-123-gdeadbeef".to_string(),
                latest_version: "v1.2.3".to_string(),
                want_result: true,
            },
            TestItem {
                name: "latest is newer than version build from source".to_string(),
                current_version: "v1.2.3-123-gdeadbeef".to_string(),
                latest_version: "v1.2.4".to_string(),
                want_result: true,
            },
            TestItem {
                name: "latest is current".to_string(),
                current_version: "v1.2.5".to_string(),
                latest_version: "v1.2.5".to_string(),
                want_result: false,
            },
            TestItem {
                name: "latest is older".to_string(),
                current_version: "v0.10.0-pre.1".to_string(),
                latest_version: "v0.9.0".to_string(),
                want_result: false,
            },
            TestItem {
                name: "latest is a pre-release".to_string(),
                current_version: "v0.1.0".to_string(),
                latest_version: "v0.2.0-pre.1".to_string(),
                want_result: true,
            },
            TestItem {
                name: "latest is a pre-release (current without v)".to_string(),
                current_version: "0.1.0".to_string(),
                latest_version: "v0.2.0-pre.1".to_string(),
                want_result: true,
            },
        ];

        for t in tests {
            let result = crate::update::version_greater_then(&t.latest_version, &t.current_version).unwrap();

            assert_eq!(result, t.want_result, "test {} failed", t.name);
        }
    }
}
