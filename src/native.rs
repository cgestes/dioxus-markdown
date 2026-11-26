#[path = "native/blocks.rs"]
mod blocks;
#[path = "native/commands.rs"]
mod commands;
#[path = "native/comments.rs"]
mod comments;
#[path = "native/document.rs"]
mod document;
#[path = "native/formatting.rs"]
mod formatting;
#[path = "native/grid.rs"]
mod grid;
#[path = "native/inspector.rs"]
mod inspector;
#[path = "native/link_card.rs"]
mod link_card;
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
#[path = "native/page.rs"]
mod page;
#[path = "native/personal.rs"]
mod personal;
#[path = "native/pointer.rs"]
mod pointer;
#[path = "native/projection.rs"]
mod projection;
#[path = "native/restart.rs"]
mod restart;
#[path = "native/selection.rs"]
mod selection_model;
#[path = "native/slash.rs"]
mod slash;
#[path = "native/split.rs"]
mod split;
#[path = "native/topics.rs"]
mod topics;
#[path = "native/topics_cli.rs"]
mod topics_cli;
#[path = "native/topics_demo.rs"]
mod topics_demo;
#[path = "native/topics_live.rs"]
mod topics_live;
#[path = "native/topics_view.rs"]
mod topics_view;
#[path = "native/typefaces.rs"]
mod typefaces;
#[path = "native/ui/mod.rs"]
mod ui;
#[path = "native/worker.rs"]
mod worker;
use margin_editor_core::table;
#[path = "native/writing.rs"]
mod writing;

use dioxus_native::{NodeHandle, prelude::*};
use document::{BlockKind, Sections};
use ui::Icon;
use margin_editor_core::checkbox::markdown_markers;
use margin_editor_core::note::Note;
use writing::Writing;

const SAMPLE: &str = "# Field notes\n\nRecorder packed [ ]. Rain cover ready [x].\n\n## Listening stops\n\n| Stop | Listen for | Take |\n| :--- | :--- | ---: |\n| North lock | Water through the gate | 45 s |\n| Footbridge | Steps on the steel deck | 30 s |\n\n- [ ] Leave **ten seconds** of silence.\n- [x] Set the recorder to `48 kHz`.";
/// The editor's own styles; tokens and components come from `ui::CSS`.
const CSS: &str = concat!(
    include_str!("native/editor.css"),
    "\n",
    include_str!("native/split.css"),
    "\n",
    include_str!("native/inspector.css"),
    "\n",
    include_str!("native/comments.css"),
);

/// `margin ui` (or `margin --gallery`) shows the design system's gallery.
fn gallery_requested() -> bool {
    std::env::args().skip(1).any(|a| a == "ui" || a == "--gallery")
}

fn main() {
    if let Some(code) = topics_cli::cli(&std::env::args().skip(1).collect::<Vec<_>>()) {
        std::process::exit(code);
    }
    let (width, height) = if topics::Start::from_args().is_some() {
        (1320.0, 860.0)
    } else if gallery_requested() {
        (1180.0, 900.0)
    } else {
        (920.0, 650.0)
    };
    let window = dioxus_native::WindowAttributes::default()
        .with_title("Margin")
        .with_surface_size(dioxus_native::LogicalSize::new(width, height));
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

#[derive(Clone, Copy, Default, PartialEq)]
enum View {
    #[default]
    Live,
    Split,
    Source,
}

/// Which comments a notebook starts with, and who answers them.
#[derive(Clone, Default, PartialEq)]
enum Comments {
    /// None yet; the local stand-in answers.
    #[default]
    Empty,
    /// Two sample threads on the sample note; the stand-in answers.
    Sample,
    /// The page's agent answers (see `comments::CommentAgent`).
    Agent(comments::CommentAgent),
}

/// How a notebook opens.
#[derive(Clone, Default, PartialEq)]
struct NotebookOptions {
    view: View,
    /// Show the debug inspector from the start.
    inspect: bool,
    comments: Comments,
    /// The page of the space this notebook edits: it then loads and saves
    /// the page's comment file, and its bar has Save.
    page: Option<page::PageSource>,
    /// Set when another window hosts the editor (the Topics window): the
    /// editor's bar then starts with the host's way back and ends with its
    /// actions, and the page aligns with the host's column.
    host: Option<HostBar>,
}

/// The host's part of an embedded editor's bar.
#[derive(Clone, PartialEq)]
struct HostBar {
    back: String,
    onback: EventHandler<()>,
    /// Where the page is, after the back button: `house-systems.md`.
    crumb: String,
    actions: Vec<HostAction>,
}

#[derive(Clone, PartialEq)]
struct HostAction {
    label: String,
    icon: Icon,
    onclick: EventHandler<()>,
}

fn app() -> Element {
    if let Some(start) = topics::Start::from_args() {
        return rsx! { topics::Shell { start } };
    }
    if gallery_requested() {
        return rsx! { ui::gallery::Gallery {} };
    }
    if std::env::args().any(|arg| arg == "--test-window") {
        return notebook_view(
            "# Input check\n\nabcd\nefgh\nijkl\nmnop\n\n| A | B |\n| --- | --- |\n| one | two |\n| three | four |",
            NotebookOptions { inspect: true, ..Default::default() },
        );
    }
    let split = std::env::args().any(|arg| arg == "--split");
    let inspect = std::env::args().any(|arg| arg == "--test-editor");
    if let Some(path) = sync_file_arg() {
        if std::env::args_os().any(|arg| arg == "--vault" || arg == "--note" || arg == "--passphrase-file") {
            return rsx! { p { role: "alert", "Open the sync file and the personal vault in separate windows." } };
        }
        return rsx! { SyncFileEditor { path, split, inspect } };
    }
    if inspect || std::env::args().any(|arg| arg == "--demo") {
        return rsx! { SampleEditor { split, inspect } };
    }
    rsx! { personal::Desktop { split } }
}

/// The sample note's options: sample comment threads unless inspecting.
fn sample_options(split: bool, inspect: bool) -> NotebookOptions {
    NotebookOptions {
        view: if split { View::Split } else { View::Live },
        inspect,
        comments: if inspect { Comments::Empty } else { Comments::Sample },
        ..Default::default()
    }
}

#[component]
fn SampleEditor(split: bool, inspect: bool) -> Element {
    notebook_view(SAMPLE, sample_options(split, inspect))
}

#[cfg(test)]
fn notebook(initial: &'static str) -> Element {
    notebook_view(initial, NotebookOptions::default())
}

fn sync_file_arg() -> Option<std::path::PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--sync-file" {
            return args.next().map(std::path::PathBuf::from);
        }
        if let Some(path) = arg.strip_prefix("--sync-file=") {
            return Some(std::path::PathBuf::from(path));
        }
    }
    None
}

#[derive(Clone)]
struct SyncFile {
    path: std::path::PathBuf,
    state: std::rc::Rc<margin_editor_core::sync::NoteSync>,
}

#[component]
fn SyncFileEditor(path: std::path::PathBuf, split: bool, inspect: bool) -> Element {
    use margin_editor_core::sync::NoteSync;
    let loaded = use_hook(|| {
        let sync = match NoteSync::load_from_file(&path) {
            Ok(sync) => sync,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => NoteSync::new(Note::new(SAMPLE), 1),
            Err(e) => return Err(e.to_string()),
        };
        Ok(SyncFile { path, state: std::rc::Rc::new(sync) })
    });
    let file = match loaded {
        Ok(file) => file,
        Err(error) => return rsx! { p { role: "alert", "Couldn’t open the sync file. {error}" } },
    };
    use_context_provider(|| file);
    notebook_view(SAMPLE, sample_options(split, inspect))
}

fn notebook_view(initial: &str, options: NotebookOptions) -> Element {
    let NotebookOptions { view: initial_view, inspect: inspect_at_start, comments: start_comments, page: page_source, host } = options;
    let embedded = host.is_some();
    let seed_comments = start_comments == Comments::Sample;
    let agent = match start_comments {
        Comments::Agent(agent) => agent,
        _ => comments::CommentAgent::mock(),
    };
    let personal = use_signal(try_consume_context::<personal::Session>);
    let menu = use_hook(try_consume_context::<personal::Menu>);
    let sync_file = use_hook(try_consume_context::<SyncFile>);
    let note = use_signal(|| {
        if let Some(s) = personal.peek().as_ref() {
            Note::from_doc(s.doc.clone())
        } else if let Some(file) = &sync_file {
            Note::from_doc(file.state.note().document().clone())
        } else {
            Note::new(initial)
        }
    });
    let vault_status = personal::use_persistence(personal.peek().clone(), note);
    let comments = use_signal(move || {
        if let Some(s) = personal.peek().as_ref() {
            s.threads.clone()
        } else if seed_comments {
            comments::seed_sample_comments(&note.peek())
        } else {
            Vec::new()
        }
    });
    let mut comments_open = use_signal(move || seed_comments && initial_view == View::Live);
    let active_comment = use_signal(move || {
        if seed_comments {
            comments.peek().iter().find(|t| !t.file.resolved).map(|t| t.id)
        } else {
            None
        }
    });
    let draft_comment = use_signal(|| None);
    let mut view = use_signal(|| initial_view);
    let source = move || view() != View::Live;
    let message = use_signal(String::new);
    let prior_discussion = use_hook(|| {
        std::rc::Rc::new(std::cell::RefCell::new(
            serde_json::to_vec(&*comments.peek()).unwrap_or_default(),
        ))
    });
    use_effect(move || {
        if let Some(s) = personal.peek().as_ref() {
            let mut prior = prior_discussion.borrow_mut();
            let result = serde_json::to_vec(&*comments.read())
                .map_err(anyhow::Error::from)
                .and_then(|bytes| {
                    if *prior != bytes {
                        personal::save_discussion(&s.vault, &s.room, &prior, &bytes)?;
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
    let initial_field = match initial_view {
        View::Live if inspect_at_start => Some(Field::Body(0)),
        View::Split => Some(Field::Source),
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
    use_effect(move || {
        if let Some(file) = &sync_file {
            let _ = doc_rev();
            if let Err(error) = file.state.save_to_file(&file.path) {
                let mut status = message;
                status.set(format!("Couldn’t save the sync file. {error}"));
            }
        }
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
        personal,
        note,
        root,
        focus,
        message,
        contents,
        doc_rev,
        markers: index,
        draft_table,
        tables,
        blocks: use_memo(move || document::blocks(&contents.read(), &tables.read())),
        logical_blocks: use_memo(move || blocks::split_blocks(&contents.read())),
        goal_x: use_signal(|| None),
        span: use_signal(|| None),
        cells: use_signal(|| None),
        cell_editing: use_signal(|| false),
        table_menu: use_signal(|| None),
        quiet_cell: use_signal(|| None),
        selected_text: use_signal(|| None),
        format_position: use_signal(|| None),
        drag: use_signal(|| None),
        pressed_box: use_signal(|| None),
        mouse_box: use_signal(|| None),
        edit_mode: use_signal(move || if embedded { projection::EditMode::Rich } else { projection::EditMode::Markdown }),
        cursor_source: use_signal(|| None),
        parsed: use_memo(move || projection::parse_markdown(&contents.read())),
        dom_dirty: use_signal(|| true),
        last_tracked: use_signal(|| None),
        empty_line: use_signal(|| None),
        slash_menu: use_signal(|| None),
        slash_selected: use_signal(|| 0),
        link_card: use_signal(|| None),
        link_dismissed: use_signal(|| None),
        link_editing: use_signal(|| false),
        block_menu: use_signal(|| None),
        block_drag: use_signal(|| None),
        heights: use_signal(Vec::new),
        heading_heights: use_signal(Vec::new),
        caret_line_col: use_signal(|| None),
        syntax: use_memo(move || markdown::syntax(&contents.read())),
        ink: use_signal(Vec::new),
        callout_boxes: use_signal(Vec::new),
        code_boxes: use_signal(Vec::new),
        callout_menu: use_signal(|| None),
        caret: use_signal(|| None),
        active: use_signal(|| initial_field.filter(|f| *f != Field::Source)),
        body: use_signal(|| None),
        parked_selection: use_signal(|| None),
        marker: use_signal(|| None),
        boxes: use_signal(Vec::new),
        literal: use_signal(|| None),
        comments,
        comments_open,
        active_comment,
        draft_comment,
        comment_agent: use_signal(move || agent),
        comment_file: use_signal(comments::CommentFile::default),
        comment_stream: use_signal(Default::default),
        comment_ranges: use_memo(move || {
            let md = contents.read();
            comments::ranges(&note.read(), &md, &comments.read())
        }),
        page_state: use_signal({
            let etag = page_source.as_ref().and_then(|p| p.etag.clone());
            move || page::PageState { etag, saved_rev: *doc_rev.peek(), ..Default::default() }
        }),
        page: use_signal(move || page_source),
    };
    #[cfg(test)]
    use_context_provider(|| writing);
    comments::use_comment_file(writing);
    let mut inspecting = use_signal(|| inspect_at_start);
    let mut trace =
        use_signal(|| inspector::Trace::new(if inspect_at_start { initial } else { "" }));
    let window =
        use_hook(try_consume_context::<std::sync::Arc<dyn dioxus_native::winit::window::Window>>);
    let window_title = use_memo(move || {
        if personal.peek().is_some() {
            return "Margin · Personal vault".to_owned();
        }
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
            if personal.peek().is_none() && inspecting() && view() == View::Live {
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
        writing.boxes.read();
        writing.format_position.read();
        writing.empty_line.read();
        writing.slash_menu.read();
        writing.link_card.read();
        writing.link_editing.read();
        writing.block_menu.read();
        writing.block_drag.read();
        writing.table_menu.read();
        writing.cells.read();
        writing.span.read();
        writing.edit_mode.read();
        writing.comments.read();
        writing.comments_open.read();
        writing.active_comment.read();
        writing.draft_comment.read();
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
    let mut toggle_debug = move || {
        if source() {
            change_view(View::Live);
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
    rsx! {
        style { "{ui::CSS}" }
        style { "{CSS}" }
        main { class: "notebook", class: if embedded { "embedded" }, class: if view()==View::Split { "split-view" }, class: if is_rich { "rich-mode" }, class: if inspecting() && !source() { "inspecting" }, class: if writing.block_drag.read().is_some_and(|d| d.dragging) { "dragging-block" },
            onpointerdown: move |event| { writing.clear_goal(); writing.dismiss_menu_at(event.client_coordinates().x,event.client_coordinates().y); },
            onpointermove: move |e| writing.pointer_move(e),
            onpointerup: move |_| writing.end_drag(),
            onpointercancel: move |_| writing.end_drag(),
            oninput: move |_| reveal_input.set(true),
            onmounted: move |event| root.set(event.downcast::<NodeHandle>().cloned()),
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
            div { id: "mode-bar", class: "mode-dock",
                nav { class: "mode-pill-bar", aria_label: "Editor tools",
                    if let Some(personal::Menu(mut open)) = menu {
                        ui::Button { id: "vault-notes", label: "Notes", text: "Notes", expanded: open(), onclick: move |_| { let visible = open(); open.set(!visible); } }
                    }
                    if let Some(h) = host.as_ref() {
                        div { class: "dock-host",
                            ui::Button { id: "doc-back", label: h.back.clone(), text: h.back.clone(), icon: Icon::Back, variant: ui::Variant::Ghost, onclick: { let back = h.onback; move |_| back.call(()) } }
                            span { class: "dock-crumb", "{h.crumb}" }
                        }
                        span { class: "dock-spacer" }
                    }
                    ui::Segmented { label: "Markdown view",
                        items: vec![
                            ui::Segment { id: "rich-mode", label: "Rich", tip: Some("Rich editing (delimiters stay hidden)"), selected: is_rich },
                            ui::Segment { id: "live-mode", label: "Live", tip: Some("Keyboard-first Markdown live editing"), selected: is_md_live },
                            ui::Segment { id: "split-mode", label: "Split", tip: None, selected: view() == View::Split },
                            ui::Segment { id: "source-mode", label: "Source", tip: None, selected: view() == View::Source },
                        ],
                        onselect: move |i: usize| match i {
                            0 => set_edit_mode(projection::EditMode::Rich),
                            1 => set_edit_mode(projection::EditMode::Markdown),
                            2 => change_view(View::Split),
                            _ => change_view(View::Source),
                        },
                    }
                    ui::Button { id: "insert-table", variant: ui::Variant::Icon, label: "Insert table", tip: "Insert Markdown table", icon: Icon::Table, onclick: move |_| writing.insert_table(source()) }
                    if newer_build() {
                        ui::Button { id: "restart-app", label: "Restart", tip: "A newer build is ready. Restart Margin to run it",
                            class: "restart-app", onclick: move |_| restart::restart_app() }
                    }
                    div { class: "history-tools", role: "group", aria_label: "History",
                        ui::Button { id: "undo", variant: ui::Variant::Icon, label: "Undo", tip: undo_tip, icon: Icon::Undo, disabled: !note.read().can_undo(), onclick: move |_| writing.history(source(), false) }
                        ui::Button { id: "redo", variant: ui::Variant::Icon, label: "Redo", tip: redo_tip, icon: Icon::Redo, disabled: !note.read().can_redo(), onclick: move |_| writing.history(source(), true) }
                    }
                    {
                        let open_comments = writing.comments.read().iter().filter(|t| !t.file.resolved).count();
                        let panel_open = (writing.comments_open)() && !source();
                        let tip = if cfg!(target_os = "macos") {
                            "Toggle comments (⌘⇧M to comment on selection)"
                        } else {
                            "Toggle comments (Ctrl+Shift+M to comment on selection)"
                        };
                        rsx! {
                            span { class: "dock-comments",
                                ui::Button { id: "toggle-comments", variant: ui::Variant::Icon, label: "Comments", tip, icon: Icon::Comment,
                                    selected: panel_open,
                                    onclick: move |_| {
                                        if source() {
                                            change_view(View::Live);
                                            comments_open.set(true);
                                        } else {
                                            let next = !*comments_open.peek();
                                            comments_open.set(next);
                                        }
                                    },
                                }
                                if open_comments > 0 {
                                    span { class: "dock-comments-dot", aria_hidden: "true" }
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
                div { class: "inspector-strip",
                    inspector::Inspector { writing, trace, desktop }
                }
            }
            if source() {
                split::DocumentEditor { writing, split: view()==View::Split }
            } else {
                div { class: "editor-shell",
                    div { class: "editor-main",
                        div { class: "paper", id: "paper",
                    class: if writing.block_drag.read().is_some_and(|d| d.dragging) { "dragging-block" },
                    onpointerdown: move |e| writing.pointer_down(e),
                    if let Some(y) = writing.block_drag.read().filter(|d| d.dragging).and_then(|d| d.indicator_y) {
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
                link_card::LinkCard { writing }
                }
                    }
                    comments::CommentsPane { writing }
                }
                formatting::FormatBar { writing }
                slash::SlashMenu { writing }
                if desktop { marker_view::WindowLayout { writing } }
            }
            if view() != View::Split {
                footer { class: "statusbar",
                    if personal.peek().is_some() { span { class: "vault-status", "{vault_status}" } }
                    span { class: "status-words", {
                        let words = markdown.split_whitespace().filter(|w| w.chars().any(char::is_alphanumeric)).count();
                        format!("{words} {}", if words == 1 { "word" } else { "words" })
                    } }
                    if !source() && hint_at.is_some() {
                        span { id: "marker-hint", "{hint}" }
                        ui::Button { id: action_id.to_owned(), variant: ui::Variant::Link, label: action, onclick: move |_| {
                            if raw { writing.keep_literal(); } else { writing.toggle_literal(); }
                        } }
                    } else if !source() && writing.parked_selection.read().is_some() {
                        span { class: "parked-hint", "Press Enter or type to edit" }
                    }
                    span { class: "status-right",
                        if let Some([ln, col]) = (writing.caret_line_col)() {
                            span { class: "status-pos", "Ln {ln}, Col {col}" }
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
