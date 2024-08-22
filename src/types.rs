use parse_display::{Display, FromStr};
use serde::Deserialize;

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
    pub const fn variants() -> &'static [&'static str] {
        &["table", "json", "yaml"]
    }
}

#[derive(Deserialize)]
pub struct GltfStandardBuffer {
    pub uri: String,
}

#[derive(Deserialize)]
pub struct GltfStandardJsonLite {
    pub buffers: Vec<GltfStandardBuffer>,
}
