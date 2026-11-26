//! Containers, list rows and the small marks inside them.
use super::{Button, Hue, Icon, Tone, Variant, icon};
use crate::prelude::*;

/// A surface that groups one kind of content. `onclick` makes the whole
/// card a target (a comment thread becomes the active one).
#[component]
pub fn Card(
    #[props(default)] class: String,
    #[props(default)] id: Option<String>,
    #[props(default)] onclick: Option<EventHandler<()>>,
    children: Element,
) -> Element {
    rsx! {
        section { id, class: "ui-card {class}", class: if onclick.is_some() { "action" },
            onclick: move |_| if let Some(f) = onclick { f.call(()) },
            {children}
        }
    }
}

/// A card's heading: icon, small caps title, then a quiet note or an action
/// at the end ("All 3 →").
#[component]
pub fn CardTitle(
    text: String,
    #[props(default)] icon: Option<Icon>,
    #[props(default)] note: Option<String>,
    #[props(default)] action: Option<String>,
    #[props(default)] onaction: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        h3 { class: "ui-card-title",
            if let Some(i) = icon { {super::icon(i)} }
            span { class: "ui-card-title-text", "{text}" }
            if let Some(note) = note { span { class: "ui-card-note", "{note}" } }
            if let (Some(label), Some(act)) = (action, onaction) {
                Button { label, variant: Variant::Link, class: "ui-card-action", onclick: move |_| act.call(()) }
            }
        }
    }
}

/// Rows stacked with rules between them.
#[component]
pub fn Rows(children: Element) -> Element {
    rsx! { div { class: "ui-rows", {children} } }
}

/// A list item: something at the start (a date, a glyph, a tile), a title,
/// an optional one-line snippet, a line of small facts (`sub`), and a quiet
/// value at the end. With `onclick` the whole row is a button. Children go
/// under the text (an expanded conversation, say).
#[component]
pub fn Row(
    title: String,
    #[props(default)] leading: Option<Element>,
    #[props(default)] snippet: Option<String>,
    #[props(default)] sub: Option<Element>,
    #[props(default)] trailing: Option<String>,
    #[props(default)] onclick: Option<EventHandler<()>>,
    #[props(default)] dot: bool,
    #[props(default)] children: Element,
) -> Element {
    let body = rsx! {
        {leading}
        div { class: "ui-row-body",
            div { class: "ui-row-title", "{title}" if dot { span { class: "ui-dot" } } }
            if let Some(s) = snippet { div { class: "ui-row-snippet", "{s}" } }
            if let Some(sub) = sub { div { class: "ui-row-sub", {sub} } }
            {children}
        }
        if let Some(t) = trailing { span { class: "ui-row-meta", "{t}" } }
    };
    match onclick {
        Some(f) => rsx! { button { class: "ui-row action", onclick: move |_| f.call(()), {body} } },
        None => rsx! { div { class: "ui-row", {body} } },
    }
}

/// An entry of a side navigation: a mark, a label with an optional hint
/// under it, then a count or an unread dot. `selected` marks where you are;
/// `indent` nests it under the entry above.
#[component]
pub fn NavItem(
    label: String,
    #[props(default)] hint: String,
    #[props(default)] leading: Option<Element>,
    #[props(default)] count: Option<(usize, Tone)>,
    #[props(default)] dot: bool,
    #[props(default)] selected: bool,
    #[props(default)] indent: bool,
    onclick: EventHandler<()>,
) -> Element {
    rsx! {
        button { class: "ui-nav", class: if selected { "on" }, class: if indent { "indent" },
            aria_current: selected.then_some("page"),
            onclick: move |_| onclick.call(()),
            {leading}
            span { class: "ui-nav-label", "{label}" if !hint.is_empty() { small { class: "ui-nav-hint", "{hint}" } } }
            if let Some((n, tone)) = count { if n > 0 { Badge { text: n.to_string(), tone } } }
            if dot { span { class: "ui-dot" } }
        }
    }
}

/// One fact on a row's `sub` line: an optional icon and some text.
#[component]
pub fn SubItem(text: String, #[props(default)] icon: Option<Icon>) -> Element {
    rsx! { span { class: "ui-sub-item", if let Some(i) = icon { {super::icon(i)} } "{text}" } }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TileSize {
    Small,
    #[default]
    Medium,
    Large,
}

/// An icon on a tinted square: a topic's mark.
#[component]
pub fn Tile(icon: Icon, hue: Hue, #[props(default)] size: TileSize) -> Element {
    let size = match size {
        TileSize::Small => "small",
        TileSize::Medium => "",
        TileSize::Large => "large",
    };
    rsx! { span { class: "ui-tile {size} {hue.class()}", {super::icon(icon)} } }
}

/// A short label: a count, a kind (`PDF`), an id (`Q1`).
#[component]
pub fn Badge(text: String, #[props(default)] tone: Tone) -> Element {
    rsx! { span { class: "ui-badge {tone.class()}", "{text}" } }
}

/// A message across the top of a place: an error, a note.
#[component]
pub fn Banner(text: String, #[props(default)] tone: Tone) -> Element {
    rsx! { div { class: "ui-banner {tone.class()}", role: if tone == Tone::Alert { "alert" } else { "status" }, "{text}" } }
}

/// A calendar chip: weekday or month above the day (`THU` / `26`).
#[component]
pub fn DateBox(top: String, day: String) -> Element {
    rsx! { span { class: "ui-date", small { "{top}" } b { "{day}" } } }
}

/// Where a conversation comes from, as the design system draws it. The app
/// maps its own idea of a source onto one of these.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Channel {
    Telegram,
    Slack,
    Chat,
    #[default]
    Other,
}

impl Channel {
    fn class(self) -> &'static str {
        match self {
            Self::Telegram => "tg",
            Self::Slack => "sl",
            Self::Chat => "ch",
            Self::Other => "ot",
        }
    }
}

/// Where a conversation comes from, as a round glyph.
#[component]
pub fn ChannelGlyph(channel: Channel) -> Element {
    rsx! { span { class: "ui-glyph {channel.class()}", {icon(Icon::for_channel(channel))} } }
}

/// Who wrote a message: the agent's sparkle, or a person's initial.
#[component]
pub fn Avatar(#[props(default)] ai: bool, #[props(default)] initial: String) -> Element {
    rsx! {
        span { class: "ui-avatar", class: if ai { "ai" },
            if ai { {Icon::Sparkle.svg("i")} } else { "{initial}" }
        }
    }
}
