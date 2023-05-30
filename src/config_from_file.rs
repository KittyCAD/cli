use anyhow::{anyhow, Result};

use crate::config_alias::AliasConfig;

// This type implements a Config interface and represents a config file on disk.
#[derive(Debug, Clone)]
pub struct FileConfig {
    pub map: crate::config_map::ConfigMap,
}

#[derive(Debug, Clone)]
pub struct HostConfig {
    pub map: crate::config_map::ConfigMap,
    pub host: String,
}

impl FileConfig {
    fn get_hosts_table(&self) -> Result<toml_edit::Table> {
        match self.map.find_entry("hosts") {
            Ok(hosts) => match hosts.as_table() {
                Some(h) => Ok(h.clone()),
                None => Err(anyhow!("hosts is not a table")),
            },
            Err(e) => {
                if e.to_string().contains("not found") {
                    return Ok(toml_edit::Table::new());
                }

                Err(anyhow!("Error reading hosts table: {}", e))
            }
        }
    }

    fn get_aliases_table(&self) -> Result<toml_edit::Table> {
        match self.map.find_entry("aliases") {
            Ok(aliases) => match aliases.as_table() {
                Some(h) => Ok(h.clone()),
                None => Err(anyhow!("aliases is not a table")),
            },
            Err(e) => {
                if e.to_string().contains("not found") {
                    return Ok(toml_edit::Table::new());
                }

                Err(anyhow!("Error reading aliases table: {}", e))
            }
        }
    }

    fn get_host_entries(&self) -> Result<Vec<HostConfig>> {
        let mut host_configs = Vec::new();

        let hosts_table = self.get_hosts_table()?;

        // Iterate over the hosts table and create a HostConfig for each host.
        for (host, v) in hosts_table.iter() {
            let host_config = if let toml_edit::Item::Table(t) = v {
                t.clone()
            } else {
                toml_edit::Table::new()
            };

            let host_config = HostConfig {
                map: crate::config_map::ConfigMap { root: host_config },
                host: host.to_string(),
            };

            host_configs.push(host_config);
        }

        Ok(host_configs)
    }

    fn get_host_config(&self, hostname: &str) -> Result<HostConfig> {
        let host_configs = self.get_host_entries()?;

        for host_config in host_configs {
            if host_config.host == *hostname {
                return Ok(host_config);
            }
        }

        let default_host = crate::cmd_auth::parse_host(crate::DEFAULT_HOST)?.to_string();
        if hostname == default_host {
            Err(anyhow!(
                "Try authenticating with `kittycad auth login` or be sure you are using the env var: `KITTYCAD_TOKEN`."
            ))
        } else {
            Err(anyhow!("host `{}` not found. Try authenticating with `kittycad auth login` or be sure you are using the env var: `KITTYCAD_TOKEN`.", hostname))
        }
    }

    fn make_host_config(&self, hostname: &str) -> Result<HostConfig> {
        let host_config = HostConfig {
            map: crate::config_map::ConfigMap {
                root: toml_edit::Table::new(),
            },
            host: hostname.to_string(),
        };

        let mut hosts_table = self.get_hosts_table()?;

        hosts_table.insert(hostname, toml_edit::Item::Table(host_config.map.root.clone()));

        Ok(host_config)
    }
}

impl crate::config::Config for FileConfig {
    fn get(&self, hostname: &str, key: &str) -> Result<String> {
        let (val, _) = self.get_with_source(hostname, key)?;
        Ok(val)
    }

    fn get_with_source(&self, hostname: &str, key: &str) -> Result<(String, String)> {
        if hostname.is_empty() {
            let default_source = crate::config_file::config_file()?;
            let value = self.map.get_string_value(key)?;

            return Ok((value, default_source));
        }

        let hosts_source = crate::config_file::hosts_file()?;

        let host_config = self.get_host_config(hostname)?;

        let value = host_config.map.get_string_value(key)?;

        Ok((value, hosts_source))
    }

    fn set(&mut self, hostname: &str, key: &str, value: &str) -> Result<()> {
        if hostname.is_empty() {
            return self.map.set_string_value(key, value);
        }

        let mut host_config = match self.get_host_config(hostname) {
            Ok(host_config) => host_config,
            Err(_) => {
                // Likely the host doesn't exist, so create it.
                self.make_host_config(hostname)?
            }
        };

        host_config.map.set_string_value(key, value)?;

        // Get our hosts table.
        let mut hosts_table = self.get_hosts_table()?;

        hosts_table.insert(hostname, toml_edit::Item::Table(host_config.map.root.clone()));

        // Reset the hosts.
        self.map.root.insert("hosts", toml_edit::Item::Table(hosts_table));

        Ok(())
    }

    fn unset_host(&mut self, hostname: &str) -> Result<()> {
        if hostname.is_empty() {
            return Ok(());
        }

        let mut hosts_table = self.get_hosts_table()?;

        // Remove the host from the table.
        hosts_table.remove_entry(hostname);

        // Reset the hosts.
        self.map.root.insert("hosts", toml_edit::Item::Table(hosts_table));

        Ok(())
    }

    fn hosts(&self) -> Result<Vec<String>> {
        let mut hosts = Vec::new();

        let hosts_table = self.get_hosts_table()?;

        for (host, _) in hosts_table.iter() {
            hosts.push(host.to_string());
        }

        if hosts.is_empty() {
            // Add the default host.
            hosts.push(crate::cmd_auth::parse_host(crate::DEFAULT_HOST)?.to_string());
        }

        Ok(hosts)
    }

    fn default_host(&self) -> Result<String> {
        let (host, _) = self.default_host_with_source()?;
        Ok(host)
    }

    fn default_host_with_source(&self) -> Result<(String, String)> {
        // Get all the hosts.
        let hosts = self.hosts()?;

        let hosts_source = crate::config_file::hosts_file()?;

        // Get the first host.
        if hosts.len() == 1 {
            return Ok((hosts[0].to_string(), hosts_source));
        }

        // Find the default host.
        let host_configs = self.get_host_entries()?;

        for host_config in host_configs {
            if host_config.map.get_bool_value("default")? {
                return Ok((host_config.host, hosts_source));
            }
        }

        Err(anyhow!(
            "No host has been set as default. Try setting a default with `kittycad config set -H <host> default true`. Options for hosts are: {}", hosts.join(", ")
        ))
    }

    fn aliases(&mut self) -> Result<crate::config_alias::AliasConfig> {
        let aliases_table = self.get_aliases_table()?;

        Ok(AliasConfig {
            map: crate::config_map::ConfigMap { root: aliases_table },
            parent: self,
        })
    }

    fn save_aliases(&mut self, aliases: &crate::config_map::ConfigMap) -> Result<()> {
        // Save the aliases.
        self.map
            .root
            .insert("aliases", toml_edit::Item::Table(aliases.root.clone()));

        Ok(())
    }

    fn expand_alias(&mut self, args: Vec<String>) -> Result<(Vec<String>, bool)> {
        let mut is_shell = false;

        if args.len() < 2 {
            // The command is lacking a subcommand.
            return Ok((Vec::new(), is_shell));
        }

        let mut expanded = args.clone();

        // Save the first argument and remove it at the same time.
        // The first argument is the command name, we want to add it back at the end.
        let first = expanded.remove(0);

        // Get our aliases.
        let aliases = self.aliases()?;

        // Expand the alias.
        let (mut expansion, ok) = aliases.get(expanded.first().unwrap());
        if !ok {
            // Return the original args.
            return Ok((args, is_shell));
        }

        // Get the additional arguments.
        let mut additional_args = args;

        additional_args.remove(0); // Remove the first argument.
        additional_args.remove(0); // Remove the second argument.

        if expansion.starts_with('!') {
            is_shell = true;

            expanded = vec![
                "sh".to_string(),
                "-c".to_string(),
                expansion.trim_start_matches('!').to_string(),
            ];

            if !additional_args.is_empty() {
                // Add the additional arguments.
                expanded.push("--".to_string());
                expanded.append(&mut additional_args);
            }

            return Ok((expanded, is_shell));
        }

        let mut extra_args: Vec<String> = vec![];
        for (i, a) in additional_args.iter().enumerate() {
            if !expansion.contains('$') {
                extra_args.push(a.clone());
            } else {
                expansion = expansion.replace(&format!("${}", i + 1), a);
            }
        }

        let lingering = regex::Regex::new(r"\$\d")?;
        if lingering.is_match(&expansion) {
            return Err(anyhow!("not enough arguments for alias: {}", expansion));
        }

        let mut new_args = vec![first];
        new_args.append(&mut shlex::split(&expansion).unwrap());
        new_args.append(&mut extra_args);

        Ok((new_args, is_shell))
    }

    fn check_writable(&self, _hostname: &str, _key: &str) -> Result<()> {
        // TODO: check if the config file is writable from the filesystem permissions
        Ok(())
    }

    fn write(&self) -> Result<()> {
        // Get the config file name.
        let config_filename = crate::config_file::config_file()?;

        // Get the string representation of the config file.
        let content = self.config_to_string()?;

        // Write the config file.
        crate::config_file::write_config_file(&config_filename, &content)?;

        // Get the hosts file name.
        let hosts_filename = crate::config_file::hosts_file()?;

        // Get the string representation of the hosts file.
        let content = self.hosts_to_string()?;

        // Write the hosts file.
        crate::config_file::write_config_file(&hosts_filename, &content)
    }

    fn config_to_string(&self) -> Result<String> {
        // Remove the hosts entry from the config map.
        let mut map = self.map.clone();

        map.remove_entry("hosts")?;

        let doc: toml_edit::Document = map.root.into();

        Ok(doc.to_string().trim().to_string())
    }

    fn hosts_to_string(&self) -> Result<String> {
        let doc: toml_edit::Document = self.get_hosts_table()?.into();

        Ok(doc.to_string().trim().to_string())
    }
}
