//! Which conversations belong to which folder: the routes in the space README.
use crate::page::{blank, inline, md_tables, plain};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Source {
    Telegram,
    Slack,
    Chat,
    #[default]
    Other,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Self::Telegram => "Telegram",
            Self::Slack => "Slack",
            Self::Chat => "Chat",
            Self::Other => "Channel",
        }
    }
}

/// One row of the space README's "Channel → folder" table.
#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    pub label: String,
    pub channel: String,
    /// Folder or page id: `car`, `people/nikki-sample`.
    pub target: String,
}

impl Route {
    /// Whether a session key belongs to this route's chat. Keys are
    /// `_`-separated, so the chat id must be a whole part of the key.
    pub fn matches(&self, key: &str) -> bool {
        key.split('_').any(|part| part == self.channel)
    }
}

pub fn routes(readme: &str) -> Vec<Route> {
    let mut out = Vec::new();
    for t in md_tables(readme) {
        let (Some(id), Some(folder)) = (t.column("Channel ID"), t.column("Folder")) else {
            continue;
        };
        for row in &t.rows {
            let channel = row.get(id).map(|s| plain(s)).unwrap_or_default();
            if blank(&channel) {
                continue;
            }
            let cell = inline(row.get(folder).map(String::as_str).unwrap_or_default());
            let target = cell
                .code
                .into_iter()
                .next()
                .unwrap_or_else(|| cell.text.split_whitespace().next().unwrap_or_default().into());
            let target = target.trim_end_matches('/').trim_end_matches(".md").to_string();
            if target.is_empty() {
                continue;
            }
            out.push(Route {
                label: plain(row.first().map(String::as_str).unwrap_or_default()),
                channel,
                target,
            });
        }
    }
    out
}
