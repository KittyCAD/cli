use std::collections::HashMap;

use anyhow::Result;

pub struct AliasConfig<'a> {
    pub map: crate::config_map::ConfigMap,
    pub parent: &'a mut (dyn crate::config::Config + 'a),
}

impl AliasConfig<'_> {
    pub fn get(&self, alias: &str) -> (String, bool) {
        if self.map.is_empty() {
            return ("".to_string(), false);
        }

        let value = match self.map.get_string_value(alias) {
            Ok(value) => value,
            Err(_) => "".to_string(),
        };

        (value.to_string(), !value.is_empty())
    }

    pub fn add(&mut self, alias: &str, expansion: &str) -> Result<()> {
        self.map.set_string_value(alias, expansion)?;

        self.parent.save_aliases(&self.map)?;

        // Update the parent config.
        self.parent.write()
    }

    pub fn delete(&mut self, alias: &str) -> Result<()> {
        self.map.remove_entry(alias)?;

        self.parent.save_aliases(&self.map)?;

        // Update the parent config.
        self.parent.write()
    }

    pub fn list(&self) -> HashMap<String, String> {
        let mut list: HashMap<String, String> = HashMap::new();

        for (key, value) in self.map.root.iter() {
            list.insert(key.to_string(), value.to_string());
        }

        list
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use crate::config::Config;

    #[test]
    fn test_aliases() {
        let mut c = crate::config::new_blank_config().unwrap();

        let mut aliases = c.aliases().unwrap();
        let alias_list = aliases.list();

        assert!(alias_list.is_empty());

        assert_eq!(aliases.get("empty"), ("".to_string(), false));

        // Add some aliases.
        aliases.add("alias1", "value1 thing foo").unwrap();
        aliases.add("alias2", "value2 single").unwrap();

        let alias_list = aliases.list();
        assert_eq!(alias_list.len(), 2);

        assert_eq!(aliases.get("alias1"), ("value1 thing foo".to_string(), true));
        assert_eq!(aliases.get("alias2"), ("value2 single".to_string(), true));

        assert_eq!(aliases.get("not_existing"), ("".to_string(), false));

        aliases.add("alias_3", "things hi there").unwrap();
        assert_eq!(aliases.get("alias_3"), ("things hi there".to_string(), true));

        assert_eq!(aliases.list().len(), 3);

        aliases.delete("alias_3").unwrap();
        assert_eq!(aliases.get("alias_3"), ("".to_string(), false));

        // Print the config.
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
format = "table"

[aliases]
alias1 = "value1 thing foo"
alias2 = "value2 single""#;
        assert_eq!(c.config_to_string().unwrap(), expected);
    }
}
