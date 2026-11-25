//! One turn of a conversation: the topic chat, a transcript, a comment thread.
use super::{Avatar, Icon, MarkdownView};
use dioxus_native::prelude::*;

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

/// A message: who wrote it, its Markdown body, and how far along it is.
/// While `pending` the body is shown as plain text (it is still arriving)
/// or "Thinking…"; `failed` shows it as an error. `tools` lists what the
/// agent ran. Children go under the body (a proposed edit, say).
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
    #[props(default)] children: Element,
) -> Element {
    let tools = match (tools.is_empty(), pending) {
        (true, _) => None,
        (false, true) => Some(format!("{}…", tools.join(", "))),
        (false, false) => Some(format!("used {}", tools.join(", "))),
    };
    let compact = style != MessageStyle::Card;
    let text = if pending {
        rsx! { div { class: "ui-msg-text plain", if body.is_empty() { "Thinking…" } else { "{body}" } } }
    } else if failed {
        rsx! { div { class: "ui-msg-text", "{body}" } }
    } else {
        rsx! { MarkdownView { text: body.clone(), compact } }
    };
    let kind = match style {
        MessageStyle::Bubble => "bubble",
        MessageStyle::Card => "card",
        MessageStyle::Thread => "thread",
    };
    let initial = author.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
    rsx! {
        div { class: "ui-msg {kind}", class: if ai { "ai" } else { "you" }, class: if pending { "pending" }, class: if failed { "failed" },
            if style == MessageStyle::Bubble && !ai {
                {text}
            } else {
                div { class: "ui-msg-meta",
                    if style == MessageStyle::Thread { Avatar { ai, initial } }
                    else if ai { {Icon::Sparkle.svg("i")} }
                    span { class: "ui-msg-author", "{author}" }
                    if let Some(at) = at { span { class: "ui-msg-note", "{at}" } }
                    if let Some(t) = tools { span { class: "ui-msg-note", "{t}" } }
                }
                {text}
            }
            {children}
        }
    }
}
