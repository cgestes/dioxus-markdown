//! Command-line interface for the `margin` native binary.
use super::topics::Tab as TopicTab;
use clap::{Args, Parser, Subcommand};
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::OnceLock;

static CLI: OnceLock<Cli> = OnceLock::new();

#[derive(Clone, Debug, Default, Parser)]
#[command(
    name = "margin",
    about = "Margin — local-first Markdown notebook, Topics workspace, and UI gallery",
    after_help = "Run `margin` with no subcommand to open your personal notebook editor.",
    args_conflicts_with_subcommands = true
)]
pub(super) struct Cli {
    /// Start the editor in Split view (Markdown source + reading preview).
    #[arg(long)]
    pub split: bool,

    /// Personal vault directory (defaults to Margin's app data directory).
    #[arg(
        long,
        value_name = "DIR",
        conflicts_with_all = ["sync_file", "demo", "test_editor", "test_window"]
    )]
    pub vault: Option<PathBuf>,

    /// Open a specific note room in the personal vault.
    #[arg(
        long,
        value_name = "ROOM",
        conflicts_with_all = ["sync_file", "demo", "test_editor", "test_window"]
    )]
    pub note: Option<String>,

    /// Unlock a passphrase-protected vault from a file (12–4096 bytes).
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with_all = ["sync_file", "demo", "test_editor", "test_window"]
    )]
    pub passphrase_file: Option<PathBuf>,

    /// Open a standalone unencrypted Loro sync file instead of the personal vault.
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
    /// Open the Topics workspace (SilverBullet Space + ZeroClaw agent).
    Topics(TopicsArgs),

    /// Open the Margin design-system component gallery.
    #[command(alias = "gallery")]
    Ui,

    /// Configure the SilverBullet Space, ZeroClaw gateway, and comment agent URLs.
    Setup {
        /// SilverBullet Space base URL.
        space: String,
        /// ZeroClaw gateway base URL.
        zeroclaw: String,
        /// Agent name used for comment replies.
        agent: String,
    },

    /// Pair this device with ZeroClaw using a one-time paircode.
    Pair {
        /// Code from `zeroclaw gateway get-paircode --new`.
        code: String,
    },

    /// Run one throwaway comment-agent turn against a Space page and print the reply.
    CommentTest {
        /// Space page path (e.g. `car/maintenance.md`).
        page: String,
        /// Exact quote to anchor inside the page.
        quote: String,
        /// Instruction for the comment agent.
        request: String,
    },
}

#[derive(Clone, Debug, Default, Args)]
pub(super) struct TopicsArgs {
    /// Run offline with sample topics and a stand-in agent.
    #[arg(long)]
    pub demo: bool,

    /// Print a text summary of the live topics to stdout without opening a window.
    #[arg(long, conflicts_with_all = ["demo", "open", "tab", "doc"])]
    pub print: bool,

    /// Place to open: `today`, `inbox`, or a topic id (e.g. `house`).
    #[arg(long, value_name = "PLACE")]
    pub open: Option<String>,

    /// Topic tab to open initially.
    #[arg(long, value_enum)]
    pub tab: Option<TopicTab>,

    /// Space page path to open directly in the embedded editor.
    #[arg(long, value_name = "PATH")]
    pub doc: Option<String>,
}

impl Cli {
    pub(super) fn is_gallery(&self) -> bool {
        self.gallery || matches!(self.command, Some(Command::Ui))
    }

    pub(super) fn topics_args(&self) -> Option<&TopicsArgs> {
        match &self.command {
            Some(Command::Topics(args)) => Some(args),
            _ => None,
        }
    }
}

/// Normalize legacy `--topics` leading flag into the `topics` subcommand.
fn normalize_args(args: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    let mut out: Vec<OsString> = args.into_iter().collect();
    if out.get(1).is_some_and(|a| a == "--topics") {
        out[1] = OsString::from("topics");
    }
    out
}

/// Parse CLI arguments from the process environment and store them for the app.
pub(super) fn init() -> &'static Cli {
    let parsed = Cli::parse_from(normalize_args(std::env::args_os()));
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
        let os = args.iter().map(OsString::from);
        Cli::try_parse_from(normalize_args(os))
    }

    #[test]
    fn parses_editor_topics_gallery_and_headless_commands() {
        let default = parse(&["margin"]).unwrap();
        assert!(!default.split && !default.demo && default.command.is_none());

        let split_vault = parse(&["margin", "--split", "--vault", "/tmp/v", "--note", "r1"]).unwrap();
        assert!(split_vault.split);
        assert_eq!(split_vault.vault, Some(PathBuf::from("/tmp/v")));
        assert_eq!(split_vault.note.as_deref(), Some("r1"));

        let topics = parse(&[
            "margin", "topics", "--demo", "--open", "house", "--tab", "docs", "--doc", "car/log.md",
        ])
        .unwrap();
        let t = topics.topics_args().expect("topics subcommand");
        assert!(t.demo);
        assert_eq!(t.open.as_deref(), Some("house"));
        assert_eq!(t.tab, Some(TopicTab::Docs));
        assert_eq!(t.doc.as_deref(), Some("car/log.md"));

        let legacy_topics = parse(&["margin", "--topics", "--print"]).unwrap();
        assert!(legacy_topics.topics_args().expect("legacy --topics").print);

        assert!(parse(&["margin", "ui"]).unwrap().is_gallery());
        assert!(parse(&["margin", "--gallery"]).unwrap().is_gallery());

        let sync_inspect = parse(&["margin", "--sync-file", "a.sync", "--test-editor"]).unwrap();
        assert_eq!(sync_inspect.sync_file, Some(PathBuf::from("a.sync")));
        assert!(sync_inspect.test_editor);

        assert!(matches!(
            parse(&["margin", "setup", "https://s", "https://z", "agent"]).unwrap().command,
            Some(Command::Setup { .. })
        ));
        assert!(matches!(
            parse(&["margin", "pair", "123456"]).unwrap().command,
            Some(Command::Pair { .. })
        ));
        assert!(matches!(
            parse(&["margin", "comment-test", "car/log.md", "quote", "req"]).unwrap().command,
            Some(Command::CommentTest { .. })
        ));

        assert!(parse(&["margin", "--sync-file", "a.sync", "--vault", "/tmp/v"]).is_err());
        assert!(parse(&["margin", "--vault", "/tmp/v", "topics"]).is_err());
        assert!(parse(&["margin", "--gallery", "topics"]).is_err());
        assert!(parse(&["margin", "topics", "--print", "--demo"]).is_err());
        assert!(parse(&["margin", "topics", "--tab", "bogus"]).is_err());
    }
}
