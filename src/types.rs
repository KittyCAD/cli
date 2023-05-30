use parse_display::{Display, FromStr};

#[derive(Debug, Clone, PartialEq, Eq, FromStr, Display, clap::ValueEnum)]
#[display(style = "kebab-case")]
#[derive(Default)]
pub enum FormatOutput {
    Json,
    Yaml,
    #[default]
    Table,
}



impl FormatOutput {
    pub fn variants() -> Vec<String> {
        vec!["table".to_string(), "json".to_string(), "yaml".to_string()]
    }
}
