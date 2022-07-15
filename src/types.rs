use parse_display::{Display, FromStr};

#[derive(Debug, Clone, PartialEq, Eq, FromStr, Display, clap::ValueEnum)]
#[display(style = "kebab-case")]
pub enum FormatOutput {
    Json,
    Yaml,
    Table,
}

impl Default for FormatOutput {
    fn default() -> FormatOutput {
        FormatOutput::Table
    }
}

impl FormatOutput {
    pub fn variants() -> Vec<String> {
        vec!["table".to_string(), "json".to_string(), "yaml".to_string()]
    }
}
