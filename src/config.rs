use std::fmt::Write;

use anyhow::{anyhow, Result};
use thiserror::Error;

/// This trait describes interaction with the configuration for `zoo`.
pub trait Config: Send + Sync {
    /// Returns a value from the configuration by its key.
    fn get(&self, hostname: &str, key: &str) -> Result<String>;
    /// Returns a value from the configuration by its key, with the source.
    fn get_with_source(&self, hostname: &str, key: &str) -> Result<(String, String)>;
    /// Sets a value in the configuration by its key.
    fn set(&mut self, hostname: &str, key: &str, value: &str) -> Result<()>;

    /// Remove a host.
    fn unset_host(&mut self, key: &str) -> Result<()>;
    /// Get the hosts.
    fn hosts(&self) -> Result<Vec<String>>;

    /// Get the default host.
    fn default_host(&self) -> Result<String>;
    /// Get the default host with the source.
    fn default_host_with_source(&self) -> Result<(String, String)>;

    /// Get the aliases.
    fn aliases(&mut self) -> Result<crate::config_alias::AliasConfig>;
    /// Save the aliases to our config.
    fn save_aliases(&mut self, aliases: &crate::config_map::ConfigMap) -> Result<()>;
    /// expand_alias processes argv to see if it should be rewritten according to a user's aliases. The
    /// second return value indicates whether the alias should be executed in a new shell process instead
    /// of running `zoo` itself.
    fn expand_alias(&mut self, args: Vec<String>) -> Result<(Vec<String>, bool)>;

    /// Check if the configuration can be written to.
    fn check_writable(&self, hostname: &str, key: &str) -> Result<()>;

    /// Write the configuration.
    fn write(&self) -> Result<()>;

    /// Return the string representation of the config.
    fn config_to_string(&self) -> Result<String>;

    /// Return the string representation of the hosts.
    fn hosts_to_string(&self) -> Result<String>;
}

pub struct ConfigOption {
    pub key: String,
    pub description: String,
    pub comment: String,
    pub default_value: String,
    pub allowed_values: Vec<String>,
}

pub fn config_options() -> Vec<ConfigOption> {
    vec![
        ConfigOption {
            key: "editor".to_string(),
            description: "the text editor program to use for authoring text".to_string(),
            comment: "What editor zoo should run when creating text, etc. If blank, will refer to environment."
                .to_string(),
            default_value: "".to_string(),
            allowed_values: vec![],
        },
        ConfigOption {
            key: "prompt".to_string(),
            description: "toggle interactive prompting in the terminal".to_string(),
            comment: "When to interactively prompt. This is a global config that cannot be overridden by hostname."
                .to_string(),
            default_value: "enabled".to_string(),
            allowed_values: vec!["enabled".to_string(), "disabled".to_string()],
        },
        ConfigOption {
            key: "pager".to_string(),
            description: "the terminal pager program to send standard output to".to_string(),
            comment: "A pager program to send command output to, e.g. \"less\". Set the value to \"cat\" to disable the pager.".to_string(),
            default_value: "".to_string(),
            allowed_values: vec![],
        },
        ConfigOption {
            key: "browser".to_string(),
            description: "the web browser to use for opening URLs".to_string(),
            comment: "What web browser zoo should use when opening URLs. If blank, will refer to environment.".to_string(),
            default_value: "".to_string(),
            allowed_values: vec![],
        },
        ConfigOption {
            key: "format".to_string(),
            description: "the formatting style for command output".to_string(),
            comment: "What formatting zoo should use when printing text.".to_string(),
            default_value: crate::types::FormatOutput::default().to_string(),
            allowed_values: crate::types::FormatOutput::variants(),
        },
    ]
}

pub fn validate_key(key: &str) -> Result<()> {
    for config_key in config_options() {
        if key == config_key.key {
            return Ok(());
        }
    }

    Err(anyhow!("invalid key: {}", key))
}

#[derive(Error, Debug)]
pub enum InvalidValueError {
    #[error("invalid values, valid values: {0:?}")]
    ValidValues(Vec<String>),
}

pub fn validate_value(key: &str, value: &str) -> Result<()> {
    let mut valid_values: Vec<String> = vec![];

    // Set the valid values for the key.
    for config_key in config_options() {
        if config_key.key == key {
            valid_values = config_key.allowed_values;
            break;
        }
    }

    if valid_values.is_empty() {
        return Ok(());
    }

    for v in valid_values.clone() {
        if v == value {
            return Ok(());
        }
    }

    Err(InvalidValueError::ValidValues(valid_values).into())
}

// new_from_string initializes a Config from a toml string.
#[cfg(test)]
fn new_from_string(s: &str) -> Result<impl Config> {
    let root = s.parse::<toml_edit::Document>()?;
    Ok(new_config(root))
}

pub fn new_config(t: toml_edit::Document) -> impl Config {
    crate::config_from_file::FileConfig {
        map: crate::config_map::ConfigMap {
            root: t.as_table().clone(),
        },
    }
}

pub fn new_blank_root() -> Result<toml_edit::Document> {
    let mut s = String::new();
    for option in config_options() {
        if !option.comment.is_empty() {
            writeln!(s, "# {}", option.comment)?;
            if !option.allowed_values.is_empty() {
                writeln!(s, "# Supported values: {}", option.allowed_values.join(", "))?;
            }
        }
        writeln!(s, "{} = \"{}\"\n", option.key, option.default_value)?;
    }

    Ok(s.parse::<toml_edit::Document>()?)
}

#[cfg(test)]
pub fn new_blank_config() -> Result<impl Config> {
    let root = new_blank_root()?;
    Ok(new_config(root))
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_file_config_set_no_host() {
        let mut c = new_blank_config().unwrap();
        assert!(c.set("", "editor", "vim").is_ok());
        assert!(c.set("", "prompt", "disabled").is_ok());
        assert!(c.set("", "pager", "less").is_ok());
        assert!(c.set("", "browser", "firefox").is_ok());
        assert!(c.set("", "format", "table").is_ok());

        let doc = c.config_to_string().unwrap();
        assert!(doc.contains("editor = \"vim\""));
        assert!(doc.contains("prompt = \"disabled\""));
        assert!(doc.contains("pager = \"less\""));
        assert!(doc.contains("browser = \"firefox\""));
        assert!(doc.contains("format = \"table\""));
    }

    #[test]
    fn test_file_config_set_with_host() {
        let mut c = new_blank_config().unwrap();
        assert!(c.set("example.com", "editor", "vim").is_ok());
        assert!(c.set("example.com", "prompt", "disabled").is_ok());
        assert!(c.set("example.com", "pager", "less").is_ok());
        assert!(c.set("example.com", "browser", "firefox").is_ok());
        assert!(c.set("zoo.computer", "browser", "chrome").is_ok());

        let doc = c.hosts_to_string().unwrap();

        let expected = r#"["example.com"]
editor = "vim"
prompt = "disabled"
pager = "less"
browser = "firefox"

["zoo.computer"]
browser = "chrome""#;
        assert_eq!(doc, expected);

        let hosts = c.hosts().unwrap();
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0], "example.com".to_string());
        assert_eq!(hosts[1], "zoo.computer".to_string());
    }

    #[test]
    fn test_default_config() {
        let c = new_blank_config().unwrap();
        let doc_config = c.config_to_string().unwrap();

        let expected = r#"# What editor zoo should run when creating text, etc. If blank, will refer to environment.
editor = ""

# When to interactively prompt. This is a global config that cannot be overridden by hostname.
# Supported values: enabled, disabled
prompt = "enabled"

# A pager program to send command output to, e.g. "less". Set the value to "cat" to disable the pager.
pager = ""

# What web browser zoo should use when opening URLs. If blank, will refer to environment.
browser = ""

# What formatting zoo should use when printing text.
# Supported values: table, json, yaml
format = "table""#;
        assert_eq!(doc_config, expected);

        let doc_hosts = c.hosts_to_string().unwrap();
        assert_eq!(doc_hosts, "");
    }

    #[test]
    fn test_parse_config() {
        let c = crate::config::new_from_string(
            r#"[hosts]

[hosts."thing.com"]
user = "jess"
token = "MY_TOKEN""#,
        )
        .unwrap();

        let user = c.get("thing.com", "user").unwrap();
        assert_eq!(user, "jess");

        let token = c.get("thing.com", "token").unwrap();
        assert_eq!(token, "MY_TOKEN");
    }

    #[test]
    fn test_parse_config_multiple_hosts() {
        let mut c = crate::config::new_from_string(
            r#"[hosts]

[hosts."example.org"]
user = "new_user"
token = "EXAMPLE_TOKEN"

[hosts."thing.com"]
user = "jess"
token = "MY_TOKEN""#,
        )
        .unwrap();

        let user = c.get("thing.com", "user").unwrap();
        assert_eq!(user, "jess");

        let token = c.get("thing.com", "token").unwrap();
        assert_eq!(token, "MY_TOKEN");

        let user = c.get("example.org", "user").unwrap();
        assert_eq!(user, "new_user");

        let token = c.get("example.org", "token").unwrap();
        assert_eq!(token, "EXAMPLE_TOKEN");

        // Getting the default host should return an error.
        assert_eq!(c.default_host().is_err(), true);
        if let Err(e) = c.default_host() {
            assert_eq!(e.to_string(), "No host has been set as default. Try setting a default with `zoo config set -H <host> default true`. Options for hosts are: example.org, thing.com");
        }

        c.set("example.org", "default", "true").unwrap();
        assert_eq!(c.default_host().unwrap(), "example.org".to_string());

        c.unset_host("thing.com").unwrap();
        let token = c.get("thing.com", "token");
        assert!(token.is_err());

        let expected = r#"["example.org"]
user = "new_user"
token = "EXAMPLE_TOKEN"
default = true"#;
        assert_eq!(c.hosts_to_string().unwrap(), expected);
    }

    #[test]
    fn test_validate_key() {
        let result = validate_key("invalid").unwrap_err();
        assert_eq!(result.to_string(), "invalid key: invalid");

        let result = validate_key("editor");
        assert!(result.is_ok());

        let result = validate_key("browser");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_value() {
        let result = validate_value("prompt", "invalid").unwrap_err();
        assert_eq!(
            result.to_string(),
            "invalid values, valid values: [\"enabled\", \"disabled\"]"
        );

        let result = validate_value("editor", "vim");
        assert!(result.is_ok());

        let result = validate_value("browser", "firefox");
        assert!(result.is_ok());

        let result = validate_value("prompt", "enabled");
        assert!(result.is_ok());
    }

    pub struct TestItem {
        name: String,
        args: Vec<String>,
        want_expanded: Vec<String>,
        want_is_shell: bool,
        want_err: String,
    }

    #[test]
    fn test_expand_alias() {
        let tests: Vec<TestItem> = vec![
            TestItem {
                name: "no arguments".to_string(),
                args: vec![],
                want_expanded: vec![],
                want_is_shell: false,
                want_err: "".to_string(),
            },
            TestItem {
                name: "too few arguments".to_string(),
                args: vec!["zoo".to_string()],
                want_expanded: vec![],
                want_is_shell: false,
                want_err: "".to_string(),
            },
            TestItem {
                name: "no expansion".to_string(),
                args: vec!["zoo".to_string(), "config".to_string(), "set".to_string()],
                want_expanded: vec!["zoo".to_string(), "config".to_string(), "set".to_string()],
                want_is_shell: false,
                want_err: "".to_string(),
            },
            TestItem {
                name: "simple expansion".to_string(),
                args: vec!["zoo".to_string(), "cs".to_string()],
                want_expanded: vec!["zoo".to_string(), "config".to_string(), "set".to_string()],
                want_is_shell: false,
                want_err: "".to_string(),
            },
            TestItem {
                name: "simple expansion with weird binary name".to_string(),
                args: vec!["weird".to_string(), "cs".to_string()],
                want_expanded: vec!["weird".to_string(), "config".to_string(), "set".to_string()],
                want_is_shell: false,
                want_err: "".to_string(),
            },
            TestItem {
                name: "adding arguments after expansion".to_string(),
                args: vec![
                    "zoo".to_string(),
                    "cs".to_string(),
                    "foo".to_string(),
                    "bar".to_string(),
                ],
                want_expanded: vec![
                    "zoo".to_string(),
                    "config".to_string(),
                    "set".to_string(),
                    "foo".to_string(),
                    "bar".to_string(),
                ],
                want_is_shell: false,
                want_err: "".to_string(),
            },
            TestItem {
                name: "not enough arguments for expansion".to_string(),
                args: vec!["zoo".to_string(), "ca".to_string()],
                want_expanded: vec![],
                want_is_shell: false,
                want_err: "not enough arguments for alias: config set $1 $2".to_string(),
            },
            TestItem {
                name: "not enough arguments for expansion, again".to_string(),
                args: vec!["zoo".to_string(), "ca".to_string(), "foo".to_string()],
                want_expanded: vec![],
                want_is_shell: false,
                want_err: "not enough arguments for alias: config set foo $2".to_string(),
            },
            TestItem {
                name: "satisfy expansion arguments".to_string(),
                args: vec![
                    "zoo".to_string(),
                    "ca".to_string(),
                    "foo".to_string(),
                    "bar".to_string(),
                ],
                want_expanded: vec![
                    "zoo".to_string(),
                    "config".to_string(),
                    "set".to_string(),
                    "foo".to_string(),
                    "bar".to_string(),
                ],
                want_is_shell: false,
                want_err: "".to_string(),
            },
            TestItem {
                name: "mixed positional and non-positional arguments".to_string(),
                args: vec![
                    "zoo".to_string(),
                    "ca".to_string(),
                    "foo".to_string(),
                    "bar".to_string(),
                    "-H".to_string(),
                    "example.org".to_string(),
                ],
                want_expanded: vec![
                    "zoo".to_string(),
                    "config".to_string(),
                    "set".to_string(),
                    "foo".to_string(),
                    "bar".to_string(),
                    "-H".to_string(),
                    "example.org".to_string(),
                ],
                want_is_shell: false,
                want_err: "".to_string(),
            },
            TestItem {
                name: "dolla dolla bills in expansion".to_string(),
                args: vec!["zoo".to_string(), "ci".to_string(), "$foo$".to_string()],
                want_expanded: vec![
                    "zoo".to_string(),
                    "config".to_string(),
                    "set".to_string(),
                    "$foo$".to_string(),
                    "$foo$".to_string(),
                ],
                want_is_shell: false,
                want_err: "".to_string(),
            },
        ];

        let mut config = crate::config::new_blank_config().unwrap();
        let mut c = crate::config_from_env::EnvConfig::inherit_env(&mut config);

        // Add the aliases we need for our tests.
        let mut aliases = c.aliases().unwrap();
        aliases.add("cs", "config set").unwrap();
        aliases.add("ca", "config set $1 $2").unwrap();
        aliases.add("ci", "config set $1 $1").unwrap();

        for t in tests {
            let result = c.expand_alias(t.args);

            if let Ok((expanded, is_shell)) = result {
                assert_eq!(expanded, t.want_expanded, "test: {}", t.name);
                assert_eq!(is_shell, t.want_is_shell, "test: {}", t.name);
                assert!(t.want_err.is_empty(), "test: {}", t.name);
            } else {
                assert_eq!(result.unwrap_err().to_string(), t.want_err, "test: {}", t.name);
            }
        }
    }
}
