//! Source view: one native text input owns the entire Markdown document.
use super::{ByteSelection, Field, Writing, lists, mounted_field, report, selection};
use crate::prelude::*;
use margin_editor_core::edit::text_change;

fn track(writing: Writing, range: ByteSelection) {
    writing.track(range);
    writing.note.peek().remember_selection(range);
}

fn read(writing: Writing) {
    if let Some(range) = selection(writing.root, Field::Source) {
        track(writing, range);
    }
}

fn place(mut writing: Writing, range: ByteSelection) {
    track(writing, range);
    writing.focus.set(Some((Field::Source, range)));
}

fn key(mut writing: Writing, event: KeyboardEvent) {
    let Some(range) = selection(writing.root, Field::Source) else {
        return;
    };
    track(writing, range);
    if event.is_composing() {
        return;
    }
    let key = event.key();
    let mods = event.modifiers();
    if mods.alt() {
        return;
    }
    if mods.intersects(Modifiers::CONTROL | Modifiers::META) {
        use margin_editor_core::format::{InlineSelection, Style};
        let Key::Character(k) = &key else {
            return;
        };
        let style = match (k.to_ascii_lowercase().as_str(), mods.shift()) {
            ("b", false) => Style::Bold,
            ("i", false) => Style::Italic,
            ("x", true) => Style::Strike,
            ("e", false) => Style::Code,
            ("k", false) => Style::Link,
            _ => return,
        };
        let md = writing.contents.peek().clone();
        if let Some(sel) = InlineSelection::read(&md, range) {
            event.prevent_default();
            let edit = sel.toggle(&md, style);
            let result = writing.note.write().splice(&edit.changes);
            if result.is_ok() {
                place(writing, edit.selection);
            }
            report(result, writing.message);
        }
        return;
    }
    let command = match key {
        Key::Named(NamedKey::Enter) if !mods.shift() => lists::Command::Continue,
        Key::Named(NamedKey::Backspace) if !mods.shift() => lists::Command::Backspace,
        Key::Named(NamedKey::Tab) if mods.shift() => lists::Command::Outdent,
        Key::Named(NamedKey::Tab) => lists::Command::Indent,
        _ => return,
    };
    let edit = lists::edit(&writing.contents.peek(), range, command);
    if let Some(edit) = edit {
        event.prevent_default();
        report(writing.note.write().splice(&edit.changes), writing.message);
        place(writing, edit.caret);
    }
}

fn input(mut writing: Writing, value: &str) {
    let before = writing.contents.peek();
    let (removed, inserted) = text_change(&before, value);
    if removed.is_empty() && inserted.is_empty() {
        return;
    }
    // Brackets opened as text in Live stay open around an edit made here.
    let literal = *writing.literal.peek();
    if let Some((open, close)) = literal {
        writing.literal.set(
            (open < removed.start && removed.end <= close)
                .then(|| (open, close - removed.len() + inserted.len())),
        );
    }
    drop(before);
    report(writing.note.write().replace(removed, inserted), writing.message);
    read(writing);
}

#[component]
pub(super) fn SourceEditor(writing: Writing) -> Element {
    let md = writing.contents.read();
    use_effect(move || {
        writing.focus.read();
        if writing.focus.peek().is_none() {
            read(writing);
        }
    });
    rsx! {
        div { class: "relative w-full max-w-[812px] mx-auto mt-6 px-[46px] pb-6 flex-auto flex flex-col max-[650px]:mt-8 max-[650px]:pl-[42px] max-[650px]:pr-7 [.notebook.embedded_&]:max-w-[760px] [.notebook.embedded_&]:mx-0 [.notebook.embedded_&]:mt-6 [.notebook.embedded_&]:px-0 [.notebook.embedded_&]:pb-16",
            textarea { id: "markdown", class: "flex-auto w-full h-[calc(100vh-128px)] min-h-[260px] py-[18px] px-5 border border-solid border-line rounded-md bg-surface text-ink font-mono text-[14px]/[25px] font-normal focus:outline-1 focus:outline-solid focus:outline-accent", aria_label: "Markdown source", value: md.as_str(),
                onmounted: move |e| mounted_field(e, Field::Source, writing.focus),
                onpointerup: move |_| read(writing), onkeyup: move |_| read(writing),
                onkeydown: move |e| key(writing, e), oninput: move |e| input(writing, &e.value()),
                "{md}"
            }
        }
    }
}
