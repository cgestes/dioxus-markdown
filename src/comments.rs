//! Comment threads anchored to the text of a page in the space, and the
//! protocol for asking an agent about them. Pure: no I/O.
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

/// How a comment thread's ZeroClaw session is named.
pub const SESSION_PREFIX: &str = "margin-comment-";

/// The ZeroClaw session that holds one thread's conversation.
pub fn session_id(thread: &str) -> String {
    format!("{SESSION_PREFIX}{thread}")
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

// ── Asking the agent ────────────────────────────────────────────────────────

/// Opens the block an agent ends its reply with to propose a replacement.
pub const FENCE: &str = "```margin-edit";

/// How to answer, restated on a thread's first turn.
pub const CONTRACT: &str = "Answer briefly in Markdown; cite other pages by path. To propose a change, end with exactly one ```margin-edit block: a label of at most three words, a line `---`, then the replacement for the selected text only (one line inside a table cell). Never edit files.";

/// Where a quote sits: the heading above it, and its table cell.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Place {
    pub section: Option<String>,
    /// (row label from the first column, column header)
    pub cell: Option<(String, String)>,
}

pub fn place(md: &str, at: usize) -> Place {
    let cell = table::tables(md)
        .into_iter()
        .find(|t| t.range.contains(&at))
        .and_then(|t| {
            let (row, col) = t.position(at);
            Some((t.cell_text(md, row, 0)?, t.cell_text(md, 0, col)?))
        });
    Place {
        section: table::heading_above(md, at).map(String::from),
        cell,
    }
}

/// One turn of a thread, as the agent receives it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ask {
    pub page: String,
    pub title: String,
    pub place: Place,
    pub quote: String,
    pub request: String,
    /// What the agent saw on its last turn of this thread; `None` before
    /// the first. A different quote is flagged as changed.
    pub seen: Option<String>,
}

/// Opens Margin's comment preamble; `topics::transcript` hides it.
pub const PREAMBLE_MARK: &str = "[Margin comment]";

/// What a new comment session learns before its first message: how to
/// answer, and the page. ZeroClaw sends it only when the session is new.
pub fn preamble(page: &str, markdown: &str) -> String {
    let mut out = format!("{PREAMBLE_MARK} {CONTRACT}\n\n<page path=\"{page}\">\n{markdown}");
    if !markdown.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("</page>");
    out
}

/// Margin's comment preamble at the start of a stored message, and what
/// follows it.
pub fn strip_preamble(text: &str) -> Option<&str> {
    let rest = text.strip_prefix(PREAMBLE_MARK)?;
    rest.split_once("</page>").map(|(_, after)| after.trim_start())
}

/// The message for a turn: page, place and quote (flagged when it changed
/// since the agent's last reply), then the request.
pub fn request_text(ask: &Ask) -> String {
    let mut out = String::new();
    {
        out.push_str(&format!("Comment on `{}`", ask.page));
        if !ask.title.is_empty() {
            out.push_str(&format!(" (“{}”)", ask.title));
        }
        if let Some(s) = &ask.place.section {
            out.push_str(&format!(", section “{s}”"));
        }
        if let Some((row, col)) = &ask.place.cell {
            out.push_str(&format!(", table cell: row “{row}”, column “{col}”"));
        }
        out.push_str(".\n\nSelected text");
        if ask.seen.as_ref().is_some_and(|seen| *seen != ask.quote) {
            out.push_str(" (changed since your last reply)");
        }
        out.push_str(":\n");
        for line in ask.quote.lines() {
            out.push_str("> ");
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    }
    out.push_str("Request: ");
    out.push_str(ask.request.trim());
    out
}

/// Everything one comment turn sends to the agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub session: String,
    /// The session's name in ZeroClaw's dashboard.
    pub name: String,
    /// Sent only if the session is new.
    pub preamble: String,
    pub content: String,
}

/// The turn for `request` about `range` of the page `md`, on thread
/// `thread`. `seen` is what the agent saw last time on this thread.
pub fn turn(thread: &str, page: &str, md: &str, range: Range<usize>, request: &str, seen: Option<String>) -> Turn {
    let range = md.floor_char_boundary(range.start.min(md.len()))..md.floor_char_boundary(range.end.min(md.len()));
    let ask = Ask {
        page: page.into(),
        title: crate::heading::page_title(md).unwrap_or_default().into(),
        place: place(md, range.start),
        quote: md[range].into(),
        request: request.into(),
        seen,
    };
    Turn {
        session: session_id(thread),
        name: format!("Comment · {page}"),
        preamble: preamble(page, md),
        content: request_text(&ask),
    }
}

/// The parts of a request: (quote, request), when it carries a quote.
/// The local stand-in agent answers from them.
pub fn parse_request(content: &str) -> (Option<String>, String) {
    let content = strip_preamble(content).unwrap_or(content);
    let (head, request) = content.rsplit_once("Request: ").unwrap_or(("", content));
    let quote: Vec<&str> = head.lines().filter_map(|l| l.strip_prefix("> ")).collect();
    ((!quote.is_empty()).then(|| quote.join("\n")), request.trim().to_string())
}

/// The start of the last line-initial `FENCE`, if any.
fn fence_at(reply: &str) -> Option<usize> {
    reply
        .rmatch_indices(FENCE)
        .map(|(i, _)| i)
        .find(|&i| i == 0 || reply.as_bytes()[i - 1] == b'\n')
}

/// What to show of a reply, complete or still streaming: the text before
/// the edit block. A trailing piece that could still become the fence is
/// held back so the block never flashes on screen.
pub fn visible(reply: &str) -> &str {
    if let Some(at) = fence_at(reply) {
        return reply[..at].trim_end();
    }
    let opening = format!("\n{FENCE}");
    let held = (1..=opening.len().min(reply.len()))
        .rev()
        .find(|&k| reply.is_char_boundary(reply.len() - k) && opening.starts_with(&reply[reply.len() - k..]))
        .unwrap_or(0);
    // At the very start there is no newline before the fence.
    let held = if held == 0 && FENCE.starts_with(reply) { reply.len() } else { held };
    &reply[..reply.len() - held]
}

/// A proposed replacement parsed from a finished reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub label: String,
    pub replacement: String,
}

pub fn edit(reply: &str) -> Option<Edit> {
    let at = fence_at(reply)?;
    let body = &reply[at + FENCE.len()..];
    let body = body.split_once('\n').map_or("", |(_, rest)| rest);
    let body = match body.rfind("\n```") {
        Some(end) => &body[..end],
        None => body.strip_suffix("```").unwrap_or(body),
    };
    let (label, replacement) = match body.split_once("\n---\n") {
        Some((label, rest)) => (label.trim(), rest),
        None => match body.strip_prefix("---\n") {
            Some(rest) => ("", rest),
            None => ("", body),
        },
    };
    let replacement = replacement.trim_matches('\n');
    if replacement.trim().is_empty() {
        return None;
    }
    Some(Edit {
        label: if label.is_empty() { "Apply suggestion".into() } else { label.into() },
        replacement: replacement.into(),
    })
}

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
    fn edit_blocks_parse_and_never_show() {
        let reply = "Yes, yearly (see `car/log.md`).\n\n```margin-edit\nShorten text\n---\nOil + filter, yearly\n```\n";
        assert_eq!(visible(reply), "Yes, yearly (see `car/log.md`).");
        assert_eq!(
            edit(reply),
            Some(Edit { label: "Shorten text".into(), replacement: "Oil + filter, yearly".into() })
        );
        // While streaming, a partial fence is held back.
        assert_eq!(visible("Done.\n``"), "Done.");
        assert_eq!(visible("Done.\n```margin-ed"), "Done.");
        assert_eq!(visible("Done.\n```rust\nfn x"), "Done.\n```rust\nfn x");
        assert_eq!(visible("```marg"), "");
        // Without a label or closing fence.
        let e = edit("Ok\n```margin-edit\n---\nNew text").unwrap();
        assert_eq!((e.label.as_str(), e.replacement.as_str()), ("Apply suggestion", "New text"));
        assert_eq!(edit("Just an answer."), None);
        assert_eq!(edit("```margin-edit\nLabel\n---\n\n```"), None);
    }

    #[test]
    fn requests_carry_the_quote_and_flag_a_change() {
        let md = "# Car — maintenance\n\n## Schedule\n\n| Item | Next due |\n| --- | --- |\n| Cabin filter | 2027-04 |\n";
        let at = md.find("2027-04").unwrap();
        let p = place(md, at);
        assert_eq!(p.section.as_deref(), Some("Schedule"));
        assert_eq!(p.cell, Some(("Cabin filter".into(), "Next due".into())));
        let ask = Ask {
            page: "car/maintenance.md".into(),
            title: "Car".into(),
            place: p,
            quote: "2027-04".into(),
            request: "Is this right?".into(),
            seen: None,
        };
        let first = request_text(&ask);
        assert!(first.starts_with("Comment on `car/maintenance.md` (“Car”), section “Schedule”, table cell: row “Cabin filter”, column “Next due”."));
        assert!(first.contains("> 2027-04\n") && first.ends_with("Request: Is this right?"));
        let again = request_text(&Ask { seen: Some("2027-04".into()), request: "Shorter".into(), ..ask.clone() });
        assert!(again.contains("Selected text:\n> 2027-04") && again.ends_with("Request: Shorter"));
        let changed = request_text(&Ask { seen: Some("2027-03".into()), ..ask.clone() });
        assert!(changed.contains("Selected text (changed since your last reply):\n> 2027-04"));
        // The preamble carries the contract and the page, and is hidden again.
        let pre = preamble("car/maintenance.md", md);
        assert!(pre.contains(CONTRACT) && pre.ends_with("| Cabin filter | 2027-04 |\n</page>"));
        let stored = format!("{pre}\n\n{first}");
        assert_eq!(strip_preamble(&stored), Some(first.as_str()));
        assert_eq!(parse_request(&stored), (Some("2027-04".into()), "Is this right?".into()));
        assert_eq!(parse_request("Request: Shorter"), (None, "Shorter".into()));
        assert_eq!(parse_request(&again), (Some("2027-04".into()), "Shorter".into()));
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
        assert_eq!(session_id("0018b"), "margin-comment-0018b");
    }
}
