//! Anchored comments. A thread is the comment file's own `file::Thread` plus
//! what only this session knows (Loro cursors, the reply being written). An
//! agent answers through `agent::AgentChat`: ZeroClaw's read-only agent on a
//! page of the space, a local stand-in elsewhere; both stream the same way.
//! On a page, threads save to `_margin/comments/<page>.json` (see `page`).
use crate::notebook::page::{PageSource, WriteError};
use crate::platform::text;
use crate::ui::{Badge, Banner, Button, Card, Chip, Composer, Icon, Message, MessageStyle, Segment, Segmented, Tone, Variant};
use margin_runtime::worker::{batch, turn, work};
use crate::{Field, MarkdownView, Writing, report};
use crate::prelude::*;
use loro::cursor::Cursor;
use margin_core::comments::{self as file, ProposalState, request};
use margin_core::agent::{self, AgentChat, Canned, ChatEvent};
use margin_note::note::Note;
use margin_core::{dates, table};
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

/// Who answers comments.
#[derive(Clone)]
pub struct CommentAgent {
    pub alias: String,
    pub chat: Arc<dyn AgentChat>,
}

impl PartialEq for CommentAgent {
    fn eq(&self, other: &Self) -> bool {
        self.alias == other.alias && Arc::ptr_eq(&self.chat, &other.chat)
    }
}

impl CommentAgent {
    /// The local stand-in: an answer and an edit made from the quote and the
    /// request, at once.
    pub fn mock() -> Self {
        Self {
            alias: "Claude".into(),
            chat: Arc::new(Canned { reply: Box::new(mock_reply), pause: None }),
        }
    }

    /// For a page whose agent cannot be reached: every answer fails, saying why.
    pub fn unavailable(alias: &str, why: &str) -> Self {
        Self { alias: alias.into(), chat: Arc::new(Unavailable(why.into())) }
    }
}

struct Unavailable(String);

impl AgentChat for Unavailable {
    fn chat(&self, _: &agent::Ask, on: &mut dyn FnMut(ChatEvent) -> bool, _: &mut dyn FnMut() -> bool) {
        on(ChatEvent::Error(self.0.clone()));
    }

    fn inline(&self) -> bool {
        true
    }
}

impl Default for CommentAgent {
    fn default() -> Self {
        Self::mock()
    }
}

/// A thread: the comment file's record and this session's view of it. The
/// personal vault stores it as JSON; what only lives for a session is skipped.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CommentThread {
    /// Handle in this window's DOM ids (`comment-thread-3`).
    pub id: usize,
    /// What the comment file holds. Its anchor and proposal states are those
    /// of the last saved page; the live ones are `cursors` and `saved`'s keys.
    pub file: file::Thread,
    pub cursors: Option<(Cursor, Cursor)>,
    /// The quote the agent saw on its last reply.
    pub seen: Option<String>,
    /// The reply being written (its text streams in `Discussion::stream`).
    #[serde(skip)]
    pub pending: Option<String>,
    /// Replies that failed; shown, never saved.
    #[serde(skip)]
    pub failed: Vec<String>,
    /// Proposal states as of the last saved page, by message id.
    pub saved: HashMap<String, ProposalState>,
}

/// The notebook's comment threads, the pane that shows them, and the
/// agent that answers.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Discussion {
    pub threads: Signal<Vec<CommentThread>>,
    /// Whether the comments pane is open.
    pub open: Signal<bool>,
    /// The thread the pane shows expanded, by id.
    pub active: Signal<Option<usize>>,
    /// A comment being written on a selection, before it becomes a thread.
    pub draft: Signal<Option<DraftComment>>,
    /// Who answers comments: the page's agent, or the local stand-in.
    pub agent: Signal<CommentAgent>,
    /// The page's comment file on the server, when the notebook edits a page.
    pub file: Signal<CommentFile>,
    /// Replies still arriving, by message id; only their bubbles re-render.
    pub stream: Signal<HashMap<String, String>>,
    /// Where every thread is now, and the text under it.
    pub ranges: Memo<Vec<Placed>>,
}

/// The page's comment file: its version, and saves in flight.
#[derive(Clone, Default)]
pub struct CommentFile {
    etag: Option<String>,
    /// Read (or known absent): only then may it be written.
    loaded: bool,
    saving: bool,
    again: bool,
    /// Deleted threads, kept so a merge does not bring them back.
    tombstones: Vec<file::Thread>,
    pub error: Option<String>,
}

// Read by the test driver (`crate::testing`).
#[cfg(not(target_arch = "wasm32"))]
impl CommentFile {
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    pub fn is_saving(&self) -> bool {
        self.saving || self.again
    }
}

fn new_key() -> String {
    use std::hash::BuildHasher;
    let ms = dates::now_ms();
    file::new_id(ms, std::collections::hash_map::RandomState::new().hash_one(ms))
}

fn message(author: &str, ai: bool, body: &str) -> file::Message {
    file::Message {
        id: new_key(),
        author: author.into(),
        ai,
        at: dates::stamp(),
        body: body.into(),
        proposal: None,
    }
}

#[derive(Clone, PartialEq)]
pub struct DraftComment {
    pub range: Range<usize>,
    pub quote: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CommentFilter {
    Open,
    Resolved,
    All,
}

/// A thread on `quote` in `md`: your question and an answer proposing `new`.
pub fn sample_thread(
    note: &Note,
    md: &str,
    id: usize,
    quote: &str,
    question: &str,
    answer: &str,
    label: &str,
    new: &str,
) -> Option<CommentThread> {
    let at = md.find(quote)?;
    let range = at..at + quote.len();
    let mut reply = message("Claude", true, answer);
    reply.proposal = Some(file::Proposal {
        label: label.into(),
        old: quote.into(),
        new: new.into(),
        ..Default::default()
    });
    Some(CommentThread {
        id,
        cursors: note.anchor_range_in(md, range.clone()),
        file: file::Thread {
            id: format!("sample-{id}"),
            anchor: file::Anchor::capture(md, range),
            messages: vec![message("You", false, question), reply],
            ..Default::default()
        },
        ..Default::default()
    })
}

/// Two demo threads anchored to phrases in `SAMPLE`.
pub fn seed_sample_comments(note: &Note) -> Vec<CommentThread> {
    let md = note.markdown();
    [
        sample_thread(
            note,
            &md,
            1,
            "Water through the gate",
            "Specify the mic setup so this take is reproducible?",
            "Good call — noting the stereo pattern and high-pass filter keeps the log actionable:",
            "Update table cell",
            "Water through the gate (XY stereo, 80 Hz HPF)",
        ),
        sample_thread(
            note,
            &md,
            2,
            "Leave **ten seconds** of silence.",
            "@ai tighten this reminder so it scans faster in the field",
            "Here is a crisper version that keeps the bold emphasis:",
            "Tighten checklist item",
            "Record **10 s** of room tone first.",
        ),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// The stand-in's answer, written the way an agent writes one: the reply,
/// then a `margin-edit` block.
fn mock_reply(ask: &agent::Ask) -> String {
    let (quote, request) = request::parse_request(&ask.content);
    let (body, label, new) = mock_edit(&quote.unwrap_or_default(), &request, ask.content.contains("table cell:"));
    format!("{body}\n\n{}\n{label}\n---\n{new}\n```", request::FENCE)
}

/// A grounded reply and edit from the quote and the request.
fn mock_edit(quote: &str, prompt: &str, is_cell: bool) -> (String, String, String) {
    let trimmed = quote.trim();
    let lower = prompt.to_ascii_lowercase();
    if let Some(rest) = extract_after(prompt, &["replace with ", "change to ", "use ", "rewrite as "]) {
        let cleaned = rest.trim().trim_matches(['"', '\'']).to_owned();
        (
            "Updated the proposal with your exact phrasing:".to_owned(),
            "Replace selection".to_owned(),
            cleaned,
        )
    } else if let Some(extra) = extract_after(prompt, &["mention ", "add ", "include "]) {
        let detail = extra.trim().trim_end_matches(['.', '?', '!']);
        let base = trimmed.trim_end_matches('.');
        let punct = if trimmed.ends_with('.') { "." } else { "" };
        (
            format!("Added \"{detail}\" cleanly inline:"),
            "Add detail".to_owned(),
            format!("{base} ({detail}){punct}"),
        )
    } else if lower.contains("short") || lower.contains("tight") || lower.contains("concise") {
        let words: Vec<&str> = trimmed.split_whitespace().collect();
        let shortened = if words.len() > 4 {
            let cut = (words.len() / 2).max(3);
            let mut s = words[..cut].join(" ");
            if trimmed.ends_with('.') && !s.ends_with('.') {
                s.push('.');
            }
            s
        } else {
            format!("**{}**", trimmed.trim_matches('*'))
        };
        (
            "Trimmed it down for faster scanning:".to_owned(),
            "Shorten text".to_owned(),
            shortened,
        )
    } else if lower.contains("another") || lower.contains("option") || lower.contains("alt") {
        let alt = if trimmed.contains('(') {
            trimmed.split('(').next().unwrap_or(trimmed).trim().to_owned() + " — verified"
        } else {
            format!("{} — field note", trimmed.trim_end_matches('.'))
        };
        ("Here is an alternative phrasing:".to_owned(), "Alternative phrasing".to_owned(), alt)
    } else if ["polish", "vivid", "rewrite", "improve"].iter().any(|w| lower.contains(w)) {
        let base = trimmed.trim_end_matches('.');
        let punct = if trimmed.ends_with('.') { "." } else { "" };
        let polished = if is_cell {
            format!("{base} (clear, steady)")
        } else {
            format!("{base} — crisp and clear{punct}")
        };
        (
            "Polished the phrasing while keeping your intent:".to_owned(),
            "Polish phrasing".to_owned(),
            polished,
        )
    } else if lower.contains('?') {
        let base = trimmed.trim_end_matches('.');
        (
            format!("Looks solid in context. If you want it more explicit, you could clarify \"{base}\" directly:"),
            "Clarify selection".to_owned(),
            format!("{base} (confirmed)"),
        )
    } else {
        let base = trimmed.trim_end_matches('.');
        let tag: String = prompt.trim().trim_end_matches(['.', '?', '!']).chars().take(36).collect();
        (
            "Here is a suggested edit based on your note:".to_owned(),
            if is_cell { "Update cell" } else { "Apply suggestion" }.to_owned(),
            format!("{base} — {tag}"),
        )
    }
}

fn extract_after<'a>(text: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    let lower = text.to_ascii_lowercase();
    prefixes.iter().find_map(|p| {
        let pos = lower.find(p)?;
        let after = text[pos + p.len()..].trim();
        (!after.is_empty()).then_some(after)
    })
}

/// Where a thread's text is now: its cursors, else its saved quote.
fn thread_range(note: &Note, md: &str, t: &CommentThread) -> Option<Range<usize>> {
    t.cursors
        .as_ref()
        .and_then(|c| note.resolve_anchor_in(md, c))
        .filter(|r| md.get(r.clone()).is_some())
        .or_else(|| t.file.anchor.resolve(md))
}

/// Where a thread is in the text now, and what text is there.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub id: usize,
    pub range: Range<usize>,
    pub resolved: bool,
    pub text: String,
}

/// Every thread that is still in the text (see `Discussion::ranges`).
pub fn ranges(note: &Note, md: &str, threads: &[CommentThread]) -> Vec<Placed> {
    threads
        .iter()
        .filter_map(|t| {
            let range = thread_range(note, md, t)?;
            Some(Placed { id: t.id, text: md.get(range.clone())?.to_owned(), range, resolved: t.file.resolved })
        })
        .collect()
}

impl Writing {
    fn with_thread<R>(mut self, id: usize, f: impl FnOnce(&mut CommentThread) -> R) -> Option<R> {
        self.discussion.threads.write().iter_mut().find(|t| t.id == id).map(f)
    }

    /// The text under a thread now; its saved quote when the text is gone.
    /// Subscribes the caller to changes of any thread's text.
    fn thread_text(&self, t: &CommentThread) -> String {
        self.discussion.ranges
            .read()
            .iter()
            .find(|p| p.id == t.id)
            .map_or_else(|| t.file.anchor.exact.clone(), |p| p.text.clone())
    }

    pub fn start_comment_from_selection(mut self) {
        let md = self.contents.peek();
        let range = self
            .selected_text
            .peek()
            .or_else(|| self.absolute_selection())
            .and_then(|[a, b]| {
                let lo = md.floor_char_boundary(a.min(b).min(md.len()));
                let hi = md.floor_char_boundary(a.max(b).min(md.len()));
                if lo < hi {
                    return Some(lo..hi);
                }
                // A collapsed caret comments on its line or cell.
                let line_start = md[..lo].rfind('\n').map_or(0, |i| i + 1);
                let line_end = lo + md[lo..].find('\n').unwrap_or(md.len() - lo);
                let trimmed = md[line_start..line_end].trim();
                (!trimmed.is_empty()).then(|| {
                    let offset = md[line_start..line_end].find(trimmed).unwrap_or(0);
                    line_start + offset..line_start + offset + trimmed.len()
                })
            });
        let quote = range.as_ref().map(|r| md[r.clone()].trim().to_owned());
        drop(md);
        self.discussion.open.set(true);
        if let (Some(range), Some(quote)) = (range, quote.filter(|q| !q.is_empty())) {
            self.discussion.draft.set(Some(DraftComment { range, quote }));
            self.format_position.set(None);
        }
    }

    pub fn submit_draft_comment(mut self, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        let Some(draft) = self.discussion.draft.take() else {
            return;
        };
        let thread = {
            let md = self.contents.peek();
            let now = dates::stamp();
            CommentThread {
                id: self.discussion.threads.peek().iter().map(|t| t.id).max().unwrap_or(0) + 1,
                cursors: self.note.peek().anchor_range_in(&md, draft.range.clone()),
                file: file::Thread {
                    id: new_key(),
                    created: now.clone(),
                    updated: now,
                    anchor: file::Anchor::capture(&md, draft.range),
                    ..Default::default()
                },
                ..Default::default()
            }
        };
        let id = thread.id;
        self.discussion.threads.write().insert(0, thread);
        self.discussion.active.set(Some(id));
        self.post(id, text.trim());
    }

    pub fn reply_to_comment(self, thread_id: usize, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        self.with_thread(thread_id, |t| {
            if t.file.resolved {
                t.file.resolved = false;
                t.file.updated = dates::stamp();
            }
        });
        self.post(thread_id, text.trim());
    }

    /// Add your message to a thread and ask the agent; the answer streams
    /// into a new message, and a `margin-edit` block at its end becomes the
    /// message's proposal (it never shows as text).
    fn post(self, thread_id: usize, prompt: &str) {
        let agent = self.discussion.agent.peek().clone();
        let page = self.page.peek().as_ref().map_or_else(|| "note.md".to_string(), |p| p.path.clone());
        let Some((ask, reply, quote)) = self.with_thread(thread_id, |t| {
            let md = self.contents.peek();
            let range = thread_range(&self.note.peek(), &md, t).unwrap_or(0..0);
            let quote = md[range.clone()].to_string();
            let next = request::turn(&t.file.id, &page, &md, range, prompt, t.seen.clone());
            t.file.messages.push(message("You", false, prompt));
            let reply = message(&agent.alias, true, "");
            t.pending = Some(reply.id.clone());
            let id = reply.id.clone();
            t.file.messages.push(reply);
            let ask = agent::Ask {
                agent: agent.alias.clone(),
                session: next.session,
                name: next.name,
                content: next.content,
                preamble: Some(next.preamble),
            };
            (ask, id, quote)
        }) else {
            return;
        };
        self.save_comments();
        // The agent is read-only; anything that needs approval is refused.
        let mut events = turn(agent.chat, ask, || false);
        let mut stream = self.discussion.stream;
        spawn(async move {
            let mut text = String::new();
            let mut outcome = None;
            while let Some(events) = batch(&mut events).await {
                for e in events {
                    match e {
                        ChatEvent::Chunk(c) => text.push_str(&c),
                        ChatEvent::Done(full) => outcome = Some(Ok(full)),
                        ChatEvent::Error(e) => outcome = Some(Err(e)),
                        _ => {}
                    }
                }
                let shown = request::visible(&text);
                if stream.peek().get(&reply).map(String::as_str) != Some(shown) {
                    stream.write().insert(reply.clone(), shown.to_string());
                }
            }
            stream.write().remove(&reply);
            let outcome = outcome.unwrap_or_else(|| Err("the connection closed early".into()));
            self.with_thread(thread_id, |t| {
                t.pending = None;
                let Some(m) = t.file.messages.iter_mut().find(|m| m.id == reply) else {
                    return;
                };
                match outcome {
                    Ok(full) => {
                        m.body = request::visible(&full).to_owned();
                        m.proposal = request::edit(&full).map(|e| file::Proposal {
                            label: e.label,
                            old: quote.clone(),
                            new: e.replacement,
                            ..Default::default()
                        });
                        t.seen = Some(quote);
                    }
                    Err(e) => {
                        m.body = format!("{} couldn’t answer: {e}", m.author);
                        t.failed.push(reply.clone());
                    }
                }
            });
            self.save_comments();
        });
    }

    /// Apply a proposal (`apply`) or undo it, as one undoable edit. Refuses
    /// when the text under the thread is not what the proposal expects.
    fn swap_proposal(mut self, thread_id: usize, message: usize, apply: bool) {
        let found = {
            let md = self.contents.peek();
            let threads = self.discussion.threads.peek();
            threads.iter().find(|t| t.id == thread_id).and_then(|t| {
                let p = t.file.messages.get(message)?.proposal.clone()?;
                let range = thread_range(&self.note.peek(), &md, t)?;
                let ready = (p.state == ProposalState::Applied) != apply && p.fits(&md[range.clone()]);
                ready.then_some((p, range))
            })
        };
        let Some((p, range)) = found else {
            return;
        };
        let text = if apply { &p.new } else { &p.old };
        let replacement = match self.field_at(range.start) {
            Field::Cell(..) => table::cell_value(text),
            _ => text.clone(),
        };
        if let Some(sel) = self.absolute_selection() {
            self.note.peek().remember_selection(sel);
        }
        let end = range.start + replacement.len();
        let result = self.note.write().replace(range.clone(), &replacement);
        if result.is_ok() {
            let md = self.note.peek().markdown();
            let cursors = self.note.peek().anchor_range_in(&md, range.start..end);
            self.with_thread(thread_id, |t| {
                t.cursors = cursors;
                if !apply {
                    t.file.resolved = false;
                }
                if let Some(p) = t.file.messages.get_mut(message).and_then(|m| m.proposal.as_mut()) {
                    p.state = if apply { ProposalState::Applied } else { ProposalState::Open };
                    p.updated = dates::stamp();
                }
            });
            self.body.set(None);
            self.open(self.field_at(range.start), [range.start, end]);
            self.save_comments();
        }
        report(result, self.message);
    }

    pub fn apply_comment_proposal(self, thread_id: usize, message: usize) {
        self.swap_proposal(thread_id, message, true);
    }

    pub fn undo_comment_proposal(self, thread_id: usize, message: usize) {
        self.swap_proposal(thread_id, message, false);
    }

    pub fn toggle_resolve_comment(self, thread_id: usize) {
        self.with_thread(thread_id, |t| {
            t.file.resolved = !t.file.resolved;
            t.file.updated = dates::stamp();
        });
        self.save_comments();
    }

    pub fn delete_comment_thread(mut self, thread_id: usize) {
        let removed = {
            let mut threads = self.discussion.threads.write();
            let at = threads.iter().position(|t| t.id == thread_id);
            at.map(|i| threads.remove(i))
        };
        if let Some(t) = removed {
            self.discussion.file.write().tombstones.push(file::Thread {
                id: t.file.id,
                updated: dates::stamp(),
                deleted: true,
                ..Default::default()
            });
        }
        if *self.discussion.active.peek() == Some(thread_id) {
            self.discussion.active.set(None);
        }
        self.save_comments();
    }

    pub fn jump_to_comment(mut self, thread_id: usize) {
        self.discussion.active.set(Some(thread_id));
        let range = self
            .discussion.ranges
            .peek()
            .iter()
            .find(|p| p.id == thread_id)
            .map(|p| p.range.clone());
        if let Some(r) = range {
            self.open(self.field_at(r.start), [r.start, r.end]);
        }
    }

    pub fn sync_active_comment_at(mut self, pos: usize) {
        if !*self.discussion.open.peek() {
            return;
        }
        let hit = self
            .discussion.ranges
            .peek()
            .iter()
            .find(|p| pos >= p.range.start && pos <= p.range.end)
            .map(|p| p.id);
        if let Some(id) = hit
            && *self.discussion.active.peek() != Some(id)
        {
            self.discussion.active.set(Some(id));
        }
    }

    /// The comment file as it should be on the server: anchors and proposal
    /// states of the last saved page, finished messages only.
    pub(super) fn comment_file_now(&self, page: &str) -> file::Sidecar {
        let mut out = file::Sidecar::new(page);
        for t in self.discussion.threads.peek().iter() {
            let mut thread = t.file.clone();
            thread
                .messages
                .retain(|m| t.pending.as_ref() != Some(&m.id) && !t.failed.contains(&m.id));
            for m in &mut thread.messages {
                if let Some(p) = &mut m.proposal {
                    p.state = t.saved.get(&m.id).copied().unwrap_or_default();
                }
            }
            out.threads.push(thread);
        }
        out.threads.extend(self.discussion.file.peek().tombstones.iter().cloned());
        out
    }

    /// The page was saved: anchors and proposal states now describe it.
    pub(super) fn commit_comments(self) {
        let mut comments = self.discussion.threads;
        {
            let md = self.contents.peek();
            let note = self.note.peek();
            for t in comments.write().iter_mut() {
                if let Some(r) = thread_range(&note, &md, t) {
                    t.file.anchor = file::Anchor::capture(&md, r);
                }
                t.saved = t
                    .file
                    .messages
                    .iter()
                    .filter_map(|m| Some((m.id.clone(), m.proposal.as_ref()?.state)))
                    .collect();
            }
        }
        self.save_comments();
    }

    /// Write the comment file. A concurrent change (by the agent, another
    /// device) is merged and written again; saves never overlap, and a change
    /// during a save is written right after it.
    pub fn save_comments(self) {
        let Some(page) = self.page.peek().clone() else {
            return;
        };
        let mut state = self.discussion.file;
        {
            let mut f = state.write();
            if !f.loaded {
                return;
            }
            if f.saving {
                f.again = true;
                return;
            }
            f.saving = true;
        }
        let ours = self.comment_file_now(&page.path);
        let etag = state.peek().etag.clone();
        spawn(async move {
            let (files, path) = (page.files.clone(), file::sidecar_path(&page.path));
            let result = work(move || match files.write(&path, &ours.to_json(), etag.as_deref()) {
                Err(WriteError::Conflict) => {
                    let theirs = files.read(&path)?.ok_or("the comment file was deleted while saving")?;
                    let merged = file::merge(&ours, &file::Sidecar::parse(&theirs.text)?);
                    files.write(&path, &merged.to_json(), theirs.etag.as_deref()).map_err(|e| e.to_string())
                }
                other => other.map_err(|e| e.to_string()),
            })
            .await;
            let again = {
                let mut f = state.write();
                f.saving = false;
                match result {
                    Ok(etag) => {
                        f.etag = etag;
                        f.error = None;
                    }
                    Err(e) => f.error = Some(format!("Comments not saved: {e}")),
                }
                std::mem::take(&mut f.again)
            };
            if again {
                self.save_comments();
            }
        });
    }

    /// Show a comment file's threads, each at its text if it is still there.
    pub(super) fn load_comment_file(mut self, saved: file::Sidecar) {
        let mut threads = Vec::new();
        let mut tombstones = Vec::new();
        {
            let md = self.contents.peek();
            let note = self.note.peek();
            for t in saved.threads {
                if t.deleted {
                    tombstones.push(t);
                    continue;
                }
                let cursors = t.anchor.resolve(&md).and_then(|r| note.anchor_range_in(&md, r));
                threads.push(CommentThread {
                    id: threads.len() + 1,
                    cursors,
                    seen: t.messages.iter().any(|m| m.ai).then(|| t.anchor.exact.clone()),
                    saved: t
                        .messages
                        .iter()
                        .filter_map(|m| Some((m.id.clone(), m.proposal.as_ref()?.state)))
                        .collect(),
                    file: t,
                    ..Default::default()
                });
            }
        }
        self.discussion.threads.set(threads);
        self.discussion.file.write().tombstones = tombstones;
    }
}

/// Read the page's comment file once, when the notebook edits a page.
pub fn use_comment_file(writing: Writing) {
    use_hook(move || {
        let Some(page) = writing.page.peek().clone() else {
            return;
        };
        let mut state = writing.discussion.file;
        spawn(async move {
            let PageSource { files, path, .. } = page;
            match work(move || files.read(&file::sidecar_path(&path))).await {
                Ok(Some(found)) => match file::Sidecar::parse(&found.text) {
                    Ok(saved) => {
                        writing.load_comment_file(saved);
                        let mut f = state.write();
                        f.etag = found.etag;
                        f.loaded = true;
                    }
                    // Unreadable: say so, and never overwrite it.
                    Err(e) => state.write().error = Some(e),
                },
                Ok(None) => state.write().loaded = true,
                Err(e) => state.write().error = Some(format!("Comments not loaded: {e}")),
            }
        });
    });
}

/// Paints warm inline highlights behind/over anchored comment ranges using Parley geometry.
#[component]
pub fn CommentInk(writing: Writing, field: Field) -> Element {
    let ranges = writing.discussion.ranges.read();
    if ranges.is_empty() {
        return rsx! {};
    }
    writing.ink.read();
    writing.boxes.read();
    writing.caret.read();
    let active_id = *writing.discussion.active.read();
    let md = writing.contents.read();
    let bounds = writing.range(field, &md);
    let hits: Vec<(usize, Range<usize>, bool, bool)> = ranges
        .iter()
        .filter(|p| !p.resolved || active_id == Some(p.id))
        .filter_map(|p| {
            let (start, end) = (p.range.start.max(bounds.start), p.range.end.min(bounds.end));
            (start < end).then_some((p.id, start..end, active_id == Some(p.id), p.resolved))
        })
        .collect();
    drop(ranges);
    if hits.is_empty() {
        return rsx! {};
    }
    let root = writing.root.read();
    let Some(doc) = root.as_ref().map(|h| h.doc()) else {
        return rsx! {};
    };
    let Some(node) = doc.get_element_by_id(&field.id()).and_then(|id| doc.get_node(id)) else {
        return rsx! {};
    };
    let Some(input) = node.element_data().and_then(|e| e.text_input_data()) else {
        return rsx! {};
    };
    let Some(layout) = input.editor.try_layout() else {
        return rsx! {};
    };
    let proj = writing.projection(field);
    let l = node.final_layout();
    let scale = f64::from(layout.scale());
    let x_off = f64::from(l.padding.left + l.border.left - if input.is_multiline { 0.0 } else { input.scroll_offset });
    let y_off = f64::from(l.padding.top + l.border.top - if input.is_multiline { input.scroll_offset } else { 0.0 })
        + node.text_input_v_centering_offset(scale);
    let mut rects: Vec<(usize, f64, f64, f64, f64, bool, bool)> = Vec::new();
    for (tid, range, is_active, is_resolved) in hits {
        let c0 = text::Cursor::from_byte_index(layout, proj.source_to_display(range.start), text::Affinity::Downstream);
        let c1 = text::Cursor::from_byte_index(layout, proj.source_to_display(range.end), text::Affinity::Downstream);
        for (rect, _) in text::Selection::new(c0, c1).geometry(layout) {
            let w = (rect.x1 - rect.x0) / scale;
            let h = (rect.y1 - rect.y0) / scale;
            if w > 0.5 && h > 0.5 {
                rects.push((tid, x_off + rect.x0 / scale, y_off + rect.y0 / scale, w, h, is_active, is_resolved));
            }
        }
    }
    rsx! {
        for (idx, (tid, x, y, w, h, is_active, is_resolved)) in rects.into_iter().enumerate() {
            {
                let ink_cls = if is_resolved {
                    "bg-[rgba(91,91,214,0.08)] border-b-[rgba(91,91,214,0.28)]"
                } else if is_active {
                    "bg-[rgba(236,179,76,0.40)] border-b-warn"
                } else {
                    "bg-[rgba(236,179,76,0.22)] border-b-[rgba(201,122,22,0.55)]"
                };
                rsx! {
                    span {
                        key: "ci-{tid}-{idx}",
                        id: if idx == 0 { format!("comment-hl-{tid}") } else { format!("comment-hl-{tid}-{idx}") },
                        class: "comment-ink absolute pointer-events-none select-none border-0 border-b-2 border-solid rounded-[2px] {ink_cls}",
                        aria_hidden: "true",
                        style: format!("left:{x:.1}px; top:{y:.1}px; width:{w:.1}px; height:{h:.1}px;"),
                    }
                }
            }
        }
    }
}

/// A reply still arriving: its text comes from the stream, so a new chunk
/// re-renders this message only.
#[component]
fn StreamingReply(writing: Writing, id: String, author: String) -> Element {
    let body = writing.discussion.stream.read().get(&id).cloned().unwrap_or_default();
    rsx! { Message { author, body, ai: true, pending: true, style: MessageStyle::Thread } }
}

#[component]
pub fn CommentsPane(writing: Writing) -> Element {
    let mut writing = writing;
    if !(writing.discussion.open)() {
        return rsx! {};
    }
    let mut filter = use_signal(|| CommentFilter::Open);
    let threads = writing.discussion.threads.read();
    let open_count = threads.iter().filter(|t| !t.file.resolved).count();
    let resolved_count = threads.len() - open_count;
    let active_id = *writing.discussion.active.read();
    let draft = writing.discussion.draft.read().clone();
    let cur_filter = filter();
    let error = writing.discussion.file.read().error.clone();
    let segments = vec![
        Segment { id: "filter-comments-open", label: "Open", tip: None, selected: cur_filter == CommentFilter::Open },
        Segment { id: "filter-comments-resolved", label: "Resolved", tip: None, selected: cur_filter == CommentFilter::Resolved },
        Segment { id: "filter-comments-all", label: "All", tip: None, selected: cur_filter == CommentFilter::All },
    ];
    let visible: Vec<&CommentThread> = threads
        .iter()
        .filter(|t| match cur_filter {
            CommentFilter::Open => !t.file.resolved,
            CommentFilter::Resolved => t.file.resolved,
            CommentFilter::All => true,
        })
        .collect();
    let empty = visible.is_empty() && draft.is_none();
    rsx! {
        aside {
            id: "comments-pane",
            class: "sticky top-9 self-start flex-[0_0_304px] w-[304px] max-h-[calc(100vh-68px)] min-h-0 flex flex-col bg-rail border-0 border-l border-solid border-line select-none max-[720px]:fixed max-[720px]:top-9 max-[720px]:right-0 max-[720px]:bottom-8 max-[720px]:z-[16] max-[720px]:max-h-none max-[720px]:shadow-side",
            aria_label: "Document comments",
            div { class: "flex flex-col gap-2 pt-2.5 px-3 pb-2 border-0 border-b border-solid border-line bg-page [&_.ui-segment]:flex-1",
                div { class: "flex items-center gap-1.5",
                    span { class: "text-ink type-control", "Comments" }
                    if open_count > 0 { Badge { text: open_count.to_string(), tone: Tone::Warn } }
                    span { class: "flex-auto" }
                    Button { id: "close-comments", label: "Close comments", text: "×", variant: Variant::Ghost,
                        onclick: move |_| writing.discussion.open.set(false) }
                }
                Segmented { label: "Which threads", items: segments, onselect: move |i: usize| {
                    filter.set([CommentFilter::Open, CommentFilter::Resolved, CommentFilter::All][i]);
                } }
                span { class: "text-faint type-mono-ui", "{open_count} open · {resolved_count} resolved" }
            }
            div { class: "flex-auto flex flex-col gap-2.5 p-2.5 overflow-y-auto",
                if let Some(e) = error { Banner { text: e, tone: Tone::Alert } }
                if let Some(draft) = draft {
                    Card { id: "new-comment-card", class: "flex flex-col gap-[7px] !p-2.5 !border-warn-line",
                        div { class: "flex items-center gap-1.5",
                            span { class: "flex-auto min-w-0 py-0.5 px-1.5 border-0 border-l-[2.5px] border-solid border-l-warn rounded-[3px] bg-warn-wash text-muted type-hint font-medium text-left overflow-hidden whitespace-nowrap text-ellipsis", "{draft.quote}" }
                            Badge { text: writing.discussion.agent.read().alias.clone(), tone: Tone::Accent }
                        }
                        div { class: "flex flex-wrap gap-1",
                            for (label, ask) in [("Tighten", "Tighten this phrasing"), ("Polish", "Polish and make clearer"), ("Add detail", "Add field detail")] {
                                Chip { label, onclick: move |_| writing.submit_draft_comment(ask) }
                            }
                        }
                        Composer { placeholder: "Comment or ask the agent to edit…", send_label: "Post",
                            field_id: "new-comment-input", send_id: "new-comment-post",
                            onsend: move |text: String| writing.submit_draft_comment(&text),
                            onescape: move |_| writing.discussion.draft.set(None) }
                        div { class: "flex justify-end",
                            Button { id: "new-comment-cancel", label: "Cancel", variant: Variant::Link,
                                onclick: move |_| writing.discussion.draft.set(None) }
                        }
                    }
                }
                if empty {
                    div { class: "py-[18px] px-2.5 text-muted text-center",
                        p { class: "mt-0 mx-0 mb-1.5 text-ink type-control",
                            if cur_filter == CommentFilter::Resolved { "No resolved comments yet." } else { "No open comments." }
                        }
                        p { class: "m-0 text-muted type-hint leading-[1.5]", "Select text in the note and click Comment (or " {crate::platform::shortcut("Shift+M")} ") to ask the agent for scoped edits." }
                    }
                }
                for thread in visible {
                    ThreadCard { key: "ct-{thread.id}", writing, id: thread.id, active: active_id == Some(thread.id) }
                }
            }
        }
    }
}

/// One thread: its quote, its messages, proposals and, when active, a reply box.
#[component]
fn ThreadCard(writing: Writing, id: usize, active: bool) -> Element {
    let mut writing = writing;
    let threads = writing.discussion.threads.read();
    let Some(thread) = threads.iter().find(|t| t.id == id) else {
        return rsx! {};
    };
    let tid = id;
    let resolved = thread.file.resolved;
    let current = writing.thread_text(thread);
    let quote = current.clone();
    let card_state = format!(
        "flex flex-col gap-[7px] !p-2.5{}{}",
        if active { " !border-warn-line" } else { "" },
        if resolved { " opacity-76" } else { "" },
    );
    let quote_accent = if resolved {
        "border-l-accent"
    } else {
        "border-l-warn"
    };
    rsx! {
        Card { id: format!("comment-thread-{tid}"), class: card_state,
            onclick: move |_| if *writing.discussion.active.peek() != Some(tid) { writing.discussion.active.set(Some(tid)) },
            div { class: "flex items-center gap-1.5",
                Button { id: format!("comment-jump-{tid}"), label: "Jump to anchored text", text: quote, variant: Variant::Bare,
                    class: format!("flex-auto min-w-0 h-auto justify-start py-0.5 px-1.5 border-0 border-l-[2.5px] border-solid {quote_accent} rounded-[3px] bg-warn-wash text-muted hover:text-ink type-hint font-medium text-left overflow-hidden whitespace-nowrap text-ellipsis"),
                    stop: true, onclick: move |_| writing.jump_to_comment(tid) }
                Button { id: format!("comment-resolve-{tid}"), label: if resolved { "Reopen thread" } else { "Resolve thread" },
                    text: if resolved { "Reopen" } else { "Resolve" }, variant: Variant::Ghost, stop: true,
                    onclick: move |_| writing.toggle_resolve_comment(tid) }
                Button { id: format!("comment-delete-{tid}"), label: "Delete comment thread", text: "×", variant: Variant::Ghost,
                    danger: true, stop: true, onclick: move |_| writing.delete_comment_thread(tid) }
            }
            for (mi, m) in thread.file.messages.iter().enumerate() {
                if thread.pending.as_ref() == Some(&m.id) {
                    StreamingReply { key: "{m.id}", writing, id: m.id.clone(), author: m.author.clone() }
                } else {
                    Message { key: "{m.id}", author: m.author.clone(), body: m.body.clone(), ai: m.ai,
                        failed: thread.failed.contains(&m.id), style: MessageStyle::Thread,
                        rich: rsx! { MarkdownView { text: m.body.clone(), compact: true } },
                        if let Some(p) = m.proposal.clone() {
                            ProposalView { writing, thread: tid, message: mi, fits: p.fits(&current), resolved, proposal: p }
                        }
                    }
                }
            }
            if active {
                div { class: "flex flex-col gap-1.5 mt-0.5 pt-2 border-0 border-t border-solid border-line",
                    div { class: "flex flex-wrap gap-1",
                        for (label, ask) in [("Shorten", "Shorten this"), ("Polish", "Polish phrasing"), ("Try another", "Try another option")] {
                            Chip { label, stop: true, onclick: move |_| writing.reply_to_comment(tid, ask) }
                        }
                    }
                    Composer { placeholder: "Reply or ask the agent to act…", send_label: "Ask",
                        field_id: format!("comment-reply-{tid}"), send_id: format!("comment-send-{tid}"),
                        onsend: move |text: String| writing.reply_to_comment(tid, &text) }
                }
            }
        }
    }
}

/// A proposed replacement: the change, then Apply, Undo, or — when the text
/// under the thread is no longer what it was made for — Ask again.
#[component]
fn ProposalView(writing: Writing, thread: usize, message: usize, proposal: file::Proposal, fits: bool, resolved: bool) -> Element {
    let (tid, mi) = (thread, message);
    let applied = proposal.state == ProposalState::Applied;
    rsx! {
        div { class: "mt-1 overflow-hidden border border-solid border-line rounded-md bg-page",
            div { class: "flex items-center gap-[5px] py-[5px] px-2 border-0 border-b border-solid border-line bg-wash text-accent-deep type-label [&_.i]:w-3 [&_.i]:h-3",
                {Icon::Sparkle.svg("i")} span { "{proposal.label}" }
            }
            div { class: "flex flex-col gap-[3px] py-1.5 px-2 type-mono-ui leading-[1.45] select-text",
                div { class: "py-0.5 px-[5px] rounded-sm bg-alert-wash text-alert line-through", "- {proposal.old}" }
                div { class: "py-0.5 px-[5px] rounded-sm bg-pale text-accent-deep", "+ {proposal.new}" }
            }
            div { class: "flex flex-wrap items-center gap-[5px] py-1.5 px-2 border-0 border-t border-solid border-line bg-surface",
                if !fits {
                    span { class: "flex-[1_1_100%] text-alert type-hint", "The text changed since this suggestion." }
                    Button { id: format!("comment-ask-again-{tid}-{mi}"), label: "Ask again", stop: true,
                        onclick: move |_| writing.reply_to_comment(tid, "The selected text changed. Propose the edit again for the current text.") }
                } else if applied {
                    Button { id: format!("comment-undo-{tid}-{mi}"), label: "Applied · Undo", icon: Icon::Check, text: "Applied · Undo", stop: true,
                        onclick: move |_| writing.undo_comment_proposal(tid, mi) }
                    if !resolved {
                        Button { id: format!("comment-resolve-after-{tid}-{mi}"), label: "Resolve", stop: true,
                            onclick: move |_| writing.toggle_resolve_comment(tid) }
                    }
                } else if proposal.state == ProposalState::Dismissed {
                    span { class: "flex-[1_1_100%] text-alert type-hint", "Dismissed." }
                } else {
                    Button { id: format!("comment-apply-{tid}-{mi}"), label: "Apply to doc", variant: Variant::Primary, stop: true,
                        onclick: move |_| writing.apply_comment_proposal(tid, mi) }
                }
            }
        }
    }
}
