//! Command-line interface for the `margin` native binary.
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::OnceLock;

static CLI: OnceLock<Cli> = OnceLock::new();

#[derive(Clone, Debug, Default, Parser)]
#[command(
    name = "margin",
    about = "Margin — local-first Markdown notebook and UI gallery",
    args_conflicts_with_subcommands = true
)]
#[cfg_attr(
    feature = "vault",
    command(after_help = "Run `margin` with no subcommand to open your personal notebook on the encrypted vault.")
)]
#[cfg_attr(
    not(feature = "vault"),
    command(after_help = "Run `margin` with no subcommand to open the Field notes sample, kept in memory. \
                          This build has no vault: build with `--features vault` for your personal notebook.")
)]
pub(super) struct Cli {
    /// Start the editor in Split view (Markdown source + reading preview).
    #[arg(long)]
    pub split: bool,

    #[cfg(feature = "vault")]
    /// Personal vault directory (defaults to Margin's app data directory).
    #[arg(
        long,
        value_name = "DIR",
        conflicts_with_all = ["sync_file", "demo", "test_editor", "test_window"]
    )]
    pub vault: Option<PathBuf>,

    #[cfg(feature = "vault")]
    /// Open a specific note room in the personal vault.
    #[arg(
        long,
        value_name = "ROOM",
        conflicts_with_all = ["sync_file", "demo", "test_editor", "test_window"]
    )]
    pub note: Option<String>,

    #[cfg(feature = "vault")]
    /// Unlock a passphrase-protected vault from a file (12–4096 bytes).
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with_all = ["sync_file", "demo", "test_editor", "test_window"]
    )]
    pub passphrase_file: Option<PathBuf>,

    // Without the vault, its options remain (hidden) only to name the missing feature.
    #[cfg(not(feature = "vault"))]
    #[arg(long, hide = true, value_parser = without_vault)]
    #[allow(dead_code)]
    pub vault: Option<String>,

    #[cfg(not(feature = "vault"))]
    #[arg(long, hide = true, value_parser = without_vault)]
    #[allow(dead_code)]
    pub note: Option<String>,

    #[cfg(not(feature = "vault"))]
    #[arg(long, hide = true, value_parser = without_vault)]
    #[allow(dead_code)]
    pub passphrase_file: Option<String>,

    #[cfg_attr(feature = "vault", doc = "Open a standalone unencrypted Loro sync file instead of the personal vault.")]
    #[cfg_attr(not(feature = "vault"), doc = "Open a standalone unencrypted Loro sync file, and keep the note in it.")]
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with_all = ["demo", "test_window"]
    )]
    pub sync_file: Option<PathBuf>,

    /// Open the in-memory "Field notes" sample note with sample comments.
    #[arg(long, conflicts_with_all = ["test_editor", "test_window"])]
    pub demo: bool,

    /// Open the sample editor with the debug inspector panel.
    #[arg(long, conflicts_with = "test_window")]
    pub test_editor: bool,

    /// Open the minimal input-check note with the debug inspector panel.
    #[arg(long)]
    pub test_window: bool,

    /// Legacy flag alias for `margin ui`.
    #[arg(long, hide = true)]
    pub gallery: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Clone, Debug, Subcommand)]
pub(super) enum Command {
    /// Open the Margin design-system component gallery.
    #[command(alias = "gallery")]
    Ui,
}

/// Rejects a vault option in a build without the vault, naming the feature.
#[cfg(not(feature = "vault"))]
fn without_vault(_: &str) -> Result<String, String> {
    Err("this margin was built without the vault; build it with `--features vault`".into())
}

impl Cli {
    pub(super) fn is_gallery(&self) -> bool {
        self.gallery || matches!(self.command, Some(Command::Ui))
    }
}

/// Parse CLI arguments from the process environment and store them for the app.
pub(super) fn init() -> &'static Cli {
    let parsed = Cli::parse();
    CLI.get_or_init(|| parsed)
}

/// Access the parsed CLI arguments (defaults to empty args in unit tests).
pub(super) fn current() -> &'static Cli {
    CLI.get_or_init(Cli::default)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(args)
    }

    #[test]
    fn parses_editor_and_gallery_options() {
        let default = parse(&["margin"]).unwrap();
        assert!(!default.split && !default.demo && default.command.is_none());

        assert!(parse(&["margin", "--split"]).unwrap().split);

        assert!(parse(&["margin", "ui"]).unwrap().is_gallery());
        assert!(parse(&["margin", "gallery"]).unwrap().is_gallery());
        assert!(parse(&["margin", "--gallery"]).unwrap().is_gallery());

        let sync_inspect = parse(&["margin", "--sync-file", "a.sync", "--test-editor"]).unwrap();
        assert_eq!(sync_inspect.sync_file, Some(PathBuf::from("a.sync")));
        assert!(sync_inspect.test_editor);

        // The Topics window and its commands are `osom`'s now.
        assert!(parse(&["margin", "topics"]).is_err());
        assert!(parse(&["margin", "pair", "123456"]).is_err());
    }

    #[cfg(feature = "vault")]
    #[test]
    fn parses_vault_options() {
        let split_vault = parse(&["margin", "--split", "--vault", "/tmp/v", "--note", "r1"]).unwrap();
        assert!(split_vault.split);
        assert_eq!(split_vault.vault, Some(PathBuf::from("/tmp/v")));
        assert_eq!(split_vault.note.as_deref(), Some("r1"));
        assert!(parse(&["margin", "--sync-file", "a.sync", "--vault", "/tmp/v"]).is_err());
        assert!(parse(&["margin", "--vault", "/tmp/v", "ui"]).is_err());
    }

    #[cfg(not(feature = "vault"))]
    #[test]
    fn rejects_vault_options_without_the_vault() {
        for flag in ["--vault", "--note", "--passphrase-file"] {
            let error = parse(&["margin", flag, "x"]).unwrap_err().to_string();
            assert!(error.contains("--features vault"), "{flag}: {error}");
        }
    }
}
