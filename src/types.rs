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
    // If you add another variant, add it to the `variants()` method below too.
}

#[derive(Debug, Clone, PartialEq, Eq, FromStr, Display, clap::ValueEnum, Copy)]
#[display(style = "kebab-case")]
#[derive(Default)]
pub enum CameraView {
    #[default]
    Front,
    Top,
    RightSide,
    FourWays,
    Iso,
}

impl FormatOutput {
    pub const fn variants() -> &'static [&'static str] {
        &["table", "json", "yaml"]
    }

    /// Whether stdout must be reserved for machine-readable output.
    /// Human-readable status messages belong on stderr for these formats.
    pub const fn is_machine_friendly_output(&self) -> bool {
        match self {
            FormatOutput::Json => true,
            FormatOutput::Yaml => true,
            FormatOutput::Table => false,
        }
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

#[derive(Debug, Clone, PartialEq, Eq, FromStr, Display, clap::ValueEnum, Copy)]
#[display(style = "kebab-case")]
#[derive(Default)]
pub enum CameraStyle {
    #[default]
    Ortho,
    Perspective,
}
