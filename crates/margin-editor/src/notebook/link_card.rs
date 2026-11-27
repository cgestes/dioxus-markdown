//! Link preview and quick-action popover shown when the caret sits inside a
//! Markdown link (`[label](target)` or `<url>`).
use super::link_preview::{
    CachedPage, LinkPreview, OpenTarget, build_preview, extract_markdown_summary,
    format_markdown_link, is_external_url, page_cache_key, page_path,
};
use super::projection::ParsedLink;
use crate::platform::text;
use crate::ui::{Button, Icon, Popover, TextField, Variant, icon};
use super::{Field, Writing, report};
use crate::prelude::*;
use std::collections::HashMap;
use std::ops::Range;

const CARD_WIDTH: f64 = 336.0;
const CARD_EST_HEIGHT: f64 = 176.0;

/// The card of the link under the caret.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Links {
    pub card: Signal<Option<LinkCardState>>,
    /// A link whose card was closed; it stays closed until the caret leaves it.
    pub dismissed: Signal<Option<Range<usize>>>,
    /// The card's fields are being edited.
    pub editing: Signal<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LinkCardState {
    pub field: Field,
    pub link: ParsedLink,
    pub x: f32,
    pub y: f32,
}

struct LinkBounds {
    first_x: f64,
    last_x: f64,
    top: f64,
    bottom: f64,
}

fn link_fragment_bounds(
    layout: &text::Layout,
    start_byte: usize,
    end_byte: usize,
    caret_byte: usize,
) -> LinkBounds {
    let start_cursor =
        text::Cursor::from_byte_index(layout, start_byte, text::Affinity::Downstream);
    let end_cursor =
        text::Cursor::from_byte_index(layout, end_byte, text::Affinity::Downstream);
    let frags = if end_byte > start_byte {
        text::Selection::new(start_cursor, end_cursor).geometry(layout)
    } else {
        Vec::new()
    };
    if frags.is_empty() {
        let g = start_cursor.geometry(layout, 0.0);
        let caret =
            text::Cursor::from_byte_index(layout, caret_byte, text::Affinity::Downstream)
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
    doc: &crate::platform::Document,
    paper_rect: crate::platform::BoundingRect,
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
        if *self.links.editing.peek() && self.links.card.peek().is_some() {
            return;
        }
        let next = self.compute_link_card();
        if *self.links.card.peek() != next {
            self.links.card.set(next);
        }
    }

    fn compute_link_card(mut self) -> Option<LinkCardState> {
        if self.drag.peek().is_some()
            || self.span.peek().is_some()
            || self.grid.cells.peek().is_some()
            || self.selected_text.peek().is_some()
            || self.slash.menu.peek().is_some()
            || self.grid.menu.peek().is_some()
            || self.handles.menu.peek().is_some()
            || self.callout_menu.peek().is_some()
            || self.code_lang_menu.peek().is_some()
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
            if self.links.dismissed.peek().is_some() {
                self.links.dismissed.set(None);
            }
            return None;
        };
        if self.links.dismissed.peek().as_ref() == Some(&link.outer) {
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
        self.links.editing.set(false);
        let card = self.links.card.peek().clone();
        if let Some(card) = card {
            self.links.dismissed.set(Some(card.link.outer));
            self.links.card.set(None);
        }
    }

    pub(super) fn cancel_link_edit(mut self) {
        self.links.editing.set(false);
        if let Some(card) = self.links.card.peek().clone() {
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
                crate::platform::open_url(&url);
            }
            OpenTarget::Page(path) => {
                if let Some(page) = self.page.peek().as_ref() {
                    crate::platform::open_url(&page.files.web_url(&path));
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
        self.links.editing.set(false);
        self.links.dismissed.set(None);
        self.links.card.set(None);
        if result.is_ok() {
            self.open(self.field_at(next_caret), [next_caret; 2]);
        }
        report(result, self.message);
    }

    pub(super) fn remove_active_link(self) {
        let Some(card) = self.links.card.peek().clone() else {
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

    pub(crate) fn save_edited_link(self, new_label: &str, new_url: &str) {
        let Some(card) = self.links.card.peek().clone() else {
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

fn maybe_fetch_page(
    writing: Writing,
    page_root: &str,
    url: &str,
    mut page_cache: Signal<HashMap<String, CachedPage>>,
    mut fetching: Signal<Option<String>>,
) {
    if is_external_url(url) || url.trim().starts_with('#') {
        return;
    }
    let Some(path) = page_path(url) else {
        return;
    };
    let key = page_cache_key(page_root, &path);
    if page_cache.peek().contains_key(&key) || fetching.peek().as_deref() == Some(key.as_str()) {
        return;
    }
    let Some(page_src) = writing.page.peek().clone() else {
        return;
    };
    fetching.set(Some(key.clone()));
    spawn(async move {
        let files = page_src.files.clone();
        let lookup = path.clone();
        let fetched = files.read(lookup).await;
        match fetched {
            Ok(Some(page)) => {
                let (title, excerpt) = extract_markdown_summary(&page.text);
                page_cache.write().insert(
                    key.clone(),
                    CachedPage {
                        title,
                        excerpt,
                        found: true,
                    },
                );
            }
            Ok(None) => {
                page_cache.write().insert(
                    key.clone(),
                    CachedPage {
                        title: None,
                        excerpt: None,
                        found: false,
                    },
                );
            }
            // Do not negatively cache transient transport/server errors as "Uncreated page".
            Err(_) => {}
        }
        if fetching.peek().as_deref() == Some(key.as_str()) {
            fetching.set(None);
        }
    });
}

const EDIT_FIELD: &str = "flex items-center px-[9px] h-[30px] border border-solid border-line-strong rounded-md bg-surface type-ui focus-within:border-accent [&_.ui-field_input]:h-7 [&_.ui-field_input]:leading-7 [&_.ui-field-ph]:leading-7";
const META_ITEM: &str = "flex items-center gap-[5px] whitespace-nowrap [&_.i]:w-[12.5px] [&_.i]:h-[12.5px] [&_.i]:text-faint";

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
        div { class: "flex items-center gap-2 py-[9px] pr-2.5 pl-[13px] border-0 border-b border-solid border-line",
            span { class: "text-ink-strong type-control leading-[26px]", "Edit link" }
        }
        div { class: "flex flex-col gap-[9px] pt-[11px] px-[13px] pb-3",
            div { class: "flex flex-col gap-1",
                span { class: "text-faint type-label uppercase", "Text" }
                div { class: EDIT_FIELD,
                    TextField {
                        id: "link-edit-label",
                        value: edit_label,
                        placeholder: "Link text",
                        onenter: save_edit,
                        onescape: cancel_edit,
                    }
                }
            }
            div { class: "flex flex-col gap-1",
                span { class: "text-faint type-label uppercase", "Target" }
                div { class: EDIT_FIELD,
                    TextField {
                        id: "link-edit-url",
                        value: edit_url,
                        placeholder: "Path or https://…",
                        onenter: save_edit,
                        onescape: cancel_edit,
                    }
                }
            }
            div { class: "flex items-center justify-end gap-1.5 mt-0.5 [&_.ui-btn]:h-7 [&_.ui-btn]:px-[11px]",
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

    let target_state = if can_open {
        "cursor-pointer hover:bg-transparent hover:text-accent-deep [&:hover_.i]:text-accent-deep"
    } else {
        "cursor-default hover:bg-transparent hover:text-ink [&:hover_.i]:text-muted"
    };

    rsx! {
        div { class: "flex items-center gap-2 py-[9px] pr-2.5 pl-[13px] border-0 border-b border-solid border-line",
            button {
                id: "link-card-open",
                class: "flex items-center justify-start gap-[7px] flex-auto min-w-0 h-[26px] m-0 pr-1 pl-0 border-0 rounded-sm bg-transparent text-ink font-sans text-[13.5px] leading-[1.3] font-medium text-left [&_.i]:w-[14px] [&_.i]:h-[14px] [&_.i]:text-muted {target_state}",
                aria_label: if can_open { format!("Open {}", preview.display_target) } else { preview.display_target.clone() },
                title: current_url,
                onclick: move |_| writing.open_link_target(open_target.clone()),
                {icon(if preview.is_internal { Icon::File } else { Icon::Open })}
                span { class: "flex-auto min-w-0 overflow-hidden text-ellipsis whitespace-nowrap", "{preview.display_target}" }
            }
            div { class: "flex items-center gap-0.5 flex-none [&_.ui-btn.icon]:w-[26px] [&_.ui-btn.icon]:h-[26px] [&_.ui-btn.icon]:rounded-sm [&_.ui-btn.icon]:text-muted [&_.ui-btn.icon_.tool-icon]:w-[14px] [&_.ui-btn.icon_.tool-icon]:h-[14px] [&_.ui-btn.icon_.tool-icon_svg]:w-[14px] [&_.ui-btn.icon_.tool-icon_svg]:h-[14px]", role: "group", aria_label: "Link actions",
                if !writing.read_only {
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
                            w.links.editing.set(true);
                        },
                    }
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
        div { class: "flex flex-col gap-[5px] pt-3 px-[14px] pb-[13px]",
            div { class: "text-ink-strong font-display text-[15px] leading-[1.3] font-semibold tracking-[-0.01em] [overflow-wrap:anywhere]", "{preview.title}" }
            if let Some(sub) = &preview.folder_or_host {
                div { class: "flex items-center gap-[5px] text-faint font-sans text-[12.5px] leading-[1.3] font-normal [&_.i]:w-[13px] [&_.i]:h-[13px] [&_.i]:text-faint",
                    {icon(if preview.is_internal { Icon::Folder } else { Icon::Link })}
                    span { "{sub}" }
                }
            }
            if let Some(excerpt) = &preview.excerpt {
                p { class: "mt-[3px] mx-0 mb-0 text-muted font-sans text-[13px] leading-[1.45] font-normal [overflow-wrap:anywhere]", "{excerpt}" }
            }
            if preview.meta_left.is_some() || preview.meta_right.is_some() {
                div { class: "flex items-center flex-wrap gap-y-1.5 gap-x-[14px] mt-1.5 text-faint type-hint",
                    if let Some(left) = &preview.meta_left {
                        span { class: META_ITEM,
                            {icon(if preview.is_internal { Icon::Clock } else { Icon::Link })}
                            span { "{left}" }
                        }
                    }
                    if let Some(right) = &preview.meta_right {
                        span { class: META_ITEM,
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
    let page_cache = use_signal(HashMap::<String, CachedPage>::new);
    let fetching = use_signal(|| None::<String>);

    use_effect(move || {
        if (writing.links.editing)()
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

    let Some(card) = (writing.links.card)() else {
        return rsx! {};
    };
    let editing = (writing.links.editing)();
    let md = writing.contents.read();
    let parsed = writing.parsed.read();
    let page_root = writing.page.read().as_ref().map(|p| p.files.web_url(""));

    if let Some(root) = page_root.as_deref() {
        maybe_fetch_page(writing, root, &card.link.url, page_cache, fetching);
    }

    let preview = build_preview(
        &md,
        &parsed.links,
        &card.link,
        page_root.as_deref(),
        &page_cache.read(),
    );
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
            class: "absolute z-[13] !p-0 w-[336px] overflow-hidden",
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
