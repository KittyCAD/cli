use anyhow::Result;
use clap::Parser;
use parse_display::{Display, FromStr};

/// Shortcut to open the Zoo documentation or Account in your browser.
///
/// If no arguments are given, the default is to open the Zoo documentation.
///
///     # open the Zoo docs in your browser
///     $ zoo open docs
///
///     # open your Zoo account in your browser
///     $ zoo open account
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOpen {
    #[clap(name = "shortcut", default_value_t, value_enum)]
    shortcut: OpenShortcut,
}

/// The type of shortcut to open.
#[derive(PartialEq, Debug, Clone, FromStr, Display, clap::ValueEnum)]
#[display(style = "kebab-case")]
#[derive(Default)]
pub enum OpenShortcut {
    /// Open the Zoo documentation in your browser.
    #[default]
    Docs,
    /// Open the Zoo API reference in your browser.
    ApiRef,
    /// Open the Zoo CLI reference in your browser.
    CliRef,
    /// Open your Zoo account in your browser.
    Account,
    /// Open the Zoo Discord in your browser.
    Discord,
    /// Open the Zoo store in your browser.
    Store,
    /// Open the Zoo blog in your browser.
    Blog,
    /// Open the repository for the `zoo` CLI in your browser.
    Repo,
    /// Open the changelog for the `zoo` CLI in your browser.
    Changelog,
}

impl OpenShortcut {
    fn get_url(&self) -> String {
        match self {
            OpenShortcut::Docs => "https://zoo.dev/docs".to_string(),
            OpenShortcut::ApiRef => "https://zoo.dev/docs/api".to_string(),
            OpenShortcut::CliRef => "https://zoo.dev/docs/cli".to_string(),
            OpenShortcut::Account => "https://zoo.dev/account".to_string(),
            OpenShortcut::Discord => "https://discord.com/invite/Bee65eqawJ".to_string(),
            OpenShortcut::Store => "https://store.zoo.dev".to_string(),
            OpenShortcut::Blog => "https://zoo.dev/blog".to_string(),
            OpenShortcut::Repo => "https://github.com/KittyCAD/cli".to_string(),
            OpenShortcut::Changelog => changelog_url(clap::crate_version!()),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl crate::cmd::Command for CmdOpen {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        ctx.browser("", &self.shortcut.get_url())
    }
}

/// Returns the URL to the changelog for the given version.
pub fn changelog_url(version: &str) -> String {
    format!("https://github.com/KittyCAD/cli/releases/tag/v{version}")
}
