//! Comment threads anchored to the text of a page in the space: the comment
//! file. What an agent is asked about a thread, and how its reply is read, is
//! in `request`. Pure: no I/O.
//!
//! Threads live next to their page in `_margin/comments/<page>.json` so the
//! agent, SilverBullet and git all see plain files. While a page is open the
//! editor tracks each thread with Loro cursors; on disk an [`Anchor`] keeps
//! the quoted text with some context, so a thread finds its text again after
//! the page was edited elsewhere (by the agent, in SilverBullet, through git).
use crate::table;
use serde::{Deserialize, Serialize};
use std::ops::Range;

/// Folder of the comment files inside the space. SilverBullet refuses
/// dot-paths, hence the underscore.
pub const ROOT: &str = "_margin";

/// The comment file of a page: `car/maintenance.md` →
/// `_margin/comments/car/maintenance.md.json`.
pub fn sidecar_path(page: &str) -> String {
    format!("{ROOT}/comments/{page}.json")
}

/// A time-ordered id: 13 hex digits of milliseconds, then 8 of entropy.
pub fn new_id(unix_ms: u64, entropy: u64) -> String {
    format!("{unix_ms:013x}{:08x}", entropy & 0xffff_ffff)
}

/// Characters of context kept on each side of a quote.
const CONTEXT: usize = 32;

/// A quote and the text around it (W3C `TextQuoteSelector`), plus where it
/// last was.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    pub exact: String,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub suffix: String,
    /// UTF-8 byte offset of the quote when it was last seen.
    #[serde(default)]
    pub start: usize,
}

impl Anchor {
    pub fn capture(md: &str, range: Range<usize>) -> Self {
        let start = md.floor_char_boundary(range.start.min(md.len()));
        let end = md.floor_char_boundary(range.end.min(md.len())).max(start);
        let before = &md[..start];
        let prefix_at = before
            .char_indices()
            .rev()
            .nth(CONTEXT - 1)
            .map_or(0, |(i, _)| i);
        let after = &md[end..];
        let suffix_end = after
            .char_indices()
            .nth(CONTEXT)
            .map_or(after.len(), |(i, _)| i);
        Self {
            exact: md[start..end].into(),
            prefix: before[prefix_at..].into(),
            suffix: after[..suffix_end].into(),
            start,
        }
    }

    /// Where the quote is now: of every occurrence, the one whose
    /// surroundings match best, then the one nearest its last position.
    pub fn resolve(&self, md: &str) -> Option<Range<usize>> {
        if self.exact.is_empty() {
            return None;
        }
        md.match_indices(self.exact.as_str())
            .map(|(at, _)| {
                let end = at + self.exact.len();
                let before = common_suffix(&md[..at], &self.prefix);
                let after = common_prefix(&md[end..], &self.suffix);
                (before + after, at.abs_diff(self.start), at)
            })
            .max_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)))
            .map(|(_, _, at)| at..at + self.exact.len())
    }
}

fn common_prefix(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

fn common_suffix(a: &str, b: &str) -> usize {
    a.chars()
        .rev()
        .zip(b.chars().rev())
        .take_while(|(x, y)| x == y)
        .count()
}

/// Where a proposal stands. Whether it still fits the page is not stored:
/// it is read from the page (the text under the anchor against `old`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProposalState {
    #[default]
    Open,
    Applied,
    Dismissed,
}

impl Proposal {
    /// The text a proposal expects under its anchor: the original before it
    /// is applied, the replacement after.
    pub fn expects(&self) -> &str {
        match self.state {
            ProposalState::Applied => &self.new,
            _ => &self.old,
        }
    }

    /// Whether `current` (the text under the anchor now) is what this
    /// proposal was made for. Cell text compares unescaped and on one line.
    pub fn fits(&self, current: &str) -> bool {
        let norm = |t: &str| table::decoded(t).replace(['\r', '\n'], " ").trim().to_string();
        norm(current) == norm(self.expects())
    }
}

/// A replacement for a thread's quoted text.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub label: String,
    /// The text the agent saw.
    pub old: String,
    pub new: String,
    #[serde(default)]
    pub state: ProposalState,
    /// When `state` last changed (RFC 3339).
    #[serde(default)]
    pub updated: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    /// `you`, or the agent's alias.
    pub author: String,
    #[serde(default)]
    pub ai: bool,
    #[serde(default)]
    pub at: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal: Option<Proposal>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    pub id: String,
    #[serde(default)]
    pub created: String,
    /// When `resolved` or `deleted` last changed (RFC 3339).
    #[serde(default)]
    pub updated: String,
    #[serde(default)]
    pub resolved: bool,
    /// Kept as a tombstone so a merge does not bring the thread back.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deleted: bool,
    pub anchor: Anchor,
    #[serde(default)]
    pub messages: Vec<Message>,
}

/// The comment file of one page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sidecar {
    pub version: u32,
    pub page: String,
    #[serde(default)]
    pub threads: Vec<Thread>,
}

impl Sidecar {
    pub fn new(page: &str) -> Self {
        Self {
            version: 1,
            page: page.into(),
            threads: Vec::new(),
        }
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| format!("comment file: {e}"))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("comments serialize") + "\n"
    }
}

/// Combine two versions of a comment file: threads and messages are unions
/// by id (messages are append-only); for a thread's `resolved`/`deleted` and
/// a proposal's `state`, the later change wins. Threads keep `ours` order,
/// with threads only `theirs` has appended.
pub fn merge(ours: &Sidecar, theirs: &Sidecar) -> Sidecar {
    let mut out = ours.clone();
    for t in &theirs.threads {
        let Some(mine) = out.threads.iter_mut().find(|m| m.id == t.id) else {
            out.threads.push(t.clone());
            continue;
        };
        if t.updated > mine.updated {
            mine.resolved = t.resolved;
            mine.deleted = t.deleted;
            mine.updated = t.updated.clone();
        }
        for m in &t.messages {
            match mine.messages.iter_mut().find(|x| x.id == m.id) {
                None => mine.messages.push(m.clone()),
                Some(x) => {
                    if let (Some(p), Some(q)) = (&mut x.proposal, &m.proposal)
                        && q.updated > p.updated
                    {
                        p.state = q.state;
                        p.updated = q.updated.clone();
                    }
                }
            }
        }
        mine.messages.sort_by(|a, b| a.at.cmp(&b.at).then(a.id.cmp(&b.id)));
    }
    out
}

pub mod request;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_find_their_quote_after_edits_elsewhere() {
        let md = "# Car\n\nOil every year. Brakes every year. Tyres every year.\n";
        let at = md.find("every year. Tyres").unwrap();
        let a = Anchor::capture(md, at..at + "every year".len());
        assert_eq!(a.exact, "every year");
        assert!(a.prefix.ends_with("Brakes "));
        assert!(a.suffix.starts_with(". Tyres"));
        // The same words appear three times; context picks the right one
        // even after text was inserted before it.
        let edited = md.replace("# Car\n", "# Car — Mazda 3\n\nIntro line.\n");
        let r = a.resolve(&edited).unwrap();
        assert_eq!(&edited[r.start - 7..r.start], "Brakes ");
        // Gone entirely: detached.
        assert_eq!(a.resolve("# Car\n\nNothing here.\n"), None);
    }

    #[test]
    fn anchors_keep_char_boundaries() {
        let md = "Été — café crème ☕ près du four.";
        let at = md.find("crème").unwrap();
        let a = Anchor::capture(md, at..at + "crème".len());
        assert_eq!(a.resolve(md), Some(at..at + "crème".len()));
        assert_eq!(a.prefix, "Été — café ");
    }

    #[test]
    fn proposals_fit_the_text_they_were_made_for() {
        let mut p = Proposal { old: "Water | gate".into(), new: "Rain".into(), ..Default::default() };
        assert!(p.fits("Water \\| gate"), "cell text compares unescaped");
        assert!(!p.fits("Rain"));
        p.state = ProposalState::Applied;
        assert!(p.fits("Rain") && !p.fits("Water | gate"));
    }

    #[test]
    fn merging_unions_messages_and_keeps_the_later_state() {
        let msg = |id: &str, at: &str| Message { id: id.into(), author: "you".into(), at: at.into(), body: id.into(), ..Default::default() };
        let mut base = Sidecar::new("p.md");
        base.threads.push(Thread { id: "t1".into(), messages: vec![msg("m1", "1")], ..Default::default() });
        let mut ours = base.clone();
        ours.threads[0].messages.push(msg("m2", "2"));
        ours.threads[0].resolved = true;
        ours.threads[0].updated = "5".into();
        let mut theirs = base.clone();
        theirs.threads[0].messages.push(msg("m3", "3"));
        theirs.threads[0].updated = "4".into();
        theirs.threads.push(Thread { id: "t2".into(), ..Default::default() });
        let m = merge(&ours, &theirs);
        let ids: Vec<_> = m.threads[0].messages.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, ["m1", "m2", "m3"]);
        assert!(m.threads[0].resolved, "our later resolve wins");
        assert_eq!(m.threads.len(), 2);
        let round = Sidecar::parse(&m.to_json()).unwrap();
        assert_eq!(round, m);
    }

    #[test]
    fn ids_sort_by_time_and_paths_are_stable() {
        assert!(new_id(1_700_000_000_000, 7) < new_id(1_700_000_000_001, 1));
        assert_eq!(new_id(1, 0xabc).len(), 21);
        assert_eq!(sidecar_path("car/maintenance.md"), "_margin/comments/car/maintenance.md.json");
        assert_eq!(request::session_id("0018b"), "margin-comment-0018b");
    }
}
