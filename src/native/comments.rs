//! Anchored document comments with in-memory threads and interactive mock LLM replies/actions.
use super::{Field, Writing, report};
use dioxus_native::prelude::*;
use loro::cursor::Cursor;
use margin_editor_core::note::Note;
use std::ops::Range;

fn encode_cell_text(text: &str) -> String {
    text.replace(['\r', '\n'], " ").replace('|', "\\|")
}

fn ai_sparkle(size: u8) -> Element {
    rsx! {
        span { class: "comment-svg-icon", aria_hidden: "true",
            svg {
                width: "{size}",
                height: "{size}",
                view_box: "0 0 16 16",
                fill: "currentColor",
                path { d: "M8 1.2 9.7 6.3 14.8 8 9.7 9.7 8 14.8 6.3 9.7 1.2 8 6.3 6.3Z" }
            }
        }
    }
}

fn check_icon() -> Element {
    rsx! {
        span { class: "comment-svg-icon", aria_hidden: "true",
            svg {
                width: "11",
                height: "11",
                view_box: "0 0 16 16",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2.2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M3 8.5 6.5 12 13 4.5" }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct EditProposal {
    pub id: usize,
    pub label: String,
    pub old_text: String,
    pub new_text: String,
    pub applied: bool,
}

#[derive(Clone, PartialEq)]
pub struct CommentMessage {
    pub id: usize,
    pub author: String,
    pub is_ai: bool,
    pub body: String,
    pub proposal: Option<EditProposal>,
}

#[derive(Clone)]
pub struct CommentThread {
    pub id: usize,
    pub anchor: Option<(Cursor, Cursor)>,
    pub quote: String,
    pub resolved: bool,
    pub messages: Vec<CommentMessage>,
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

/// Seed two realistic in-memory demo threads anchored to phrases in `SAMPLE`.
pub fn seed_sample_comments(note: &Note) -> Vec<CommentThread> {
    let md = note.markdown();
    let mut threads = Vec::new();
    if let Some(at) = md.find("Water through the gate") {
        let range = at..at + "Water through the gate".len();
        threads.push(CommentThread {
            id: 1,
            anchor: note.anchor_range(range),
            quote: "Water through the gate".into(),
            resolved: false,
            messages: vec![
                CommentMessage {
                    id: 101,
                    author: "You".into(),
                    is_ai: false,
                    body: "Specify the mic setup so this take is reproducible?".into(),
                    proposal: None,
                },
                CommentMessage {
                    id: 102,
                    author: "Claude".into(),
                    is_ai: true,
                    body: "Good call — noting the stereo pattern and high-pass filter keeps the log actionable:".into(),
                    proposal: Some(EditProposal {
                        id: 1001,
                        label: "Update table cell".into(),
                        old_text: "Water through the gate".into(),
                        new_text: "Water through the gate (XY stereo, 80 Hz HPF)".into(),
                        applied: false,
                    }),
                },
            ],
        });
    }
    if let Some(at) = md.find("Leave **ten seconds** of silence.") {
        let range = at..at + "Leave **ten seconds** of silence.".len();
        threads.push(CommentThread {
            id: 2,
            anchor: note.anchor_range(range),
            quote: "Leave **ten seconds** of silence.".into(),
            resolved: false,
            messages: vec![
                CommentMessage {
                    id: 201,
                    author: "You".into(),
                    is_ai: false,
                    body: "@ai tighten this reminder so it scans faster in the field".into(),
                    proposal: None,
                },
                CommentMessage {
                    id: 202,
                    author: "Claude".into(),
                    is_ai: true,
                    body: "Here is a crisper version that keeps the bold emphasis:".into(),
                    proposal: Some(EditProposal {
                        id: 1002,
                        label: "Tighten checklist item".into(),
                        old_text: "Leave **ten seconds** of silence.".into(),
                        new_text: "Record **10 s** of room tone first.".into(),
                        applied: false,
                    }),
                },
            ],
        });
    }
    threads
}

/// Fake in-memory LLM responder that inspects the anchored text and user prompt
/// to generate a grounded reply + reviewable diff proposal.
fn mock_llm_reply(
    quote: &str,
    prompt: &str,
    is_cell: bool,
    msg_id: usize,
    prop_id: usize,
) -> CommentMessage {
    let trimmed = quote.trim();
    let lower = prompt.to_ascii_lowercase();
    let (body, label, new_text) = if let Some(rest) = extract_after(
        prompt,
        &["replace with ", "change to ", "use ", "rewrite as "],
    ) {
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
        (
            "Here is an alternative phrasing:".to_owned(),
            "Alternative phrasing".to_owned(),
            alt,
        )
    } else if lower.contains("polish")
        || lower.contains("vivid")
        || lower.contains("rewrite")
        || lower.contains("improve")
    {
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
            format!(
                "Looks solid in context. If you want it more explicit, you could clarify \"{base}\" directly:"
            ),
            "Clarify selection".to_owned(),
            format!("{base} (confirmed)"),
        )
    } else {
        let base = trimmed.trim_end_matches('.');
        let note_tag = prompt.trim().trim_end_matches(['.', '?', '!']);
        let short_tag: String = note_tag.chars().take(36).collect();
        (
            "Here is a suggested edit based on your note:".to_owned(),
            if is_cell {
                "Update cell".to_owned()
            } else {
                "Apply suggestion".to_owned()
            },
            format!("{base} — {short_tag}"),
        )
    };

    CommentMessage {
        id: msg_id,
        author: "Claude".into(),
        is_ai: true,
        body,
        proposal: Some(EditProposal {
            id: prop_id,
            label,
            old_text: trimmed.to_owned(),
            new_text,
            applied: false,
        }),
    }
}

fn extract_after<'a>(text: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    let lower = text.to_ascii_lowercase();
    for p in prefixes {
        if let Some(pos) = lower.find(p) {
            let after = text[pos + p.len()..].trim();
            if !after.is_empty() {
                return Some(after);
            }
        }
    }
    None
}

fn resolve_thread_range(note: &Note, md: &str, thread: &CommentThread) -> Option<Range<usize>> {
    if let Some(anchor) = &thread.anchor
        && let Some(r) = note.resolve_anchor(anchor)
        && md.get(r.clone()).is_some()
    {
        return Some(r);
    }
    if !thread.quote.is_empty()
        && let Some(pos) = md.find(&thread.quote)
    {
        return Some(pos..pos + thread.quote.len());
    }
    None
}

impl Writing {
    pub fn start_comment_from_selection(mut self) {
        let md = self.contents.read().clone();
        let range = self
            .selected_text
            .peek()
            .or_else(|| self.absolute_selection())
            .and_then(|[a, b]| {
                let lo = md.floor_char_boundary(a.min(b).min(md.len()));
                let hi = md.floor_char_boundary(a.max(b).min(md.len()));
                if lo < hi {
                    Some(lo..hi)
                } else {
                    // Fallback to the current non-empty line or cell when cursor is collapsed.
                    let line_start = md[..lo].rfind('\n').map_or(0, |i| i + 1);
                    let line_end = lo + md[lo..].find('\n').unwrap_or(md.len() - lo);
                    let trimmed = md[line_start..line_end].trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        let offset = md[line_start..line_end].find(trimmed).unwrap_or(0);
                        Some(line_start + offset..line_start + offset + trimmed.len())
                    }
                }
            });
        let Some(range) = range else {
            self.comments_open.set(true);
            return;
        };
        let quote = md[range.clone()].trim().to_owned();
        if quote.is_empty() {
            self.comments_open.set(true);
            return;
        }
        self.comments_open.set(true);
        self.draft_comment.set(Some(DraftComment { range, quote }));
        self.format_position.set(None);
    }

    pub fn submit_draft_comment(mut self, text: &str) {
        let prompt = text.trim();
        if prompt.is_empty() {
            return;
        }
        let Some(draft) = self.draft_comment.take() else {
            return;
        };
        let anchor = self.note.peek().anchor_range(draft.range.clone());
        let is_cell = matches!(self.field_at(draft.range.start), Field::Cell(..));
        let mut threads = self.comments.write();
        let next_tid = threads.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        let next_mid = threads
            .iter()
            .flat_map(|t| t.messages.iter().map(|m| m.id))
            .max()
            .unwrap_or(100)
            + 1;
        let next_pid = threads
            .iter()
            .flat_map(|t| {
                t.messages
                    .iter()
                    .filter_map(|m| m.proposal.as_ref().map(|p| p.id))
            })
            .max()
            .unwrap_or(1000)
            + 1;
        let user_msg = CommentMessage {
            id: next_mid,
            author: "You".into(),
            is_ai: false,
            body: prompt.to_owned(),
            proposal: None,
        };
        let ai_msg = mock_llm_reply(&draft.quote, prompt, is_cell, next_mid + 1, next_pid);
        threads.insert(
            0,
            CommentThread {
                id: next_tid,
                anchor,
                quote: draft.quote,
                resolved: false,
                messages: vec![user_msg, ai_msg],
            },
        );
        drop(threads);
        self.active_comment.set(Some(next_tid));
    }

    pub fn reply_to_comment(mut self, thread_id: usize, text: &str) {
        let prompt = text.trim();
        if prompt.is_empty() {
            return;
        }
        let md = self.contents.read().clone();
        let (quote, is_cell, next_mid, next_pid) = {
            let threads = self.comments.read();
            let Some(thread) = threads.iter().find(|t| t.id == thread_id) else {
                return;
            };
            let current_range = resolve_thread_range(&self.note.peek(), &md, thread);
            let live_quote = current_range
                .as_ref()
                .and_then(|r| md.get(r.clone()))
                .map_or_else(|| thread.quote.clone(), str::to_owned);
            let is_cell = current_range
                .as_ref()
                .is_some_and(|r| matches!(self.field_at(r.start), Field::Cell(..)));
            let next_mid = threads
                .iter()
                .flat_map(|t| t.messages.iter().map(|m| m.id))
                .max()
                .unwrap_or(100)
                + 1;
            let next_pid = threads
                .iter()
                .flat_map(|t| {
                    t.messages
                        .iter()
                        .filter_map(|m| m.proposal.as_ref().map(|p| p.id))
                })
                .max()
                .unwrap_or(1000)
                + 1;
            (live_quote, is_cell, next_mid, next_pid)
        };
        let user_msg = CommentMessage {
            id: next_mid,
            author: "You".into(),
            is_ai: false,
            body: prompt.to_owned(),
            proposal: None,
        };
        let ai_msg = mock_llm_reply(&quote, prompt, is_cell, next_mid + 1, next_pid);
        let mut threads = self.comments.write();
        if let Some(thread) = threads.iter_mut().find(|t| t.id == thread_id) {
            thread.resolved = false;
            thread.messages.push(user_msg);
            thread.messages.push(ai_msg);
        }
    }

    pub fn apply_comment_proposal(mut self, thread_id: usize, proposal_id: usize) {
        let md = self.contents.read().clone();
        let (range, replacement) = {
            let threads = self.comments.read();
            let Some(thread) = threads.iter().find(|t| t.id == thread_id) else {
                return;
            };
            let Some(prop) = thread
                .messages
                .iter()
                .filter_map(|m| m.proposal.as_ref())
                .find(|p| p.id == proposal_id)
            else {
                return;
            };
            if prop.applied {
                return;
            }
            let Some(range) = resolve_thread_range(&self.note.peek(), &md, thread) else {
                return;
            };
            let is_cell = matches!(self.field_at(range.start), Field::Cell(..));
            let replacement = if is_cell {
                encode_cell_text(&prop.new_text)
            } else {
                prop.new_text.clone()
            };
            (range, replacement)
        };
        if let Some(sel) = self.absolute_selection() {
            self.note.peek().remember_selection(sel);
        }
        let new_end = range.start + replacement.len();
        let result = self.note.write().replace(range.clone(), &replacement);
        if result.is_ok() {
            let new_anchor = self.note.peek().anchor_range(range.start..new_end);
            let mut threads = self.comments.write();
            if let Some(thread) = threads.iter_mut().find(|t| t.id == thread_id) {
                thread.anchor = new_anchor;
                thread.quote = replacement.clone();
                for m in &mut thread.messages {
                    if let Some(p) = &mut m.proposal {
                        p.applied = p.id == proposal_id;
                    }
                }
            }
            drop(threads);
            self.body.set(None);
            let field = self.field_at(range.start);
            self.open(field, [range.start, new_end]);
        }
        report(result, self.message);
    }

    pub fn undo_comment_proposal(mut self, thread_id: usize, proposal_id: usize) {
        let md = self.contents.read().clone();
        let (range, restore_text) = {
            let threads = self.comments.read();
            let Some(thread) = threads.iter().find(|t| t.id == thread_id) else {
                return;
            };
            let Some(prop) = thread
                .messages
                .iter()
                .filter_map(|m| m.proposal.as_ref())
                .find(|p| p.id == proposal_id && p.applied)
            else {
                return;
            };
            let Some(range) = resolve_thread_range(&self.note.peek(), &md, thread) else {
                return;
            };
            let is_cell = matches!(self.field_at(range.start), Field::Cell(..));
            let restore = if is_cell {
                encode_cell_text(&prop.old_text)
            } else {
                prop.old_text.clone()
            };
            (range, restore)
        };
        if let Some(sel) = self.absolute_selection() {
            self.note.peek().remember_selection(sel);
        }
        let new_end = range.start + restore_text.len();
        let result = self.note.write().replace(range.clone(), &restore_text);
        if result.is_ok() {
            let new_anchor = self.note.peek().anchor_range(range.start..new_end);
            let mut threads = self.comments.write();
            if let Some(thread) = threads.iter_mut().find(|t| t.id == thread_id) {
                thread.anchor = new_anchor;
                thread.quote = restore_text.clone();
                thread.resolved = false;
                for m in &mut thread.messages {
                    if let Some(p) = &mut m.proposal
                        && p.id == proposal_id
                    {
                        p.applied = false;
                    }
                }
            }
            drop(threads);
            self.body.set(None);
            let field = self.field_at(range.start);
            self.open(field, [range.start, new_end]);
        }
        report(result, self.message);
    }

    pub fn toggle_resolve_comment(mut self, thread_id: usize) {
        let mut threads = self.comments.write();
        if let Some(thread) = threads.iter_mut().find(|t| t.id == thread_id) {
            thread.resolved = !thread.resolved;
        }
    }

    pub fn delete_comment_thread(mut self, thread_id: usize) {
        self.comments.write().retain(|t| t.id != thread_id);
        if *self.active_comment.peek() == Some(thread_id) {
            self.active_comment.set(None);
        }
    }

    pub fn jump_to_comment(mut self, thread_id: usize) {
        self.active_comment.set(Some(thread_id));
        let md = self.contents.read().clone();
        let range = {
            let threads = self.comments.read();
            threads
                .iter()
                .find(|t| t.id == thread_id)
                .and_then(|t| resolve_thread_range(&self.note.peek(), &md, t))
        };
        if let Some(r) = range {
            let field = self.field_at(r.start);
            self.open(field, [r.start, r.end]);
        }
    }

    pub fn sync_active_comment_at(mut self, pos: usize) {
        if !*self.comments_open.peek() {
            return;
        }
        let md = self.contents.read();
        let hit = {
            let threads = self.comments.read();
            threads.iter().find_map(|t| {
                let r = resolve_thread_range(&self.note.peek(), &md, t)?;
                (pos >= r.start && pos <= r.end).then_some(t.id)
            })
        };
        if let Some(tid) = hit
            && *self.active_comment.peek() != Some(tid)
        {
            self.active_comment.set(Some(tid));
        }
    }
}

/// Paints warm inline highlights behind/over anchored comment ranges using Parley geometry.
#[component]
pub fn CommentInk(writing: Writing, field: Field) -> Element {
    let threads = writing.comments.read();
    if threads.is_empty() {
        return rsx! {};
    }
    writing.ink.read();
    writing.boxes.read();
    let active_id = *writing.active_comment.read();
    let md = writing.contents.read();
    let bounds = writing.range(field, &md);
    let note = writing.note.peek();
    let mut hits: Vec<(usize, Range<usize>, bool, bool)> = Vec::new();
    for t in threads.iter() {
        if t.resolved && active_id != Some(t.id) {
            continue;
        }
        let Some(r) = resolve_thread_range(&note, &md, t) else {
            continue;
        };
        let start = r.start.max(bounds.start);
        let end = r.end.min(bounds.end);
        if start < end {
            hits.push((t.id, start..end, active_id == Some(t.id), t.resolved));
        }
    }
    drop(note);
    drop(threads);
    if hits.is_empty() {
        return rsx! {};
    }
    let root = writing.root.read();
    let Some(doc) = root.as_ref().map(|h| h.doc()) else {
        return rsx! {};
    };
    let Some(node) = doc
        .get_element_by_id(&field.id())
        .and_then(|id| doc.get_node(id))
    else {
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
    let x_off = f64::from(
        l.padding.left + l.border.left
            - if input.is_multiline {
                0.0
            } else {
                input.scroll_offset
            },
    );
    let y_off = f64::from(
        l.padding.top + l.border.top
            - if input.is_multiline {
                input.scroll_offset
            } else {
                0.0
            },
    ) + node.text_input_v_centering_offset(scale);

    let mut rects: Vec<(usize, f64, f64, f64, f64, bool, bool)> = Vec::new();
    for (tid, range, is_active, is_resolved) in hits {
        let c0 = parley::Cursor::from_byte_index(
            layout,
            proj.source_to_display(range.start),
            parley::Affinity::Downstream,
        );
        let c1 = parley::Cursor::from_byte_index(
            layout,
            proj.source_to_display(range.end),
            parley::Affinity::Downstream,
        );
        for (rect, _) in parley::Selection::new(c0, c1).geometry(layout) {
            let w = (rect.x1 - rect.x0) / scale;
            let h = (rect.y1 - rect.y0) / scale;
            if w > 0.5 && h > 0.5 {
                rects.push((
                    tid,
                    x_off + rect.x0 / scale,
                    y_off + rect.y0 / scale,
                    w,
                    h,
                    is_active,
                    is_resolved,
                ));
            }
        }
    }
    rsx! {
        for (idx, (tid, x, y, w, h, is_active, is_resolved)) in rects.into_iter().enumerate() {
            span {
                key: "ci-{tid}-{idx}",
                id: if idx == 0 { format!("comment-hl-{tid}") } else { format!("comment-hl-{tid}-{idx}") },
                class: "comment-ink",
                class: if is_active { "active" },
                class: if is_resolved { "resolved" },
                aria_hidden: "true",
                style: format!("left:{x:.1}px; top:{y:.1}px; width:{w:.1}px; height:{h:.1}px;"),
            }
        }
    }
}

#[component]
pub fn CommentsPane(writing: Writing) -> Element {
    let mut writing = writing;
    if !(writing.comments_open)() {
        return rsx! {};
    }
    let mut filter = use_signal(|| CommentFilter::Open);
    let mut draft_text = use_signal(String::new);
    let mut reply_text = use_signal(String::new);
    let mut reply_for = use_signal(|| None::<usize>);

    let threads = writing.comments.read().clone();
    let open_count = threads.iter().filter(|t| !t.resolved).count();
    let resolved_count = threads.len().saturating_sub(open_count);
    let active_id = *writing.active_comment.read();
    let draft = writing.draft_comment.read().clone();
    let cur_filter = filter();

    let visible: Vec<CommentThread> = threads
        .into_iter()
        .filter(|t| match cur_filter {
            CommentFilter::Open => !t.resolved,
            CommentFilter::Resolved => t.resolved,
            CommentFilter::All => true,
        })
        .collect();

    rsx! {
        aside { id: "comments-pane", class: "comments-pane", aria_label: "Document comments",
            div { class: "comments-head",
                div { class: "comments-head-row",
                    div { class: "comments-title-wrap",
                        span { class: "comments-title", "Comments" }
                        if open_count > 0 {
                            span { class: "comments-badge", "{open_count}" }
                        }
                    }
                    button {
                        id: "close-comments",
                        class: "comments-close-btn",
                        aria_label: "Close comments",
                        title: "Close comments",
                        onclick: move |_| writing.comments_open.set(false),
                        "×"
                    }
                }
                div { class: "comments-filters", role: "tablist",
                    button {
                        id: "filter-comments-open",
                        class: "comments-filter-btn",
                        class: if cur_filter == CommentFilter::Open { "selected" },
                        onclick: move |_| filter.set(CommentFilter::Open),
                        "Open ({open_count})"
                    }
                    button {
                        id: "filter-comments-resolved",
                        class: "comments-filter-btn",
                        class: if cur_filter == CommentFilter::Resolved { "selected" },
                        onclick: move |_| filter.set(CommentFilter::Resolved),
                        "Resolved ({resolved_count})"
                    }
                    button {
                        id: "filter-comments-all",
                        class: "comments-filter-btn",
                        class: if cur_filter == CommentFilter::All { "selected" },
                        onclick: move |_| filter.set(CommentFilter::All),
                        "All"
                    }
                }
            }

            div { class: "comments-list",
                if let Some(draft) = draft {
                    div { id: "new-comment-card", class: "comment-card draft",
                        div { class: "comment-card-head",
                            span { class: "comment-quote", "{draft.quote}" }
                            span { class: "comment-ai-tag", {ai_sparkle(10)} "AI reply" }
                        }
                        div { class: "comment-quick-chips",
                            button { class: "comment-chip",
                                onclick: move |_| {
                                    writing.submit_draft_comment("Tighten this phrasing");
                                    draft_text.set(String::new());
                                },
                                "Tighten"
                            }
                            button { class: "comment-chip",
                                onclick: move |_| {
                                    writing.submit_draft_comment("Polish and make clearer");
                                    draft_text.set(String::new());
                                },
                                "Polish"
                            }
                            button { class: "comment-chip",
                                onclick: move |_| {
                                    writing.submit_draft_comment("Add field detail");
                                    draft_text.set(String::new());
                                },
                                "Add detail"
                            }
                        }
                        div { class: "comment-input-row",
                            input {
                                id: "new-comment-input",
                                class: "comment-input",
                                placeholder: "Comment or ask Claude to edit…",
                                value: "{draft_text}",
                                oninput: move |e| {
                                    e.stop_propagation();
                                    draft_text.set(e.value());
                                },
                                onkeydown: move |e| {
                                    if e.key() == Key::Named(NamedKey::Enter) {
                                        e.prevent_default();
                                        e.stop_propagation();
                                        let v = draft_text.peek().clone();
                                        writing.submit_draft_comment(&v);
                                        draft_text.set(String::new());
                                    } else if e.key() == Key::Named(NamedKey::Escape) {
                                        e.prevent_default();
                                        e.stop_propagation();
                                        writing.draft_comment.set(None);
                                        draft_text.set(String::new());
                                    }
                                },
                            }
                            button {
                                id: "new-comment-post",
                                class: "comment-send-btn",
                                onclick: move |_| {
                                    let v = draft_text.peek().clone();
                                    writing.submit_draft_comment(&v);
                                    draft_text.set(String::new());
                                },
                                "Post"
                            }
                        }
                        div { class: "comment-draft-foot",
                            button {
                                id: "new-comment-cancel",
                                class: "comment-link-btn",
                                onclick: move |_| {
                                    writing.draft_comment.set(None);
                                    draft_text.set(String::new());
                                },
                                "Cancel"
                            }
                        }
                    }
                }

                if visible.is_empty() && writing.draft_comment.read().is_none() {
                    div { class: "comments-empty",
                        p { class: "comments-empty-title",
                            if cur_filter == CommentFilter::Resolved {
                                "No resolved comments yet."
                            } else {
                                "No open comments."
                            }
                        }
                        p { class: "comments-empty-sub",
                            "Select text in the note and click Comment (or ⌘⇧M) to ask Claude for scoped edits."
                        }
                    }
                }

                for thread in visible {
                    {
                        let tid = thread.id;
                        let is_active = active_id == Some(tid);
                        let is_resolved = thread.resolved;
                        let input_val = if *reply_for.read() == Some(tid) {
                            reply_text.read().clone()
                        } else {
                            String::new()
                        };
                        rsx! {
                            div {
                                key: "ct-{tid}",
                                id: format!("comment-thread-{tid}"),
                                class: "comment-card",
                                class: if is_active { "active" },
                                class: if is_resolved { "resolved" },
                                onclick: move |_| {
                                    if *writing.active_comment.peek() != Some(tid) {
                                        writing.active_comment.set(Some(tid));
                                    }
                                },
                                div { class: "comment-card-head",
                                    button {
                                        id: format!("comment-jump-{tid}"),
                                        class: "comment-quote-btn",
                                        title: "Jump to anchored text",
                                        onclick: move |e| {
                                            e.stop_propagation();
                                            writing.jump_to_comment(tid);
                                        },
                                        "{thread.quote}"
                                    }
                                    div { class: "comment-head-actions",
                                        button {
                                            id: format!("comment-resolve-{tid}"),
                                            class: "comment-mini-btn",
                                            title: if is_resolved { "Reopen thread" } else { "Resolve thread" },
                                            onclick: move |e| {
                                                e.stop_propagation();
                                                writing.toggle_resolve_comment(tid);
                                            },
                                            if is_resolved { "Reopen" } else { "Resolve" }
                                        }
                                        button {
                                            id: format!("comment-delete-{tid}"),
                                            class: "comment-mini-btn remove",
                                            aria_label: "Delete comment thread",
                                            title: "Delete thread",
                                            onclick: move |e| {
                                                e.stop_propagation();
                                                writing.delete_comment_thread(tid);
                                            },
                                            "×"
                                        }
                                    }
                                }
                                for msg in thread.messages {
                                    {
                                        let mid = msg.id;
                                        rsx! {
                                            div { key: "cm-{mid}", class: "comment-msg",
                                                div { class: "comment-msg-meta",
                                                    span { class: "comment-avatar", class: if msg.is_ai { "ai" },
                                                        if msg.is_ai { {ai_sparkle(10)} } else { "Y" }
                                                    }
                                                    span { class: "comment-author", "{msg.author}" }
                                                }
                                                p { class: "comment-msg-body", "{msg.body}" }
                                                if let Some(prop) = msg.proposal {
                                                    {
                                                        let pid = prop.id;
                                                        let applied = prop.applied;
                                                        rsx! {
                                                            div { class: "comment-diff",
                                                                div { class: "comment-diff-head",
                                                                    {ai_sparkle(10)}
                                                                    span { "{prop.label}" }
                                                                }
                                                                div { class: "comment-diff-lines",
                                                                    div { class: "comment-diff-del", "- {prop.old_text}" }
                                                                    div { class: "comment-diff-add", "+ {prop.new_text}" }
                                                                }
                                                                div { class: "comment-diff-actions",
                                                                    if applied {
                                                                        button {
                                                                            id: format!("comment-undo-{tid}-{pid}"),
                                                                            class: "comment-act-btn applied",
                                                                            onclick: move |e| {
                                                                                e.stop_propagation();
                                                                                writing.undo_comment_proposal(tid, pid);
                                                                            },
                                                                            {check_icon()}
                                                                            "Applied · Undo"
                                                                        }
                                                                        if !is_resolved {
                                                                            button {
                                                                                id: format!("comment-resolve-after-{tid}-{pid}"),
                                                                                class: "comment-act-btn",
                                                                                onclick: move |e| {
                                                                                    e.stop_propagation();
                                                                                    writing.toggle_resolve_comment(tid);
                                                                                },
                                                                                "Resolve"
                                                                            }
                                                                        }
                                                                    } else {
                                                                        button {
                                                                            id: format!("comment-apply-{tid}-{pid}"),
                                                                            class: "comment-act-btn primary",
                                                                            onclick: move |e| {
                                                                                e.stop_propagation();
                                                                                writing.apply_comment_proposal(tid, pid);
                                                                            },
                                                                            "Apply to doc"
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if is_active {
                                    div { class: "comment-reply-wrap",
                                        div { class: "comment-quick-chips",
                                            button { class: "comment-chip",
                                                onclick: move |e| {
                                                    e.stop_propagation();
                                                    writing.reply_to_comment(tid, "Shorten this");
                                                },
                                                "Shorten"
                                            }
                                            button { class: "comment-chip",
                                                onclick: move |e| {
                                                    e.stop_propagation();
                                                    writing.reply_to_comment(tid, "Polish phrasing");
                                                },
                                                "Polish"
                                            }
                                            button { class: "comment-chip",
                                                onclick: move |e| {
                                                    e.stop_propagation();
                                                    writing.reply_to_comment(tid, "Try another option");
                                                },
                                                "Try another"
                                            }
                                        }
                                        div { class: "comment-input-row",
                                            input {
                                                id: format!("comment-reply-{tid}"),
                                                class: "comment-input",
                                                placeholder: "Reply or ask Claude to act…",
                                                value: "{input_val}",
                                                oninput: move |e| {
                                                    e.stop_propagation();
                                                    reply_for.set(Some(tid));
                                                    reply_text.set(e.value());
                                                },
                                                onkeydown: move |e| {
                                                    if e.key() == Key::Named(NamedKey::Enter) {
                                                        e.prevent_default();
                                                        e.stop_propagation();
                                                        let v = if *reply_for.peek() == Some(tid) {
                                                            reply_text.peek().clone()
                                                        } else {
                                                            String::new()
                                                        };
                                                        writing.reply_to_comment(tid, &v);
                                                        reply_text.set(String::new());
                                                    }
                                                },
                                            }
                                            button {
                                                id: format!("comment-send-{tid}"),
                                                class: "comment-send-btn",
                                                onclick: move |e| {
                                                    e.stop_propagation();
                                                    let v = if *reply_for.peek() == Some(tid) {
                                                        reply_text.peek().clone()
                                                    } else {
                                                        String::new()
                                                    };
                                                    writing.reply_to_comment(tid, &v);
                                                    reply_text.set(String::new());
                                                },
                                                "Ask"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
