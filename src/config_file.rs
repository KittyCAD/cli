use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};

const KITTYCAD_CONFIG_DIR: &str = "KITTYCAD_CONFIG_DIR";
const XDG_CONFIG_HOME: &str = "XDG_CONFIG_HOME";
const XDG_STATE_HOME: &str = "XDG_STATE_HOME";
#[allow(dead_code)]
const XDG_DATA_HOME: &str = "XDG_DATA_HOME";
const APP_DATA: &str = "CommandData";
const LOCAL_APP_DATA: &str = "LocalCommandData";

// Config path precedence
// 1. KITTYCAD_CONFIG_DIR
// 2. XDG_CONFIG_HOME
// 3. CommandData (windows only)
// 4. HOME
pub fn config_dir() -> Result<String> {
    let path: PathBuf;

    let kittycad_config_dir = get_env_var(KITTYCAD_CONFIG_DIR);
    let xdg_config_home = get_env_var(XDG_CONFIG_HOME);
    let app_data = get_env_var(APP_DATA);

    if !kittycad_config_dir.is_empty() {
        path = Path::new(&kittycad_config_dir).to_path_buf();
    } else if !xdg_config_home.is_empty() {
        path = Path::new(&xdg_config_home).join("kittycad");
    } else if !app_data.is_empty() && std::env::consts::OS == "windows" {
        path = Path::new(&app_data).join("KittyCAD CLI");
    } else {
        match dirs::home_dir() {
            Some(home) => {
                path = home.join(".config").join("kittycad");
            }
            None => {
                return Err(anyhow!("could not find home directory"));
            }
        }
    }

    // Convert the path into a string slice
    match path.to_str() {
        None => return Err(anyhow!("path is not a valid UTF-8 sequence")),
        Some(s) => Ok(s.to_string()),
    }
}

// State path precedence
// 2. XDG_STATE_HOME
// 3. LocalCommandData (windows only)
// 4. HOME
pub fn state_dir() -> Result<String> {
    let path: PathBuf;

    let xdg_state_home = get_env_var(XDG_STATE_HOME);
    let local_app_data = get_env_var(LOCAL_APP_DATA);

    if !xdg_state_home.is_empty() {
        path = Path::new(&xdg_state_home).join("kittycad");
    } else if !local_app_data.is_empty() && std::env::consts::OS == "windows" {
        path = Path::new(&local_app_data).join("KittyCAD CLI");
    } else {
        match dirs::home_dir() {
            Some(home) => {
                path = home.join(".local").join("state").join("kittycad");
            }
            None => {
                return Err(anyhow!("could not find home directory"));
            }
        }
    }

    // Convert the path into a string slice
    match path.to_str() {
        None => return Err(anyhow!("path is not a valid UTF-8 sequence")),
        Some(s) => Ok(s.to_string()),
    }
}

// Data path precedence
// 2. XDG_DATA_HOME
// 3. LocalCommandData (windows only)
// 4. HOME
#[allow(dead_code)]
pub fn data_dir() -> Result<String> {
    let path: PathBuf;

    let xdg_data_home = get_env_var(XDG_DATA_HOME);
    let local_app_data = get_env_var(LOCAL_APP_DATA);

    if !xdg_data_home.is_empty() {
        path = Path::new(&xdg_data_home).join("kittycad");
    } else if !local_app_data.is_empty() && std::env::consts::OS == "windows" {
        path = Path::new(&local_app_data).join("KittyCAD CLI");
    } else {
        match dirs::home_dir() {
            Some(home) => {
                path = home.join(".local").join("share").join("kittycad");
            }
            None => {
                return Err(anyhow!("could not find home directory"));
            }
        }
    }

    // Convert the path into a string slice
    match path.to_str() {
        None => return Err(anyhow!("path is not a valid UTF-8 sequence")),
        Some(s) => Ok(s.to_string()),
    }
}

pub fn config_file() -> Result<String> {
    let config_dir = config_dir()?;
    let path = Path::new(&config_dir).join("config.toml");

    // Convert the path into a string slice
    match path.to_str() {
        None => return Err(anyhow!("path is not a valid UTF-8 sequence")),
        Some(s) => Ok(s.to_string()),
    }
}

pub fn hosts_file() -> Result<String> {
    let config_dir = config_dir()?;
    let path = Path::new(&config_dir).join("hosts.toml");

    // Convert the path into a string slice
    match path.to_str() {
        None => return Err(anyhow!("path is not a valid UTF-8 sequence")),
        Some(s) => Ok(s.to_string()),
    }
}

pub fn state_file() -> Result<String> {
    let state_dir = state_dir()?;
    let path = Path::new(&state_dir).join("state.toml");

    // Convert the path into a string slice
    match path.to_str() {
        None => return Err(anyhow!("path is not a valid UTF-8 sequence")),
        Some(s) => Ok(s.to_string()),
    }
}

pub fn parse_default_config() -> Result<impl crate::config::Config> {
    let config_file_path = config_file()?;

    // If the config file does not exist, create it.
    let path = Path::new(&config_file_path);
    let mut root = if !path.exists() {
        // Get the default config from a blank.
        crate::config::new_blank_root()?
    } else {
        // Get the default config from the file.
        let contents = read_config_file(&config_file_path)?;
        contents.parse::<toml_edit::Document>()?
    };

    // Parse the hosts file.
    let hosts_file_path = hosts_file()?;
    let path = Path::new(&hosts_file_path);
    if path.exists() {
        let contents = read_config_file(&hosts_file_path)?;
        let doc = contents.parse::<toml_edit::Document>()?;
        let hosts = doc.as_table().clone();
        root.insert("hosts", toml_edit::Item::Table(hosts));
    }

    Ok(crate::config::new_config(root))
}

fn read_config_file(filename: &str) -> Result<String> {
    fs::read_to_string(filename).with_context(|| format!("failed to read from {}", filename))
}

pub fn write_config_file(filename: &str, data: &str) -> Result<()> {
    let path = Path::new(filename);
    let parent = path.parent().unwrap();
    fs::create_dir_all(parent).with_context(|| format!("failed to create directory {}", parent.display()))?;

    let mut file = fs::File::create(filename)?;
    file.write_all(data.as_bytes())
        .with_context(|| format!("failed to write to {}", filename))
}

#[allow(dead_code)]
fn backup_config_file(filename: String) -> Result<()> {
    fs::rename(&filename, &format!("{}.bak", filename)).with_context(|| format!("failed to backup {}", filename))
}

pub fn get_env_var(key: &str) -> String {
    match env::var(key) {
        Ok(val) => val,
        Err(_) => "".to_string(),
    }
}
