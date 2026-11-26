//! Link preview and quick-action popover shown when the caret sits inside a
//! Markdown link (`[label](target)` or `<url>`).
use super::document::Sections;
use super::projection::ParsedLink;
use super::ui::{Button, Icon, Popover, TextField, Variant, icon};
use super::worker::work;
use super::{Field, Writing, report};
use dioxus_native::prelude::*;
use std::collections::HashMap;

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
    if is_internal {
        if trimmed.starts_with('#') {
            return trimmed.to_owned();
        }
        let clean = normalize_internal_path(trimmed);
        if let Some((_, frag)) = trimmed.split_once('#') {
            if clean.is_empty() {
                format!("#{frag}")
            } else {
                format!("{clean}#{frag}")
            }
        } else if clean.is_empty() {
            trimmed.to_owned()
        } else {
            clean
        }
    } else {
        let stripped = trimmed
            .strip_prefix("https://")
            .or_else(|| trimmed.strip_prefix("http://"))
            .or_else(|| trimmed.strip_prefix("mailto:"))
            .unwrap_or(trimmed);
        stripped.trim_end_matches('/').to_owned()
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
            body_parts.push(plain);
            if body_parts.iter().map(|s| s.len()).sum::<usize>() > 200 {
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

    // 1. In-document heading anchor (`#heading-slug`).
    if url.starts_with('#') {
        let note_title = {
            let t = Sections::read(md).title(md).trim();
            (!t.is_empty()).then(|| t.to_owned())
        };
        let backlinks = format_backlinks(count_doc_backlinks(all_links, url));
        if let Some((offset, heading, excerpt)) = resolve_heading_anchor(md, url) {
            return LinkPreview {
                is_internal: true,
                display_target: url.to_owned(),
                title: heading,
                folder_or_host: note_title,
                excerpt: excerpt.or(title_attr),
                meta_left: Some("Section in this note".to_owned()),
                meta_right: Some(backlinks),
                open_target: OpenTarget::HeadingOffset(offset),
            };
        }
        let fallback_title = if !raw_label.is_empty() {
            raw_label.to_owned()
        } else {
            prettify_stem(url.trim_start_matches('#'))
        };
        return LinkPreview {
            is_internal: true,
            display_target: url.to_owned(),
            title: fallback_title,
            folder_or_host: note_title,
            excerpt: title_attr,
            meta_left: Some("Anchor link".to_owned()),
            meta_right: Some(backlinks),
            open_target: OpenTarget::None,
        };
    }

    // 2. External URL (`https://`, `http://`, `mailto:`, `www.`).
    if is_external_url(url) {
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
        let excerpt = title_attr.or_else(|| {
            if disp != title {
                truncate_excerpt(url, 168)
            } else {
                None
            }
        });
        return LinkPreview {
            is_internal: false,
            display_target: disp,
            title,
            folder_or_host: host,
            excerpt,
            meta_left: Some("External link".to_owned()),
            meta_right: Some("Open in browser".to_owned()),
            open_target: OpenTarget::ExternalUrl(normalized_external_url(url)),
        };
    }

    // 3. Internal page / note link (e.g. `brain/log`, `car/maintenance.md`).
    let disp = display_target(url, true);
    let folder = extract_folder(&disp);
    let backlinks = format_backlinks(count_doc_backlinks(all_links, url));
    let space_path = space_md_path(url);
    let cached = space_path.as_ref().and_then(|p| space_cache.get(p));

    let (title, excerpt, meta_left) = match cached {
        Some(page) if page.found => (
            page.title.clone().unwrap_or_else(|| {
                if !raw_label.is_empty() {
                    raw_label.to_owned()
                } else {
                    prettify_stem(&disp)
                }
            }),
            page.excerpt.clone().or(title_attr),
            Some("Space page".to_owned()),
        ),
        Some(_) => (
            if !raw_label.is_empty() {
                raw_label.to_owned()
            } else {
                prettify_stem(&disp)
            },
            title_attr,
            Some("Uncreated page".to_owned()),
        ),
        None => (
            if !raw_label.is_empty() {
                raw_label.to_owned()
            } else {
                prettify_stem(&disp)
            },
            title_attr,
            Some(if has_space {
                "Space page".to_owned()
            } else {
                "Linked page".to_owned()
            }),
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
        meta_left,
        meta_right: Some(backlinks),
        open_target,
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
        let mode = *self.edit_mode.peek();
        let span_start_src = if mode == super::projection::EditMode::Markdown {
            link.outer.start
        } else {
            link.label_range.start.min(link.outer.end)
        };
        let span_end_src = if mode == super::projection::EditMode::Markdown {
            link.outer.end
        } else {
            link.label_range.end.min(link.outer.end)
        };
        let start_byte = proj.source_to_display(span_start_src).min(raw_len);
        let end_byte = proj.source_to_display(span_end_src).min(raw_len);
        let caret_byte = proj.source_to_display(b).min(raw_len);
        let start_cursor =
            parley::Cursor::from_byte_index(layout, start_byte, parley::Affinity::Downstream);
        let end_cursor =
            parley::Cursor::from_byte_index(layout, end_byte, parley::Affinity::Downstream);
        let caret_geom =
            parley::Cursor::from_byte_index(layout, caret_byte, parley::Affinity::Downstream)
                .geometry(layout, 0.0);
        let frags = if end_byte > start_byte {
            parley::Selection::new(start_cursor, end_cursor).geometry(layout)
        } else {
            Vec::new()
        };
        let origin_x = rect.x + f64::from(l.border.left + l.padding.left)
            - if input.is_multiline {
                0.0
            } else {
                f64::from(input.scroll_offset)
            };
        let origin_y = rect.y
            + f64::from(l.border.top + l.padding.top)
            + node.text_input_v_centering_offset(scale)
            - if input.is_multiline {
                f64::from(input.scroll_offset)
            } else {
                0.0
            };
        let (first_x, last_x, span_top, span_bottom) = if !frags.is_empty() {
            let mut min_y = f64::INFINITY;
            let mut max_y = f64::NEG_INFINITY;
            let mut top_x = f64::INFINITY;
            let mut bot_x = f64::INFINITY;
            for (r, _) in &frags {
                let y0 = f64::from(r.y0);
                let y1 = f64::from(r.y1);
                let x0 = f64::from(r.x0);
                if y0 < min_y - 1.0 {
                    min_y = y0;
                    top_x = x0;
                } else if (y0 - min_y).abs() <= 1.0 {
                    top_x = top_x.min(x0);
                }
                if y1 > max_y + 1.0 {
                    max_y = y1;
                    bot_x = x0;
                } else if (y1 - max_y).abs() <= 1.0 {
                    bot_x = bot_x.min(x0);
                }
            }
            (top_x, bot_x, min_y, max_y)
        } else {
            let g = start_cursor.geometry(layout, 0.0);
            (
                g.x0,
                caret_geom.x0,
                caret_geom.y0,
                caret_geom.y1,
            )
        };
        let link_top = origin_y + span_top / scale;
        let link_bottom = origin_y + span_bottom / scale;
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
        let anchor_x = if place_below { last_x } else { first_x };
        let client_x = (origin_x + anchor_x / scale)
            .clamp(8.0, (f64::from(vw) - CARD_WIDTH - 8.0).max(8.0));
        let client_y = if place_below {
            below_y.clamp(min_top, (f64::from(vh) - card_h.min(120.0) - 8.0).max(min_top))
        } else {
            above_y.max(min_top)
        };
        let x = (client_x - paper_rect.x).round() as f32;
        let y = (client_y - paper_rect.y).round() as f32;

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

    pub(super) fn remove_active_link(mut self) {
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
        self.note.peek().remember_selection(sel);
        let result = self
            .note
            .write()
            .replace(card.link.outer.clone(), replacement);
        self.link_editing.set(false);
        self.link_dismissed.set(None);
        self.link_card.set(None);
        if result.is_ok() {
            let caret = card.link.outer.start + replacement.len();
            self.open(self.field_at(caret), [caret; 2]);
        }
        report(result, self.message);
    }

    pub(super) fn save_edited_link(mut self, new_label: &str, new_url: &str) {
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
        self.note.peek().remember_selection(sel);
        let result = self
            .note
            .write()
            .replace(card.link.outer.clone(), &replacement);
        self.link_editing.set(false);
        self.link_dismissed.set(None);
        self.link_card.set(None);
        if result.is_ok() {
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
            self.open(self.field_at(caret), [caret; 2]);
        }
        report(result, self.message);
    }
}

#[component]
pub(super) fn LinkCard(writing: Writing) -> Element {
    let mut edit_label = use_signal(String::new);
    let mut edit_url = use_signal(String::new);
    let mut copied_url = use_signal(|| None::<String>);
    let mut space_cache = use_signal(HashMap::<String, CachedPage>::new);
    let mut fetching = use_signal(|| None::<String>);

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

    // Fetch real Space page title and excerpt asynchronously once per internal target.
    if has_space && !is_external_url(&card.link.url) && !card.link.url.trim().starts_with('#')
        && let Some(path) = space_md_path(&card.link.url)
        && !space_cache.peek().contains_key(&path)
        && fetching.peek().as_deref() != Some(path.as_str())
    {
        fetching.set(Some(path.clone()));
        if let Some(page_src) = writing.page.peek().clone() {
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
    }

    let preview = build_preview(&md, &parsed.links, &card.link, has_space, &space_cache.read());
    let current_label = md
        .get(card.link.label_range.clone())
        .unwrap_or("")
        .to_owned();
    let current_url = card.link.url.clone();
    drop(parsed);
    drop(md);

    let is_copied = copied_url.read().as_deref() == Some(current_url.as_str());
    let can_open = preview.open_target != OpenTarget::None;
    let can_remove = card.link.outer != card.link.label_range;
    let open_target = preview.open_target.clone();
    let copy_url = current_url.clone();
    let start_edit_label = current_label.clone();
    let start_edit_url = current_url.clone();

    let save_edit = move |()| {
        let label = edit_label.peek().clone();
        let url = edit_url.peek().clone();
        writing.save_edited_link(&label, &url);
    };
    let cancel_edit = move |()| {
        writing.cancel_link_edit();
    };

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
                            onclick: move |_| writing.cancel_link_edit(),
                        }
                        Button {
                            id: "link-edit-save",
                            variant: Variant::Primary,
                            label: "Save",
                            onclick: move |_| {
                                let label = edit_label.peek().clone();
                                let url = edit_url.peek().clone();
                                writing.save_edited_link(&label, &url);
                            },
                        }
                    }
                }
            } else {
                div { class: "ui-link-card-head",
                    button {
                        id: "link-card-open",
                        class: "ui-link-card-target",
                        class: if !can_open { "static" },
                        aria_label: if can_open { format!("Open {}", preview.display_target) } else { preview.display_target.clone() },
                        title: current_url.clone(),
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
    }
}
