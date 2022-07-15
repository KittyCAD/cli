use anyhow::Result;
use clap::Parser;
use parse_display::{Display, FromStr};

/// Shortcut to open the KittyCAD documentation or Account in your browser.
///
/// If no arguments are given, the default is to open the KittyCAD documentation.
///
///     # open the KittyCAD docs in your browser
///     $ kittycad open docs
///
///     # open your KittyCAD account in your browser
///     $ kittycad open account
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOpen {
    #[clap(name = "shortcut", default_value_t, arg_enum)]
    shortcut: OpenShortcut,
}

/// The type of shortcut to open.
#[derive(PartialEq, Debug, Clone, FromStr, Display, clap::ValueEnum)]
#[display(style = "kebab-case")]
pub enum OpenShortcut {
    /// Open the KittyCAD documentation in your browser.
    Docs,
    /// Open the KittyCAD API reference in your browser.
    ApiRef,
    /// Open the KittyCAD CLI reference in your browser.
    CliRef,
    /// Open your KittyCAD account in your browser.
    Account,
    /// Open the KittyCAD Discord in your browser.
    Discord,
    /// Open the KittyCAD store in your browser.
    Store,
    /// Open the KittyCAD blog in your browser.
    Blog,
    /// Open the repository for the `kittycad` CLI in your browser.
    Repo,
    /// Open the changelog for the `kittycad` CLI in your browser.
    Changelog,
}

impl Default for OpenShortcut {
    fn default() -> Self {
        OpenShortcut::Docs
    }
}

impl OpenShortcut {
    fn get_url(&self) -> String {
        match self {
            OpenShortcut::Docs => "https://docs.kittycad.io".to_string(),
            OpenShortcut::ApiRef => "https://docs.kittycad.io/api".to_string(),
            OpenShortcut::CliRef => "https://docs.kittycad.io/cli".to_string(),
            OpenShortcut::Account => "https://kittycad.io/account".to_string(),
            OpenShortcut::Discord => "https://discord.com/invite/Bee65eqawJ".to_string(),
            OpenShortcut::Store => "https://store.kittycad.io".to_string(),
            OpenShortcut::Blog => "https://kittycad.io/blog".to_string(),
            OpenShortcut::Repo => "https://github.com/KittyCAD/cli".to_string(),
            OpenShortcut::Changelog => changelog_url(clap::crate_version!()),
        }
    }
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdOpen {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        ctx.browser("", &self.shortcut.get_url())
    }
}

/// Returns the URL to the changelog for the given version.
pub fn changelog_url(version: &str) -> String {
    format!("https://github.com/KittyCAD/cli/releases/tag/v{}", version)
}
