#[path = "native/commands.rs"]
mod commands;
#[path = "native/document.rs"]
mod document;
#[path = "native/formatting.rs"]
mod formatting;
#[path = "native/grid.rs"]
mod grid;
#[path = "native/inspector.rs"]
mod inspector;
#[path = "native/lists.rs"]
mod lists;
#[cfg(target_os = "macos")]
#[path = "native/macos.rs"]
mod macos;
#[path = "native/markdown.rs"]
mod markdown;
#[path = "native/marker_view.rs"]
mod marker_view;
#[path = "native/navigation.rs"]
mod navigation;
#[path = "native/pointer.rs"]
mod pointer;
#[path = "native/projection.rs"]
mod projection;
#[path = "native/restart.rs"]
mod restart;
#[path = "native/selection.rs"]
mod selection_model;
#[path = "native/split.rs"]
mod split;
#[path = "native/typefaces.rs"]
mod typefaces;
use margin_editor_core::table;
#[path = "native/writing.rs"]
mod writing;

use dioxus_native::{NodeHandle, prelude::*};
use document::{BlockKind, Sections};
use margin_editor_core::checkbox::markdown_markers;
use margin_editor_core::note::Note;
use writing::Writing;

const SAMPLE: &str = "# Field notes\n\nRecorder packed [ ]. Rain cover ready [x].\n\n## Listening stops\n\n| Stop | Listen for | Take |\n| :--- | :--- | ---: |\n| North lock | Water through the gate | 45 s |\n| Footbridge | Steps on the steel deck | 30 s |\n\n- [ ] Leave **ten seconds** of silence.\n- [x] Set the recorder to `48 kHz`.";
const CSS: &str = concat!(
    include_str!("../assets/theme.css"),
    "\n",
    include_str!("../assets/table.css"),
    "\n",
    include_str!("native/editor.css"),
    "\n",
    include_str!("native/split.css"),
    "\n",
    include_str!("native/inspector.css"),
);

fn main() {
    let window = dioxus_native::WindowAttributes::default()
        .with_title("Margin")
        .with_surface_size(dioxus_native::LogicalSize::new(920.0, 650.0));
    #[cfg(target_os = "macos")]
    macos::launch(window);
    #[cfg(not(target_os = "macos"))]
    {
        let config = dioxus_native::Config::new()
            .with_window_attributes(window)
            .with_font_ctx(typefaces::notebook_fonts());
        dioxus_native::launch_cfg(app, vec![], vec![Box::new(config)]);
    }
}

fn report<T>(result: loro::LoroResult<T>, mut message: Signal<String>) {
    if let Err(error) = result {
        message.set(error.to_string());
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ToolIcon {
    Bold,
    Italic,
    Strike,
    Code,
    Link,
    Plus,
    Table,
    Undo,
    Redo,
}

impl ToolIcon {
    fn draw(self) -> Element {
        let path = match self {
            Self::Bold => "M6 4h8a4 4 0 0 1 0 8H6zm0 8h9a4 4 0 0 1 0 8H6z",
            Self::Italic => "M19 4h-9m4 16H5M15 4 9 20",
            Self::Strike => {
                "M17.5 6.5C16.5 4.8 14.8 4 12 4c-3.5 0-5.4 1.7-5.4 4 0 1.5.8 2.6 2.3 3.3M4 12h16m-5.8 1.4c2 .8 3 1.7 3 3.3 0 2.2-2 3.7-5.4 3.7-2.5 0-4.6-.8-6-2.5"
            }
            Self::Code => "m8 7-5 5 5 5m8-10 5 5-5 5",
            Self::Link => {
                "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"
            }
            Self::Plus => "M12 5v14M5 12h14",
            Self::Table => {
                "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2ZM3 9h18M9 3v18"
            }
            Self::Undo => "M9 14 4 9l5-5M4 9h10.5a5.5 5.5 0 0 1 0 11H11",
            Self::Redo => "m15 14 5-5-5-5M20 9H9.5a5.5 5.5 0 0 0 0 11H13",
        };
        rsx! { span { class: "tool-icon", aria_hidden: "true", svg { width: "16", height: "16", view_box: "0 0 24 24",
            fill: "none", stroke: "currentColor", stroke_width: "1.7", stroke_linecap: "round",
            stroke_linejoin: "round", path { d: path }
        } } }
    }
}

#[component]
fn EditorButton(
    label: &'static str,
    glyph: Option<&'static str>,
    tip: Option<&'static str>,
    icon: Option<ToolIcon>,
    class: Option<&'static str>,
    id: Option<String>,
    selected: Option<bool>,
    expanded: Option<bool>,
    #[props(default)] disabled: bool,
    onactivate: EventHandler<()>,
) -> Element {
    let mut space_down = use_signal(|| false);
    let class = class.unwrap_or_default();
    rsx! {
        button {
            id, disabled, aria_disabled: disabled.then_some("true"), aria_label: label, title: tip.unwrap_or(label), class: "{class}", class: if selected == Some(true) { "selected" },
            aria_expanded: expanded.map(|s| if s { "true" } else { "false" }),
            aria_pressed: selected.map(|s| if s { "true" } else { "false" }),
            onclick: move |_| onactivate.call(()),
            onkeydown: move |event| {
                if event.is_composing() || event.modifiers().intersects(Modifiers::CONTROL | Modifiers::META | Modifiers::ALT) { return; }
                match event.key() {
                    Key::Named(NamedKey::Enter) => {
                        event.prevent_default();
                        if !event.is_auto_repeating() { onactivate.call(()); }
                    }
                    Key::Character(s) if s == " " => {
                        event.prevent_default();
                        if !event.is_auto_repeating() { space_down.set(true); }
                    }
                    _ => {}
                }
            },
            onkeyup: move |event| {
                if matches!(event.key(), Key::Character(s) if s == " ") && *space_down.peek() {
                    event.prevent_default();
                    space_down.set(false);
                    onactivate.call(());
                }
            },
            onblur: move |_| space_down.set(false),
            if let Some(icon) = icon { {icon.draw()} }
            if let Some(text) = glyph.or_else(|| icon.is_none().then_some(label)) {
                span { class: "tool-caption", "{text}" }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Source,
    Title,
    TitleGap,
    Heading(usize),
    Body(usize),
    Cell(usize, usize, usize),
}

impl Field {
    fn from_input_id(id: &str) -> Option<Self> {
        match id {
            "note-title" => Some(Self::Title),
            "title-gap" => Some(Self::TitleGap),
            "note-body" => Some(Self::Body(0)),
            _ if id.starts_with("note-body-") => {
                Some(Self::Body(id.strip_prefix("note-body-")?.parse().ok()?))
            }
            _ if id.starts_with("note-heading-") => Some(Self::Heading(
                id.strip_prefix("note-heading-")?.parse().ok()?,
            )),
            _ => {
                let mut parts = id.strip_prefix("cell-")?.split('-');
                Some(Self::Cell(
                    parts.next()?.parse().ok()?,
                    parts.next()?.parse().ok()?,
                    parts.next()?.parse().ok()?,
                ))
            }
        }
    }

    fn id(self) -> String {
        match self {
            Self::Source => "markdown".to_owned(),
            Self::Title => "note-title".to_owned(),
            Self::TitleGap => "title-gap".to_owned(),
            Self::Heading(i) => format!("note-heading-{i}"),
            Self::Body(0) => "note-body".to_owned(),
            Self::Body(i) => format!("note-body-{i}"),
            Self::Cell(t, r, c) => format!("cell-{t}-{r}-{c}"),
        }
    }
}

// Anchor first, moving end second. Parley and the Markdown parser both use UTF-8 bytes.
type ByteSelection = [usize; 2];

fn selection(root: Signal<Option<NodeHandle>>, field: Field) -> Option<ByteSelection> {
    let root = root.peek();
    let doc = root.as_ref()?.doc();
    let id = doc.get_element_by_id(&field.id())?;
    let editor = &doc.get_node(id)?.element_data()?.text_input_data()?.editor;
    let selected = editor.raw_selection();
    Some([selected.anchor().index(), selected.focus().index()])
}

type FocusRequest = Signal<Option<(Field, ByteSelection)>>;

fn restore_focus(root: &NodeHandle, mut request: FocusRequest) {
    let Some((field, selection)) = *request.peek() else {
        return;
    };
    let mut doc = root.doc_mut();
    let Some(id) = doc.get_element_by_id(&field.id()) else {
        return;
    };
    // Blitz creates the text editor on first layout. A newly mounted field can
    // receive the next keystroke as soon as the mode-switch callback returns.
    if doc
        .get_node(id)
        .and_then(|n| n.element_data()?.text_input_data())
        .is_none()
    {
        doc.resolve(0.0);
    }
    let Some(input) = doc
        .get_node(id)
        .and_then(|n| n.element_data()?.text_input_data())
    else {
        return;
    };
    let text = input.editor.raw_text();
    let [anchor, focus] = selection.map(|i| text.floor_char_boundary(i.min(text.len())));
    doc.set_focus_to(id);
    doc.with_text_input(id, |mut editor| editor.select_byte_range(anchor, focus));
    if let Field::Cell(t, r, c) = field {
        use blitz_dom::ScrollBehavior;
        if let Some(container) = doc.get_element_by_id(&format!("grid-scroll-{t}"))
            && let (Some(cell), Some(view)) = (
                doc.get_element_by_id(&format!("slot-{t}-{r}-{c}"))
                    .and_then(|n| doc.get_client_bounding_rect(n)),
                doc.get_client_bounding_rect(container),
            )
        {
            let dx =
                (cell.x - view.x).min(0.0) + (cell.x + cell.width - view.x - view.width).max(0.0);
            if dx != 0.0 {
                doc.scroll_by(container, dx, 0.0, ScrollBehavior::Instant);
            }
        }
    }
    selection_model::scroll_caret(&mut doc, id);
    request.set(None);
}

fn mounted_field(event: MountedEvent, field: Field, request: FocusRequest) {
    if request.peek().is_some_and(|(target, _)| target == field)
        && let Some(handle) = event.downcast::<NodeHandle>()
    {
        restore_focus(handle, request);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum View {
    Live,
    Split,
    Source,
}

fn app() -> Element {
    if std::env::args().any(|arg| arg == "--test-window") {
        return notebook_view(
            "# Input check\n\nabcd\nefgh\nijkl\nmnop\n\n| A | B |\n| --- | --- |\n| one | two |\n| three | four |",
            View::Live,
            true,
        );
    }
    let mode = if std::env::args().any(|arg| arg == "--split") {
        View::Split
    } else {
        View::Live
    };
    notebook_view(
        SAMPLE,
        mode,
        std::env::args().any(|arg| arg == "--test-editor"),
    )
}

#[cfg(test)]
fn notebook(initial: &'static str) -> Element {
    notebook_view(initial, View::Live, false)
}

fn notebook_view(initial: &str, initial_view: View, inspect_at_start: bool) -> Element {
    let note = use_signal(|| Note::new(initial));
    let mut view = use_signal(|| initial_view);
    let source = move || view() != View::Live;
    let message = use_signal(String::new);
    let newer_build = restart::use_newer_build();
    let mut root = use_signal(|| None::<NodeHandle>);
    let mut bookmark = use_signal(|| [0, 0]);
    let initial_field = match initial_view {
        View::Live if inspect_at_start => Some(Field::Body(0)),
        View::Split => Some(Field::Source),
        _ => None,
    };
    let mut focus = use_signal(|| initial_field.map(|field| (field, [0; 2])));
    let contents = use_memo(move || note.read().markdown());
    let index = use_memo(move || markdown_markers(&contents.read()));
    let draft_table = use_signal(|| None);
    let tables = use_memo(move || {
        let mut list = table::tables(&contents.read());
        if let Some(at) = draft_table() {
            list.retain(|t| t.range.start != at);
        }
        list
    });
    let writing = Writing {
        note,
        root,
        focus,
        message,
        contents,
        markers: index,
        draft_table,
        tables,
        blocks: use_memo(move || document::blocks(&contents.read(), &tables.read())),
        goal_x: use_signal(|| None),
        span: use_signal(|| None),
        cells: use_signal(|| None),
        cell_editing: use_signal(|| false),
        table_menu: use_signal(|| None),
        table_menu_top: use_signal(|| None),
        quiet_cell: use_signal(|| None),
        selected_text: use_signal(|| None),
        format_position: use_signal(|| None),
        drag: use_signal(|| None),
        pressed_box: use_signal(|| None),
        hovered_box: use_signal(|| None),
        mouse_box: use_signal(|| None),
        edit_mode: use_signal(|| projection::EditMode::Markdown),
        cursor_source: use_signal(|| None),
        heights: use_signal(Vec::new),
        heading_heights: use_signal(Vec::new),
        caret_line_col: use_signal(|| None),
        syntax: use_memo(move || markdown::syntax(&contents.read())),
        ink: use_signal(Vec::new),
        active: use_signal(|| initial_field.filter(|f| *f != Field::Source)),
        body: use_signal(|| None),
        parked_selection: use_signal(|| None),
        marker: use_signal(|| None),
        boxes: use_signal(Vec::new),
        literal: use_signal(|| None),
    };
    #[cfg(test)]
    use_context_provider(|| writing);
    let mut inspecting = use_signal(|| inspect_at_start);
    let mut trace =
        use_signal(|| inspector::Trace::new(if inspect_at_start { initial } else { "" }));
    let window =
        use_hook(try_consume_context::<std::sync::Arc<dyn dioxus_native::winit::window::Window>>);
    let window_title = use_memo(move || {
        let md = contents.read();
        let title = Sections::read(&md).title(&md).trim();
        let title = if title.is_empty() { "Untitled" } else { title };
        match restart::worktree_name() {
            Some(name) => format!("Margin · {name} · {title}"),
            None => format!("Margin · {title}"),
        }
    });
    let desktop = window.is_some();
    use_effect(move || {
        let title = window_title.read();
        if let Some(window) = &window {
            if inspecting() && view() == View::Live {
                window.set_title(&format!("{title}  |  {}", trace.read().title_status()));
            } else {
                window.set_title(&title);
            }
        }
    });
    // A toolbar action can target an existing field or one the mode switch
    // will create. Its mounted handler picks up a request the effect leaves pending.
    use_effect(move || {
        if focus.read().is_some()
            && let Some(handle) = root.peek().as_ref()
        {
            restore_focus(handle, focus);
            writing.refresh_format_position();
        }
    });
    // Window scrolling must follow ordinary native typing as well as commands
    // that move focus. The new line's geometry is available after the DOM edit.
    let mut reveal_input = use_signal(|| false);
    use_effect(move || {
        if !reveal_input() {
            return;
        }
        reveal_input.set(false);
        if let Some(handle) = root.peek().as_ref() {
            let mut doc = handle.doc_mut();
            if let Some(id) = doc.get_focussed_node_id() {
                doc.resolve(0.0);
                selection_model::scroll_caret(&mut doc, id);
            }
        }
    });
    let markdown = contents();
    let sections = writing.sections(&markdown);
    let body_start = sections.body;
    let block_list = writing.blocks.read().clone();
    let title_gap = sections.origin == document::BodyStart::AfterBlankLine;
    let hint_at = (writing.marker)();
    let raw = (writing.literal)().is_some();
    use_effect(move || {
        root.read();
        writing.contents.read();
        writing.active.read();
        writing.literal.read();
        writing.focus.read();
        writing.edit_mode.read();
        writing.cursor_source.read();
        writing.mouse_box.read();
        writing.refresh_boxes();
    });
    // Resolve pending DOM mutations immediately after Dioxus renders so Blitz's
    // hit-testing (paint_children) never dereferences a deleted NodeId.
    use_effect(move || {
        writing.contents.read();
        writing.ink.read();
        writing.boxes.read();
        writing.format_position.read();
        writing.table_menu.read();
        writing.cells.read();
        writing.span.read();
        writing.edit_mode.read();
        view.read();
        if let Some(handle) = root.peek().as_ref() {
            handle.doc_mut().resolve(0.0);
        }
    });
    let (hint, action, action_id) = if raw {
        ("Editing bracket text", "Keep literal", "keep-literal")
    } else {
        ("Space or X toggles", "Edit as text · F2", "edit-as-text")
    };
    let undo_tip = if cfg!(target_os = "macos") {
        "Undo (⌘Z)"
    } else {
        "Undo (Ctrl+Z)"
    };
    let redo_tip = if cfg!(target_os = "macos") {
        "Redo (⌘⇧Z)"
    } else {
        "Redo (Ctrl+Y)"
    };
    let mut change_view = move |next| {
        let md = note.peek().markdown();
        let range = if source() {
            selection(root, Field::Source)
        } else {
            writing
                .absolute_selection()
                .or(*writing.parked_selection.peek())
        }
        .unwrap_or(*bookmark.peek())
        .map(|i| md.floor_char_boundary(i.min(md.len())));
        bookmark.set(range);
        writing.reset();
        if next == View::Live {
            writing.open(writing.field_at(range[1]), range);
        } else {
            focus.set(Some((Field::Source, range)));
        }
        view.set(next);
    };
    let mut set_edit_mode = move |next_mode: projection::EditMode| {
        let mut w = writing;
        let current_sel = if source() {
            selection(root, Field::Source)
        } else {
            w.absolute_selection().or(*w.parked_selection.peek())
        };
        w.edit_mode.set(next_mode);
        if source() {
            change_view(View::Live);
        } else if let Some(range) = current_sel {
            w.open(w.field_at(range[1]), range);
        }
    };
    let is_rich = view() == View::Live && (writing.edit_mode)() == projection::EditMode::Rich;
    let is_md_live = view() == View::Live && (writing.edit_mode)() == projection::EditMode::Markdown;
    rsx! {
        style { "{CSS}" }
        main { class: "notebook", class: if view()==View::Split { "split-view" }, class: if is_rich { "rich-mode" }, class: if inspecting() && !source() { "inspecting" },
            onpointerdown: move |event| { writing.clear_goal(); writing.dismiss_menu_at(event.client_coordinates().x,event.client_coordinates().y); },
            oninput: move |_| reveal_input.set(true),
            onmounted: move |event| root.set(event.downcast::<NodeHandle>().cloned()),
            onkeydown: move |e| {
                if !source() && !e.is_composing() && e.modifiers().is_empty() {
                    match e.key() {
                        Key::Named(NamedKey::F8) => {
                            e.prevent_default();
                            if !e.is_auto_repeating() {
                                let next = !inspecting();
                                trace.set(if next { inspector::Trace::new(&contents.peek()) } else { inspector::Trace::new("") });
                                inspecting.set(next);
                            }
                            return;
                        }
                        Key::Named(NamedKey::F9) if inspecting() => {
                            e.prevent_default();
                            if !e.is_auto_repeating() { inspector::save(writing, trace); }
                            return;
                        }
                        _ => {}
                    }
                }
                if !e.is_composing() && !e.modifiers().alt()
                    && e.modifiers().intersects(Modifiers::CONTROL | Modifiers::META)
                    && let Key::Character(k) = e.key()
                    && (k.eq_ignore_ascii_case("z") || k.eq_ignore_ascii_case("y")) {
                    e.prevent_default();
                    writing.history(source(), e.modifiers().shift() || k.eq_ignore_ascii_case("y"));
                }
            },
            header { id: "workspace-bar", class: "workspace-bar",
                div { class: "brand",
                    span { class: "brandmark", aria_hidden: "true", i {} i {} i {} }
                    span { class: "wordmark", "Margin" }
                }
                if inspecting() && !source() {
                    inspector::Inspector { writing, trace, desktop }
                }
                nav { aria_label: "Editor tools",
                    div { class: "mode", role: "group", aria_label: "Markdown view",
                        EditorButton { label: "Rich", id: "rich-mode", tip: "Traditional rich editing with toolbar", selected: is_rich, onactivate: move |_| set_edit_mode(projection::EditMode::Rich) }
                        EditorButton { label: "Live", id: "live-mode", tip: "Keyboard-first Markdown live editing", selected: is_md_live, onactivate: move |_| set_edit_mode(projection::EditMode::Markdown) }
                        EditorButton { label: "Split", id: "split-mode", selected: view()==View::Split, onactivate: move |_| change_view(View::Split) }
                        EditorButton { label: "Source", id: "source-mode", selected: view()==View::Source, onactivate: move |_| change_view(View::Source) }
                    }
                    EditorButton { id: "insert-table", label: "Insert table", tip: "Insert Markdown table", icon: ToolIcon::Table, glyph: "Table", onactivate: move |_| writing.insert_table(source()) }
                    if newer_build() {
                        EditorButton { id: "restart-app", label: "Restart", tip: "A newer build is ready. Restart Margin to run it",
                            class: "restart-app", onactivate: move |_| restart::restart_app() }
                    }
                    div { class: "history-tools", role: "group", aria_label: "History",
                        EditorButton { id: "undo", label: "Undo", tip: undo_tip, icon: ToolIcon::Undo, disabled: !note.read().can_undo(), onactivate: move |_| writing.history(source(), false) }
                        EditorButton { id: "redo", label: "Redo", tip: redo_tip, icon: ToolIcon::Redo, disabled: !note.read().can_redo(), onactivate: move |_| writing.history(source(), true) }
                    }
                }
            }
            if is_rich {
                div { id: "rich-toolbar", class: "rich-toolbar", role: "toolbar", aria_label: "Formatting toolbar",
                    onpointerdown: move |e| e.prevent_default(),
                    div { class: "rich-group",
                        EditorButton { id: "rich-h1", label: "Heading 1", glyph: "H1", tip: "Heading 1 (⌘1)",
                            onactivate: move |_| { if let Some(f) = *writing.active.peek() { writing.set_heading_level(f, 1); } } }
                        EditorButton { id: "rich-h2", label: "Heading 2", glyph: "H2", tip: "Heading 2 (⌘2)",
                            onactivate: move |_| { if let Some(f) = *writing.active.peek() { writing.set_heading_level(f, 2); } } }
                        EditorButton { id: "rich-h3", label: "Heading 3", glyph: "H3", tip: "Heading 3 (⌘3)",
                            onactivate: move |_| { if let Some(f) = *writing.active.peek() { writing.set_heading_level(f, 3); } } }
                        EditorButton { id: "rich-p", label: "Paragraph", glyph: "¶", tip: "Paragraph (⌘0)",
                            onactivate: move |_| { if let Some(f) = *writing.active.peek() { writing.set_heading_level(f, 0); } } }
                    }
                    span { class: "rich-sep", aria_hidden: "true" }
                    div { class: "rich-group",
                        EditorButton { id: "rich-bold", label: "Bold", tip: "Bold (⌘B)", icon: ToolIcon::Bold,
                            onactivate: move |_| { writing.format(margin_editor_core::format::Style::Bold); } }
                        EditorButton { id: "rich-italic", label: "Italic", tip: "Italic (⌘I)", icon: ToolIcon::Italic,
                            onactivate: move |_| { writing.format(margin_editor_core::format::Style::Italic); } }
                        EditorButton { id: "rich-strike", label: "Strikethrough", tip: "Strikethrough (⌘⇧X)", icon: ToolIcon::Strike,
                            onactivate: move |_| { writing.format(margin_editor_core::format::Style::Strike); } }
                        EditorButton { id: "rich-code", label: "Inline code", tip: "Inline code (⌘E)", icon: ToolIcon::Code,
                            onactivate: move |_| { writing.format(margin_editor_core::format::Style::Code); } }
                        EditorButton { id: "rich-link", label: "Link", tip: "Link (⌘K)", icon: ToolIcon::Link,
                            onactivate: move |_| { writing.format(margin_editor_core::format::Style::Link); } }
                    }
                    span { class: "rich-sep", aria_hidden: "true" }
                    div { class: "rich-group",
                        EditorButton { id: "rich-bullet", label: "Bullet list", glyph: "• List", tip: "Toggle bullet list",
                            onactivate: move |_| writing.toggle_line_prefix("- ") }
                        EditorButton { id: "rich-task", label: "Checklist", glyph: "☑ Task", tip: "Toggle checklist (⌘Enter)",
                            onactivate: move |_| { if let Some(f) = *writing.active.peek() { writing.toggle_line_task(f); } } }
                        EditorButton { id: "rich-quote", label: "Quote", glyph: "❝ Quote", tip: "Toggle blockquote",
                            onactivate: move |_| writing.toggle_line_prefix("> ") }
                        EditorButton { id: "rich-rule", label: "Divider", glyph: "— Rule", tip: "Insert horizontal rule",
                            onactivate: move |_| writing.insert_rule() }
                    }
                }
            }
            if source() {
                split::DocumentEditor { writing, split: view()==View::Split }
            } else {
                div { class: "paper", id: "paper",
                    onpointerdown: move |e| writing.pointer_down(e),
                    onpointermove: move |e| writing.pointer_move(e),
                    onpointerup: move |_| writing.end_drag(),
                    onpointercancel: move |_| writing.end_drag(),
                    onpointerleave: move |_| writing.clear_hover(),
                    grid::Heading { writing, field: Field::Title }
                if title_gap {
                    div { class: "title-row",
                    input { id: "title-gap", class: "blank-editor", aria_label: "Blank line after title", value: "",
                        onmounted: move |e| mounted_field(e, Field::TitleGap, focus),
                        onfocus: move |_| writing.track_field(Field::TitleGap),
                        onclick: move |_| writing.pointer(Field::TitleGap),
                        onkeydown: move |e| writing.key(Field::TitleGap, e),
                        oninput: move |e| writing.input(Field::TitleGap, &e.value()),
                    }
                    selection_model::SelectionInk { writing, field: Field::TitleGap }
                    }
                }
                section {
                    id: "body-surface", class: "sheet-body", tabindex: "-1", aria_label: "Note body",
                    onkeydown: move |e| {
                        if writing.active.peek().is_some() || e.is_composing() {
                            return;
                        }
                        let range = writing.parked_selection.peek().unwrap_or([body_start; 2]);
                        let field = writing.field_at(range[1]);
                        match e.key() {
                            Key::Named(NamedKey::Enter) => {
                                e.prevent_default();
                                writing.open(field, range);
                            }
                            Key::Named(
                                NamedKey::ArrowLeft
                                | NamedKey::ArrowRight
                                | NamedKey::ArrowUp
                                | NamedKey::ArrowDown,
                            ) => {
                                e.prevent_default();
                                writing.open(field, [range[1]; 2]);
                            }
                            Key::Character(s)
                                if !e
                                    .modifiers()
                                    .intersects(Modifiers::CONTROL | Modifiers::META | Modifiers::ALT) =>
                            {
                                e.prevent_default();
                                let lo = range[0].min(range[1]);
                                let hi = range[0].max(range[1]);
                                let pos = lo + s.len();
                                let mut w = writing;
                                report(w.note.write().replace(lo..hi, &s), w.message);
                                w.open(w.field_at(pos), [pos; 2]);
                            }
                            _ => {}
                        }
                    },
                    for i in 0..=block_list.len() {
                        if document::gap(&markdown, body_start, &block_list, i).is_some() {
                            grid::Prose { key: "prose-{i}", writing, index: i, segmented: !block_list.is_empty() }
                        }
                        if let Some(block) = block_list.get(i) {
                            match block.kind {
                                BlockKind::Heading { .. } => rsx! { grid::Heading { key: "heading-{i}", writing, field: Field::Heading(i) } },
                                BlockKind::Table(t) => rsx! { grid::Grid { key: "table-{t}", writing, index: t } },
                            }
                        }
                    }
                }
                }
                formatting::FormatBar { writing }
                if desktop { marker_view::WindowLayout { writing } }
            }
            if view() != View::Split {
                footer { class: "statusbar",
                    span { class: "status-format", i { aria_hidden: "true" } "Markdown" }
                    span { class: "status-sep", aria_hidden: "true" }
                    span { class: "status-words", {
                        let words = markdown.split_whitespace().filter(|w| w.chars().any(char::is_alphanumeric)).count();
                        format!("{words} {}", if words == 1 { "word" } else { "words" })
                    } }
                    if !source() && hint_at.is_some() {
                        span { id: "marker-hint", "{hint}" }
                        EditorButton { id: action_id.to_owned(), label: action, onactivate: move |_| {
                            if raw { writing.keep_literal(); } else { writing.toggle_literal(); }
                        } }
                    } else if !source() && writing.parked_selection.read().is_some() {
                        span { class: "parked-hint", "Press Enter or type to edit" }
                    }
                    span { class: "status-right",
                        if let Some([ln, col]) = (writing.caret_line_col)() {
                            span { "Ln {ln}, Col {col}" }
                            span { class: "status-sep", aria_hidden: "true" }
                        }
                        span { class: "status-lines", {
                            let count = markdown.bytes().filter(|c| *c == b'\n').count() + 1;
                            format!("{count} {}", if count == 1 { "line" } else { "lines" })
                        } }
                    }
                }
            }
            if !message.read().is_empty() { p { class: "error", "{message}" } }
        }
    }
}

#[cfg(test)]
#[path = "native/tests.rs"]
mod tests;
