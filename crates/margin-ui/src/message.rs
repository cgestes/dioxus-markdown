//! One turn of a conversation: the topic chat, a transcript, a comment thread.
use super::{Avatar, Icon};
use crate::prelude::*;

/// Where a message is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MessageStyle {
    /// The chat panel: your words in a bubble, the agent's as text.
    #[default]
    Bubble,
    /// A transcript: each turn on its own card.
    Card,
    /// A comment thread: an avatar and the author above the text.
    Thread,
}

/// A message: who wrote it, its body, and how far along it is. `body` is
/// plain text. A finished message shows `rich` in its place when the caller
/// gives one: the body as the caller draws it (the editor crate's
/// `MarkdownView`, say). While `pending` the plain text is shown (it is
/// still arriving) or "Thinking…"; `failed` shows it as an error. `tools`
/// lists what the agent ran. Children go under the body (a proposed edit,
/// say).
#[component]
pub fn Message(
    author: String,
    body: String,
    #[props(default)] ai: bool,
    #[props(default)] pending: bool,
    #[props(default)] failed: bool,
    #[props(default)] tools: Vec<String>,
    #[props(default)] at: Option<String>,
    #[props(default)] style: MessageStyle,
    #[props(default)] rich: Option<Element>,
    #[props(default)] children: Element,
) -> Element {
    let tools = match (tools.is_empty(), pending) {
        (true, _) => None,
        (false, true) => Some(format!("{}…", tools.join(", "))),
        (false, false) => Some(format!("used {}", tools.join(", "))),
    };
    let text_color = if pending {
        "text-faint"
    } else if failed {
        "text-alert"
    } else if style == MessageStyle::Bubble && !ai {
        "text-accent-deep"
    } else {
        "text-ink"
    };
    let text_size = if style == MessageStyle::Thread {
        "text-[12px]"
    } else {
        "text-[13px]"
    };
    let text = match rich {
        Some(rich) if !pending && !failed => rich,
        _ => {
            let content = if pending && body.is_empty() { "Thinking…" } else { body.as_str() };
            rsx! {
                div { class: "font-sans {text_size} leading-[1.45] whitespace-pre-wrap [overflow-wrap:anywhere] {text_color}",
                    "{content}"
                }
            }
        }
    };
    let wrap_cls = match (style, ai) {
        (MessageStyle::Bubble, false) => "self-end max-w-[88%] py-2 px-[11px] rounded-[12px_12px_3px_12px] bg-pale",
        (MessageStyle::Bubble, true) => "max-w-[96%]",
        (MessageStyle::Card, false) => "ml-12 py-2.5 px-[14px] border border-solid border-line rounded-lg bg-pale",
        (MessageStyle::Card, true) => "py-2.5 px-[14px] border border-solid border-line rounded-lg bg-surface",
        (MessageStyle::Thread, _) => "[&_.ui-md]:text-[12px]",
    };
    let author_cls = match (style, ai) {
        (MessageStyle::Thread, _) => "text-ink font-sans text-[11px] leading-none font-semibold tracking-normal normal-case",
        (MessageStyle::Card, false) => "text-accent-deep font-sans text-[10.5px] font-semibold tracking-[0.06em] uppercase",
        _ => "text-accent font-sans text-[10.5px] font-semibold tracking-[0.06em] uppercase",
    };
    let initial = author.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
    rsx! {
        div { class: "flex flex-col gap-[3px] min-w-0 text-[13px] {wrap_cls}",
            if style == MessageStyle::Bubble && !ai {
                {text}
            } else {
                div { class: "flex items-center gap-1.5 [&>.i]:w-3 [&>.i]:h-3 [&>.i]:text-accent",
                    if style == MessageStyle::Thread { Avatar { ai, initial } }
                    else if ai { {Icon::Sparkle.svg("i")} }
                    span { class: "{author_cls}", "{author}" }
                    if let Some(at) = at { span { class: "text-faint font-mono text-[10.5px] font-normal", "{at}" } }
                    if let Some(t) = tools { span { class: "text-faint font-mono text-[10.5px] font-normal", "{t}" } }
                }
                {text}
            }
            {children}
        }
    }
}
