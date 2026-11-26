//! Link preview and quick-action popover shown when the caret sits inside a
//! Markdown link (`[label](target)` or `<url>`).
use super::document::Sections;
use super::projection::ParsedLink;
use super::ui::{Button, Icon, Popover, TextField, Variant, icon};
use super::worker::work;
use super::{Field, Writing, report};
use dioxus_native::prelude::*;
use std::collections::HashMap;
use std::ops::Range;

const CARD_WIDTH: f64 = 336.0;
const CARD_EST_HEIGHT: f64 = 176.0;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct LinkCardState {
    pub field: Field,
    pub link: ParsedLink,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CachedPage {
    title: Option<String>,
    excerpt: Option<String>,
    found: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum OpenTarget {
    ExternalUrl(String),
    HeadingOffset(usize),
    SpacePage(String),
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LinkPreview {
    is_internal: bool,
    display_target: String,
    title: String,
    folder_or_host: Option<String>,
    excerpt: Option<String>,
    meta_left: Option<String>,
    meta_right: Option<String>,
    open_target: OpenTarget,
}

fn is_external_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("mailto:")
        || lower.starts_with("tel:")
        || lower.starts_with("ftp://")
        || lower.starts_with("www.")
}

fn normalized_external_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.to_ascii_lowercase().starts_with("www.") {
        format!("https://{trimmed}")
    } else {
        trimmed.to_owned()
    }
}

fn normalize_internal_path(url: &str) -> String {
    let base = url.trim().split('#').next().unwrap_or("").trim();
    let base = base.trim_start_matches("./").trim_start_matches('/');
    base.strip_suffix(".md").unwrap_or(base).to_owned()
}

fn space_md_path(url: &str) -> Option<String> {
    let norm = normalize_internal_path(url);
    if norm.is_empty() || norm.contains("..") {
        return None;
    }
    Some(format!("{norm}.md"))
}

fn display_target(url: &str, is_internal: bool) -> String {
    let trimmed = url.trim();
    if !is_internal {
        let stripped = trimmed
            .strip_prefix("https://")
            .or_else(|| trimmed.strip_prefix("http://"))
            .or_else(|| trimmed.strip_prefix("mailto:"))
            .unwrap_or(trimmed);
        return stripped.trim_end_matches('/').to_owned();
    }
    if trimmed.starts_with('#') {
        return trimmed.to_owned();
    }
    let clean = normalize_internal_path(trimmed);
    match (clean.is_empty(), trimmed.split_once('#')) {
        (true, Some((_, frag))) => format!("#{frag}"),
        (false, Some((_, frag))) => format!("{clean}#{frag}"),
        (true, None) => trimmed.to_owned(),
        (false, None) => clean,
    }
}

fn extract_folder(target: &str) -> Option<String> {
    let base = target.split('#').next().unwrap_or(target);
    let (dir, _) = base.rsplit_once('/')?;
    let dir = dir.trim_matches('/');
    (!dir.is_empty()).then(|| dir.to_owned())
}

fn extract_host(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if let Some(rest) = trimmed.strip_prefix("mailto:") {
        let addr = rest.split('?').next().unwrap_or(rest).trim();
        return (!addr.is_empty()).then(|| addr.to_owned());
    }
    let after_scheme = trimmed
        .split_once("://")
        .map_or(trimmed, |(_, rest)| rest);
    let authority = after_scheme.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host).trim();
    (!host.is_empty()).then(|| host.to_owned())
}

fn prettify_stem(target: &str) -> String {
    let base = target.split('#').next().unwrap_or(target);
    let stem = base.rsplit('/').next().unwrap_or(base);
    let stem = stem.strip_suffix(".md").unwrap_or(stem);
    let words: Vec<String> = stem
        .split(['-', '_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();
    if words.is_empty() {
        target.to_owned()
    } else {
        words.join(" ")
    }
}

fn slugify(text: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in text.trim().chars() {
        if ch.is_alphanumeric() {
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
            prev_dash = false;
        } else if (ch.is_whitespace() || ch == '-' || ch == '_') && !out.is_empty() && !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_end_matches('-').to_owned()
}

fn strip_inline_markdown(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            if let Some(close_bracket) = line[i + 1..].find(']') {
                let label_end = i + 1 + close_bracket;
                if line[label_end + 1..].starts_with('(')
                    && let Some(close_paren) = line[label_end + 2..].find(')')
                {
                    out.push_str(&line[i + 1..label_end]);
                    i = label_end + 2 + close_paren + 1;
                    continue;
                }
            }
        }
        let ch = line[i..].chars().next().unwrap();
        if !matches!(ch, '*' | '_' | '`' | '~') {
            out.push(ch);
        }
        i += ch.len_utf8();
    }
    out
}

fn truncate_excerpt(text: &str, max_chars: usize) -> Option<String> {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    let count = collapsed.chars().count();
    if count <= max_chars {
        return Some(collapsed);
    }
    let mut cut = collapsed
        .char_indices()
        .nth(max_chars)
        .map_or(collapsed.len(), |(idx, _)| idx);
    if let Some(space) = collapsed[..cut].rfind(' ')
        && space > cut / 2
    {
        cut = space;
    }
    Some(format!("{}…", collapsed[..cut].trim_end_matches([',', ';', ':'])))
}

fn extract_markdown_summary(md: &str) -> (Option<String>, Option<String>) {
    let mut lines = md.lines().peekable();
    // Skip optional YAML frontmatter.
    if lines.peek().is_some_and(|l| l.trim() == "---") {
        lines.next();
        for l in lines.by_ref() {
            if l.trim() == "---" || l.trim() == "..." {
                break;
            }
        }
    }
    let mut title = None;
    let mut body_parts = Vec::new();
    let mut body_len = 0usize;
    for raw in lines {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            if !body_parts.is_empty() {
                break;
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('#') {
            let heading = rest.trim_start_matches('#').trim();
            if title.is_none() && !heading.is_empty() {
                title = Some(strip_inline_markdown(heading));
            }
            continue;
        }
        if trimmed.starts_with("```")
            || trimmed.starts_with("~~~")
            || trimmed.starts_with('|')
            || trimmed.chars().all(|c| matches!(c, '-' | '*' | '_' | ' '))
        {
            continue;
        }
        let clean = trimmed
            .trim_start_matches("> ")
            .trim_start_matches("- [ ] ")
            .trim_start_matches("- [x] ")
            .trim_start_matches("- [X] ")
            .trim_start_matches("- ")
            .trim_start_matches("* ");
        let plain = strip_inline_markdown(clean);
        if !plain.is_empty() {
            body_len += plain.len();
            body_parts.push(plain);
            if body_len > 200 {
                break;
            }
        }
    }
    let excerpt = truncate_excerpt(&body_parts.join(" "), 168);
    (title, excerpt)
}

fn resolve_heading_anchor(md: &str, fragment: &str) -> Option<(usize, String, Option<String>)> {
    let want = slugify(fragment.trim_start_matches('#'));
    if want.is_empty() {
        return None;
    }
    let mut offset = 0;
    let mut matched = None;
    for raw_line in md.split_inclusive('\n') {
        let line = raw_line.trim_end_matches(['\r', '\n']);
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            let hashes = trimmed.bytes().take_while(|b| *b == b'#').count();
            if (1..=6).contains(&hashes) {
                let text = trimmed[hashes..].trim();
                if !text.is_empty() && slugify(text) == want {
                    matched = Some((offset, strip_inline_markdown(text), offset + raw_line.len()));
                    break;
                }
            }
        }
        offset += raw_line.len();
    }
    let (start, heading, after) = matched?;
    let (_, excerpt) = extract_markdown_summary(&md[after.min(md.len())..]);
    Some((start, heading, excerpt))
}

fn count_doc_backlinks(links: &[ParsedLink], target_url: &str) -> usize {
    let norm = normalize_internal_path(target_url);
    let frag = target_url.trim().starts_with('#');
    let count = links
        .iter()
        .filter(|l| {
            if frag {
                l.url.trim().eq_ignore_ascii_case(target_url.trim())
            } else {
                !is_external_url(&l.url) && normalize_internal_path(&l.url) == norm
            }
        })
        .count();
    count.max(1)
}

fn format_backlinks(count: usize) -> String {
    if count == 1 {
        "1 backlink".to_owned()
    } else {
        format!("{count} backlinks")
    }
}

fn fallback_title(raw_label: &str, stem_source: &str) -> String {
    if !raw_label.is_empty() {
        raw_label.to_owned()
    } else {
        prettify_stem(stem_source)
    }
}

fn anchor_preview(
    md: &str,
    all_links: &[ParsedLink],
    url: &str,
    raw_label: &str,
    title_attr: Option<String>,
) -> LinkPreview {
    let note_title = {
        let t = Sections::read(md).title(md).trim();
        (!t.is_empty()).then(|| t.to_owned())
    };
    let backlinks = format_backlinks(count_doc_backlinks(all_links, url));
    let (title, excerpt, meta_left, open_target) =
        if let Some((offset, heading, excerpt)) = resolve_heading_anchor(md, url) {
            (
                heading,
                excerpt.or(title_attr),
                "Section in this note",
                OpenTarget::HeadingOffset(offset),
            )
        } else {
            (
                fallback_title(raw_label, url.trim_start_matches('#')),
                title_attr,
                "Anchor link",
                OpenTarget::None,
            )
        };
    LinkPreview {
        is_internal: true,
        display_target: url.to_owned(),
        title,
        folder_or_host: note_title,
        excerpt,
        meta_left: Some(meta_left.to_owned()),
        meta_right: Some(backlinks),
        open_target,
    }
}

fn external_preview(url: &str, raw_label: &str, title_attr: Option<String>) -> LinkPreview {
    let host = extract_host(url);
    let disp = display_target(url, false);
    let title = if !raw_label.is_empty()
        && !raw_label.eq_ignore_ascii_case(url)
        && !raw_label.eq_ignore_ascii_case(&disp)
    {
        raw_label.to_owned()
    } else {
        host.clone().unwrap_or_else(|| disp.clone())
    };
    let excerpt = title_attr.or_else(|| (disp != title).then(|| truncate_excerpt(url, 168)).flatten());
    LinkPreview {
        is_internal: false,
        display_target: disp,
        title,
        folder_or_host: host,
        excerpt,
        meta_left: Some("External link".to_owned()),
        meta_right: Some("Open in browser".to_owned()),
        open_target: OpenTarget::ExternalUrl(normalized_external_url(url)),
    }
}

fn internal_preview(
    all_links: &[ParsedLink],
    url: &str,
    raw_label: &str,
    title_attr: Option<String>,
    has_space: bool,
    space_cache: &HashMap<String, CachedPage>,
) -> LinkPreview {
    let disp = display_target(url, true);
    let folder = extract_folder(&disp);
    let backlinks = format_backlinks(count_doc_backlinks(all_links, url));
    let space_path = space_md_path(url);
    let cached = space_path.as_ref().and_then(|p| space_cache.get(p));

    let (title, excerpt, meta_left) = match cached {
        Some(page) if page.found => (
            page.title
                .clone()
                .unwrap_or_else(|| fallback_title(raw_label, &disp)),
            page.excerpt.clone().or(title_attr),
            "Space page",
        ),
        Some(_) => (fallback_title(raw_label, &disp), title_attr, "Uncreated page"),
        None => (
            fallback_title(raw_label, &disp),
            title_attr,
            if has_space { "Space page" } else { "Linked page" },
        ),
    };

    let open_target = match space_path {
        Some(path) if has_space => OpenTarget::SpacePage(path),
        _ => OpenTarget::None,
    };

    LinkPreview {
        is_internal: true,
        display_target: disp,
        title,
        folder_or_host: folder,
        excerpt,
        meta_left: Some(meta_left.to_owned()),
        meta_right: Some(backlinks),
        open_target,
    }
}

fn build_preview(
    md: &str,
    all_links: &[ParsedLink],
    link: &ParsedLink,
    has_space: bool,
    space_cache: &HashMap<String, CachedPage>,
) -> LinkPreview {
    let raw_label = md
        .get(link.label_range.clone())
        .map(str::trim)
        .unwrap_or("");
    let url = link.url.trim();
    let title_attr = truncate_excerpt(&link.title, 168);

    if url.starts_with('#') {
        anchor_preview(md, all_links, url, raw_label, title_attr)
    } else if is_external_url(url) {
        external_preview(url, raw_label, title_attr)
    } else {
        internal_preview(all_links, url, raw_label, title_attr, has_space, space_cache)
    }
}

fn format_markdown_link(label: &str, url: &str, title: &str, was_autolink: bool) -> String {
    let url = url.trim();
    let label = label.trim();
    if url.is_empty() {
        return label.to_owned();
    }
    if was_autolink && (label.is_empty() || label == url) && title.is_empty() {
        return format!("<{url}>");
    }
    let text = if label.is_empty() { url } else { label };
    if title.is_empty() {
        format!("[{text}]({url})")
    } else {
        let escaped = title.replace('"', "\\\"");
        format!("[{text}]({url} \"{escaped}\")")
    }
}

struct LinkBounds {
    first_x: f64,
    last_x: f64,
    top: f64,
    bottom: f64,
}

fn link_fragment_bounds(
    layout: &parley::Layout<blitz_dom::node::TextBrush>,
    start_byte: usize,
    end_byte: usize,
    caret_byte: usize,
) -> LinkBounds {
    let start_cursor =
        parley::Cursor::from_byte_index(layout, start_byte, parley::Affinity::Downstream);
    let end_cursor =
        parley::Cursor::from_byte_index(layout, end_byte, parley::Affinity::Downstream);
    let frags = if end_byte > start_byte {
        parley::Selection::new(start_cursor, end_cursor).geometry(layout)
    } else {
        Vec::new()
    };
    if frags.is_empty() {
        let g = start_cursor.geometry(layout, 0.0);
        let caret =
            parley::Cursor::from_byte_index(layout, caret_byte, parley::Affinity::Downstream)
                .geometry(layout, 0.0);
        return LinkBounds {
            first_x: g.x0,
            last_x: caret.x0,
            top: caret.y0,
            bottom: caret.y1,
        };
    }
    let mut top = f64::INFINITY;
    let mut bottom = f64::NEG_INFINITY;
    let mut first_x = f64::INFINITY;
    let mut last_x = f64::INFINITY;
    for (r, _) in &frags {
        let (y0, y1, x0) = (f64::from(r.y0), f64::from(r.y1), f64::from(r.x0));
        if y0 < top - 1.0 {
            top = y0;
            first_x = x0;
        } else if (y0 - top).abs() <= 1.0 {
            first_x = first_x.min(x0);
        }
        if y1 > bottom + 1.0 {
            bottom = y1;
            last_x = x0;
        } else if (y1 - bottom).abs() <= 1.0 {
            last_x = last_x.min(x0);
        }
    }
    LinkBounds {
        first_x,
        last_x,
        top,
        bottom,
    }
}

fn place_card_in_paper(
    doc: &blitz_dom::BaseDocument,
    paper_rect: blitz_dom::BoundingRect,
    origin_x: f64,
    origin_y: f64,
    scale: f64,
    bounds: LinkBounds,
    link: &ParsedLink,
) -> Option<(f32, f32)> {
    let link_top = origin_y + bounds.top / scale;
    let link_bottom = origin_y + bounds.bottom / scale;
    let (vw, vh) = doc.viewport().logical_size();
    if link_bottom < 0.0 || link_top > f64::from(vh) {
        return None;
    }
    let bar_bottom = doc
        .get_element_by_id("mode-bar")
        .and_then(|n| doc.get_client_bounding_rect(n))
        .map_or(36.0, |r| r.y + r.height);
    let min_top = (bar_bottom + 6.0).max(42.0);
    let card_h = doc
        .get_element_by_id("link-card")
        .and_then(|n| doc.get_client_bounding_rect(n))
        .map(|r| r.height)
        .filter(|&h| h > 40.0)
        .unwrap_or_else(|| {
            if link.title.trim().is_empty() && !link.url.trim().starts_with('#') {
                120.0
            } else {
                CARD_EST_HEIGHT
            }
        });
    let gap = 4.0;
    let below_y = link_bottom + gap;
    let above_y = link_top - card_h - gap;
    let place_below = below_y + card_h <= f64::from(vh) - 8.0 || above_y < min_top;
    let anchor_x = if place_below {
        bounds.last_x
    } else {
        bounds.first_x
    };
    let client_x = (origin_x + anchor_x / scale)
        .clamp(8.0, (f64::from(vw) - CARD_WIDTH - 8.0).max(8.0));
    let client_y = if place_below {
        below_y.clamp(min_top, (f64::from(vh) - card_h.min(120.0) - 8.0).max(min_top))
    } else {
        above_y.max(min_top)
    };
    Some((
        (client_x - paper_rect.x).round() as f32,
        (client_y - paper_rect.y).round() as f32,
    ))
}

impl Writing {
    pub(super) fn link_at_pos(self, field: Field, pos: usize) -> Option<ParsedLink> {
        let md = self.contents.read();
        let field_range = self.range(field, &md);
        let parsed = self.parsed.read();
        parsed
            .links
            .iter()
            .find(|l| {
                l.outer.start >= field_range.start
                    && l.outer.end <= field_range.end
                    && pos > l.outer.start
                    && pos < l.outer.end
            })
            .cloned()
    }

    pub(super) fn refresh_link_card(mut self) {
        if *self.link_editing.peek() && self.link_card.peek().is_some() {
            return;
        }
        let next = self.compute_link_card();
        if *self.link_card.peek() != next {
            self.link_card.set(next);
        }
    }

    fn compute_link_card(mut self) -> Option<LinkCardState> {
        if self.drag.peek().is_some()
            || self.span.peek().is_some()
            || self.cells.peek().is_some()
            || self.selected_text.peek().is_some()
            || self.slash_menu.peek().is_some()
            || self.table_menu.peek().is_some()
            || self.block_menu.peek().is_some()
            || self.marker.peek().is_some()
        {
            return None;
        }
        let field = (*self.active.peek())?;
        if matches!(field, Field::TitleGap | Field::Source) {
            return None;
        }
        let [a, b] = self.absolute_selection()?;
        if a != b {
            return None;
        }
        let Some(link) = self.link_at_pos(field, b) else {
            if self.link_dismissed.peek().is_some() {
                self.link_dismissed.set(None);
            }
            return None;
        };
        if self.link_dismissed.peek().as_ref() == Some(&link.outer) {
            return None;
        }
        let handle = self.root.peek().clone()?;
        self.resolve_dom(&handle);
        let doc = handle.doc();
        let id = doc.get_element_by_id(&field.id())?;
        if doc.get_focussed_node_id() != Some(id) && self.focus.peek().is_none() {
            return None;
        }
        let node = doc.get_node(id)?;
        let input = node.element_data()?.text_input_data()?;
        let proj = self.projection_peek(field);
        // Wait until the native textarea buffer matches the active projection so
        // display byte offsets never index into a stale collapsed/expanded layout.
        if input.editor.raw_text() != proj.display {
            return None;
        }
        let paper_rect = doc
            .get_element_by_id("paper")
            .and_then(|n| doc.get_client_bounding_rect(n))?;
        let layout = input.editor.try_layout()?;
        let rect = doc.get_client_bounding_rect(id)?;
        let l = node.final_layout();
        let scale = f64::from(layout.scale());
        let raw_len = input.editor.raw_text().len();
        let (span_start_src, span_end_src) =
            if *self.edit_mode.peek() == super::projection::EditMode::Markdown {
                (link.outer.start, link.outer.end)
            } else {
                (
                    link.label_range.start.min(link.outer.end),
                    link.label_range.end.min(link.outer.end),
                )
            };
        let bounds = link_fragment_bounds(
            layout,
            proj.source_to_display(span_start_src).min(raw_len),
            proj.source_to_display(span_end_src).min(raw_len),
            proj.source_to_display(b).min(raw_len),
        );
        let origin_x = rect.x + f64::from(l.border.left + l.padding.left)
            - if input.is_multiline { 0.0 } else { f64::from(input.scroll_offset) };
        let origin_y = rect.y
            + f64::from(l.border.top + l.padding.top)
            + node.text_input_v_centering_offset(scale)
            - if input.is_multiline { f64::from(input.scroll_offset) } else { 0.0 };
        let (x, y) = place_card_in_paper(&doc, paper_rect, origin_x, origin_y, scale, bounds, &link)?;

        Some(LinkCardState { field, link, x, y })
    }

    pub(super) fn dismiss_link_card(mut self) {
        self.link_editing.set(false);
        let card = self.link_card.peek().clone();
        if let Some(card) = card {
            self.link_dismissed.set(Some(card.link.outer));
            self.link_card.set(None);
        }
    }

    pub(super) fn cancel_link_edit(mut self) {
        self.link_editing.set(false);
        if let Some(card) = self.link_card.peek().clone() {
            let pos = card.link.label_range.start.min(card.link.outer.end);
            self.open(card.field, [pos; 2]);
        }
    }

    pub(super) fn copy_link_target(self, target: &str) -> bool {
        let Some(handle) = self.root.peek().clone() else {
            return false;
        };
        handle
            .doc()
            .shell_provider
            .set_clipboard_text(target.to_owned())
            .is_ok()
    }

    fn open_link_target(self, target: OpenTarget) {
        match target {
            OpenTarget::HeadingOffset(pos) => {
                self.dismiss_link_card();
                self.open(self.field_at(pos), [pos; 2]);
            }
            OpenTarget::ExternalUrl(url) => {
                let _ = webbrowser::open(&url);
            }
            OpenTarget::SpacePage(path) => {
                if let Some(page) = self.page.peek().as_ref() {
                    let _ = webbrowser::open(&page.space.page_url(&path));
                }
            }
            OpenTarget::None => {}
        }
    }

    fn commit_link_edit(
        mut self,
        range: Range<usize>,
        replacement: &str,
        remember_sel: [usize; 2],
        next_caret: usize,
    ) {
        self.note.peek().remember_selection(remember_sel);
        let result = self.note.write().replace(range, replacement);
        self.link_editing.set(false);
        self.link_dismissed.set(None);
        self.link_card.set(None);
        if result.is_ok() {
            self.open(self.field_at(next_caret), [next_caret; 2]);
        }
        report(result, self.message);
    }

    pub(super) fn remove_active_link(self) {
        let Some(card) = self.link_card.peek().clone() else {
            return;
        };
        let md = self.contents.read().clone();
        if card.link.outer.end > md.len() || card.link.label_range.end > md.len() {
            self.dismiss_link_card();
            return;
        }
        let label = &md[card.link.label_range.clone()];
        let replacement = if label.is_empty() {
            card.link.url.as_str()
        } else {
            label
        };
        let sel = self
            .absolute_selection()
            .unwrap_or([card.link.label_range.start; 2]);
        let caret = card.link.outer.start + replacement.len();
        self.commit_link_edit(card.link.outer, replacement, sel, caret);
    }

    pub(super) fn save_edited_link(self, new_label: &str, new_url: &str) {
        let Some(card) = self.link_card.peek().clone() else {
            return;
        };
        let md = self.contents.read().clone();
        if card.link.outer.end > md.len() {
            self.dismiss_link_card();
            return;
        }
        let replacement = format_markdown_link(
            new_label,
            new_url,
            &card.link.title,
            card.link.is_autolink,
        );
        let sel = [card.link.label_range.start.min(md.len()); 2];
        let trimmed_url = new_url.trim();
        let caret = if trimmed_url.is_empty() {
            card.link.outer.start + replacement.len()
        } else {
            let inner_len = if new_label.trim().is_empty() {
                trimmed_url.len()
            } else {
                new_label.trim().len()
            };
            card.link.outer.start + 1 + inner_len
        };
        self.commit_link_edit(card.link.outer, &replacement, sel, caret);
    }
}

fn maybe_fetch_space_page(
    writing: Writing,
    url: &str,
    mut space_cache: Signal<HashMap<String, CachedPage>>,
    mut fetching: Signal<Option<String>>,
) {
    if is_external_url(url) || url.trim().starts_with('#') {
        return;
    }
    let Some(path) = space_md_path(url) else {
        return;
    };
    if space_cache.peek().contains_key(&path) || fetching.peek().as_deref() == Some(path.as_str()) {
        return;
    }
    let Some(page_src) = writing.page.peek().clone() else {
        return;
    };
    fetching.set(Some(path.clone()));
    spawn(async move {
        let space = page_src.space.clone();
        let lookup = path.clone();
        let fetched = work(move || space.read_optional(&lookup)).await;
        let entry = match fetched {
            Ok(Some(page)) => {
                let (title, excerpt) = extract_markdown_summary(&page.text);
                CachedPage {
                    title,
                    excerpt,
                    found: true,
                }
            }
            _ => CachedPage {
                title: None,
                excerpt: None,
                found: false,
            },
        };
        space_cache.write().insert(path.clone(), entry);
        if fetching.peek().as_deref() == Some(path.as_str()) {
            fetching.set(None);
        }
    });
}

#[component]
fn LinkEditForm(
    writing: Writing,
    edit_label: Signal<String>,
    edit_url: Signal<String>,
) -> Element {
    let save_edit = move |()| {
        let label = edit_label.peek().clone();
        let url = edit_url.peek().clone();
        writing.save_edited_link(&label, &url);
    };
    let cancel_edit = move |()| writing.cancel_link_edit();

    rsx! {
        div { class: "ui-link-card-head",
            span { class: "ui-link-card-edit-heading", "Edit link" }
        }
        div { class: "ui-link-card-form",
            div { class: "ui-link-card-field",
                span { class: "ui-link-card-label", "Text" }
                div { class: "ui-link-card-input",
                    TextField {
                        id: "link-edit-label",
                        value: edit_label,
                        placeholder: "Link text",
                        onenter: save_edit,
                        onescape: cancel_edit,
                    }
                }
            }
            div { class: "ui-link-card-field",
                span { class: "ui-link-card-label", "Target" }
                div { class: "ui-link-card-input",
                    TextField {
                        id: "link-edit-url",
                        value: edit_url,
                        placeholder: "Path or https://…",
                        onenter: save_edit,
                        onescape: cancel_edit,
                    }
                }
            }
            div { class: "ui-link-card-form-actions",
                Button {
                    id: "link-edit-cancel",
                    variant: Variant::Ghost,
                    label: "Cancel",
                    onclick: move |_| cancel_edit(()),
                }
                Button {
                    id: "link-edit-save",
                    variant: Variant::Primary,
                    label: "Save",
                    onclick: move |_| save_edit(()),
                }
            }
        }
    }
}

#[component]
fn LinkPreviewBody(
    writing: Writing,
    preview: LinkPreview,
    current_label: String,
    current_url: String,
    can_remove: bool,
    mut edit_label: Signal<String>,
    mut edit_url: Signal<String>,
    mut copied_url: Signal<Option<String>>,
) -> Element {
    let is_copied = copied_url.read().as_deref() == Some(current_url.as_str());
    let can_open = preview.open_target != OpenTarget::None;
    let open_target = preview.open_target.clone();
    let copy_url = current_url.clone();
    let start_edit_label = current_label;
    let start_edit_url = current_url.clone();

    rsx! {
        div { class: "ui-link-card-head",
            button {
                id: "link-card-open",
                class: "ui-link-card-target",
                class: if !can_open { "static" },
                aria_label: if can_open { format!("Open {}", preview.display_target) } else { preview.display_target.clone() },
                title: current_url,
                onclick: move |_| writing.open_link_target(open_target.clone()),
                {icon(if preview.is_internal { Icon::File } else { Icon::Open })}
                span { class: "ui-link-card-path", "{preview.display_target}" }
            }
            div { class: "ui-link-card-actions", role: "group", aria_label: "Link actions",
                Button {
                    id: "link-card-edit",
                    variant: Variant::Icon,
                    label: "Edit link",
                    tip: "Edit link",
                    icon: Icon::Pencil,
                    onclick: move |_| {
                        edit_label.set(start_edit_label.clone());
                        edit_url.set(start_edit_url.clone());
                        copied_url.set(None);
                        let mut w = writing;
                        w.link_editing.set(true);
                    },
                }
                Button {
                    id: "link-card-copy",
                    variant: Variant::Icon,
                    label: "Copy link",
                    tip: if is_copied { "Copied" } else { "Copy link" },
                    icon: if is_copied { Icon::Check } else { Icon::Copy },
                    selected: is_copied.then_some(true),
                    onclick: move |_| {
                        if writing.copy_link_target(&copy_url) {
                            copied_url.set(Some(copy_url.clone()));
                        }
                    },
                }
                if can_remove {
                    Button {
                        id: "link-card-remove",
                        variant: Variant::Icon,
                        label: "Remove link",
                        tip: "Remove link",
                        icon: Icon::Trash,
                        danger: true,
                        onclick: move |_| writing.remove_active_link(),
                    }
                }
            }
        }
        div { class: "ui-link-card-body",
            div { class: "ui-link-card-title", "{preview.title}" }
            if let Some(sub) = &preview.folder_or_host {
                div { class: "ui-link-card-sub",
                    {icon(if preview.is_internal { Icon::Folder } else { Icon::Link })}
                    span { "{sub}" }
                }
            }
            if let Some(excerpt) = &preview.excerpt {
                p { class: "ui-link-card-excerpt", "{excerpt}" }
            }
            if preview.meta_left.is_some() || preview.meta_right.is_some() {
                div { class: "ui-link-card-meta",
                    if let Some(left) = &preview.meta_left {
                        span { class: "ui-link-card-meta-item",
                            {icon(if preview.is_internal { Icon::Clock } else { Icon::Link })}
                            span { "{left}" }
                        }
                    }
                    if let Some(right) = &preview.meta_right {
                        span { class: "ui-link-card-meta-item",
                            {icon(if preview.is_internal { Icon::Link } else { Icon::Open })}
                            span { "{right}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn LinkCard(writing: Writing) -> Element {
    let edit_label = use_signal(String::new);
    let edit_url = use_signal(String::new);
    let copied_url = use_signal(|| None::<String>);
    let space_cache = use_signal(HashMap::<String, CachedPage>::new);
    let fetching = use_signal(|| None::<String>);

    use_effect(move || {
        if (writing.link_editing)()
            && let Some(handle) = writing.root.peek().as_ref()
        {
            {
                let mut doc = handle.doc_mut();
                doc.resolve(0.0);
                if let Some(id) = doc.get_element_by_id("link-edit-label") {
                    doc.set_focus_to(id);
                }
            }
            writing.refresh_boxes();
        }
    });

    let Some(card) = (writing.link_card)() else {
        return rsx! {};
    };
    let editing = (writing.link_editing)();
    let md = writing.contents.read();
    let parsed = writing.parsed.read();
    let has_space = writing.page.read().is_some();

    if has_space {
        maybe_fetch_space_page(writing, &card.link.url, space_cache, fetching);
    }

    let preview = build_preview(&md, &parsed.links, &card.link, has_space, &space_cache.read());
    let current_label = md
        .get(card.link.label_range.clone())
        .unwrap_or("")
        .to_owned();
    let current_url = card.link.url.clone();
    drop(parsed);
    drop(md);

    let can_remove = card.link.outer != card.link.label_range;

    rsx! {
        Popover {
            id: "link-card",
            class: "ui-link-card link-card",
            role: "dialog",
            label: "Link details",
            keep_focus: !editing,
            contain: true,
            style: format!("left:{:.0}px; top:{:.0}px;", card.x, card.y),
            if editing {
                LinkEditForm { writing, edit_label, edit_url }
            } else {
                LinkPreviewBody {
                    writing,
                    preview,
                    current_label,
                    current_url,
                    can_remove,
                    edit_label,
                    edit_url,
                    copied_url,
                }
            }
        }
    }
}
