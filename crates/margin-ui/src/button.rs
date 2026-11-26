//! Buttons and the controls built from them: chips, segmented switches, tabs.
use super::Icon;
use dioxus_native::prelude::*;

/// How a button looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Variant {
    /// A bordered surface button: secondary actions.
    #[default]
    Default,
    /// The one main action of a place: filled with the accent.
    Primary,
    /// No border until hovered: toolbars and quiet actions.
    Ghost,
    /// A square icon-only button.
    Icon,
    /// Text in the accent colour: "All →", "Open in editor →".
    Link,
    /// A rounded pill: filters and quick replies (see `Chip`).
    Chip,
    /// One option of a `Segmented` switch.
    Segment,
    /// No look of its own; the caller's class styles it (editor internals).
    Bare,
}

impl Variant {
    fn class(self) -> &'static str {
        match self {
            Self::Default => "ui-btn",
            Self::Primary => "ui-btn primary",
            Self::Ghost => "ui-btn ghost",
            Self::Icon => "ui-btn icon",
            Self::Link => "ui-btn link",
            Self::Chip => "ui-chip",
            Self::Segment => "ui-segment",
            Self::Bare => "",
        }
    }
}

/// A button that also activates from the keyboard (Enter on press, Space on
/// release), with an accessible name and tooltip.
///
/// `label` is the accessible name; it is also the visible text unless `text`
/// or an `icon` is given. `stop` keeps the click from reaching the parent.
#[component]
pub fn Button(
    label: String,
    #[props(default)] text: Option<String>,
    #[props(default)] icon: Option<Icon>,
    #[props(default)] variant: Variant,
    #[props(default)] class: String,
    #[props(default)] id: Option<String>,
    #[props(default)] tip: Option<String>,
    #[props(default)] selected: Option<bool>,
    #[props(default)] expanded: Option<bool>,
    #[props(default)] disabled: bool,
    #[props(default)] stop: bool,
    /// A short text mark before the caption when there is no icon (`H1`, `↑`).
    #[props(default)] badge: Option<String>,
    /// Trailing text in mono: a shortcut, a count.
    #[props(default)] hint: Option<String>,
    /// Destructive: alert colours on hover.
    #[props(default)] danger: bool,
    /// ARIA role, for menu items and list options.
    #[props(default)] role: Option<String>,
    /// Pointer entered (menus move their keyboard highlight with the mouse).
    #[props(default)] onhover: Option<EventHandler<()>>,
    /// Pressing does not take focus from the text being edited.
    #[props(default)] keep_focus: bool,
    /// `Some(false)` leaves the button out of the Tab order.
    #[props(default)] focusable: Option<bool>,
    onclick: EventHandler<()>,
) -> Element {
    let mut space_down = use_signal(|| false);
    let caption = text.or_else(|| (icon.is_none() && badge.is_none()).then(|| label.clone()));
    let title = tip.unwrap_or_else(|| label.clone());
    let option = role.as_deref() == Some("option");
    rsx! {
        button {
            id, disabled, aria_disabled: disabled.then_some("true"), aria_label: "{label}", title: "{title}", role,
            class: "{variant.class()} {class}", class: if selected == Some(true) { "selected" }, class: if danger { "danger" },
            aria_expanded: expanded.map(|s| if s { "true" } else { "false" }),
            aria_pressed: (!option).then_some(selected).flatten().map(|s| if s { "true" } else { "false" }),
            aria_selected: option.then(|| if selected == Some(true) { "true" } else { "false" }),
            tabindex: (focusable == Some(false)).then_some("-1"),
            onpointerdown: move |e| {
                if keep_focus {
                    e.prevent_default();
                }
            },
            onpointerenter: move |_| {
                if let Some(h) = onhover {
                    h.call(());
                }
            },
            onclick: move |e| {
                if stop {
                    e.stop_propagation();
                }
                onclick.call(());
            },
            onkeydown: move |event| {
                if event.is_composing() || event.modifiers().intersects(Modifiers::CONTROL | Modifiers::META | Modifiers::ALT) {
                    return;
                }
                match event.key() {
                    Key::Named(NamedKey::Enter) => {
                        event.prevent_default();
                        if !event.is_auto_repeating() {
                            onclick.call(());
                        }
                    }
                    Key::Character(s) if s == " " => {
                        event.prevent_default();
                        if !event.is_auto_repeating() {
                            space_down.set(true);
                        }
                    }
                    _ => {}
                }
            },
            onkeyup: move |event| {
                if matches!(event.key(), Key::Character(s) if s == " ") && *space_down.peek() {
                    event.prevent_default();
                    space_down.set(false);
                    onclick.call(());
                }
            },
            onblur: move |_| space_down.set(false),
            if let Some(icon) = icon { {icon.draw()} } else if let Some(b) = badge { span { class: "ui-badge-glyph", "{b}" } }
            if let Some(text) = caption { span { class: "tool-caption", "{text}" } }
            if let Some(h) = hint { span { class: "ui-btn-hint", "{h}" } }
        }
    }
}

/// A rounded toggle: a filter, a quick reply.
#[component]
pub fn Chip(
    label: String,
    #[props(default)] icon: Option<Icon>,
    #[props(default)] selected: bool,
    #[props(default)] id: Option<String>,
    #[props(default)] stop: bool,
    onclick: EventHandler<()>,
) -> Element {
    rsx! {
        Button { label: label.clone(), text: label, icon, variant: Variant::Chip, id, stop,
            selected: selected.then_some(true), onclick }
    }
}

/// One option of a `Segmented` switch.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub id: &'static str,
    pub label: &'static str,
    pub tip: Option<&'static str>,
    pub selected: bool,
}

/// Mutually exclusive modes side by side (Rich · Live · Split · Source).
#[component]
pub fn Segmented(label: &'static str, items: Vec<Segment>, onselect: EventHandler<usize>) -> Element {
    rsx! {
        div { class: "ui-segmented", role: "group", aria_label: label,
            for (i, s) in items.into_iter().enumerate() {
                Button { key: "{s.id}", id: s.id.to_string(), label: s.label.to_string(), tip: s.tip.map(String::from),
                    variant: Variant::Segment, selected: Some(s.selected), onclick: move |_| onselect.call(i) }
            }
        }
    }
}

/// One tab: its label and an optional count.
#[derive(Clone, Debug, PartialEq)]
pub struct TabItem {
    pub label: String,
    pub count: usize,
}

/// Views of one place, underlined when chosen.
#[component]
pub fn Tabs(items: Vec<TabItem>, selected: usize, onselect: EventHandler<usize>) -> Element {
    rsx! {
        div { class: "ui-tabs", role: "tablist",
            for (i, t) in items.into_iter().enumerate() {
                button { key: "{i}", class: "ui-tab", class: if i == selected { "on" }, role: "tab",
                    aria_selected: if i == selected { "true" } else { "false" },
                    onclick: move |_| onselect.call(i),
                    "{t.label}"
                    if t.count > 0 { span { class: "ui-tab-count", "{t.count}" } }
                }
            }
        }
    }
}
