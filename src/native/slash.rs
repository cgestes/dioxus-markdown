//! Slash command menu (`/`) on an empty line for fast keyboard/mouse block insertion.
use super::{Field, Icon, Writing, report};
use dioxus_native::prelude::*;
use margin_editor_core::callout::CalloutKind;

#[derive(Clone, PartialEq, Debug)]
pub struct SlashState {
    pub field: Field,
    pub slash_source: usize,
    pub query: String,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SlashKind {
    H1,
    H2,
    H3,
    Bullet,
    Task,
    Quote,
    Callout(CalloutKind),
    CodeBlock,
    Table,
    Rule,
    Image,
}

pub struct SlashItem {
    pub kind: SlashKind,
    pub id: &'static str,
    pub title: &'static str,
    pub hint: &'static str,
    pub keywords: &'static [&'static str],
    pub glyph: Option<&'static str>,
    pub icon: Option<Icon>,
}

const ITEMS: &[SlashItem] = &[
    SlashItem {
        kind: SlashKind::H1,
        id: "slash-h1",
        title: "Heading 1",
        hint: "#",
        keywords: &["h1", "heading1", "title", "big"],
        glyph: Some("H1"),
        icon: None,
    },
    SlashItem {
        kind: SlashKind::H2,
        id: "slash-h2",
        title: "Heading 2",
        hint: "##",
        keywords: &["h2", "heading2", "section"],
        glyph: Some("H2"),
        icon: None,
    },
    SlashItem {
        kind: SlashKind::H3,
        id: "slash-h3",
        title: "Heading 3",
        hint: "###",
        keywords: &["h3", "heading3", "sub"],
        glyph: Some("H3"),
        icon: None,
    },
    SlashItem {
        kind: SlashKind::Bullet,
        id: "slash-bullet",
        title: "Bullet list",
        hint: "-",
        keywords: &["bullet", "list", "ul"],
        glyph: None,
        icon: Some(Icon::List),
    },
    SlashItem {
        kind: SlashKind::Task,
        id: "slash-task",
        title: "Checklist",
        hint: "- [ ]",
        keywords: &["task", "todo", "check", "checkbox"],
        glyph: None,
        icon: Some(Icon::Task),
    },
    SlashItem {
        kind: SlashKind::Quote,
        id: "slash-quote",
        title: "Blockquote",
        hint: ">",
        keywords: &["quote", "blockquote"],
        glyph: None,
        icon: Some(Icon::Quote),
    },
    SlashItem {
        kind: SlashKind::Callout(CalloutKind::Note),
        id: "slash-callout",
        title: "Note callout",
        hint: "[!NOTE]",
        keywords: &["callout", "note", "info", "admonition", "banner"],
        glyph: None,
        icon: Some(Icon::Info),
    },
    SlashItem {
        kind: SlashKind::Callout(CalloutKind::Tip),
        id: "slash-tip",
        title: "Tip callout",
        hint: "[!TIP]",
        keywords: &["tip", "callout", "hint", "idea", "admonition"],
        glyph: None,
        icon: Some(Icon::Spark),
    },
    SlashItem {
        kind: SlashKind::Callout(CalloutKind::Important),
        id: "slash-important",
        title: "Important callout",
        hint: "[!IMPORTANT]",
        keywords: &["important", "callout", "key", "admonition"],
        glyph: None,
        icon: Some(Icon::Important),
    },
    SlashItem {
        kind: SlashKind::Callout(CalloutKind::Warning),
        id: "slash-warning",
        title: "Warning callout",
        hint: "[!WARNING]",
        keywords: &["warning", "warn", "callout", "alert", "admonition"],
        glyph: None,
        icon: Some(Icon::AlertTriangle),
    },
    SlashItem {
        kind: SlashKind::Callout(CalloutKind::Caution),
        id: "slash-caution",
        title: "Caution callout",
        hint: "[!CAUTION]",
        keywords: &["caution", "danger", "callout", "error", "admonition"],
        glyph: None,
        icon: Some(Icon::AlertOctagon),
    },
    SlashItem {
        kind: SlashKind::CodeBlock,
        id: "slash-code",
        title: "Code block",
        hint: "```",
        keywords: &["code", "fence", "pre", "snippet"],
        glyph: None,
        icon: Some(Icon::Code),
    },
    SlashItem {
        kind: SlashKind::Table,
        id: "slash-table",
        title: "Table",
        hint: "| |",
        keywords: &["table", "grid", "columns"],
        glyph: None,
        icon: Some(Icon::Table),
    },
    SlashItem {
        kind: SlashKind::Rule,
        id: "slash-rule",
        title: "Divider",
        hint: "---",
        keywords: &["rule", "hr", "divider", "line"],
        glyph: None,
        icon: Some(Icon::Rule),
    },
    SlashItem {
        kind: SlashKind::Image,
        id: "slash-image",
        title: "Image",
        hint: "![ ]",
        keywords: &["image", "img", "photo", "picture"],
        glyph: None,
        icon: Some(Icon::Image),
    },
];

pub fn matching_items(query: &str) -> Vec<&'static SlashItem> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return ITEMS.iter().collect();
    }
    let mut primary = Vec::new();
    let mut callouts = Vec::new();
    for item in ITEMS {
        if item.title.to_ascii_lowercase().contains(&q)
            || item.keywords.iter().any(|k| k.contains(&q))
        {
            if matches!(item.kind, SlashKind::Callout(_)) {
                callouts.push(item);
            } else {
                primary.push(item);
            }
        }
    }
    primary.extend(callouts);
    primary
}

impl Writing {
    pub fn refresh_slash(mut self) {
        let next = self.compute_slash();
        let prev = self.slash_menu.peek().clone();
        if prev != next {
            match (&prev, &next) {
                (Some(p), Some(n)) if p.slash_source == n.slash_source => {
                    let count = matching_items(&n.query).len().max(1);
                    let sel = (*self.slash_selected.peek()).min(count - 1);
                    if *self.slash_selected.peek() != sel {
                        self.slash_selected.set(sel);
                    }
                }
                _ => {
                    if *self.slash_selected.peek() != 0 {
                        self.slash_selected.set(0);
                    }
                }
            }
            if prev.is_none() && next.is_some() {
                self.close_menus();
            }
            self.slash_menu.set(next);
        }
    }

    fn compute_slash(self) -> Option<SlashState> {
        if self.span.peek().is_some()
            || self.cells.peek().is_some()
            || self.drag.peek().is_some()
            || self.selected_text.peek().is_some()
            || self.marker.peek().is_some()
        {
            return None;
        }
        let field = (*self.active.peek())?;
        if !matches!(field, Field::Body(_)) {
            return None;
        }
        let [a, b] = self.absolute_selection()?;
        if a != b {
            return None;
        }
        let md = self.contents.read();
        let head = md.floor_char_boundary(b.min(md.len()));
        let line_start = md[..head].rfind('\n').map_or(0, |i| i + 1);
        let line_end = head + md[head..].find('\n').unwrap_or(md.len() - head);
        let line = md[line_start..line_end].trim_end_matches('\r');
        if !line.starts_with('/') || head < line_start + 1 || line.len() > 24 {
            return None;
        }
        let query = &md[line_start + 1..head];
        if query.chars().any(char::is_whitespace) || line_end > head {
            return None;
        }
        let matches = matching_items(query);
        if matches.is_empty() {
            return None;
        }
        let handle = self.root.peek().clone()?;
        let doc = handle.doc();
        let id = doc.get_element_by_id(&field.id())?;
        let node = doc.get_node(id)?;
        let input = node.element_data()?.text_input_data()?;
        let layout = input.editor.try_layout()?;
        let rect = doc.get_client_bounding_rect(id)?;
        let caret = input.editor.raw_selection().focus().geometry(layout, 0.0);
        let scale = f64::from(layout.scale());
        let l = node.final_layout();
        let (vw, vh) = doc.viewport().logical_size();
        let menu_w = 216.0_f64;
        let menu_h = (matches.len() as f64 * 30.0 + 12.0).min(280.0);
        let raw_x = rect.x + f64::from(l.border.left + l.padding.left) + caret.x0 / scale;
        let below_y = rect.y + f64::from(l.border.top + l.padding.top) + caret.y1 / scale
            - f64::from(input.scroll_offset)
            + 4.0;
        let above_y = rect.y + f64::from(l.border.top + l.padding.top) + caret.y0 / scale
            - f64::from(input.scroll_offset)
            - menu_h
            - 4.0;
        let x = raw_x.clamp(8.0, (f64::from(vw) - menu_w - 8.0).max(8.0)) as f32;
        let y = if below_y + menu_h <= f64::from(vh) - 8.0 || above_y < 42.0 {
            below_y.clamp(42.0, (f64::from(vh) - menu_h - 8.0).max(42.0))
        } else {
            above_y
        } as f32;
        Some(SlashState {
            field,
            slash_source: line_start,
            query: query.to_owned(),
            x,
            y,
        })
    }

    pub fn slash_key(mut self, event: &KeyboardEvent) -> bool {
        let Some(state) = self.slash_menu.peek().clone() else {
            return false;
        };
        if event.is_composing()
            || event
                .modifiers()
                .intersects(Modifiers::CONTROL | Modifiers::META | Modifiers::ALT)
        {
            return false;
        }
        let matches = matching_items(&state.query);
        if matches.is_empty() {
            self.slash_menu.set(None);
            return false;
        }
        let sel = (*self.slash_selected.peek()).min(matches.len() - 1);
        match event.key() {
            Key::Named(NamedKey::Escape) => {
                event.prevent_default();
                event.stop_propagation();
                self.slash_menu.set(None);
                true
            }
            Key::Named(NamedKey::ArrowDown) => {
                event.prevent_default();
                event.stop_propagation();
                self.slash_selected.set((sel + 1) % matches.len());
                true
            }
            Key::Named(NamedKey::ArrowUp) => {
                event.prevent_default();
                event.stop_propagation();
                self.slash_selected.set(if sel == 0 {
                    matches.len() - 1
                } else {
                    sel - 1
                });
                true
            }
            Key::Named(NamedKey::Enter | NamedKey::Tab) => {
                event.prevent_default();
                event.stop_propagation();
                let item = matches[sel];
                self.execute_slash(item.kind);
                true
            }
            _ => false,
        }
    }

    pub fn execute_slash(mut self, kind: SlashKind) {
        let Some(state) = self.slash_menu.take() else {
            return;
        };
        let end = state.slash_source + 1 + state.query.len();
        let md = self.contents.read().clone();
        if end > md.len() || state.slash_source > end {
            return;
        }
        // Remove the `/query` trigger text first so the line is clean.
        report(
            self.note.write().replace(state.slash_source..end, ""),
            self.message,
        );
        self.open(self.field_at(state.slash_source), [state.slash_source; 2]);
        let field = self.field_at(state.slash_source);
        match kind {
            SlashKind::H1 => {
                self.set_heading_level(field, 1);
            }
            SlashKind::H2 => {
                self.set_heading_level(field, 2);
            }
            SlashKind::H3 => {
                self.set_heading_level(field, 3);
            }
            SlashKind::Bullet => self.toggle_line_prefix("- "),
            SlashKind::Task => {
                self.toggle_line_task(field);
            }
            SlashKind::Quote => self.toggle_line_prefix("> "),
            SlashKind::Callout(kind) => self.insert_callout(kind),
            SlashKind::CodeBlock => self.insert_code_block(),
            SlashKind::Table => self.insert_table(false),
            SlashKind::Rule => self.insert_rule(),
            SlashKind::Image => self.insert_image(),
        }
    }
}

#[component]
pub fn SlashMenu(writing: Writing) -> Element {
    let Some(state) = (writing.slash_menu)() else {
        return rsx! {};
    };
    let matches = matching_items(&state.query);
    if matches.is_empty() {
        return rsx! {};
    }
    let active_idx = (writing.slash_selected)().min(matches.len() - 1);
    rsx! {
        crate::ui::Menu {
            id: "slash-menu",
            class: "slash-menu",
            role: "listbox",
            label: "Insert block",
            style: format!("left:{:.0}px; top:{:.0}px;", state.x, state.y),
            for (i, item) in matches.into_iter().enumerate() {
                crate::ui::MenuItem {
                    key: "{item.id}",
                    id: item.id.to_string(),
                    label: item.title.to_string(),
                    icon: item.icon,
                    badge: item.glyph.map(String::from),
                    hint: item.hint.to_string(),
                    option: true,
                    active: i == active_idx,
                    onhover: move |_| {
                        let mut w = writing;
                        if *w.slash_selected.peek() != i {
                            w.slash_selected.set(i);
                        }
                    },
                    onclick: move |_| writing.execute_slash(item.kind),
                }
            }
        }
    }
}
