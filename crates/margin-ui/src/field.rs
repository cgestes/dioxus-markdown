//! Text entry and checkboxes.
use super::{Button, Icon, Variant};
use crate::prelude::*;

/// A text field with a placeholder Blitz can show (an overlay while empty).
/// Keys stay inside the field, so the editor around it does not react.
#[component]
pub fn TextField(
    value: Signal<String>,
    placeholder: String,
    #[props(default)] multiline: bool,
    #[props(default)] id: Option<String>,
    #[props(default)] onenter: Option<EventHandler<()>>,
    #[props(default)] onescape: Option<EventHandler<()>>,
) -> Element {
    let mut value = value;
    let keys = move |e: KeyboardEvent| {
        e.stop_propagation();
        if e.is_composing() {
            return;
        }
        match e.key() {
            Key::Named(NamedKey::Enter) if !e.modifiers().shift() => {
                if let Some(enter) = onenter {
                    e.prevent_default();
                    enter.call(());
                }
            }
            Key::Named(NamedKey::Escape) => {
                if let Some(escape) = onescape {
                    e.prevent_default();
                    escape.call(());
                }
            }
            _ => {}
        }
    };
    let input = move |e: FormEvent| {
        e.stop_propagation();
        value.set(e.value());
    };
    rsx! {
        span { class: "ui-field", class: if multiline { "multiline" },
            if multiline {
                textarea { id, rows: "2", value: "{value}", oninput: input, onkeydown: keys }
            } else {
                input { id, value: "{value}", oninput: input, onkeydown: keys }
            }
            if value.read().is_empty() { span { class: "ui-field-ph", "{placeholder}" } }
        }
    }
}

/// A message box: a field and its send button. Holds its own draft, so
/// typing re-renders only this. `onsend` receives the text and the field
/// clears. Without `send_label` the button is an arrow.
#[component]
pub fn Composer(
    placeholder: String,
    onsend: EventHandler<String>,
    #[props(default)] multiline: bool,
    #[props(default)] send_label: Option<String>,
    #[props(default)] field_id: Option<String>,
    #[props(default)] send_id: Option<String>,
    #[props(default)] onescape: Option<EventHandler<()>>,
) -> Element {
    let mut draft = use_signal(String::new);
    let send = use_callback(move |()| {
        let text = draft.peek().trim().to_string();
        draft.set(String::new());
        onsend.call(text);
    });
    let (variant, icon) = match send_label {
        Some(_) => (Variant::Primary, None),
        None => (Variant::Icon, Some(Icon::Send)),
    };
    rsx! {
        div { class: "ui-composer", class: if multiline { "multiline" },
            TextField { value: draft, placeholder, multiline, id: field_id, onenter: send, onescape }
            Button { id: send_id, label: send_label.clone().unwrap_or_else(|| "Send".into()), text: send_label,
                icon, variant, class: "ui-send", stop: true, onclick: send }
        }
    }
}

/// The notebook's checkbox: a 15px box, filled with a tick when checked.
#[component]
pub fn Checkbox(checked: bool) -> Element {
    rsx! {
        span { class: "ui-check", aria_hidden: "true",
            span { class: "marker-face", class: if checked { "checked" } }
            if checked { span { class: "marker-tick" } }
        }
    }
}

/// A checklist line: the box, the text, and an optional tag at the end.
#[component]
pub fn CheckItem(
    text: String,
    checked: bool,
    #[props(default)] tag: Option<String>,
    onclick: EventHandler<()>,
) -> Element {
    rsx! {
        button { class: "ui-check-item", class: if checked { "done" }, role: "checkbox",
            aria_checked: if checked { "true" } else { "false" },
            onclick: move |_| onclick.call(()),
            Checkbox { checked }
            span { class: "ui-check-text", "{text}" }
            if let Some(tag) = tag { span { class: "ui-check-tag", "{tag}" } }
        }
    }
}
