//! Searchable combobox built from Margin's `Popover`, `MenuItem` and `Icon`
//! primitives, without browser JS dependencies so it runs natively on Blitz
//! as well as Dioxus Web.
use super::{Icon, MenuItem, Popover, icon};
use crate::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use blitz_dom::ScrollBehavior;
#[cfg(not(target_arch = "wasm32"))]
use dioxus_html::geometry::WheelDelta;
#[cfg(not(target_arch = "wasm32"))]
use dioxus_native::NodeHandle;

#[cfg(not(target_arch = "wasm32"))]
const MENU_WIDTH: f64 = 214.0;
#[cfg(not(target_arch = "wasm32"))]
const MAX_LIST_HEIGHT: f64 = 210.0;
#[cfg(not(target_arch = "wasm32"))]
const MIN_LIST_HEIGHT: f64 = 60.0;
#[cfg(not(target_arch = "wasm32"))]
const SEARCH_CHROME_HEIGHT: f64 = 42.0;
#[cfg(not(target_arch = "wasm32"))]
const VIEWPORT_PAD: f64 = 8.0;
#[cfg(not(target_arch = "wasm32"))]
const TRIGGER_GAP: f64 = 4.0;
const ROW_HEIGHT: f64 = 26.0;
const ROW_STEP: f64 = 27.0;

/// A mounted element: Blitz's node natively, Dioxus' handle in a browser.
#[cfg(not(target_arch = "wasm32"))]
type DomHandle = NodeHandle;
#[cfg(target_arch = "wasm32")]
type DomHandle = std::rc::Rc<MountedData>;

/// Where a list scrolled to `current`, `height` tall, must scroll so that
/// option `idx` shows; `None` when it already does.
fn scroll_target(current: f64, height: f64, idx: usize) -> Option<f64> {
    if height <= 0.0 {
        return None;
    }
    let row_top = idx as f64 * ROW_STEP;
    let row_bottom = row_top + ROW_HEIGHT;
    let next = if row_top < current {
        row_top
    } else if row_bottom > current + height {
        row_bottom - height
    } else {
        current
    };
    ((next - current).abs() > 0.5).then_some(next)
}

struct PopoverPlacement {
    menu_style: String,
    list_style: String,
}

#[cfg(not(target_arch = "wasm32"))]
fn compute_placement(trigger: &DomHandle) -> Option<PopoverPlacement> {
    let doc = trigger.doc();
    let rect = doc.get_client_bounding_rect(trigger.node_id())?;
    let (vw_u, vh_u) = doc.viewport().logical_size();
    let vw = f64::from(vw_u);
    let vh = f64::from(vh_u);
    if vw <= 0.0 || vh <= 0.0 {
        return None;
    }
    let left = (rect.x + rect.width - MENU_WIDTH).clamp(
        VIEWPORT_PAD,
        (vw - MENU_WIDTH - VIEWPORT_PAD).max(VIEWPORT_PAD),
    );
    let below_top = rect.y + rect.height + TRIGGER_GAP;
    let below_space = (vh - below_top - VIEWPORT_PAD).max(0.0);
    let above_space = (rect.y - TRIGGER_GAP - VIEWPORT_PAD).max(0.0);
    let desired_h = SEARCH_CHROME_HEIGHT + MAX_LIST_HEIGHT;

    let (menu_style, max_list_h) = if below_space >= desired_h || below_space >= above_space {
        let top = below_top.clamp(VIEWPORT_PAD, (vh - VIEWPORT_PAD).max(VIEWPORT_PAD));
        let max_h = (below_space - SEARCH_CHROME_HEIGHT).clamp(MIN_LIST_HEIGHT, MAX_LIST_HEIGHT);
        (format!("left:{left:.1}px; top:{top:.1}px;"), max_h)
    } else {
        let bottom = (vh - (rect.y - TRIGGER_GAP))
            .clamp(VIEWPORT_PAD, (vh - VIEWPORT_PAD).max(VIEWPORT_PAD));
        let max_h = (above_space - SEARCH_CHROME_HEIGHT).clamp(MIN_LIST_HEIGHT, MAX_LIST_HEIGHT);
        (format!("left:{left:.1}px; bottom:{bottom:.1}px;"), max_h)
    };
    Some(PopoverPlacement {
        menu_style,
        list_style: format!("max-height:{max_list_h:.0}px;"),
    })
}

#[cfg(target_arch = "wasm32")]
fn compute_placement(_trigger: &DomHandle) -> Option<PopoverPlacement> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
fn reset_list_scroll(list_handle: Signal<Option<DomHandle>>) {
    if let Some(handle) = list_handle.peek().as_ref() {
        let mut doc = handle.doc_mut();
        doc.scroll_to(handle.node_id(), 0.0, 0.0, ScrollBehavior::Instant);
        doc.shell_provider.request_redraw();
    }
}

#[cfg(target_arch = "wasm32")]
fn reset_list_scroll(list_handle: Signal<Option<DomHandle>>) {
    if let Some(list) = list_handle.peek().clone() {
        spawn(async move {
            let top = dioxus::html::geometry::PixelsVector2D::new(0.0, 0.0);
            let _ = list.scroll(top, dioxus::html::ScrollBehavior::Instant).await;
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn scroll_option_into_view(list_handle: Signal<Option<DomHandle>>, idx: usize) {
    let binding = list_handle.peek();
    let Some(handle) = binding.as_ref() else {
        return;
    };
    let mut doc = handle.doc_mut();
    let id = handle.node_id();
    let Some(node) = doc.get_node(id) else {
        return;
    };
    let height = f64::from(node.final_layout().size.height);
    if let Some(next) = scroll_target(node.scroll_offset().y, height, idx) {
        doc.scroll_to(id, 0.0, next, ScrollBehavior::Instant);
        doc.shell_provider.request_redraw();
    }
}

#[cfg(target_arch = "wasm32")]
fn scroll_option_into_view(list_handle: Signal<Option<DomHandle>>, idx: usize) {
    let Some(list) = list_handle.peek().clone() else {
        return;
    };
    spawn(async move {
        let (Ok(offset), Ok(rect)) = (list.get_scroll_offset().await, list.get_client_rect().await) else {
            return;
        };
        if let Some(next) = scroll_target(offset.y, rect.size.height, idx) {
            let to = dioxus::html::geometry::PixelsVector2D::new(0.0, next);
            let _ = list.scroll(to, dioxus::html::ScrollBehavior::Instant).await;
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn capture_handle(e: &MountedEvent) -> Option<DomHandle> {
    e.downcast::<NodeHandle>().cloned()
}

#[cfg(target_arch = "wasm32")]
fn capture_handle(e: &MountedEvent) -> Option<DomHandle> {
    Some(e.data())
}

#[cfg(not(target_arch = "wasm32"))]
fn focus_mounted_input(e: &MountedEvent) {
    if let Some(handle) = e.downcast::<NodeHandle>() {
        handle.doc_mut().set_focus_to(handle.node_id());
    }
}

#[cfg(target_arch = "wasm32")]
fn focus_mounted_input(e: &MountedEvent) {
    let input = e.data();
    spawn(async move {
        let _ = input.set_focus(true).await;
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn handle_list_wheel(e: WheelEvent, list_handle: Signal<Option<DomHandle>>) {
    e.prevent_default();
    e.stop_propagation();
    let dy = match e.delta() {
        WheelDelta::Pixels(v) => v.y,
        WheelDelta::Lines(v) => v.y * 20.0,
        WheelDelta::Pages(v) => v.y * MAX_LIST_HEIGHT,
    };
    if dy != 0.0
        && let Some(handle) = list_handle.peek().as_ref()
    {
        let mut doc = handle.doc_mut();
        doc.scroll_by(handle.node_id(), 0.0, -dy, ScrollBehavior::Instant);
        doc.shell_provider.request_redraw();
    }
}

#[cfg(target_arch = "wasm32")]
fn handle_list_wheel(e: WheelEvent, _list_handle: Signal<Option<DomHandle>>) {
    e.stop_propagation();
}

/// One selectable option in a [`Combobox`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComboboxItem {
    /// Machine value returned to `onselect` (may be empty, e.g. plain text).
    pub value: String,
    /// Human-readable label shown in the list.
    pub label: String,
    /// Optional monospace hint shown at the trailing edge of the row.
    pub hint: Option<String>,
    /// Extra search terms (aliases, extensions) matched when filtering.
    pub keywords: Vec<String>,
    /// Optional DOM id for the rendered option button.
    pub id: Option<String>,
}

impl ComboboxItem {
    fn matches_exact(&self, q: &str) -> bool {
        self.value.eq_ignore_ascii_case(q)
            || self.label.eq_ignore_ascii_case(q)
            || self.keywords.iter().any(|k| k.eq_ignore_ascii_case(q))
    }

    fn matches_selected(&self, selected: &str) -> bool {
        self.value.eq_ignore_ascii_case(selected)
            || self
                .keywords
                .iter()
                .any(|k| k.eq_ignore_ascii_case(selected))
    }

    fn match_rank(&self, lower_q: &str) -> Option<u8> {
        if self.matches_exact(lower_q) {
            return Some(0);
        }
        let label = self.label.to_ascii_lowercase();
        let value = self.value.to_ascii_lowercase();
        if value.starts_with(lower_q)
            || label.starts_with(lower_q)
            || self
                .keywords
                .iter()
                .any(|k| k.to_ascii_lowercase().starts_with(lower_q))
        {
            return Some(1);
        }
        if label.contains(lower_q)
            || value.contains(lower_q)
            || self
                .keywords
                .iter()
                .any(|k| k.to_ascii_lowercase().contains(lower_q))
        {
            return Some(2);
        }
        None
    }
}

fn filter_items<'a>(items: &'a [ComboboxItem], query: &str) -> Vec<&'a ComboboxItem> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return items.iter().collect();
    }
    let mut ranked: Vec<(u8, usize, &'a ComboboxItem)> = items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| item.match_rank(&q).map(|rank| (rank, idx, item)))
        .collect();
    ranked.sort_by_key(|(rank, idx, _)| (*rank, *idx));
    ranked.into_iter().map(|(_, _, item)| item).collect()
}

/// A trigger button paired with a searchable listbox popover.
///
/// The caller controls `open` (so a surface can enforce one open menu at a
/// time) and passes the selectable `items`. Filtering, keyboard navigation
/// (`ArrowUp`, `ArrowDown`, `Enter`, `Escape`) and search-input focus live
/// inside this component.
#[component]
pub fn Combobox(
    label: String,
    value_label: String,
    selected: String,
    items: Vec<ComboboxItem>,
    open: bool,
    ontoggle: EventHandler<bool>,
    onselect: EventHandler<String>,
    #[props(default)] onescape: Option<EventHandler<()>>,
    #[props(default = "Search…".to_owned())] placeholder: String,
    #[props(default = "No match".to_owned())] empty_label: String,
    #[props(default)] allow_custom: bool,
    #[props(default = 40)] max_visible: usize,
    #[props(default)] tip: Option<String>,
    #[props(default)] trigger_id: Option<String>,
    #[props(default)] menu_id: Option<String>,
    #[props(default)] search_id: Option<String>,
    #[props(default)] placeholder_id: Option<String>,
    #[props(default)] class: String,
    #[props(default)] menu_class: String,
) -> Element {
    let mut query = use_signal(String::new);
    let mut active_idx = use_signal(|| 0usize);
    let mut trigger_handle: Signal<Option<DomHandle>> = use_signal(|| None);
    let mut list_handle: Signal<Option<DomHandle>> = use_signal(|| None);

    let q = query.read().clone();
    let trimmed_q = q.trim();
    let mut options: Vec<(ComboboxItem, bool)> = Vec::new();
    let placement = if open {
        let matches = filter_items(&items, &q);
        let exact_match = matches.iter().any(|o| o.matches_exact(trimmed_q));
        options.extend(
            matches
                .into_iter()
                .take(max_visible.max(1))
                .map(|o| (o.clone(), false)),
        );
        if allow_custom && !trimmed_q.is_empty() && !exact_match {
            options.push((
                ComboboxItem {
                    value: trimmed_q.to_owned(),
                    label: format!("Use \"{trimmed_q}\""),
                    hint: None,
                    keywords: Vec::new(),
                    id: Some("ui-combobox-pick-custom".to_owned()),
                },
                true,
            ));
        }
        trigger_handle.read().as_ref().and_then(compute_placement)
    } else {
        None
    };

    let total_options = options.len();
    let clamped_active = if total_options == 0 {
        0
    } else {
        (*active_idx.read()).min(total_options - 1)
    };
    let option_values: Vec<String> = options.iter().map(|(o, _)| o.value.clone()).collect();
    let input_id = search_id.unwrap_or_else(|| "ui-combobox-search".to_owned());
    let list_id = menu_id
        .as_ref()
        .map_or_else(|| "ui-combobox-list".to_owned(), |m| format!("{m}-list"));
    let selected_trimmed = selected.trim().to_owned();
    let menu_class_full = if placement.is_some() {
        format!("fixed z-30 w-[214px] p-1 gap-1 {menu_class}")
    } else {
        format!("absolute right-0 top-[26px] z-30 w-[214px] p-1 gap-1 {menu_class}")
    };
    let menu_style = placement.as_ref().map(|p| p.menu_style.clone());
    let list_style = placement.as_ref().map(|p| p.list_style.clone());

    let mut reset_search = move || {
        query.set(String::new());
        active_idx.set(0);
        reset_list_scroll(list_handle);
    };

    let trigger_state = if open {
        "bg-pale text-accent-deep"
    } else {
        "bg-transparent text-faint hover:bg-pale hover:text-accent-deep"
    };

    rsx! {
        div { class: "relative inline-flex items-center {class}",
            button {
                id: trigger_id,
                class: "inline-flex items-center gap-[3px] h-[22px] m-0 px-1.5 border border-solid border-transparent rounded-sm type-mono-ui cursor-pointer select-none whitespace-nowrap focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-1 [&_.i]:w-[11px] [&_.i]:h-[11px] [&_.i]:opacity-75 {trigger_state}",
                aria_label: format!("{label}: {value_label}"),
                aria_haspopup: "listbox",
                aria_expanded: if open { "true" } else { "false" },
                title: tip,
                onmounted: move |e| {
                    trigger_handle.set(capture_handle(&e));
                },
                onpointerdown: move |e| {
                    e.prevent_default();
                    e.stop_propagation();
                },
                onclick: move |e| {
                    e.stop_propagation();
                    reset_search();
                    ontoggle.call(!open);
                },
                span { "{value_label}" }
                {icon(Icon::ChevronDown)}
            }
            if open {
                Popover {
                    id: menu_id,
                    class: menu_class_full,
                    style: menu_style,
                    label: label.clone(),
                    keep_focus: false,
                    onwheel: move |e: WheelEvent| {
                        e.prevent_default();
                        e.stop_propagation();
                    },
                    div { class: "flex items-center gap-1.5 h-7 px-2 border border-solid border-line rounded-sm bg-page type-hint focus-within:border-accent [&_.i]:w-[13px] [&_.i]:h-[13px] [&_.i]:text-faint [&_.i]:flex-none",
                        {icon(Icon::Search)}
                        span { class: "ui-field relative flex flex-auto min-w-0 items-center h-[26px]",
                            input {
                                id: input_id,
                                class: "flex-auto min-w-0 w-full h-[26px] leading-[26px] m-0 p-0 border-0 outline-none bg-transparent text-ink [font:inherit] select-text",
                                role: "combobox",
                                aria_label: format!("Filter {label}"),
                                aria_autocomplete: "list",
                                aria_controls: list_id.clone(),
                                value: "{q}",
                                onmounted: move |e| focus_mounted_input(&e),
                                oninput: move |e| {
                                    e.stop_propagation();
                                    query.set(e.value());
                                    active_idx.set(0);
                                    reset_list_scroll(list_handle);
                                },
                                onkeydown: move |e| {
                                    e.stop_propagation();
                                    if e.is_composing() {
                                        return;
                                    }
                                    match e.key() {
                                        Key::Named(NamedKey::ArrowDown) => {
                                            e.prevent_default();
                                            if total_options > 0 {
                                                let next = (clamped_active + 1) % total_options;
                                                active_idx.set(next);
                                                scroll_option_into_view(list_handle, next);
                                            }
                                        }
                                        Key::Named(NamedKey::ArrowUp) => {
                                            e.prevent_default();
                                            if total_options > 0 {
                                                let next = (clamped_active + total_options - 1) % total_options;
                                                active_idx.set(next);
                                                scroll_option_into_view(list_handle, next);
                                            }
                                        }
                                        Key::Named(NamedKey::Enter) => {
                                            e.prevent_default();
                                            if let Some(val) = option_values.get(clamped_active).cloned() {
                                                reset_search();
                                                onselect.call(val);
                                            }
                                        }
                                        Key::Named(NamedKey::Escape) => {
                                            e.prevent_default();
                                            reset_search();
                                            if let Some(esc) = onescape {
                                                esc.call(());
                                            } else {
                                                ontoggle.call(false);
                                            }
                                        }
                                        _ => {}
                                    }
                                },
                            }
                            if q.is_empty() {
                                span { id: placeholder_id, class: "ui-field-ph absolute left-0 top-0 bottom-0 flex items-center leading-[26px] whitespace-nowrap text-faint [font:inherit] pointer-events-none", "{placeholder}" }
                            }
                        }
                    }
                    div {
                        id: list_id,
                        class: "flex flex-col gap-[1px] max-h-[210px] overflow-x-hidden overflow-y-auto [&_.ui-menu-item]:h-[26px] [&_.ui-menu-item]:min-w-0 [&_.ui-menu-item]:overflow-hidden [&_.ui-menu-item]:type-hint [&_.ui-menu-item:hover:not(.active)]:bg-transparent [&_.ui-menu-item:hover:not(.active)]:text-ink [&_.ui-menu-item:hover:not(.active)_.tool-icon]:text-muted [&_.ui-menu-item:hover:not(.active)_.ui-badge-glyph]:text-muted [&_.ui-menu-item_.tool-caption]:min-w-0 [&_.ui-menu-item_.tool-caption]:overflow-hidden [&_.ui-menu-item_.tool-caption]:text-ellipsis [&_.ui-btn-hint]:flex-none",
                        style: list_style,
                        role: "listbox",
                        aria_label: label.clone(),
                        onmounted: move |e| {
                            list_handle.set(capture_handle(&e));
                        },
                        onwheel: move |e| handle_list_wheel(e, list_handle),
                        for (idx, (opt, is_custom)) in options.into_iter().enumerate() {
                            {
                                let val = opt.value.clone();
                                let is_selected = !is_custom && opt.matches_selected(&selected_trimmed);
                                let key = if is_custom {
                                    format!("custom-{val}")
                                } else {
                                    format!("{}-{val}", opt.label)
                                };
                                rsx! {
                                    MenuItem {
                                        key: "{key}",
                                        id: opt.id,
                                        label: opt.label,
                                        hint: opt.hint,
                                        icon: is_selected.then_some(Icon::Check),
                                        active: idx == clamped_active,
                                        selected: is_selected,
                                        option: true,
                                        onhover: move |_| {
                                            if *active_idx.peek() != idx {
                                                active_idx.set(idx);
                                            }
                                        },
                                        onclick: move |_| {
                                            reset_search();
                                            onselect.call(val.clone());
                                        },
                                    }
                                }
                            }
                        }
                        if total_options == 0 {
                            div { class: "p-2 text-faint type-hint text-center", "{empty_label}" }
                        }
                    }
                }
            }
        }
    }
}
