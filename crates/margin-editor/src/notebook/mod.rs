//! The Live notebook: the editor surface, its editing modules and the view that ties them together.
pub(crate) mod commands;
pub(crate) mod formatting;
pub(crate) mod grid;
pub(crate) mod inspector;
pub(crate) mod link_card;
pub(crate) mod markdown;
pub(crate) mod marker_view;
pub(crate) mod navigation;
pub(crate) mod page;
pub(crate) mod pointer;
pub(crate) mod selection_model;
pub(crate) mod slash;
pub(crate) mod source;
pub(crate) mod store;
pub(crate) mod writing;
// The Markdown model, kept under the paths the editor modules use.
pub(crate) use margin_projection::{blocks, document, link_preview, lists, projection};

use crate::platform::NodeHandle;
use crate::prelude::*;
use margin_editor_core::checkbox::markdown_markers;
use margin_note::note::Note;
use margin_editor_core::table;
use margin_ui::Icon;
pub(crate) use writing::Writing;
pub(crate) use document::{LandmarkKind, Sections};
use margin_ui as ui;
use crate::support::restart;
use crate::CSS;
pub use store::{InMemory, NoteStore, Store};

pub(crate) fn report<T>(result: loro::LoroResult<T>, mut message: Signal<String>) {
    if let Err(error) = result {
        message.set(error.to_string());
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Field {
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

    pub(crate) fn id(self) -> String {
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
pub(crate) type ByteSelection = [usize; 2];

pub(crate) fn selection(root: Signal<Option<NodeHandle>>, field: Field) -> Option<ByteSelection> {
    let root = root.peek();
    let doc = root.as_ref()?.doc();
    let id = doc.get_element_by_id(&field.id())?;
    let editor = &doc.get_node(id)?.element_data()?.text_input_data()?.editor;
    let selected = editor.raw_selection();
    Some([selected.anchor().index(), selected.focus().index()])
}

pub(crate) type FocusRequest = Signal<Option<(Field, ByteSelection)>>;

pub(crate) fn restore_focus(root: &NodeHandle, mut request: FocusRequest) {
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
        use crate::platform::ScrollBehavior;
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

pub(crate) fn mounted_field(event: MountedEvent, field: Field, request: FocusRequest) {
    if request.peek().is_some_and(|(target, _)| target == field)
        && let Some(handle) = crate::platform::mounted(&event)
    {
        restore_focus(&handle, request);
    }
}

/// The view a notebook opens in.
#[derive(Clone, Copy, Default, PartialEq)]
pub enum View {
    /// The formatted text: Markdown's delimiters stay hidden.
    Rich,
    /// The formatted text, with the Markdown shown where the caret is.
    #[default]
    Live,
    /// The whole note as Markdown source.
    Source,
}

/// What the notebook shows: the note's fields (Rich and Live, which differ
/// by `EditMode`), or its source.
#[derive(Clone, Copy, PartialEq)]
enum Pane {
    Fields,
    Source,
}

/// Which comments a notebook starts with, and who answers them.
#[derive(Clone, Default, PartialEq)]
pub enum Comments {
    /// None yet; the local stand-in answers.
    #[default]
    Empty,
    /// Two sample threads on the sample note; the stand-in answers.
    Sample,
    /// The page's agent answers (see `crate::comments::CommentAgent`).
    Agent(crate::comments::CommentAgent),
}

/// How a notebook opens.
#[derive(Clone, Default, PartialEq)]
pub struct NotebookOptions {
    pub view: View,
    /// Show the debug inspector from the start.
    pub inspect: bool,
    pub comments: Comments,
    /// What the app around the notebook supplies: its window's title, its
    /// menu, its bar.
    pub host: Host,
    /// Where the note lives (a vault, a sync file, a page on a server);
    /// without one it is edited in memory and the status bar says nothing
    /// about it.
    pub store: Option<Store>,
}

/// What the app around a notebook supplies to it.
#[derive(Clone, Default, PartialEq)]
pub struct Host {
    /// The window's title while this notebook is open. Without one the
    /// notebook titles the window after the note.
    pub title: Option<String>,
    /// The open state of the app's own menu (its list of notes): the bar
    /// then starts with a Notes button that toggles it.
    pub menu: Option<Signal<bool>>,
    /// Set when the notebook is a pane of the app's window (the Topics
    /// window): the bar then starts with the app's way back and ends with
    /// its actions, the page aligns with the app's column, and the window's
    /// title stays the app's.
    pub bar: Option<HostBar>,
}

/// The app's part of the bar of a notebook it shows as a pane.
#[derive(Clone, PartialEq)]
pub struct HostBar {
    pub back: String,
    pub onback: EventHandler<()>,
    /// Where the page is, after the back button: `house-systems.md`.
    pub crumb: String,
    pub actions: Vec<HostAction>,
}

#[derive(Clone, PartialEq)]
pub struct HostAction {
    pub label: String,
    pub icon: Icon,
    pub onclick: EventHandler<()>,
}


pub fn notebook_view(initial: &str, options: NotebookOptions) -> Element {
    let NotebookOptions { view: initial_view, inspect: inspect_at_start, comments: start_comments, host, store } = options;
    let Host { title: host_title, menu: host_menu, bar: host } = host;
    let (initial_pane, initial_mode) = match initial_view {
        View::Rich => (Pane::Fields, projection::EditMode::Rich),
        View::Live => (Pane::Fields, projection::EditMode::Markdown),
        View::Source => (Pane::Source, projection::EditMode::Markdown),
    };
    let page_source = store.as_ref().and_then(|s| s.page());
    let embedded = host.is_some();
    let seed_comments = start_comments == Comments::Sample;
    let agent = match start_comments {
        Comments::Agent(agent) => agent,
        _ => crate::comments::CommentAgent::mock(),
    };
    let store = use_signal(move || store);
    let note = use_signal(|| store.peek().as_ref().and_then(|s| s.note()).unwrap_or_else(|| Note::new(initial)));
    // The store's line for the status bar ("Saved on device", "Not saved").
    let store_status = match store.peek().clone() {
        Some(s) => s.use_persistence(note),
        None => use_signal(String::new),
    };
    let comments = use_signal(move || {
        if let Some(threads) = store.peek().as_ref().and_then(|s| s.threads()) {
            threads
        } else if seed_comments {
            crate::comments::seed_sample_comments(&note.peek())
        } else {
            Vec::new()
        }
    });
    // On a phone the pane would cover the note, so it waits for the comments button.
    let mut comments_open =
        use_signal(move || seed_comments && initial_pane == Pane::Fields && !crate::platform::phone_sized());
    let active_comment = use_signal(move || {
        if seed_comments {
            comments.peek().iter().find(|t| !t.file.resolved).map(|t| t.id)
        } else {
            None
        }
    });
    let draft_comment = use_signal(|| None);
    let mut view = use_signal(|| initial_pane);
    let source = move || view() != Pane::Fields;
    let message = use_signal(String::new);
    let prior_discussion = use_hook(|| {
        std::rc::Rc::new(std::cell::RefCell::new(
            serde_json::to_vec(&*comments.peek()).unwrap_or_default(),
        ))
    });
    use_effect(move || {
        if let Some(s) = store.peek().as_ref() {
            let mut prior = prior_discussion.borrow_mut();
            let result = serde_json::to_vec(&*comments.read())
                .map_err(anyhow::Error::from)
                .and_then(|bytes| {
                    if *prior != bytes {
                        s.save_discussion(&prior, &bytes)?;
                        *prior = bytes;
                    }
                    Ok(())
                });
            if let Err(e) = result {
                let mut m = message;
                m.set(e.to_string());
            }
        }
    });
    let newer_build = restart::use_newer_build();
    let mut root = use_signal(|| None::<NodeHandle>);
    let mut bookmark = use_signal(|| [0, 0]);
    let initial_field = match initial_pane {
        Pane::Fields if inspect_at_start => Some(Field::Body(0)),
        _ => None,
    };
    let mut focus = use_signal(|| initial_field.map(|field| (field, [0; 2])));
    let contents = use_memo(move || note.read().markdown());
    let mut rev_counter = 0u64;
    let doc_rev = use_memo(move || {
        contents.read();
        rev_counter += 1;
        rev_counter
    });
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
        store,
        note,
        root,
        focus,
        message,
        contents,
        doc_rev,
        markers: index,
        draft_table,
        tables,
        landmarks: use_memo(move || document::landmarks(&contents.read(), &tables.read())),
        logical_blocks: use_memo(move || blocks::split_blocks(&contents.read())),
        goal_x: use_signal(|| None),
        span: use_signal(|| None),
        grid: grid::TableUi {
            cells: use_signal(|| None),
            editing: use_signal(|| false),
            menu: use_signal(|| None),
            quiet: use_signal(|| None),
            row_heights: use_signal(Vec::new),
        },
        selected_text: use_signal(|| None),
        format_position: use_signal(|| None),
        drag: use_signal(|| None),
        pressed_box: use_signal(|| None),
        mouse_box: use_signal(|| None),
        edit_mode: use_signal(move || initial_mode),
        cursor_source: use_signal(|| None),
        parsed: use_memo(move || projection::parse_markdown(&contents.read())),
        dom_dirty: use_signal(|| true),
        last_tracked: use_signal(|| None),
        empty_line: use_signal(|| None),
        slash: slash::Slash { menu: use_signal(|| None), selected: use_signal(|| 0) },
        links: link_card::Links { card: use_signal(|| None), dismissed: use_signal(|| None), editing: use_signal(|| false) },
        handles: writing::BlockHandles { menu: use_signal(|| None), drag: use_signal(|| None) },
        heights: use_signal(Vec::new),
        heading_heights: use_signal(Vec::new),
        caret_line_col: use_signal(|| None),
        ink: use_signal(Vec::new),
        callout_boxes: use_signal(Vec::new),
        code_boxes: use_signal(Vec::new),
        callout_menu: use_signal(|| None),
        code_lang_menu: use_signal(|| None),
        caret: use_signal(|| None),
        active: use_signal(|| initial_field.filter(|f| *f != Field::Source)),
        body: use_signal(|| None),
        parked_selection: use_signal(|| None),
        marker: use_signal(|| None),
        boxes: use_signal(Vec::new),
        literal: use_signal(|| None),
        discussion: crate::comments::Discussion {
            threads: comments,
            open: comments_open,
            active: active_comment,
            draft: draft_comment,
            agent: use_signal(move || agent),
            file: use_signal(crate::comments::CommentFile::default),
            stream: use_signal(Default::default),
            ranges: use_memo(move || {
                let md = contents.read();
                crate::comments::ranges(&note.read(), &md, &comments.read())
            }),
        },
        page_state: use_signal({
            let etag = page_source.as_ref().and_then(|p| p.etag.clone());
            move || page::PageState { etag, saved_rev: *doc_rev.peek(), ..Default::default() }
        }),
        page: use_signal(move || page_source),
    };
    #[cfg(test)]
    use_context_provider(|| writing);
    crate::comments::use_comment_file(writing);
    let mut inspecting = use_signal(|| inspect_at_start);
    let mut trace =
        use_signal(|| inspector::Trace::new(if inspect_at_start { initial } else { "" }));
    let window = crate::platform::use_window();
    let names_place = host_title.is_some();
    let window_title = use_memo(move || {
        if let Some(title) = host_title.clone() {
            return title;
        }
        let md = contents.read();
        let title = Sections::read(&md).title(&md).trim();
        let title = if title.is_empty() { "Untitled" } else { title };
        match restart::worktree_name() {
            Some(name) => format!("{} · {name} · {title}", crate::app_title()),
            None => format!("{} · {title}", crate::app_title()),
        }
    });
    let desktop = window.is_some();
    use_effect(move || {
        let title = window_title.read();
        // A host (Osom) owns its window's title.
        if let Some(window) = window.as_ref().filter(|_| !embedded) {
            if !names_place && inspecting() && view() == Pane::Fields {
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
            writing.refresh_empty_line();
            writing.refresh_slash();
            writing.refresh_link_card();
            writing.refresh_boxes();
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
            {
                let mut doc = handle.doc_mut();
                if let Some(id) = doc.get_focussed_node_id() {
                    doc.resolve(0.0);
                    selection_model::scroll_caret(&mut doc, id);
                }
            }
            writing.refresh_boxes();
            writing.refresh_link_card();
        }
    });
    let markdown = contents();
    let sections = writing.sections(&markdown);
    let body_start = sections.body;
    let landmarks = writing.landmarks.read().clone();
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
        if let Some(handle) = root.peek().as_ref() {
            writing.resolve_dom(handle);
        }
        writing.refresh_boxes();
        writing.refresh_link_card();
    });
    // Resolve pending DOM mutations immediately after Dioxus renders so Blitz's
    // hit-testing (paint_children) never dereferences a deleted NodeId.
    {
        let dirty = writing.dom_dirty;
        *dirty.write_unchecked() = true;
    }
    use_effect(move || {
        writing.contents.read();
        writing.ink.read();
        writing.callout_boxes.read();
        writing.code_boxes.read();
        writing.callout_menu.read();
        writing.code_lang_menu.read();
        writing.boxes.read();
        writing.format_position.read();
        writing.empty_line.read();
        writing.slash.menu.read();
        writing.links.card.read();
        writing.links.editing.read();
        writing.handles.menu.read();
        writing.handles.drag.read();
        writing.grid.menu.read();
        writing.grid.cells.read();
        writing.span.read();
        writing.edit_mode.read();
        writing.discussion.threads.read();
        writing.discussion.open.read();
        writing.discussion.active.read();
        writing.discussion.draft.read();
        view.read();
        let dirty = writing.dom_dirty;
        *dirty.write_unchecked() = true;
        if let Some(handle) = root.peek().as_ref() {
            writing.resolve_dom(handle);
        }
    });
    let (hint, action, action_id) = if raw {
        ("Editing bracket text", "Keep literal", "keep-literal")
    } else {
        ("Space or X toggles", "Edit as text · F2", "edit-as-text")
    };
    use crate::platform::{apple_keys, shortcut};
    let undo_tip = format!("Undo ({})", shortcut("Z"));
    let redo_tip = format!("Redo ({})", shortcut(if apple_keys() { "Shift+Z" } else { "Y" }));
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
        if next == Pane::Fields {
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
            change_view(Pane::Fields);
        } else if let Some(range) = current_sel {
            w.open(w.field_at(range[1]), range);
        }
    };
    let is_rich = view() == Pane::Fields && (writing.edit_mode)() == projection::EditMode::Rich;
    let is_md_live = view() == Pane::Fields && (writing.edit_mode)() == projection::EditMode::Markdown;
    let mut toggle_debug = move || {
        if source() {
            change_view(Pane::Fields);
            trace.set(inspector::Trace::new(&contents.peek()));
            inspecting.set(true);
            return;
        }
        let next = !inspecting();
        trace.set(if next {
            inspector::Trace::new(&contents.peek())
        } else {
            inspector::Trace::new("")
        });
        inspecting.set(next);
    };
    const DOCK_TOOLS: &str = "sticky top-0 z-[6] flex items-center bg-page [&_.ui-btn.icon]:w-[26px] [&_.ui-btn.icon]:h-6 [&_.ui-btn.icon]:rounded-[5px] max-[485px]:[&_.ui-segment]:min-w-[34px] max-[485px]:[&_.ui-segment]:px-[5px] max-[485px]:[&_.ui-btn.icon]:w-6";
    let dock_layout = if embedded {
        "justify-start gap-2 h-auto min-h-10 m-0 pb-2 border-0 border-b border-solid border-line"
    } else {
        "flex-none justify-center h-9 py-1 px-3 max-[485px]:px-1.5"
    };
    let pill_bar_extra = if embedded { "flex-auto min-w-0" } else { "" };
    let statusbar_pad = if embedded { "px-0" } else { "px-5 max-[650px]:px-3" };
    let notebook_min_h = if embedded { "min-h-0" } else { "min-h-screen" };
    rsx! {
        style { "{ui::CSS}" }
        style { "{CSS}" }
        crate::platform::Head {}
        main { class: "notebook flex flex-col {notebook_min_h}", class: if embedded { "embedded" }, class: if is_rich { "rich-mode" }, class: if inspecting() && !source() { "inspecting" }, class: if writing.handles.drag.read().is_some_and(|d| d.dragging) { "dragging-block" },
            onpointerdown: move |event| { writing.clear_goal(); writing.dismiss_menu_at(event.client_coordinates().x,event.client_coordinates().y); },
            onpointermove: move |e| writing.pointer_move(e),
            onpointerup: move |_| writing.end_drag(),
            onpointercancel: move |_| writing.cancel_drag(),
            oninput: move |_| reveal_input.set(true),
            // A browser hands pasted text over with this event. Natively the key
            // handlers read the clipboard (`document_key`) and no event follows.
            onpaste: move |e| {
                if !source()
                    && let Some(text) = e.data_transfer().get_as_text()
                    && let Some(field) = writing.focused_field()
                    && writing.paste(field, &text)
                {
                    e.prevent_default();
                }
            },
            onmounted: move |event| root.set(crate::platform::mounted(&event)),
            onkeydown: move |e| {
                if !source() && !e.is_composing() && e.modifiers().is_empty() {
                    match e.key() {
                        Key::Named(NamedKey::F8) => {
                            e.prevent_default();
                            if !e.is_auto_repeating() {
                                toggle_debug();
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
            div { id: "mode-bar", class: "{DOCK_TOOLS} {dock_layout}",
                nav { class: "flex items-center gap-[5px] max-[485px]:gap-[3px] {pill_bar_extra}", aria_label: "Editor tools",
                    if let Some(mut open) = host_menu {
                        ui::Button { id: "notes-menu", label: "Notes", text: "Notes", expanded: open(), onclick: move |_| { let visible = open(); open.set(!visible); } }
                    }
                    if let Some(h) = host.as_ref() {
                        div { class: "flex items-center gap-2 min-w-0",
                            ui::Button { id: "doc-back", label: h.back.clone(), text: h.back.clone(), icon: Icon::Back, variant: ui::Variant::Ghost, onclick: { let back = h.onback; move |_| back.call(()) } }
                            span { class: "min-w-0 overflow-hidden text-muted type-mono-ui whitespace-nowrap", "{h.crumb}" }
                        }
                        span { class: "flex-auto" }
                    }
                    ui::Segmented { label: "Markdown view",
                        items: vec![
                            ui::Segment { id: "rich-mode", label: "Rich", tip: Some("Rich editing (delimiters stay hidden)"), selected: is_rich },
                            ui::Segment { id: "live-mode", label: "Live", tip: Some("Keyboard-first Markdown live editing"), selected: is_md_live },
                            ui::Segment { id: "source-mode", label: "Source", tip: None, selected: view() == Pane::Source },
                        ],
                        onselect: move |i: usize| match i {
                            0 => set_edit_mode(projection::EditMode::Rich),
                            1 => set_edit_mode(projection::EditMode::Markdown),
                            _ => change_view(Pane::Source),
                        },
                    }
                    ui::Button { id: "insert-table", variant: ui::Variant::Icon, label: "Insert table", tip: "Insert Markdown table", icon: Icon::Table, onclick: move |_| writing.insert_table(source()) }
                    if newer_build() {
                        ui::Button { id: "restart-app", label: "Restart", tip: "A newer build is ready. Restart Margin to run it",
                            class: "!text-accent-deep !border-accent-deep", onclick: move |_| restart::restart_app() }
                    }
                    div { class: "flex items-center pl-[5px] border-0 border-l border-solid border-line gap-px", role: "group", aria_label: "History",
                        ui::Button { id: "undo", variant: ui::Variant::Icon, label: "Undo", tip: undo_tip, icon: Icon::Undo, disabled: !note.read().can_undo(), onclick: move |_| writing.history(source(), false) }
                        ui::Button { id: "redo", variant: ui::Variant::Icon, label: "Redo", tip: redo_tip, icon: Icon::Redo, disabled: !note.read().can_redo(), onclick: move |_| writing.history(source(), true) }
                    }
                    {
                        let open_comments = writing.discussion.threads.read().iter().filter(|t| !t.file.resolved).count();
                        let panel_open = (writing.discussion.open)() && !source();
                        let tip = format!("Toggle comments ({} to comment on selection)", shortcut("Shift+M"));
                        rsx! {
                            span { class: "relative flex",
                                ui::Button { id: "toggle-comments", variant: ui::Variant::Icon, label: "Comments", tip, icon: Icon::Comment,
                                    selected: panel_open,
                                    onclick: move |_| {
                                        if source() {
                                            change_view(Pane::Fields);
                                            comments_open.set(true);
                                        } else {
                                            let next = !*comments_open.peek();
                                            comments_open.set(next);
                                        }
                                    },
                                }
                                if open_comments > 0 {
                                    span { class: "absolute top-[3px] right-[3px] w-1.5 h-1.5 rounded-full bg-warn pointer-events-none", aria_hidden: "true" }
                                }
                            }
                        }
                    }
                    ui::Button { id: "toggle-debug", variant: ui::Variant::Icon, label: "Debug display", tip: "Toggle debug display (F8)", icon: Icon::Bug, selected: inspecting() && !source(), onclick: move |_| toggle_debug() }
                    page::PageBar { writing }
                    if let Some(h) = host.as_ref() {
                        for action in h.actions.iter() {
                            ui::Button { label: action.label.clone(), text: action.label.clone(), icon: action.icon, onclick: { let go = action.onclick; move |_| go.call(()) } }
                        }
                    }
                }
            }
            if inspecting() && !source() {
                div { class: "fixed top-9 left-0 right-0 h-0 z-10",
                    inspector::Inspector { writing, trace, desktop }
                }
            }
            if source() {
                source::SourceEditor { writing }
            } else {
                div { class: "flex-auto flex items-stretch min-h-0 w-full",
                    div { class: "flex-auto min-w-0 flex flex-col",
                        div { class: "paper", id: "paper",
                    class: if writing.handles.drag.read().is_some_and(|d| d.dragging) { "dragging-block" },
                    onpointerdown: move |e| writing.pointer_down(e),
                    if let Some(y) = writing.handles.drag.read().filter(|d| d.dragging).and_then(|d| d.indicator_y) {
                        div { id: "block-drop-line", class: "block-drop-line", aria_hidden: "true", style: "top:{y:.1}px;",
                            span { class: "block-drop-dot" }
                        }
                    }
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
                    grid::EmptyLineBar { writing, field: Field::TitleGap }
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
                    for i in 0..=landmarks.len() {
                        if document::gap(&markdown, body_start, &landmarks, i).is_some() {
                            grid::Prose { key: "prose-{i}", writing, index: i, segmented: !landmarks.is_empty() }
                        }
                        if let Some(landmark) = landmarks.get(i) {
                            match landmark.kind {
                                LandmarkKind::Heading { .. } => rsx! { grid::Heading { key: "heading-{i}", writing, field: Field::Heading(i) } },
                                LandmarkKind::Table(t) => rsx! { grid::Grid { key: "table-{t}", writing, index: t } },
                            }
                        }
                    }
                }
                link_card::LinkCard { writing }
                }
                    }
                    crate::comments::CommentsPane { writing }
                }
                formatting::FormatBar { writing }
                slash::SlashMenu { writing }
                if desktop { marker_view::WindowLayout { writing } }
            }
            footer { class: "flex items-center gap-2 min-h-8 mt-auto py-[3px] border-0 border-t border-solid border-line text-muted type-hint max-[650px]:gap-[5px] {statusbar_pad}",
                if !store_status.read().is_empty() { span { id: "save-status", class: "text-muted font-sans text-[11px] leading-[normal]", "{store_status}" } }
                span { class: "text-muted", {
                    let words = markdown.split_whitespace().filter(|w| w.chars().any(char::is_alphanumeric)).count();
                    format!("{words} {}", if words == 1 { "word" } else { "words" })
                } }
                if !source() && hint_at.is_some() {
                    span { id: "marker-hint", class: "pl-2 border-0 border-l border-solid border-line", "{hint}" }
                    ui::Button { id: action_id.to_owned(), variant: ui::Variant::Link, label: action, onclick: move |_| {
                        if raw { writing.keep_literal(); } else { writing.toggle_literal(); }
                    } }
                } else if !source() && writing.parked_selection.read().is_some() {
                    span { class: "pl-2 border-0 border-l border-solid border-line", "Press Enter or type to edit" }
                }
                span { class: "ml-auto flex items-center gap-2 type-mono-ui text-muted",
                    if let Some([ln, col]) = (writing.caret_line_col)() {
                        span { "Ln {ln}, Col {col}" }
                        span { class: "w-px h-[11px] bg-line", aria_hidden: "true" }
                    }
                    span { {
                        let count = markdown.bytes().filter(|c| *c == b'\n').count() + 1;
                        format!("{count} {}", if count == 1 { "line" } else { "lines" })
                    } }
                }
            }
            if !message.read().is_empty() { p { class: "my-2 mx-7 text-alert type-ui", "{message}" } }
        }
    }
}
