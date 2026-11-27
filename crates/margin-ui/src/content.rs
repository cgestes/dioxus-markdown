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
    let action_cls = if onclick.is_some() {
        "cursor-pointer hover:border-line-strong"
    } else {
        ""
    };
    rsx! {
        section {
            id,
            class: "ui-card min-w-0 py-[14px] px-4 border border-solid border-line rounded-lg bg-surface [&>.ui-row:first-child]:pt-0.5 [&>.ui-row:first-child]:border-t-0 {action_cls} {class}",
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
        h3 { class: "flex items-center gap-[7px] m-0 mb-2.5 text-faint type-label uppercase [&_.i]:w-[14px] [&_.i]:h-[14px]",
            if let Some(i) = icon { {super::icon(i)} }
            span { "{text}" }
            if let Some(note) = note {
                span { class: "ml-auto text-faint font-mono text-[10.5px] font-normal tracking-normal normal-case", "{note}" }
            }
            if let (Some(label), Some(act)) = (action, onaction) {
                Button { label, variant: Variant::Link, class: "ml-auto", onclick: move |_| act.call(()) }
            }
        }
    }
}

/// Rows stacked with rules between them.
#[component]
pub fn Rows(children: Element) -> Element {
    rsx! { div { class: "flex flex-col [&>.ui-row:first-child]:pt-0.5 [&>.ui-row:first-child]:border-t-0", {children} } }
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
    let row_base = "ui-row flex items-start justify-start gap-[11px] w-full h-auto m-0 py-2 px-0 border-0 border-t border-solid border-line rounded-none bg-transparent text-ink font-sans text-[13px] leading-[1.45] font-normal text-left";
    let body = rsx! {
        {leading}
        div { class: "flex flex-col flex-auto min-w-0",
            div { class: "ui-row-title flex items-center gap-1.5 font-semibold group-hover:text-accent-deep",
                "{title}"
                if dot { span { class: "flex-none w-[7px] h-[7px] rounded-full bg-accent" } }
            }
            if let Some(s) = snippet {
                div { class: "overflow-hidden text-muted text-[12.5px] whitespace-nowrap text-ellipsis", "{s}" }
            }
            if let Some(sub) = sub {
                div { class: "flex flex-wrap gap-y-1 gap-x-2.5 mt-0.5 text-faint font-mono text-[11px] font-normal", {sub} }
            }
            {children}
        }
        if let Some(t) = trailing {
            span { class: "flex-none text-faint font-mono text-[11px] font-normal whitespace-nowrap", "{t}" }
        }
    };
    match onclick {
        Some(f) => rsx! {
            button {
                class: "{row_base} group cursor-pointer hover:bg-transparent hover:text-ink",
                onclick: move |_| f.call(()),
                {body}
            }
        },
        None => rsx! { div { class: "{row_base}", {body} } },
    }
}

/// An entry of a side navigation: a mark, a label with an optional hint
/// under it, then a count or an unread dot. `selected` marks where you are;
/// `indent` nests it under the entry above. The hint keeps to one line and
/// is cut at the entry's edge (the renderer draws no ellipsis, so callers
/// shorten it), and an entry never shrinks below its text: a long list
/// scrolls rather than stacking entries on top of each other.
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
    let state_cls = if selected {
        "border-line bg-surface hover:border-line hover:bg-surface"
    } else {
        "border-transparent bg-transparent hover:bg-pale"
    };
    let indent_cls = if indent {
        "pl-6 [&_.ui-tile]:basis-5 [&_.ui-tile]:w-5 [&_.ui-tile]:h-5 [&_.ui-tile]:rounded-[6px] [&_.ui-tile_.i]:w-[13px] [&_.ui-tile_.i]:h-[13px]"
    } else {
        ""
    };
    rsx! {
        button {
            class: "ui-nav box-border flex flex-none items-center justify-start gap-[9px] w-full h-auto min-h-8 m-0 py-[5px] px-2 border border-solid rounded-md text-ink hover:text-ink font-sans text-[13px] leading-[1.3] font-normal text-left cursor-pointer {state_cls} {indent_cls}",
            aria_current: selected.then_some("page"),
            onclick: move |_| onclick.call(()),
            {leading}
            span { class: "flex flex-col flex-auto min-w-0",
                "{label}"
                if !hint.is_empty() {
                    small { class: "overflow-hidden text-faint font-mono text-[10.5px] leading-[1.3] font-normal whitespace-nowrap", "{hint}" }
                }
            }
            if let Some((n, tone)) = count { if n > 0 { Badge { text: n.to_string(), tone } } }
            if dot { span { class: "flex-none w-[7px] h-[7px] rounded-full bg-accent" } }
        }
    }
}

/// One fact on a row's `sub` line: an optional icon and some text.
#[component]
pub fn SubItem(text: String, #[props(default)] icon: Option<Icon>) -> Element {
    rsx! {
        span { class: "flex items-center gap-1 [&_.i]:w-3 [&_.i]:h-3",
            if let Some(i) = icon { {super::icon(i)} }
            "{text}"
        }
    }
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
    let size_cls = match size {
        TileSize::Small => "basis-4 w-4 h-4 rounded-sm [&_.i]:w-[11px] [&_.i]:h-[11px]",
        TileSize::Medium => "basis-6 w-6 h-6 rounded-md [&_.i]:w-[15px] [&_.i]:h-[15px]",
        TileSize::Large => "basis-10 w-10 h-10 rounded-[11px] [&_.i]:w-[22px] [&_.i]:h-[22px]",
    };
    let hue_cls = match hue {
        Hue::Slate => "bg-hue-slate-wash text-hue-slate",
        Hue::Rose => "bg-hue-rose-wash text-hue-rose",
        Hue::Clay => "bg-hue-clay-wash text-hue-clay",
        Hue::Ochre => "bg-hue-ochre-wash text-hue-ochre",
        Hue::Plum => "bg-hue-plum-wash text-hue-plum",
        Hue::Pine => "bg-hue-pine-wash text-hue-pine",
    };
    rsx! {
        span { class: "ui-tile flex items-center justify-center shrink-0 grow-0 {size_cls} {hue_cls}",
            {super::icon(icon)}
        }
    }
}

/// A short label: a count, a kind (`PDF`), an id (`Q1`).
#[component]
pub fn Badge(text: String, #[props(default)] tone: Tone) -> Element {
    let tone_cls = match tone {
        Tone::Neutral => "bg-rail text-muted",
        Tone::Accent => "bg-pale text-accent-deep",
        Tone::Ok => "bg-pale text-ok",
        Tone::Info => "bg-info-wash text-info",
        Tone::Warn => "bg-warn-wash text-warn-ink",
        Tone::Alert => "bg-alert-wash text-alert",
    };
    rsx! {
        span { class: "flex-none py-[1px] px-1.5 rounded-sm font-mono text-[10.5px] leading-[1.5] font-semibold whitespace-nowrap {tone_cls}",
            "{text}"
        }
    }
}

/// A message across the top of a place: an error, a note.
#[component]
pub fn Banner(text: String, #[props(default)] tone: Tone) -> Element {
    let tone_cls = match tone {
        Tone::Alert => "border-alert-line bg-alert-wash text-alert",
        Tone::Warn => "border-warn-line bg-warn-wash text-warn-ink",
        Tone::Info => "border-info-wash bg-info-wash text-info",
        Tone::Neutral | Tone::Accent | Tone::Ok => "border-line bg-wash text-muted",
    };
    rsx! {
        div {
            class: "ui-banner py-2.5 px-3 border border-solid rounded-md font-sans text-[12.5px] leading-[1.45] font-normal {tone_cls}",
            role: if tone == Tone::Alert { "alert" } else { "status" },
            "{text}"
        }
    }
}

/// A calendar chip: weekday or month above the day (`THU` / `26`).
#[component]
pub fn DateBox(top: String, day: String) -> Element {
    rsx! {
        span { class: "flex flex-col items-center flex-none w-10 pt-[3px] pb-1 border border-solid border-line rounded-md leading-[1.1]",
            small { class: "text-alert font-sans text-[9.5px] font-semibold tracking-[0.08em] uppercase", "{top}" }
            b { class: "font-mono text-[16px] font-semibold", "{day}" }
        }
    }
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

/// Where a conversation comes from, as a round glyph.
#[component]
pub fn ChannelGlyph(channel: Channel) -> Element {
    let tone_cls = match channel {
        Channel::Telegram => "bg-telegram-wash text-telegram",
        Channel::Slack => "bg-slack-wash text-slack",
        Channel::Chat => "bg-chat-wash text-chat",
        Channel::Other => "bg-rail text-muted",
    };
    rsx! {
        span { class: "flex items-center justify-center flex-none w-[26px] h-[26px] rounded-full [&_.i]:w-[14px] [&_.i]:h-[14px] {tone_cls}",
            {icon(Icon::for_channel(channel))}
        }
    }
}

/// Who wrote a message: the agent's sparkle, or a person's initial.
#[component]
pub fn Avatar(#[props(default)] ai: bool, #[props(default)] initial: String) -> Element {
    let bg = if ai { "bg-accent" } else { "bg-muted" };
    rsx! {
        span { class: "flex items-center justify-center flex-none w-4 h-4 rounded-full text-surface font-sans text-[9px] leading-none font-semibold [&_.i]:w-2.5 [&_.i]:h-2.5 [&_.i]:text-surface {bg}",
            if ai { {Icon::Sparkle.svg("i")} } else { "{initial}" }
        }
    }
}
