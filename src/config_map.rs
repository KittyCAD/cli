use anyhow::{anyhow, Result};

// ConfigMap implements a low-level get/set config that is backed by an in-memory tree of toml
// nodes. It allows us to interact with a toml-based config programmatically, preserving any
// comments that were present when the toml was parsed.
#[derive(Clone, Debug)]
pub struct ConfigMap {
    pub root: toml_edit::Table,
}

impl ConfigMap {
    pub fn is_empty(&self) -> bool {
        self.root.is_empty()
    }

    pub fn get_string_value(&self, key: &str) -> Result<String> {
        match self.root.get(key) {
            Some(toml_edit::Item::Value(toml_edit::Value::String(s))) => Ok(s.value().to_string()),
            Some(v) => Err(anyhow!("Expected string value for key '{}', found '{:?}'", key, v)),
            None => Err(anyhow!("Key '{}' not found", key)),
        }
    }

    pub fn get_bool_value(&self, key: &str) -> Result<bool> {
        match self.root.get(key) {
            Some(toml_edit::Item::Value(toml_edit::Value::Boolean(s))) => Ok(*s.value()),
            Some(v) => Err(anyhow!("Expected bool value for key '{}', found '{:?}'", key, v)),
            None => Ok(false),
        }
    }

    pub fn set_string_value(&mut self, key: &str, value: &str) -> Result<()> {
        if key == "default" && (value == "true" || value == "false") {
            // Add this as a bool.
            self.root.insert(key, toml_edit::value(value == "true"));
            return Ok(());
        }

        self.root.insert(key, toml_edit::value(value));
        Ok(())
    }

    pub fn find_entry(&self, key: &str) -> Result<toml_edit::Item> {
        match self.root.get(key) {
            Some(v) => Ok(v.clone()),
            None => Err(anyhow!("Key '{}' not found", key)),
        }
    }

    pub fn remove_entry(&mut self, key: &str) -> Result<()> {
        self.root.remove_entry(key);
        Ok(())
    }
}
