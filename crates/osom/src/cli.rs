//! Command-line interface for `osom`.
use crate::Tab;
use clap::{Args, Parser, Subcommand};
use std::sync::OnceLock;

static CLI: OnceLock<Cli> = OnceLock::new();

#[derive(Clone, Debug, Default, Parser)]
#[command(
    name = "osom",
    about = "Osom — your agent's topics: the SilverBullet space and the ZeroClaw agent that keeps it",
    after_help = "Run `osom` with no subcommand to open the Topics window.",
    args_conflicts_with_subcommands = true
)]
pub struct Cli {
    #[command(flatten)]
    pub window: TopicsArgs,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    /// Open the Topics window (the same flags as `osom` itself).
    Topics(TopicsArgs),

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
pub struct TopicsArgs {
    /// Print a text summary of the live topics to stdout without opening a window.
    #[arg(long, conflicts_with_all = ["open", "tab", "doc"])]
    pub print: bool,

    /// Place to open: `today`, `inbox`, or a topic id (e.g. `house`).
    #[arg(long, value_name = "PLACE")]
    pub open: Option<String>,

    /// Topic tab to open initially.
    #[arg(long, value_enum)]
    pub tab: Option<Tab>,

    /// Space page path to open directly in the embedded editor.
    #[arg(long, value_name = "PATH")]
    pub doc: Option<String>,
}

impl Cli {
    /// The window's flags: from `osom topics …`, or given to `osom` directly.
    pub fn topics_args(&self) -> &TopicsArgs {
        match &self.command {
            Some(Command::Topics(args)) => args,
            _ => &self.window,
        }
    }
}

/// Parse the process's arguments and keep them for the window.
pub fn init() -> &'static Cli {
    let parsed = Cli::parse();
    CLI.get_or_init(|| parsed)
}

/// The parsed arguments (empty in unit tests).
pub fn current() -> &'static Cli {
    CLI.get_or_init(Cli::default)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(args)
    }

    #[test]
    fn parses_window_flags_and_headless_commands() {
        let default = parse(&["osom"]).unwrap();
        assert!(!default.topics_args().print && default.command.is_none());

        let direct = parse(&["osom", "--open", "house", "--tab", "docs", "--doc", "car/log.md"]).unwrap();
        let t = direct.topics_args();
        assert_eq!(t.open.as_deref(), Some("house"));
        assert_eq!(t.tab, Some(Tab::Docs));
        assert_eq!(t.doc.as_deref(), Some("car/log.md"));

        let sub = parse(&["osom", "topics", "--tab", "calendar"]).unwrap();
        assert_eq!(sub.topics_args().tab, Some(Tab::Calendar));
        assert!(parse(&["osom", "topics", "--print"]).unwrap().topics_args().print);
        assert!(parse(&["osom", "--print"]).unwrap().topics_args().print);

        assert!(matches!(
            parse(&["osom", "setup", "https://s", "https://z", "agent"]).unwrap().command,
            Some(Command::Setup { .. })
        ));
        assert!(matches!(parse(&["osom", "pair", "123456"]).unwrap().command, Some(Command::Pair { .. })));
        assert!(matches!(
            parse(&["osom", "comment-test", "car/log.md", "quote", "req"]).unwrap().command,
            Some(Command::CommentTest { .. })
        ));

        assert!(parse(&["osom", "--demo"]).is_err());
        assert!(parse(&["osom", "topics", "--print", "--open", "car"]).is_err());
        assert!(parse(&["osom", "--tab", "bogus"]).is_err());
    }
}
