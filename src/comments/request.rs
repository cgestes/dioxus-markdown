//! Asking an agent about a comment thread: the message for one turn, the
//! session it goes to, and how the reply is read (the text to show, and the
//! edit it proposes). Pure: no I/O.
use crate::{heading, table};
use std::ops::Range;

/// How a comment thread's ZeroClaw session is named.
pub const SESSION_PREFIX: &str = "margin-comment-";

/// The ZeroClaw session that holds one thread's conversation.
pub fn session_id(thread: &str) -> String {
    format!("{SESSION_PREFIX}{thread}")
}

/// Opens the block an agent ends its reply with to propose a replacement.
pub const FENCE: &str = "```margin-edit";

/// How to answer, restated on a thread's first turn.
const CONTRACT: &str = "Answer briefly in Markdown; cite other pages by path. To propose a change, end with exactly one ```margin-edit block: a label of at most three words, a line `---`, then the replacement for the selected text only (one line inside a table cell). Never edit files.";

/// Where a quote sits: the heading above it, and its table cell.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Place {
    pub section: Option<String>,
    /// (row label from the first column, column header)
    pub cell: Option<(String, String)>,
}

fn place(md: &str, at: usize) -> Place {
    let cell = table::tables(md)
        .into_iter()
        .find(|t| t.range.contains(&at))
        .and_then(|t| {
            let (row, col) = t.position(at);
            Some((t.cell_text(md, row, 0)?, t.cell_text(md, 0, col)?))
        });
    Place {
        section: heading::above(md, at).map(String::from),
        cell,
    }
}

/// One turn of a thread, as the agent receives it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Request {
    pub page: String,
    pub title: String,
    pub place: Place,
    pub quote: String,
    pub request: String,
    /// What the agent saw on its last turn of this thread; `None` before
    /// the first. A different quote is flagged as changed.
    pub seen: Option<String>,
}

/// Opens Margin's comment preamble; `osom_topics::transcript` hides it
/// (`strip_preamble`).
const PREAMBLE_MARK: &str = "[Margin comment]";

/// What a new comment session learns before its first message: how to
/// answer, and the page. ZeroClaw sends it only when the session is new.
fn preamble(page: &str, markdown: &str) -> String {
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
fn request_text(ask: &Request) -> String {
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
    let ask = Request {
        page: page.into(),
        title: heading::page_title(md).unwrap_or_default().into(),
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
        let ask = Request {
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
        let again = request_text(&Request { seen: Some("2027-04".into()), request: "Shorter".into(), ..ask.clone() });
        assert!(again.contains("Selected text:\n> 2027-04") && again.ends_with("Request: Shorter"));
        let changed = request_text(&Request { seen: Some("2027-03".into()), ..ask.clone() });
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
}
