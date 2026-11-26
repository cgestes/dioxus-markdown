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
        span { class: "ui-field relative flex flex-auto min-w-0",
            if multiline {
                textarea {
                    id, rows: "2", value: "{value}",
                    class: "flex-auto min-w-0 m-0 p-0 border-0 outline-none bg-transparent text-ink [font:inherit] select-text resize-none",
                    oninput: input, onkeydown: keys
                }
            } else {
                input {
                    id, value: "{value}",
                    class: "flex-auto min-w-0 m-0 p-0 border-0 outline-none bg-transparent text-ink [font:inherit] select-text",
                    oninput: input, onkeydown: keys
                }
            }
            if value.read().is_empty() {
                span { class: "ui-field-ph absolute left-0 top-0 text-faint [font:inherit] pointer-events-none", "{placeholder}" }
            }
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
    let (variant, icon, send_cls) = match send_label {
        Some(_) => (Variant::Primary, None, "!h-7"),
        None => (
            Variant::Icon,
            Some(Icon::Send),
            "!w-7 !h-7 !bg-accent !text-on-accent hover:!bg-accent-deep hover:!text-on-accent",
        ),
    };
    let layout_cls = if multiline {
        "items-end py-1.5 pr-1.5 pl-[11px] [&_.ui-field_textarea]:h-11 [&_.ui-field_textarea]:py-[3px] [&_.ui-field-ph]:top-[3px] [&_.ui-field-ph]:leading-[1.4]"
    } else {
        "items-center py-1 pr-1 pl-2.5 [&_.ui-field_input]:h-[26px] [&_.ui-field_input]:leading-[26px] [&_.ui-field-ph]:leading-[26px]"
    };
    rsx! {
        div { class: "flex gap-1.5 border border-solid border-line-strong rounded-lg bg-surface font-sans text-[13px] leading-[1.4] font-normal focus-within:border-accent {layout_cls}",
            TextField { value: draft, placeholder, multiline, id: field_id, onenter: send, onescape }
            Button { id: send_id, label: send_label.clone().unwrap_or_else(|| "Send".into()), text: send_label,
                icon, variant, class: send_cls, stop: true, onclick: send }
        }
    }
}

/// The notebook's checkbox: a 15px box, filled with a tick when checked.
#[component]
pub fn Checkbox(checked: bool) -> Element {
    rsx! {
        span { class: "ui-check relative flex items-center justify-center flex-[0_0_15px] w-[15px] h-[15px]", aria_hidden: "true",
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
    let text_cls = if checked {
        "flex-auto text-faint line-through"
    } else {
        "flex-auto"
    };
    rsx! {
        button {
            class: "flex items-start justify-start gap-2.5 w-full h-auto m-0 py-1 px-0 border-0 rounded-none bg-transparent text-ink font-sans text-[13px] leading-[1.45] font-normal text-left cursor-pointer [&_.ui-check]:mt-0.5 [&:hover_.marker-face]:border-accent",
            role: "checkbox",
            aria_checked: if checked { "true" } else { "false" },
            onclick: move |_| onclick.call(()),
            Checkbox { checked }
            span { class: "{text_cls}", "{text}" }
            if let Some(tag) = tag {
                span { class: "ml-auto text-faint font-mono text-[10.5px] font-normal whitespace-nowrap", "{tag}" }
            }
        }
    }
}
