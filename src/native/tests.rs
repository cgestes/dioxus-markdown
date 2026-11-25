const SAMPLE: &str = "# Field notes\n\nRecorder packed [ ]. Rain cover ready [x].\n\n- [ ] Pack the windscreen\n- [x] Charge the recorder\n\nThe literal example is `[ ]`.";

use super::*;
use blitz_dom::Document;
use blitz_traits::events::{BlitzImeEvent, BlitzKeyEvent, KeyState, MouseEventButtons, UiEvent};
use blitz_traits::shell::Viewport;
use dioxus_native::{DioxusDocument, NodeId};

fn render(doc: &mut DioxusDocument) {
    for _ in 0..8 {
        let changed = doc.poll(None);
        doc.inner.borrow_mut().resolve(0.0);
        if !changed {
            return;
        }
    }
    panic!("editor did not settle");
}

fn editor() -> DioxusDocument {
    let config = dioxus_native::DocumentConfig {
        font_ctx: Some(typefaces::notebook_fonts()),
        viewport: Some(Viewport {
            window_size: (820, 520),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut doc = DioxusDocument::new(VirtualDom::new(|| notebook(SAMPLE)), config);
    doc.initial_build();
    render(&mut doc);
    doc
}

fn id(doc: &DioxusDocument, name: &str) -> NodeId {
    doc.inner
        .borrow()
        .get_element_by_id(name)
        .unwrap_or_else(|| panic!("missing {name}"))
}

fn click(doc: &mut DioxusDocument, name: &str) {
    click_at(doc, name, None);
}

fn click_at(doc: &mut DioxusDocument, name: &str, point: Option<(f32, f32)>) {
    click_with(doc, name, point, Modifiers::empty());
}
fn click_with(
    doc: &mut DioxusDocument,
    name: &str,
    point: Option<(f32, f32)>,
    modifiers: Modifiers,
) {
    let element = id(doc, name);
    let mut pointer = doc
        .inner
        .borrow()
        .get_node(element)
        .unwrap()
        .synthetic_click_event_data(modifiers);
    {
        let dom = doc.inner.borrow();
        let rect = dom.get_client_bounding_rect(element).unwrap();
        let scroll = dom.viewport_scroll();
        let (dx, dy) = point.unwrap_or(((rect.width * 0.5) as f32, (rect.height * 0.5) as f32));
        let x = rect.x as f32 + dx;
        pointer.coords.page_x = x + scroll.x as f32;
        pointer.coords.client_x = x;
        pointer.coords.screen_x = x;
        let y = rect.y as f32 + dy;
        pointer.coords.page_y = y + scroll.y as f32;
        pointer.coords.client_y = y;
        pointer.coords.screen_y = y;
    }
    doc.handle_ui_event(UiEvent::PointerMove(pointer.clone()));
    render(doc);
    pointer.buttons = MouseEventButtons::Primary;
    doc.handle_ui_event(UiEvent::PointerDown(pointer.clone()));
    pointer.buttons = MouseEventButtons::None;
    doc.handle_ui_event(UiEvent::PointerUp(pointer));
    render(doc);
}

fn select(doc: &DioxusDocument, name: &str, [anchor, focus]: ByteSelection) {
    let id = id(doc, name);
    doc.inner
        .borrow_mut()
        .with_text_input(id, |mut e| e.select_byte_range(anchor, focus));
}

fn assert_focus(doc: &DioxusDocument, name: &str, offsets: ByteSelection) {
    let id = id(doc, name);
    let dom = doc.inner.borrow();
    assert_eq!(dom.get_focussed_node_id(), Some(id), "focus in {name}");
    let input = dom
        .get_node(id)
        .unwrap()
        .element_data()
        .unwrap()
        .text_input_data()
        .unwrap();
    let selected = input.editor.raw_selection();
    assert_eq!(
        [selected.anchor().index(), selected.focus().index()],
        offsets,
        "selection in {name}"
    );
}

fn send_key(doc: &mut DioxusDocument, key: Key, state: KeyState, repeat: bool) {
    let text = match &key {
        Key::Character(s) => Some(s.as_str().into()),
        _ => None,
    };
    let event = BlitzKeyEvent {
        key,
        code: Code::Unidentified,
        modifiers: Modifiers::empty(),
        location: Location::Standard,
        is_auto_repeating: repeat,
        is_composing: false,
        state,
        text,
    };
    doc.handle_ui_event(match state {
        KeyState::Pressed => UiEvent::KeyDown(event),
        KeyState::Released => UiEvent::KeyUp(event),
    });
    render(doc);
}

fn press(doc: &mut DioxusDocument, key: Key, repeat: bool) {
    send_key(doc, key, KeyState::Pressed, repeat);
}

fn backspace_text(doc: &mut DioxusDocument) {
    #[cfg(target_os = "macos")]
    {
        doc.handle_ui_event(UiEvent::AppleStandardKeybinding("deleteBackward:".into()));
        render(doc);
    }
    press(doc, Key::Named(NamedKey::Backspace), false);
}

fn field(doc: &DioxusDocument, id: NodeId) -> String {
    doc.inner
        .borrow()
        .get_node(id)
        .unwrap()
        .element_data()
        .unwrap()
        .text_input_data()
        .unwrap()
        .editor
        .raw_text()
        .to_owned()
}

fn move_body(doc: &mut DioxusDocument, range: ByteSelection) {
    select(doc, "note-body", range);
    send_key(doc, Key::Named(NamedKey::Shift), KeyState::Released, false);
}

fn glyph_position(doc: &DioxusDocument, name: &str, index: usize) -> (f32, f32) {
    let dom = doc.inner.borrow();
    let node = dom.get_node(id(doc, name)).unwrap();
    let field = node.element_data().unwrap().text_input_data().unwrap();
    let layout = field.editor.try_layout().unwrap();
    let ch = parley::Cluster::from_byte_index(layout, index).unwrap();
    let line = ch.line();
    let metrics = line.metrics();
    (
        ch.visual_offset().unwrap() / layout.scale(),
        (metrics.block_min_coord + metrics.block_max_coord) * 0.5 / layout.scale()
            - field.scroll_offset,
    )
}

fn markdown(doc: &DioxusDocument) -> String {
    doc.vdom
        .in_scope(ScopeId::APP, || consume_context::<Writing>().note.peek().markdown())
}

fn body_display(doc: &DioxusDocument, source_offset: usize) -> usize {
    doc.vdom.in_scope(ScopeId::APP, || {
        consume_context::<Writing>()
            .projection(Field::Body(0))
            .source_to_display(source_offset)
    })
}

fn open_body_at(doc: &mut DioxusDocument, source_sel: ByteSelection) {
    doc.vdom.in_scope(ScopeId::APP, || {
        let w = consume_context::<Writing>();
        w.open(Field::Body(0), source_sel);
    });
    render(doc);
}

#[test]
fn native_key_dispatch_toggles_both_kinds_and_edits_literal_text_in_place() {
    let mut doc = editor();
    click(&mut doc, "note-title");
    press(&mut doc, Key::Named(NamedKey::Tab), false);
    for open in [SAMPLE.find("[ ]").unwrap(), SAMPLE.find("- [ ]").unwrap() + 2] {
        open_body_at(&mut doc, [open + 1; 2]);
        press(&mut doc, Key::Character(" ".into()), false);
        assert_eq!(&markdown(&doc)[open..open + 3], "[x]");
        press(&mut doc, Key::Character("x".into()), true);
        press(&mut doc, Key::Character("q".into()), false);
        assert_eq!(&markdown(&doc)[open..open + 3], "[x]");
        press(&mut doc, Key::Character("x".into()), false);
        assert_eq!(markdown(&doc), SAMPLE);
    }
    let open = SAMPLE.find("[ ]").unwrap();
    open_body_at(&mut doc, [open + 1; 2]);
    press(&mut doc, Key::Named(NamedKey::F2), false);
    press(&mut doc, Key::Character("q".into()), false);
    press(&mut doc, Key::Character("r".into()), false);
    press(&mut doc, Key::Named(NamedKey::Escape), false);
    assert_eq!(markdown(&doc), SAMPLE.replacen("[ ]", "[qr ]", 1));
    assert!(
        doc.inner
            .borrow()
            .get_element_by_id(&format!("checkbox-{open}"))
            .is_none()
    );
}

#[test]
fn edit_as_text_returns_focus_and_replaces_the_selected_character() {
    let mut doc = editor();
    let open = SAMPLE.find("[ ]").unwrap();
    click(&mut doc, "note-title");
    press(&mut doc, Key::Named(NamedKey::Tab), false);
    open_body_at(&mut doc, [open + 2, open + 1]);
    let disp = [body_display(&doc, open + 2), body_display(&doc, open + 1)];
    click(&mut doc, "edit-as-text");
    assert_focus(&doc, "note-body", disp);
    press(&mut doc, Key::Character("é".into()), false);
    assert!(markdown(&doc).contains("[é]"));
    click(&mut doc, "source-mode");
    assert_focus(&doc, "markdown", [open + 3; 2]);
    assert_eq!(value(&doc, "markdown"), SAMPLE.replacen("[ ]", "[é]", 1));
}
#[test]
fn source_and_live_restore_both_selection_ends_after_multibyte_text() {
    let mut doc = editor();
    click(&mut doc, "source-mode");
    assert_focus(&doc, "markdown", [0, 0]);
    let prefix = "Écho 🎤 ";
    doc.handle_ui_event(UiEvent::Ime(BlitzImeEvent::Preedit(
        "É".into(),
        Some((2, 2)),
    )));
    render(&mut doc);
    // Winit clears preedit immediately before committing the completed text.
    doc.handle_ui_event(UiEvent::Ime(BlitzImeEvent::Preedit(String::new(), None)));
    doc.handle_ui_event(UiEvent::Ime(BlitzImeEvent::Commit(prefix.into())));
    render(&mut doc);
    let markdown = format!("{prefix}{SAMPLE}");
    assert_eq!(field(&doc, id(&doc, "markdown")), markdown);
    for open in [
        markdown.find("[ ]").unwrap(),
        markdown.find("- [ ]").unwrap() + 2,
    ] {
        let range = [open + 2, open + 1];
        select(&doc, "markdown", range);
        click(&mut doc, "live-mode");
        let disp = range.map(|p| body_display(&doc, p));
        assert_focus(&doc, "note-body", disp);
        press(&mut doc, Key::Character("x".into()), false);
        assert_focus(&doc, "note-body", disp);
        click(&mut doc, "source-mode");
        assert_focus(&doc, "markdown", range);
        assert_eq!(&field(&doc, id(&doc, "markdown"))[open..open + 3], "[x]");
    }
    select(&doc, "markdown", [prefix.len(), 0]);
    click(&mut doc, "live-mode");
    click(&mut doc, "source-mode");
    assert_focus(&doc, "markdown", [prefix.len(), 0]);
}
#[test]
fn toolbar_commands_work_by_keyboard() {
    let mut doc = editor();
    assert!(doc.inner.borrow().get_element_by_id("workspace-bar").is_none());
    let mode_bar = bounds(&doc, "mode-bar");
    let title = bounds(&doc, "note-title");
    assert!(mode_bar[1] <= 1.0 && mode_bar[1] + mode_bar[3] <= title[1]);
    // Debug display toggle turns inspector mode on and off cleanly.
    assert!(doc.inner.borrow().get_element_by_id("save-inspection").is_none());
    click(&mut doc, "toggle-debug");
    assert!(doc.inner.borrow().get_element_by_id("save-inspection").is_some());
    click(&mut doc, "toggle-debug");
    assert!(doc.inner.borrow().get_element_by_id("save-inspection").is_none());
    let rich = id(&doc, "rich-mode");
    doc.inner.borrow_mut().set_focus_to(rich);
    assert_eq!(doc.inner.borrow().get_focussed_node_id(), Some(rich));
    press(&mut doc, Key::Named(NamedKey::Tab), false);
    assert_eq!(
        doc.inner.borrow().get_focussed_node_id(),
        Some(id(&doc, "live-mode"))
    );
    press(&mut doc, Key::Named(NamedKey::Tab), false);
    assert_eq!(
        doc.inner.borrow().get_focussed_node_id(),
        Some(id(&doc, "split-mode"))
    );
    press(&mut doc, Key::Named(NamedKey::Tab), false);
    assert_eq!(
        doc.inner.borrow().get_focussed_node_id(),
        Some(id(&doc, "source-mode"))
    );
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert_focus(&doc, "markdown", [0, 0]);
    let open = SAMPLE.find("[ ]").unwrap();
    select(&doc, "markdown", [open + 1; 2]);
    let live = id(&doc, "live-mode");
    doc.inner.borrow_mut().set_focus_to(live);
    let space = Key::Character(" ".into());
    press(&mut doc, space.clone(), false);
    assert!(doc.inner.borrow().get_element_by_id("markdown").is_some());
    send_key(&mut doc, space, KeyState::Released, false);
    assert_focus(
        &doc,
        "note-body",
        [open + 1 - Sections::read(SAMPLE).body; 2],
    );
}
fn commit_text(doc: &mut DioxusDocument, text: &str) {
    doc.handle_ui_event(UiEvent::Ime(BlitzImeEvent::Commit(text.into())));
    render(doc);
}

fn value(doc: &DioxusDocument, name: &str) -> String {
    field(doc, id(doc, name))
}

#[test]
fn edit_title_and_multiline_body_in_live_and_undo_in_source() {
    let mut doc = editor();
    click(&mut doc, "note-title");
    assert_eq!(value(&doc, "note-title"), "# Field notes");
    select(&doc, "note-title", [2 + "Field notes".len(), 2]);
    commit_text(&mut doc, "Écoutes 🎧");
    assert_eq!(value(&doc, "note-title"), "# Écoutes 🎧");
    press(&mut doc, Key::Named(NamedKey::Tab), false);
    assert_focus(&doc, "note-body", [0; 2]);
    let body_field = id(&doc, "note-body");
    let original = SAMPLE.split_once("\n\n").unwrap().1;
    select(&doc, "note-body", [0, "Recorder packed".len()]);
    commit_text(&mut doc, "At dawn\nOutside");
    let changed = original.replacen("Recorder packed", "At dawn\nOutside", 1);
    assert_eq!(id(&doc, "note-body"), body_field);
    assert_eq!(markdown(&doc), format!("# Écoutes 🎧\n\n{changed}"));
    let selection = ["At dawn\nOutside".len(), 0];
    select(&doc, "note-body", selection);
    click(&mut doc, "source-mode");
    let prefix = "# Écoutes 🎧\n\n";
    assert_eq!(value(&doc, "markdown"), format!("{prefix}{changed}"));
    assert_focus(&doc, "markdown", selection.map(|i| i + prefix.len()));
    click(&mut doc, "live-mode");
    assert_focus(&doc, "note-body", selection);
    click(&mut doc, "source-mode");
    click(&mut doc, "undo");
    assert_eq!(value(&doc, "markdown"), format!("{prefix}{original}"));
    click(&mut doc, "undo");
    assert_eq!(value(&doc, "markdown"), SAMPLE);
    click(&mut doc, "redo");
    click(&mut doc, "redo");
    assert_eq!(value(&doc, "markdown"), format!("{prefix}{changed}"));
}

#[test]
fn click_checklist_label_places_the_caret_at_the_clicked_character() {
    let mut doc = editor();
    let start = SAMPLE.find(" Pack the windscreen").unwrap();
    let disp = body_display(&doc, start);
    let (x, y) = glyph_position(&doc, "note-body", disp);
    click_at(&mut doc, "note-body", Some((x + 0.4, y)));
    press(&mut doc, Key::Character("!".into()), false);
    click(&mut doc, "source-mode");
    let mut expected = SAMPLE.to_owned();
    expected.insert(start, '!');
    assert_eq!(value(&doc, "markdown"), expected);
}

#[test]
fn structured_and_literal_checkboxes_work_while_editing_the_continuous_body() {
    let mut doc = editor();
    click(&mut doc, "note-title");
    press(&mut doc, Key::Named(NamedKey::Tab), false);
    let open = SAMPLE.find("[ ]").unwrap();
    // Selecting both brackets gives ordinary text semantics, even when only
    // the middle byte differs in the replacement.
    for token in ["[x]", "[ ]"] {
        open_body_at(&mut doc, [open, open + 3]);
        commit_text(&mut doc, token);
        assert_eq!(&markdown(&doc)[open..open + 3], token);
    }
    open_body_at(&mut doc, [open + 2, open + 1]);
    press(&mut doc, Key::Character(" ".into()), false);
    press(&mut doc, Key::Character("q".into()), false);
    assert_eq!(markdown(&doc), SAMPLE.replacen("[ ]", "[x]", 1));
    commit_text(&mut doc, "禁止");
    assert_eq!(markdown(&doc), SAMPLE.replacen("[ ]", "[x]", 1));
    click(&mut doc, "edit-as-text");
    commit_text(&mut doc, "é");
    assert_eq!(markdown(&doc), SAMPLE.replacen("[ ]", "[é]", 1));
    click(&mut doc, "keep-literal");
    click(&mut doc, "source-mode");
    assert_eq!(value(&doc, "markdown"), SAMPLE.replacen("[ ]", "\\[é]", 1));
}

#[test]
fn adding_a_title_preserves_unheaded_text_and_existing_markdown_separators() {
    let mut doc = editor();
    click(&mut doc, "source-mode");
    select(&doc, "markdown", [0, SAMPLE.len()]);
    commit_text(&mut doc, "First paragraph");
    select(&doc, "markdown", [1; 2]);
    click(&mut doc, "live-mode");
    assert_eq!(value(&doc, "note-body"), "First paragraph");
    click(&mut doc, "note-title");
    commit_text(&mut doc, "Day one");
    click(&mut doc, "source-mode");
    assert_eq!(value(&doc, "markdown"), "# Day one\n\nFirst paragraph");
    let md = "  #\tOld ##\r\n\r\nBody";
    select(&doc, "markdown", [0, value(&doc, "markdown").len()]);
    commit_text(&mut doc, md);
    let start = md.find("Old").unwrap();
    select(&doc, "markdown", [start + 3, start]);
    click(&mut doc, "live-mode");
    assert_focus(&doc, "note-title", [5, 2]);
    commit_text(&mut doc, "New");
    click(&mut doc, "source-mode");
    assert_eq!(value(&doc, "markdown"), md.replace("Old", "New"));
}
#[test]
fn edit_a_heading_only_note_and_preserve_a_leading_body_newline() {
    let mut doc = editor();
    click(&mut doc, "source-mode");
    select(&doc, "markdown", [0, SAMPLE.len()]);
    commit_text(&mut doc, "#");
    click(&mut doc, "live-mode");
    assert_focus(&doc, "note-title", [1; 2]);
    commit_text(&mut doc, "Morning");
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    commit_text(&mut doc, "First paragraph");
    assert_eq!(value(&doc, "note-body"), "First paragraph");
    click(&mut doc, "source-mode");
    assert_eq!(value(&doc, "markdown"), "# Morning\nFirst paragraph");
    select(&doc, "markdown", [0, value(&doc, "markdown").len()]);
    let compact = "# Morning\nFirst";
    commit_text(&mut doc, compact);
    let start = compact.find("First").unwrap();
    select(&doc, "markdown", [start; 2]);
    click(&mut doc, "live-mode");
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert_eq!(value(&doc, "note-body"), "\nFirst");
    select(&doc, "note-body", [2, 0]);
    press(&mut doc, Key::Named(NamedKey::Escape), false);
    assert_eq!(
        doc.inner.borrow().get_focussed_node_id(),
        Some(id(&doc, "body-surface"))
    );
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert_focus(&doc, "note-body", [2, 0]);
    click(&mut doc, "source-mode");
    assert_eq!(value(&doc, "markdown"), "# Morning\n\nFirst");
    assert_focus(&doc, "markdown", [start + 2, start]);
}

#[test]
fn switching_to_long_body_keeps_the_selected_line_in_view() {
    let mut doc = editor();
    click(&mut doc, "source-mode");
    select(&doc, "markdown", [0, SAMPLE.len()]);
    let body = "Rain on the water.\n".repeat(70);
    let markdown = format!("# Weather\n\n{body}Last line");
    commit_text(&mut doc, &markdown);
    select(&doc, "markdown", [markdown.len(); 2]);
    click(&mut doc, "live-mode");
    assert_focus(&doc, "note-body", [body.len() + "Last line".len(); 2]);
    let id = id(&doc, "note-body");
    let dom = doc.inner.borrow();
    let input = dom
        .get_node(id)
        .unwrap()
        .element_data()
        .unwrap()
        .text_input_data()
        .unwrap();
    assert!(
        input.scroll_offset > 0.0 || dom.viewport_scroll().y > 0.0,
        "long body should scroll caret into view"
    );
}

fn bounds(doc: &DioxusDocument, name: &str) -> [f64; 4] {
    let dom = doc.inner.borrow();
    let r = dom.get_client_bounding_rect(id(doc, name)).unwrap();
    [r.x, r.y, r.width, r.height]
}

#[test]
fn title_prefix_is_edited_inline_in_live_and_hidden_in_rich() {
    let mut doc = editor();
    let body = bounds(&doc, "note-body");
    // At rest (unfocused), heading hashes are collapsed out of the display text.
    assert_eq!(value(&doc, "note-title"), "Field notes");
    // Clicking the heading in Live mode reveals `# ` inline inside the heading textarea.
    click(&mut doc, "note-title");
    assert_eq!(value(&doc, "note-title"), "# Field notes");
    // Caret can move onto and before `#` via ArrowLeft.
    select(&doc, "note-title", [2; 2]);
    press(&mut doc, Key::Named(NamedKey::ArrowLeft), false);
    assert_focus(&doc, "note-title", [1; 2]);
    press(&mut doc, Key::Named(NamedKey::ArrowLeft), false);
    assert_focus(&doc, "note-title", [0; 2]);
    // Typing `#` directly upgrades H1 -> H2 (shrinking the display font size).
    let width = glyph_position(&doc, "note-title", 3).0 - glyph_position(&doc, "note-title", 2).0;
    press(&mut doc, Key::Character("#".into()), false);
    assert_eq!(value(&doc, "note-title"), "## Field notes");
    assert_focus(&doc, "note-title", [1; 2]);
    let h2_width =
        glyph_position(&doc, "note-title", 4).0 - glyph_position(&doc, "note-title", 3).0;
    assert!(h2_width < width, "H2 glyph width should shrink below H1");
    assert_eq!(bounds(&doc, "note-body"), body);
    // In Rich mode, `#` stays hidden even while focused, and `#`/Backspace at 0 adjust level.
    click(&mut doc, "rich-mode");
    click(&mut doc, "note-title");
    assert_eq!(value(&doc, "note-title"), "Field notes");
    select(&doc, "note-title", [0; 2]);
    press(&mut doc, Key::Character("#".into()), false);
    press(&mut doc, Key::Named(NamedKey::Backspace), false);
    assert_focus(&doc, "note-title", [0; 2]);
    click(&mut doc, "source-mode");
    assert_eq!(value(&doc, "markdown"), SAMPLE.replacen('#', "##", 1));
    // Back in Live mode, deleting the `#`s and space demotes the heading into body prose.
    select(&doc, "markdown", [3; 2]);
    click(&mut doc, "live-mode");
    assert_focus(&doc, "note-title", [3; 2]);
    backspace_text(&mut doc);
    backspace_text(&mut doc);
    backspace_text(&mut doc);
    assert_focus(&doc, "note-body", [0; 2]);
    assert!(value(&doc, "note-body").starts_with("Field notes"));
}

#[test]
fn resting_checkboxes_reveal_only_for_keyboard_cursor_while_mouse_keeps_togglebox() {
    let mut doc = editor();
    let body = format!(
        "{}🎧 [ ] rain follows [x] here.\n\n- [ ] next",
        "Listen to the bridge. ".repeat(7)
    );
    let full = format!("# Log\n\n{body}");
    open_markdown(&mut doc, &full, 7);
    let open = full.find("[ ]").unwrap();
    let other = full.find("[x]").unwrap();
    let first_id = format!("checkbox-{open}");
    let other_id = format!("checkbox-{other}");
    let shown = |d: &DioxusDocument, name: &str| d.inner.borrow().get_element_by_id(name).is_some();
    assert!(shown(&doc, &first_id));
    assert!(shown(&doc, &other_id));

    // Keyboard cursor reveals only the adjacent checkbox.
    for pos in [open - 1, open, open + 1, open + 2, open + 3, open + 4, other + 1] {
        open_body_at(&mut doc, [pos; 2]);
        assert_eq!(
            shown(&doc, &first_id),
            !(open..=open + 3).contains(&pos),
            "first token at {pos}"
        );
        assert_eq!(
            shown(&doc, &other_id),
            !(other..=other + 3).contains(&pos),
            "second token at {pos}"
        );
    }

    // Mouse-clicking a resting checkbox toggles it while keeping the togglebox visible!
    open_body_at(&mut doc, [7; 2]);
    assert!(shown(&doc, &first_id));
    let box_rect = bounds(&doc, &first_id);
    let body_rect = bounds(&doc, "note-body");
    let click_pt = (
        (box_rect[0] - body_rect[0] + box_rect[2] * 0.5) as f32,
        (box_rect[1] - body_rect[1] + box_rect[3] * 0.5) as f32,
    );
    // Hovering a checkbox must immediately report a pointer cursor (not I-beam text cursor).
    {
        let mut ptr = doc
            .inner
            .borrow()
            .get_node(id(&doc, "note-body"))
            .unwrap()
            .synthetic_click_event_data(Modifiers::empty());
        ptr.coords.client_x = body_rect[0] as f32 + click_pt.0;
        ptr.coords.client_y = body_rect[1] as f32 + click_pt.1;
        ptr.coords.page_x = ptr.coords.client_x;
        ptr.coords.page_y = ptr.coords.client_y;
        doc.handle_ui_event(UiEvent::PointerMove(ptr));
        render(&mut doc);
        assert_eq!(
            format!("{:?}", doc.inner.borrow().get_cursor()),
            "Some(Pointer)",
            "hovering a checkbox should show a pointer cursor"
        );
    }
    click_at(&mut doc, "note-body", Some(click_pt));
    assert_eq!(&markdown(&doc)[open..open + 3], "[x]");
    assert!(shown(&doc, &first_id), "mouse click keeps togglebox visible");
    // Clicking again toggles it back off cleanly!
    click_at(&mut doc, "note-body", Some(click_pt));
    assert_eq!(&markdown(&doc)[open..open + 3], "[ ]");
    assert!(shown(&doc, &first_id));
}

#[test]
fn vertical_arrows_cross_the_title_boundary_and_follow_soft_wrapped_body_lines() {
    let mut doc = editor();
    let open = SAMPLE.find("[ ]").unwrap();
    assert!(
        doc.inner
            .borrow()
            .get_element_by_id(&format!("checkbox-{open}"))
            .is_some(),
        "checkbox is drawn before any field receives focus"
    );
    click(&mut doc, "note-title");
    select(&doc, "note-title", [5; 2]);
    press(&mut doc, Key::Named(NamedKey::ArrowDown), false);
    assert_focus(&doc, "title-gap", [0; 2]);
    press(&mut doc, Key::Named(NamedKey::ArrowDown), false);
    let body_id = id(&doc, "note-body");
    assert_eq!(doc.inner.borrow().get_focussed_node_id(), Some(body_id));
    let focus_pos = |doc: &DioxusDocument, name: &str| {
        doc.inner
            .borrow()
            .get_node(id(doc, name))
            .unwrap()
            .element_data()
            .unwrap()
            .text_input_data()
            .unwrap()
            .editor
            .raw_selection()
            .focus()
            .index()
    };
    assert!(focus_pos(&doc, "note-body") > 0);
    press(&mut doc, Key::Named(NamedKey::ArrowUp), false);
    assert_focus(&doc, "title-gap", [0; 2]);
    press(&mut doc, Key::Named(NamedKey::ArrowUp), false);
    assert_eq!(
        doc.inner.borrow().get_focussed_node_id(),
        Some(id(&doc, "note-title"))
    );
    assert!(focus_pos(&doc, "note-title").abs_diff(7) <= 1);
    select(&doc, "note-title", [0; 2]);
    press(&mut doc, Key::Named(NamedKey::ArrowLeft), false);
    assert_focus(&doc, "note-title", [0; 2]);
    press(&mut doc, Key::Named(NamedKey::ArrowDown), false);
    assert_focus(&doc, "title-gap", [0; 2]);
    press(&mut doc, Key::Named(NamedKey::ArrowDown), false);
    select(&doc, "note-body", [0, value(&doc, "note-body").len()]);
    commit_text(&mut doc, &"A long wrapped sentence. ".repeat(14));
    let end = value(&doc, "note-body").len() - 1;
    move_body(&mut doc, [end; 2]);
    press(&mut doc, Key::Named(NamedKey::ArrowUp), false);
    assert_eq!(doc.inner.borrow().get_focussed_node_id(), Some(body_id));
    assert!(focus_pos(&doc, "note-body") < end);
}

#[test]
fn editing_and_growing_a_native_grid_keeps_focus_and_round_trips_the_source() {
    let mut doc = editor();
    click(&mut doc, "insert-table");
    assert_focus(&doc, "cell-0-0-0", [0, 8]);
    let stable = id(&doc, "cell-0-0-0");
    commit_text(&mut doc, "Station");
    assert_eq!(id(&doc, "cell-0-0-0"), stable);
    press(&mut doc, Key::Named(NamedKey::Tab), false);
    assert_focus(&doc, "cell-0-0-1", [0, 8]);
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    commit_text(&mut doc, "été|🌿");
    click(&mut doc, "source-mode");
    assert!(value(&doc, "markdown").contains("|  | été\\|🌿 |"));
    click(&mut doc, "live-mode");
    assert_focus(&doc, "cell-0-1-1", ["été|🌿".len(); 2]);
    press(&mut doc, Key::Named(NamedKey::Tab), false);
    assert_focus(&doc, "cell-0-2-0", [0, 0]);
    click(&mut doc, "cell-0-0-1");
    click(&mut doc, "col-menu-btn-0-1");
    click(&mut doc, "col-menu-insert-right-0-1");
    assert_focus(&doc, "cell-0-0-2", [0, 8]);
    click(&mut doc, "undo");
    assert!(doc.inner.borrow().get_element_by_id("cell-0-0-2").is_none());
    click(&mut doc, "cell-0-2-0");
    click(&mut doc, "row-menu-btn-0-2");
    click(&mut doc, "row-menu-insert-below-0-2");
    assert_focus(&doc, "cell-0-3-0", [0; 2]);
    press(&mut doc, Key::Named(NamedKey::Escape), false);
    assert_focus(&doc, "note-body-1", [0; 2]);
    commit_text(&mut doc, "After.");
    click(&mut doc, "source-mode");
    let source = value(&doc, "markdown");
    assert!(source.starts_with(SAMPLE));
    assert!(source.ends_with("|  |  |\n\nAfter."), "{source:?}");
}

#[test]
fn ragged_table_paste_uses_the_native_clipboard_and_one_undo() {
    struct Clipboard;
    impl blitz_traits::shell::ShellProvider for Clipboard {
        fn get_clipboard_text(&self) -> Result<String, blitz_traits::shell::ClipboardError> {
            Ok("x|y\t2\r\n🚲\t4\r\n".into())
        }
    }
    let mut doc = editor();
    let src = "# Tape\r\n\r\nA | B | C\r\n:-- | :-: | --:\r\nleft |\r\n\r\nA paragraph [ ].\r\n\r\n```\r\n| a | b |\r\n| - | - |\r\n```";
    click(&mut doc, "source-mode");
    select(&doc, "markdown", [0, SAMPLE.len()]);
    commit_text(&mut doc, src);
    click(&mut doc, "live-mode");
    assert!(doc.inner.borrow().get_element_by_id("cell-1-0-0").is_none());
    click(&mut doc, "cell-0-1-2");
    doc.inner.borrow_mut().shell_provider = std::sync::Arc::new(Clipboard);
    doc.handle_ui_event(UiEvent::KeyDown(BlitzKeyEvent {
        key: Key::Character("v".into()),
        code: Code::KeyV,
        modifiers: Modifiers::META,
        location: Location::Standard,
        is_auto_repeating: false,
        is_composing: false,
        state: KeyState::Pressed,
        text: None,
    }));
    render(&mut doc);
    assert_eq!(value(&doc, "cell-0-1-2"), "x|y");
    assert_eq!(value(&doc, "cell-0-2-2"), "🚲");
    assert_eq!(value(&doc, "cell-0-2-3"), "4");
    click(&mut doc, "source-mode");
    let changed = value(&doc, "markdown");
    assert!(changed.contains("left |  | x\\|y | 2 |"), "{changed:?}");
    assert!(changed.ends_with(src.split_once("A paragraph").unwrap().1));
    click(&mut doc, "undo");
    assert_eq!(value(&doc, "markdown"), src);
    click(&mut doc, "redo");
    assert_eq!(value(&doc, "markdown"), changed);
}

#[test]
fn classic_markdown_lists_continue_indent_and_leave_fenced_text_literal() {
    let mut doc = editor();
    click(&mut doc, "note-body");
    select(&doc, "note-body", [0, value(&doc, "note-body").len()]);
    commit_text(&mut doc, "- [x] Listen");
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert!(markdown(&doc).ends_with("- [x] Listen\n- [ ] "));
    commit_text(&mut doc, "Wait");
    press(&mut doc, Key::Named(NamedKey::Tab), false);
    assert!(markdown(&doc).ends_with("- [x] Listen\n  - [ ] Wait"));
    let mut text = value(&doc, "note-body");
    select(&doc, "note-body", [0, text.len()]);
    commit_text(&mut doc, "9. Record");
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert_eq!(value(&doc, "note-body"), "9. Record\n10. ");
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    commit_text(&mut doc, "Next");
    assert_eq!(value(&doc, "note-body"), "9. Record\n\nNext");
    text = value(&doc, "note-body");
    select(&doc, "note-body", [0, text.len()]);
    commit_text(&mut doc, "```text\n- literal\n```");
    move_body(&mut doc, ["```text\n- literal".len(); 2]);
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert_eq!(value(&doc, "note-body"), "```text\n- literal\n\n```");
}

#[test]
fn native_markdown_marks_and_tables_keep_their_source_boundaries() {
    let md = "## Écoute\n\n**one** and *two*, ~~lost~~, `code`, [here](https://example.com).\n\n> three\n\n| A | B |\n| :-- | --: |\n| a\\|b | z |\n\nAfter [ ].\n\n| C |\n| --- |\n| D |";
    let mut doc = editor();
    click(&mut doc, "source-mode");
    select(&doc, "markdown", [0, SAMPLE.len()]);
    commit_text(&mut doc, md);
    click(&mut doc, "live-mode");
    assert_eq!(value(&doc, "cell-0-1-0"), "a|b");
    assert_eq!(value(&doc, "cell-1-1-0"), "D");
    assert_eq!(value(&doc, "note-body-1"), "\nAfter   .\n");
    assert!(
        doc.inner
            .borrow()
            .get_element_by_id(&format!("checkbox-{}", md.find("[ ]").unwrap()))
            .is_some()
    );
    let tokens = markdown::syntax(md);
    assert!(tokens.len() >= 7);
    click(&mut doc, "note-body-1");
    select(&doc, "note-body-1", [1, 6]);
    commit_text(&mut doc, "Between");
    click(&mut doc, "source-mode");
    assert_eq!(value(&doc, "markdown"), md.replace("After", "Between"));
}

#[test]
fn typing_a_heading_into_an_empty_title_and_pasting_a_marked_heading() {
    let mut doc = editor();
    click(&mut doc, "source-mode");
    select(&doc, "markdown", [0, SAMPLE.len()]);
    commit_text(&mut doc, "Body only");
    click(&mut doc, "live-mode");
    click(&mut doc, "note-title");
    press(&mut doc, Key::Character("#".into()), false);
    press(&mut doc, Key::Character("#".into()), false);
    press(&mut doc, Key::Character(" ".into()), false);
    commit_text(&mut doc, "Études");
    click(&mut doc, "source-mode");
    assert_eq!(value(&doc, "markdown"), "## Études\n\nBody only");
    click(&mut doc, "live-mode");
    click(&mut doc, "note-title");
    select(&doc, "note-title", [0, "## Études".len()]);
    commit_text(&mut doc, "### Morning");
    assert_eq!(value(&doc, "note-title"), "### Morning");
    assert_focus(&doc, "note-title", ["### Morning".len(); 2]);
    click(&mut doc, "source-mode");
    assert_eq!(value(&doc, "markdown"), "### Morning\n\nBody only");
}

#[test]
fn a_handwritten_pipe_table_accepts_the_complete_alignment_line_before_enter() {
    let mut doc = editor();
    click(&mut doc, "note-body");
    select(&doc, "note-body", [0, value(&doc, "note-body").len()]);
    commit_text(&mut doc, "| Stop | Take |");
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    for ch in "| :--- | ---: |".chars() {
        press(&mut doc, Key::Character(ch.to_string()), false);
    }
    assert_eq!(value(&doc, "note-body"), "| Stop | Take |\n| :--- | ---: |");
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert_focus(&doc, "cell-0-1-0", [0; 2]);
    commit_text(&mut doc, "Lock");
    click(&mut doc, "source-mode");
    assert!(value(&doc, "markdown").ends_with("| Stop | Take |\n| :--- | ---: |\n| Lock |  |"));
}

fn open_markdown(doc: &mut DioxusDocument, md: &str, pos: usize) {
    click(doc, "source-mode");
    select(doc, "markdown", [0, value(doc, "markdown").len()]);
    commit_text(doc, md);
    select(doc, "markdown", [pos; 2]);
    click(doc, "live-mode");
}

#[test]
fn each_heading_uses_its_own_prefix_and_caret_in_document_order() {
    let mut doc = editor();
    let md = "# One\n\nBody\n\n## Two\n\n### Three\n\nLast";
    open_markdown(&mut doc, md, md.find("Two").unwrap());
    assert_focus(&doc, "note-heading-0", [3; 2]);
    // Arrow left onto `##`, add a third `#`, then return to the text to type `É`.
    select(&doc, "note-heading-0", [2; 2]);
    press(&mut doc, Key::Character("#".into()), false);
    select(&doc, "note-heading-0", [4; 2]);
    commit_text(&mut doc, "É");
    assert_eq!(value(&doc, "note-heading-0"), "### ÉTwo");
    click(&mut doc, "note-heading-1");
    assert_eq!(value(&doc, "note-heading-1"), "### Three");
    select(&doc, "note-heading-1", [3; 2]);
    backspace_text(&mut doc);
    select(&doc, "note-heading-1", [3; 2]);
    commit_text(&mut doc, "Day ");
    click(&mut doc, "source-mode");
    assert_eq!(
        value(&doc, "markdown"),
        md.replace("## Two", "### ÉTwo")
            .replace("### Three", "## Day Three")
    );
}

#[test]
fn arrows_visit_each_blank_heading_and_table_row_once_in_both_directions() {
    let mut doc = editor();
    let md = "# One\n\nA\n\n\n## Two\n\n| Place | Time |\n| --- | --- |\n| Lock | Noon |\n\nEnd";
    open_markdown(&mut doc, md, 0);
    let path = [
        ("note-title", 2),
        ("title-gap", 0),
        ("note-body", 0),
        ("note-body", 2),
        ("note-body", 3),
        ("note-heading-0", 3),
        ("note-body-1", 0),
        ("cell-0-0-0", 0),
        ("cell-0-1-0", 0),
        ("note-body-2", 0),
        ("note-body-2", 1),
    ];
    for &(field, at) in &path[1..] {
        press(&mut doc, Key::Named(NamedKey::ArrowDown), false);
        assert_focus(&doc, field, [at; 2]);
    }
    for &(field, at) in path[..path.len() - 1].iter().rev() {
        press(&mut doc, Key::Named(NamedKey::ArrowUp), false);
        assert_focus(&doc, field, [at; 2]);
    }
    click(&mut doc, "source-mode");
    assert_eq!(value(&doc, "markdown"), md);
}

#[test]
fn native_cell_arrows_edit_text_and_cross_all_four_edges() {
    let mut doc = editor();
    let md = "# Top\n\n| A | B |\n| --- | --- |\n| xx | yy |\n\nTail";
    open_markdown(&mut doc, md, md.find("yy").unwrap() + 1);
    assert_focus(&doc, "cell-0-1-1", [1; 2]);
    press(&mut doc, Key::Named(NamedKey::ArrowLeft), false);
    assert_focus(&doc, "cell-0-1-1", [0; 2]);
    press(&mut doc, Key::Named(NamedKey::ArrowLeft), false);
    assert_focus(&doc, "cell-0-1-0", [2; 2]);
    press(&mut doc, Key::Named(NamedKey::ArrowRight), false);
    assert_focus(&doc, "cell-0-1-1", [0; 2]);
    press(&mut doc, Key::Named(NamedKey::ArrowUp), false);
    assert_focus(&doc, "cell-0-0-1", [0; 2]);
    press(&mut doc, Key::Named(NamedKey::ArrowUp), false);
    assert_focus(&doc, "title-gap", [0; 2]);
    press(&mut doc, Key::Named(NamedKey::ArrowDown), false);
    assert_focus(&doc, "cell-0-0-1", [0; 2]);
    press(&mut doc, Key::Named(NamedKey::ArrowDown), false);
    assert_focus(&doc, "cell-0-1-1", [0; 2]);
    press(&mut doc, Key::Named(NamedKey::ArrowDown), false);
    assert_focus(&doc, "note-body-1", [0; 2]);
}

#[test]
fn resting_delimiters_collapse_tightly_and_expand_only_at_cursor() {
    let md = "# Field\n\nA **bold** and *soft* note.\n\n## A *later* heading\n\n| A | B |\n| --- | --- |\n| **cell** | x |";
    let mut doc = editor();
    open_markdown(&mut doc, md, 2);
    // At rest, delimiters are collapsed completely (tight rendering).
    assert_eq!(value(&doc, "note-body"), "A bold and soft note.\n");
    assert_eq!(value(&doc, "note-heading-0"), "A later heading");
    assert_eq!(value(&doc, "cell-0-1-0"), "cell");

    // Moving cursor into **bold** expands ONLY **bold** while *soft* stays tight.
    let inside_bold = md.find("bold").unwrap() + 1;
    open_body_at(&mut doc, [inside_bold; 2]);
    assert_eq!(value(&doc, "note-body"), "A **bold** and soft note.\n");

    // Moving cursor away collapses **bold** back to tight rendering.
    open_body_at(&mut doc, [md.find("A **").unwrap(); 2]);
    assert_eq!(value(&doc, "note-body"), "A bold and soft note.\n");

    // In Rich mode, delimiters stay collapsed even when the cursor is inside **bold**.
    click(&mut doc, "rich-mode");
    open_body_at(&mut doc, [inside_bold; 2]);
    assert_eq!(value(&doc, "note-body"), "A bold and soft note.\n");
}

#[test]
fn grid_growth_controls_occupy_the_right_and_bottom_edges_only_on_engagement() {
    let mut doc = editor();
    let md = "# Note\n\n| A | B |\n| --- | --- |\n| one | two |\n\nBody";
    open_markdown(&mut doc, md, 2);
    click(&mut doc, "note-title");
    // No floating toolbar or edge + buttons; column/row menus appear only when engaged.
    assert!(doc.inner.borrow().get_element_by_id("add-column-0").is_none());
    assert!(doc.inner.borrow().get_element_by_id("add-row-0").is_none());
    assert!(doc.inner.borrow().get_element_by_id("move-col-left-0").is_none());
    assert!(doc.inner.borrow().get_element_by_id("col-menu-btn-0-0").is_none());
    assert!(doc.inner.borrow().get_element_by_id("row-menu-btn-0-1").is_none());
    let earlier = bounds(&doc, "note-body-1");
    press(&mut doc, Key::Named(NamedKey::ArrowDown), false);
    press(&mut doc, Key::Named(NamedKey::ArrowDown), false);
    assert!(doc.inner.borrow().get_element_by_id("col-menu-btn-0-0").is_some());
    press(&mut doc, Key::Named(NamedKey::ArrowDown), false);
    assert!(doc.inner.borrow().get_element_by_id("row-menu-btn-0-1").is_some());
    assert_eq!(bounds(&doc, "note-body-1"), earlier);
    click(&mut doc, "note-title");
    assert!(doc.inner.borrow().get_element_by_id("col-menu-btn-0-0").is_none());
    assert!(doc.inner.borrow().get_element_by_id("row-menu-btn-0-1").is_none());
    assert_eq!(bounds(&doc, "note-body-1"), earlier);
}

#[test]
fn paper_around_table_controls_places_a_caret_without_growing_the_grid() {
    let mut doc = editor();
    let md = "# Title\n\n## Stops\n\n| Place | Time |\n| --- | --- |\n| Lock | Noon |\n\nTail";
    open_markdown(&mut doc, md, 2);
    let [_, _, width, height] = bounds(&doc, "grid-scroll-0");
    click_at(&mut doc, "grid-scroll-0", Some((width as f32 + 9.0, 8.0)));
    assert_focus(&doc, "cell-0-0-1", [4; 2]);
    // Clicking just below the table's bottom border places the caret on the line underneath.
    click_at(&mut doc, "grid-scroll-0", Some((3.0, height as f32 + 3.0)));
    assert_focus(&doc, "note-body-2", [0; 2]);
    click_at(&mut doc, "note-heading-0", Some((-12.0, 18.0)));
    assert_focus(&doc, "note-heading-0", [3; 2]);
    click(&mut doc, "source-mode");
    assert_eq!(value(&doc, "markdown"), md);
}

#[test]
fn typing_below_table_with_formatted_prose_does_not_absorb_into_table() {
    let mut doc = editor();
    let md = "# Field notes\n\n| Stop | Take |\n| --- | --- |\n| Lock | 45 s |\n\n- [ ] Leave **ten seconds** of silence.";
    open_markdown(&mut doc, md, 2);
    let [_, _, _, height] = bounds(&doc, "grid-scroll-0");
    click_at(&mut doc, "grid-scroll-0", Some((3.0, height as f32 + 4.0)));
    assert_focus(&doc, "note-body-1", [0; 2]);
    press(&mut doc, Key::Character("h".into()), false);
    press(&mut doc, Key::Character("i".into()), false);
    let out = markdown(&doc);
    assert_eq!(
        table::tables(&out)[0].rows.len(),
        2,
        "typed text below table must not become a table row: {out:?}"
    );
    assert!(
        out.contains("| Lock | 45 s |\n\nhi\n- [ ] Leave **ten seconds** of silence."),
        "expected blank line separating table and typed text, got: {out:?}"
    );
}

#[test]
fn aligned_cells_keep_their_hit_area_and_the_menu_clears_the_table() {
    let mut doc = editor();
    let md = "# Times\n\n| Stop | Side | Take |\n| --- | :---: | ---: |\n| Lock | Ést | 45 s |\n| Bridge | West |  |\n\nTail";
    open_markdown(&mut doc, md, 2);
    let table = bounds(&doc, "grid-scroll-0");
    let tail = bounds(&doc, "note-body-1");
    let ink = |d: &DioxusDocument, name: &str| {
        let dom = d.inner.borrow();
        let node = dom.get_node(id(d, name)).unwrap();
        let text = node.element_data().unwrap().text_input_data().unwrap();
        let layout = text.editor.try_layout().unwrap();
        let origin =
            bounds(d, name)[0] + f64::from(node.final_layout().padding.left - text.scroll_offset);
        [0, text.editor.raw_text().len()].map(|byte| {
            let caret = parley::Cursor::from_byte_index(layout, byte, parley::Affinity::Downstream);
            origin + caret.geometry(layout, 0.).x0 / f64::from(layout.scale())
        })
    };
    let [x, _, w, _] = bounds(&doc, "slot-0-1-1");
    let [first, last] = ink(&doc, "cell-0-1-1");
    assert!(
        ((first + last) * 0.5 - (x + w * 0.5)).abs() < 3.,
        "centered glyphs {first}..{last} in {x}+{w}"
    );
    let [x, _, w, _] = bounds(&doc, "slot-0-1-2");
    let inset = x + w - ink(&doc, "cell-0-1-2")[1];
    assert!((14.0..19.0).contains(&inset), "right inset {inset}");
    click_at(&mut doc, "slot-0-1-2", Some((4., 20.)));
    assert_focus(&doc, "cell-0-1-2", [0; 2]);
    click_at(&mut doc, "slot-0-2-2", Some((6., 20.)));
    assert_focus(&doc, "cell-0-2-2", [0; 2]);
    click_with(&mut doc, "cell-0-1-1", None, Modifiers::SHIFT);
    let [mx, my, mw, mh] = bounds(&doc, "table-bar-0");
    assert!(
        my >= table[1] + table[3] - 1.0,
        "docked table bar stays below table cells"
    );
    assert!(mx >= 0. && my >= 0. && mx + mw <= 820. && my + mh <= 520.);
    assert_eq!(bounds(&doc, "grid-scroll-0"), table);
    assert_eq!(bounds(&doc, "note-body-1"), tail);
    press(&mut doc, Key::Named(NamedKey::Escape), false);
    assert_focus(&doc, "cell-0-1-1", ["Ést".len(); 2]);
}

#[test]
fn split_inside_a_section_then_join_at_the_paragraph_boundary() {
    let mut doc = editor();
    let md = "# Top\n\n## Café noir\n\nTail";
    open_markdown(&mut doc, md, md.find("é").unwrap() + "é".len());
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert_eq!(value(&doc, "note-heading-0"), "Café");
    assert_focus(&doc, "note-body-1", [0; 2]);
    press(&mut doc, Key::Named(NamedKey::Backspace), false);
    assert_eq!(value(&doc, "note-heading-0"), "## Café noir");
    assert_focus(&doc, "note-heading-0", ["## Café".len(); 2]);
}

fn chord(doc: &mut DioxusDocument, key: Key, modifiers: Modifiers) {
    doc.handle_ui_event(UiEvent::KeyDown(BlitzKeyEvent {
        key,
        modifiers,
        code: Code::Unidentified,
        location: Location::Standard,
        is_auto_repeating: false,
        is_composing: false,
        state: KeyState::Pressed,
        text: None,
    }));
    render(doc);
}

#[test]
fn extend_a_selection_from_heading_to_prose_and_replace_then_undo() {
    let mut doc = editor();
    let md = "# Notebook\n\n## Départ\n🌿 au port\n\nFin";
    let anchor = md.find("part").unwrap();
    open_markdown(&mut doc, md, anchor);
    chord(&mut doc, Key::Named(NamedKey::ArrowDown), Modifiers::SHIFT);
    click(&mut doc, "source-mode");
    let dom = doc.inner.borrow();
    let selected = dom
        .get_node(id(&doc, "markdown"))
        .unwrap()
        .element_data()
        .unwrap()
        .text_input_data()
        .unwrap()
        .editor
        .raw_selection();
    let head = selected.focus().index();
    assert_eq!(selected.anchor().index(), anchor);
    assert!(head >= md.find("🌿").unwrap() && head <= md.find("\n\nFin").unwrap());
    drop(dom);
    click(&mut doc, "live-mode");
    commit_text(&mut doc, "été");
    click(&mut doc, "source-mode");
    assert_eq!(
        value(&doc, "markdown"),
        format!("{}été{}", &md[..anchor], &md[head..])
    );
    click(&mut doc, "undo");
    assert_eq!(value(&doc, "markdown"), md);
    assert_focus(&doc, "markdown", [anchor, head]);
}

#[derive(Default)]
struct TestClipboard(std::sync::Mutex<String>);
impl blitz_traits::shell::ShellProvider for TestClipboard {
    fn get_clipboard_text(&self) -> Result<String, blitz_traits::shell::ClipboardError> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn set_clipboard_text(&self, text: String) -> Result<(), blitz_traits::shell::ClipboardError> {
        *self.0.lock().unwrap() = text;
        Ok(())
    }
}

#[test]
fn document_select_all_cut_paste_and_undo_from_a_table() {
    let mut doc = editor();
    let md = "# Café\r\n\r\n| A | B |\r\n| --- | --- |\r\n| 🌿 | x\\|y |\r\n\r\nTail";
    open_markdown(&mut doc, md, md.find("x\\|y").unwrap());
    let clipboard = std::sync::Arc::new(TestClipboard::default());
    doc.inner.borrow_mut().shell_provider = clipboard.clone();
    chord(&mut doc, Key::Character("a".into()), Modifiers::META);
    chord(&mut doc, Key::Character("c".into()), Modifiers::META);
    assert_eq!(*clipboard.0.lock().unwrap(), md);
    chord(&mut doc, Key::Character("x".into()), Modifiers::META);
    assert_eq!(value(&doc, "note-body"), "");
    chord(&mut doc, Key::Character("z".into()), Modifiers::META);
    click(&mut doc, "source-mode");
    assert_eq!(value(&doc, "markdown"), md);
    assert_focus(&doc, "markdown", [0, md.len()]);
    click(&mut doc, "live-mode");
    *clipboard.0.lock().unwrap() = "## Été\n\nA fresh paragraph".into();
    chord(&mut doc, Key::Character("v".into()), Modifiers::META);
    click(&mut doc, "source-mode");
    assert_eq!(value(&doc, "markdown"), "## Été\n\nA fresh paragraph");
}

#[test]
fn native_drag_from_cell_to_cell_selects_a_rectangle_and_escape_collapses_it() {
    let mut doc = editor();
    let md = "# Grid\n\n| A | B | C |\n| --- | --- | --- |\n| a | b | c |\n| d | e | f |";
    open_markdown(&mut doc, md, 2);
    let pointer = |field: &str| {
        let dom = doc.inner.borrow();
        let node = dom.get_node(id(&doc, field)).unwrap();
        node.synthetic_click_event_data(Modifiers::empty())
    };
    let mut a = pointer("cell-0-2-2");
    let mut b = pointer("cell-0-1-1");
    a.buttons = MouseEventButtons::Primary;
    b.buttons = MouseEventButtons::Primary;
    doc.handle_ui_event(UiEvent::PointerDown(a));
    render(&mut doc);
    doc.handle_ui_event(UiEvent::PointerMove(b.clone()));
    render(&mut doc);
    b.buttons = MouseEventButtons::None;
    doc.handle_ui_event(UiEvent::PointerUp(b));
    render(&mut doc);
    let range = || {
        doc.vdom
            .in_scope(ScopeId::APP, || *consume_context::<Writing>().cells.peek())
    };
    assert_eq!(
        range(),
        Some((
            0,
            table::CellRect {
                anchor: [2, 2],
                head: [1, 1]
            }
        ))
    );
    chord(&mut doc, Key::Named(NamedKey::Tab), Modifiers::SHIFT);
    assert_focus(&doc, "cell-0-2-2", [0, 1]);
    press(&mut doc, Key::Named(NamedKey::Escape), false);
    assert_focus(&doc, "cell-0-2-2", [1, 1]);
    assert!(doc.vdom.in_scope(ScopeId::APP, || {
        consume_context::<Writing>().cells.peek().is_none()
    }));
}

#[test]
fn a_native_pointer_drag_crosses_from_a_cell_to_a_heading_in_reverse() {
    let mut doc = editor();
    let md = "# Walk\n\n## Stops\n\n| A | Time |\n| --- | --- |\n| One | Noon |\n\nEnd";
    open_markdown(&mut doc, md, 2);
    let mut pointer = doc
        .inner
        .borrow()
        .get_node(id(&doc, "cell-0-1-1"))
        .unwrap()
        .synthetic_click_event_data(Modifiers::empty());
    let mut go = |doc: &DioxusDocument, name: &str, offset: usize| {
        let id = id(doc, name);
        let dom = doc.inner.borrow();
        let node = dom.get_node(id).unwrap();
        let rect = dom.get_client_bounding_rect(id).unwrap();
        let input = node.element_data().unwrap().text_input_data().unwrap();
        let layout = input.editor.try_layout().unwrap();
        let l = node.final_layout();
        let caret = parley::Cursor::from_byte_index(layout, offset, parley::Affinity::Downstream)
            .geometry(layout, 0.0);
        let x = (rect.x
            + f64::from(l.padding.left + l.border.left)
            + caret.x0 / f64::from(layout.scale())
            + 0.1) as f32;
        let y = (rect.y + rect.height * 0.5) as f32;
        pointer.coords.client_x = x;
        pointer.coords.page_x = x;
        pointer.coords.screen_x = x;
        pointer.coords.client_y = y;
        pointer.coords.page_y = y;
        pointer.coords.screen_y = y;
        pointer.clone()
    };
    let mut a = go(&doc, "cell-0-1-1", 2);
    doc.handle_ui_event(UiEvent::PointerMove(a.clone()));
    a.buttons = MouseEventButtons::Primary;
    doc.handle_ui_event(UiEvent::PointerDown(a));
    render(&mut doc);
    let mut b = go(&doc, "note-heading-0", 1);
    b.buttons = MouseEventButtons::Primary;
    doc.handle_ui_event(UiEvent::PointerMove(b.clone()));
    render(&mut doc);
    b.buttons = MouseEventButtons::None;
    doc.handle_ui_event(UiEvent::PointerUp(b));
    render(&mut doc);
    click(&mut doc, "source-mode");
    assert_focus(
        &doc,
        "markdown",
        [md.find("Noon").unwrap() + 2, md.find("Stops").unwrap() + 1],
    );
}

#[test]
fn arrows_follow_the_visible_caret_on_a_long_retina_page() {
    let mut doc = DioxusDocument::new(
        VirtualDom::new(|| notebook(SAMPLE)),
        dioxus_native::DocumentConfig {
            font_ctx: Some(typefaces::notebook_fonts()),
            viewport: Some(Viewport {
                window_size: (1200, 720),
                hidpi_scale: 2.0,
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    doc.initial_build();
    render(&mut doc);
    let md = format!("# Start\n\n## Section\n{}\n## End", "A line\n".repeat(28));
    open_markdown(&mut doc, &md, md.find("A line").unwrap());
    for _ in 0..22 {
        press(&mut doc, Key::Named(NamedKey::ArrowDown), false);
    }
    let dom = doc.inner.borrow();
    let field = id(&doc, "note-body-1");
    let rect = dom.get_client_bounding_rect(field).unwrap();
    let text = dom
        .get_node(field)
        .unwrap()
        .element_data()
        .unwrap()
        .text_input_data()
        .unwrap();
    let layout = text.editor.try_layout().unwrap();
    let box_ = text.editor.raw_selection().focus().geometry(layout, 0.0);
    assert_eq!(
        text.editor.raw_selection().focus().index(),
        22 * "A line\n".len()
    );
    assert!(dom.viewport_scroll().y > 0.0);
    assert!(rect.y + box_.y0 / f64::from(layout.scale()) >= -1.0);
    assert!(rect.y + box_.y1 / f64::from(layout.scale()) <= 361.0);
}

#[test]
fn the_revealed_checkbox_text_accepts_a_click_without_toggling() {
    let mut doc = editor();
    let open = SAMPLE.find("[ ]").unwrap();
    open_body_at(&mut doc, [open + 1; 2]);
    let disp_after = body_display(&doc, open + 2);
    let (x, y) = glyph_position(&doc, "note-body", disp_after);
    click_at(&mut doc, "note-body", Some((x + 0.1, y)));
    assert_eq!(markdown(&doc), SAMPLE);
    assert_focus(&doc, "note-body", [disp_after; 2]);
}

#[test]
fn multi_line_paste_into_a_heading_preserves_the_markdown_fragment() {
    struct Clipboard;
    impl blitz_traits::shell::ShellProvider for Clipboard {
        fn get_clipboard_text(&self) -> Result<String, blitz_traits::shell::ClipboardError> {
            Ok("air\r\n\r\n🌿".into())
        }
    }
    let mut doc = editor();
    let md = "# Log\r\n\r\n## Fresh water ###\r\n\r\nNext";
    open_markdown(&mut doc, md, md.find("water").unwrap());
    doc.inner.borrow_mut().shell_provider = std::sync::Arc::new(Clipboard);
    chord(&mut doc, Key::Character("v".into()), Modifiers::META);
    click(&mut doc, "source-mode");
    assert_eq!(
        value(&doc, "markdown"),
        md.replace("water", "air\r\n\r\n🌿water")
    );
}

#[test]
fn backspace_at_the_checklist_label_outdents_then_removes_its_prefix() {
    let mut doc = editor();
    let md = "# Walk\n\n- Parent\n  - [x] Child";
    open_markdown(&mut doc, md, md.find("Child").unwrap());
    press(&mut doc, Key::Named(NamedKey::Backspace), false);
    assert_eq!(markdown(&doc), "# Walk\n\n- Parent\n- [x] Child");
    press(&mut doc, Key::Named(NamedKey::Backspace), false);
    assert_eq!(markdown(&doc), "# Walk\n\n- Parent\nChild");
    chord(&mut doc, Key::Character("z".into()), Modifiers::META);
    assert_eq!(markdown(&doc), "# Walk\n\n- Parent\n- [x] Child");
}

fn split_document(md: &str) -> DioxusDocument {
    let mut doc = editor();
    click(&mut doc, "source-mode");
    select(&doc, "markdown", [0, SAMPLE.len()]);
    commit_text(&mut doc, md);
    select(&doc, "markdown", [0; 2]);
    click(&mut doc, "split-mode");
    assert_eq!(value(&doc, "markdown"), md);
    doc
}

#[test]
fn split_uses_one_input_across_blank_lines_headings_tables_and_mode_changes() {
    let md = "# A\n\n## B\n\n| X | Y |\n| --- | --- |\n| É | Q |\n\nTail";
    let mut doc = split_document(md);
    let source = id(&doc, "markdown");
    assert!(doc.inner.borrow().get_element_by_id("note-title").is_none());
    assert!(doc.inner.borrow().get_element_by_id("cell-0-0-0").is_none());
    for line in [4, 5] {
        press(&mut doc, Key::Named(NamedKey::ArrowDown), false);
        assert_focus(&doc, "markdown", [line; 2]);
    }
    press(&mut doc, Key::Named(NamedKey::ArrowUp), false);
    assert_focus(&doc, "markdown", [4; 2]);
    let end = md.find('Q').unwrap() + 1;
    select(&doc, "markdown", [end, 2]);
    click(&mut doc, "source-mode");
    assert_focus(&doc, "markdown", [end, 2]);
    click(&mut doc, "split-mode");
    assert_eq!(id(&doc, "markdown"), source);
    assert_focus(&doc, "markdown", [end, 2]);
    commit_text(&mut doc, "Hi");
    assert_eq!(value(&doc, "markdown"), format!("# Hi{}", &md[end..]));
    click(&mut doc, "undo");
    assert_eq!(value(&doc, "markdown"), md);
}

#[test]
fn split_preview_rail_targets_source_and_only_plus_grows_the_clicked_table() {
    let md = "# H\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n| C | D |\n| --- | --- |\n| x | y |";
    let mut doc = split_document(md);
    let tables = table::tables(md);
    let t = &tables[1];
    let start = t.range.start;
    let scroll = format!("reading-scroll-{start}");
    let first = |doc: &DioxusDocument| {
        let b = doc.inner.borrow();
        b.get_client_bounding_rect(id(doc, &scroll)).unwrap()
    };
    let frame = first(&doc);
    click_at(&mut doc, &scroll, Some((frame.width as f32 + 3., 5.)));
    assert_eq!(value(&doc, "markdown"), md);
    assert_focus(&doc, "markdown", [t.cell(0, 1).end; 2]);
    click_at(&mut doc, &scroll, Some((5., frame.height as f32 + 4.)));
    assert_focus(&doc, "markdown", [t.cell(1, 0).start; 2]);
    click(&mut doc, &format!("reading-column-{start}"));
    let grown = value(&doc, "markdown");
    let two = table::tables(&grown);
    assert_eq!((two[0].width(), two[1].width()), (2, 3));
    assert_eq!(&grown[..start], &md[..start]);
    let header = two[1].cell(0, 2);
    assert_focus(&doc, "markdown", [header.start, header.end]);
    commit_text(&mut doc, "Time");
    click(&mut doc, &format!("reading-row-{start}"));
    let changed = value(&doc, "markdown");
    assert_eq!(table::tables(&changed)[1].rows.len(), 3);
    assert!(changed.contains("Time"));
    click(&mut doc, "undo");
    assert_eq!(table::tables(&value(&doc, "markdown"))[1].rows.len(), 2);
}

#[test]
fn split_targeted_keys_work_in_the_source_and_short_cells_stay_virtual_until_typed() {
    let md = "# H\n\n- [ ] Box\n\n| A | B | C |\n| --- | --- | --- |\n| x |";
    let mut doc = split_document(md);
    let open = md.find('[').unwrap();
    click(&mut doc, &format!("reading-box-{open}"));
    assert_focus(&doc, "markdown", [open + 1; 2]);
    assert!(value(&doc, "markdown").contains("[x]"));
    press(&mut doc, Key::Character(" ".into()), false);
    assert!(value(&doc, "markdown").contains("[ ]"));
    select(&doc, "markdown", [open + 1; 2]);
    press(&mut doc, Key::Character("x".into()), false);
    assert!(value(&doc, "markdown").contains("[x]"));
    press(&mut doc, Key::Character("q".into()), false);
    assert!(value(&doc, "markdown").contains("[x]"));
    press(&mut doc, Key::Named(NamedKey::F2), false);
    commit_text(&mut doc, "q");
    assert!(value(&doc, "markdown").contains("[qx]"));
    let text = value(&doc, "markdown");
    let end = text.find("Box").unwrap() + 3;
    select(&doc, "markdown", [end; 2]);
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert!(value(&doc, "markdown").contains("Box\n- "));
    let text = value(&doc, "markdown");
    let t = table::tables(&text).remove(0);
    let slot = format!("reading-cell-{}-1-2", t.range.start);
    click(&mut doc, &slot);
    assert_eq!(value(&doc, "markdown"), text);
    commit_text(&mut doc, "🌿");
    let text = value(&doc, "markdown");
    let t = table::tables(&text).remove(0);
    assert_eq!(&text[t.cell(1, 2)], "🌿");
    press(&mut doc, Key::Named(NamedKey::Tab), false);
    let text = value(&doc, "markdown");
    let t = table::tables(&text).remove(0);
    assert_eq!(t.rows.len(), 3);
    assert_focus(&doc, "markdown", [t.cell(2, 0).start; 2]);
}

#[test]
fn split_drag_uses_parley_and_formatted_cell_letters_map_to_markdown_bytes() {
    let md = "# Étape\n\n## Deux\n\n| Stop | Temps |\n| --- | --- |\n| 🌿 | **Écoute** |";
    let mut doc = split_document(md);
    let start = md.find("Deux").unwrap() + 1;
    let end = md.find("Écoute").unwrap() + "Éc".len();
    let point = |doc: &DioxusDocument, offset| {
        let field = id(doc, "markdown");
        let d = doc.inner.borrow();
        let n = d.get_node(field).unwrap();
        let rect = d.get_client_bounding_rect(field).unwrap();
        let mut pointer = n.synthetic_click_event_data(Modifiers::empty());
        let (x, y) = glyph_position(doc, "markdown", offset);
        let l = n.final_layout();
        pointer.coords.client_x = rect.x as f32 + l.border.left + l.padding.left + x + 0.2;
        pointer.coords.client_y = rect.y as f32 + l.border.top + l.padding.top + y;
        pointer.coords.page_x = pointer.coords.client_x;
        pointer.coords.screen_x = pointer.coords.client_x;
        pointer.coords.page_y = pointer.coords.client_y;
        pointer.coords.screen_y = pointer.coords.client_y;
        pointer
    };
    let mut a = point(&doc, start);
    doc.handle_ui_event(UiEvent::PointerMove(a.clone()));
    a.buttons = MouseEventButtons::Primary;
    doc.handle_ui_event(UiEvent::PointerDown(a));
    render(&mut doc);
    let mut b = point(&doc, end);
    b.buttons = MouseEventButtons::Primary;
    doc.handle_ui_event(UiEvent::PointerMove(b.clone()));
    b.buttons = MouseEventButtons::None;
    doc.handle_ui_event(UiEvent::PointerUp(b));
    render(&mut doc);
    assert_focus(&doc, "markdown", [start, end]);
    let table = md.find("| Stop").unwrap();
    let cell = format!("reading-cell-{table}-1-1");
    let (x, y) = {
        let d = doc.inner.borrow();
        let node = d.get_node(id(&doc, &cell)).unwrap();
        let inline = node
            .element_data()
            .unwrap()
            .inline_layout_data
            .as_ref()
            .unwrap();
        let layout = &inline.layout;
        let rect =
            parley::Cursor::from_byte_index(layout, "Éc".len(), parley::Affinity::Downstream)
                .geometry(layout, 0.);
        let l = node.final_layout();
        (
            rect.x0 as f32 / layout.scale() + l.border.left + l.padding.left + 0.1,
            (rect.y0 + rect.y1) as f32 / (2. * layout.scale())
                + l.border.top
                + l.padding.top
                + inline.block_offset,
        )
    };
    click_at(&mut doc, &cell, Some((x, y)));
    assert_focus(&doc, "markdown", [end; 2]);
    commit_text(&mut doc, "!");
    assert!(value(&doc, "markdown").contains("**Éc!oute**"));
}

/// The actual painted caret, including the text field's own scroll and the window scroll.
fn painted_caret(doc: &DioxusDocument, name: &str) -> (f64, f64, f64, f64) {
    let d = doc.inner.borrow();
    let id = id(doc, name);
    assert_eq!(d.get_focussed_node_id(), Some(id), "focus in {name}");
    let n = d.get_node(id).unwrap();
    let input = n.element_data().unwrap().text_input_data().unwrap();
    let layout = input.editor.try_layout().expect("fresh text layout");
    let caret = input.editor.cursor_geometry(1.5).expect("visible caret");
    let scale = f64::from(layout.scale());
    let rect = d.get_client_bounding_rect(id).unwrap();
    let l = n.final_layout();
    let origin =
        rect.y + f64::from(l.padding.top + l.border.top) + n.text_input_v_centering_offset(scale);
    let offset = if input.is_multiline {
        f64::from(input.scroll_offset)
    } else {
        0.
    };
    let visible_top = origin.max(0.);
    let visible_bottom =
        (origin + f64::from(l.content_box_height())).min(f64::from(d.viewport().logical_size().1));
    (
        origin + caret.y0 / scale - offset,
        origin + caret.y1 / scale - offset,
        visible_top,
        visible_bottom,
    )
}

#[test]
fn return_keeps_the_new_blank_line_visible_before_another_key() {
    let md = "# Note\n\n## Section\nThe last line";
    for mode in ["live-mode", "split-mode", "source-mode"] {
        let mut doc = editor();
        open_markdown(&mut doc, md, md.len());
        let name = if mode == "live-mode" {
            "note-body-1"
        } else {
            click(&mut doc, mode);
            "markdown"
        };
        let mut expected = value(&doc, name);
        for n in 1..=31 {
            press(&mut doc, Key::Named(NamedKey::Enter), false);
            expected.push('\n');
            assert_eq!(value(&doc, name), expected, "{mode} after Return {n}");
            assert_focus(&doc, name, [expected.len(); 2]);
            let (top, bottom, visible_top, visible_bottom) = painted_caret(&doc, name);
            assert!(
                bottom > top && top >= visible_top - 1. && bottom <= visible_bottom + 1.,
                "{mode} immediately after Return {n}: caret {top}..{bottom}, viewport {visible_top}..{visible_bottom}"
            );
        }
        commit_text(&mut doc, "Here");
        assert_eq!(value(&doc, name), expected + "Here");
    }
}

#[test]
fn delete_selected_column_in_live_and_split_is_scoped_undoable_and_keeps_a_caret() {
    let md = "| Keep | Value |\n| --- | --- |\n| α | 8 |\n\n| A | B | C |\n| :--- | --- | ---: |\n| a\\|b | 🌿 | c |\n| Only |";
    let start = md.find("| A |").unwrap();
    for split in [false, true] {
        let mut doc = split_document(md);
        if !split {
            click(&mut doc, "live-mode");
        }
        let cell = |r, c| {
            if split {
                format!("reading-cell-{start}-{r}-{c}")
            } else {
                format!("cell-1-{r}-{c}")
            }
        };
        let remove = if split {
            format!("reading-delete-column-{start}")
        } else {
            "col-menu-delete-1-1".to_owned()
        };
        assert!(doc.inner.borrow().get_element_by_id(&remove).is_none());
        click(&mut doc, &cell(1, 1));
        if !split {
            // Destructive delete lives in the column menu, not the floating bar.
            assert!(doc.inner.borrow().get_element_by_id("delete-column-1").is_none());
            click(&mut doc, "col-menu-btn-1-1");
        }
        click(&mut doc, &remove);
        let expected = md
            .replace("| A | B | C |", "| A | C |")
            .replace("| :--- | --- | ---: |", "| :--- | ---: |")
            .replace("| 🌿 | c |", "| c |");
        let t = table::tables(&expected).remove(1);
        if split {
            let pos = t.cell(1, 1);
            assert_focus(&doc, "markdown", [pos.start, pos.end]);
            assert_eq!(value(&doc, "markdown"), expected);
        } else {
            assert_focus(&doc, &cell(1, 1), [0, 1]);
            click(&mut doc, "col-menu-btn-1-1");
        }
        click(&mut doc, &remove);
        if split {
            assert!(doc.inner.borrow().get_element_by_id(&remove).is_none());
        } else {
            click(&mut doc, "col-menu-btn-1-0");
            assert_eq!(
                doc.inner
                    .borrow()
                    .get_node(id(&doc, "col-menu-delete-1-0"))
                    .unwrap()
                    .attr("aria-disabled".into()),
                Some("true")
            );
        }
        if !split {
            click(&mut doc, "source-mode");
        }
        let last = value(&doc, "markdown");
        assert_eq!(&last[..start], &md[..start]);
        assert_eq!(table::tables(&last)[1].width(), 1);
        click(&mut doc, "undo");
        assert_eq!(value(&doc, "markdown"), expected);
        click(&mut doc, "undo");
        assert_eq!(value(&doc, "markdown"), md);
    }
}

#[test]
fn live_return_after_headings_creates_a_real_line_at_the_caret() {
    for (md, heading, suffix) in [
        ("# Écoute", "Écoute", ""),
        ("# Écoute\nNext", "Écoute", ""),
        ("# Écoute\n\nNext", "Écoute", ""),
        (
            "# Écoute ###\r\n| A | B |\r\n| --- | --- |",
            "Écoute",
            " ###",
        ),
        ("# Note\n\n## Écoute", "Écoute", ""),
        ("# Note\r\n\r\n## Écoute ##\r\n## Next", "Écoute", " ##"),
    ] {
        let mut doc = editor();
        let at = md.find(heading).unwrap() + heading.len();
        let end = at + suffix.len();
        let eol = table::eol(md);
        open_markdown(&mut doc, md, at);
        press(&mut doc, Key::Named(NamedKey::Enter), false);
        let after = format!("{}{eol}{}", &md[..end], &md[end..]);
        let field = {
            let d = doc.inner.borrow();
            d.get_focussed_node_id()
                .and_then(|id| d.get_node(id)?.element_data()?.id.as_deref())
                .unwrap()
                .to_owned()
        };
        assert!(
            field == "title-gap" || field.starts_with("note-body"),
            "{field} after Return in {md:?}"
        );
        let (top, bottom, view_top, view_bottom) = painted_caret(&doc, &field);
        assert!(bottom > top && top >= view_top - 1. && bottom <= view_bottom + 1.);
        commit_text(&mut doc, "Here");
        click(&mut doc, "source-mode");
        assert_eq!(
            value(&doc, "markdown"),
            format!("{}{eol}Here{}", &md[..end], &md[end..])
        );
        click(&mut doc, "undo");
        assert_eq!(
            value(&doc, "markdown"),
            after,
            "Return is a separate edit in {md:?}"
        );
        assert_focus(&doc, "markdown", [end + eol.len(); 2]);
        click(&mut doc, "undo");
        assert_eq!(value(&doc, "markdown"), md);
    }
}

#[test]
fn live_return_preserves_the_line_and_focus_before_a_table_or_heading() {
    let mut doc = editor();
    for (line, next, continuation) in [
        (
            "Écoute **au quai**",
            "| Place | Time |\n| --- | --- |\n| Pont | 3 |",
            "",
        ),
        ("Écoute **au quai**", "## Next", ""),
        ("- [x] Écoute **au quai**", "## Next", "- [ ] "),
        ("2. Écoute au quai", "## Next", "3. "),
    ] {
        let md = format!("# Note\n\n{line}\n\n{next}");
        let at = md.find(line).unwrap() + line.len();
        open_markdown(&mut doc, &md, at);
        press(&mut doc, Key::Named(NamedKey::Enter), false);
        assert_eq!(
            markdown(&doc),
            format!("# Note\n\n{line}\n{continuation}\n\n{next}")
        );
        let (top, bottom, view_top, view_bottom) = painted_caret(&doc, "note-body");
        assert!(bottom > top && top >= view_top - 1. && bottom <= view_bottom + 1.);
        commit_text(&mut doc, "Here");
        click(&mut doc, "source-mode");
        assert_eq!(
            value(&doc, "markdown"),
            format!("# Note\n\n{line}\n{continuation}Here\n\n{next}")
        );
    }
}

#[test]
fn cmd_enter_toggles_task_and_cmd_k_and_heading_shortcuts_work() {
    let mut doc = editor();
    let md = "# Walk\n\n- [ ] Pack gear\n- Plain item\nVisit canal";
    open_markdown(&mut doc, md, md.find("gear").unwrap());
    chord(&mut doc, Key::Named(NamedKey::Enter), Modifiers::META);
    assert!(markdown(&doc).contains("- [x] Pack gear"));
    chord(&mut doc, Key::Named(NamedKey::Enter), Modifiers::META);
    assert!(markdown(&doc).contains("- [ ] Pack gear"));

    let plain = markdown(&doc).find("Plain").unwrap();
    open_body_at(&mut doc, [plain; 2]);
    chord(&mut doc, Key::Named(NamedKey::Enter), Modifiers::META);
    assert!(markdown(&doc).contains("- [ ] Plain item"));

    let canal = markdown(&doc).find("canal").unwrap();
    open_body_at(&mut doc, [canal, canal + 5]);
    chord(&mut doc, Key::Character("k".into()), Modifiers::META);
    assert!(markdown(&doc).contains("[canal](https://)"));

    click(&mut doc, "note-title");
    chord(&mut doc, Key::Character("2".into()), Modifiers::META);
    click(&mut doc, "source-mode");
    assert!(value(&doc, "markdown").starts_with("## Walk\n"));
    click(&mut doc, "live-mode");
    click(&mut doc, "note-title");
    chord(&mut doc, Key::Character("0".into()), Modifiers::META);
    click(&mut doc, "source-mode");
    assert!(value(&doc, "markdown").starts_with("Walk\n"));
}

#[test]
fn parked_escape_resumes_on_typing_and_source_mode_supports_shortcuts() {
    let mut doc = editor();
    let md = "# Note\n\n- Alpha\nBeta";
    open_markdown(&mut doc, md, md.find("Beta").unwrap());
    press(&mut doc, Key::Named(NamedKey::Escape), false);
    assert_eq!(
        doc.inner.borrow().get_focussed_node_id(),
        Some(id(&doc, "body-surface"))
    );
    press(&mut doc, Key::Character("!".into()), false);
    assert!(markdown(&doc).contains("!Beta"));

    click(&mut doc, "source-mode");
    let src = value(&doc, "markdown");
    let at = src.find("!Beta").unwrap();
    select(&doc, "markdown", [at, at + 5]);
    chord(&mut doc, Key::Character("b".into()), Modifiers::META);
    assert!(value(&doc, "markdown").contains("**!Beta**"));
    let item_end = value(&doc, "markdown").find("Alpha").unwrap() + 5;
    select(&doc, "markdown", [item_end; 2]);
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert!(value(&doc, "markdown").contains("- Alpha\n- \n"));
}

#[test]
fn docked_table_bar_and_alt_arrows_move_align_and_insert_rows_and_cols() {
    let mut doc = editor();
    let md = "# Grid\n\n| Stop | Take |\n| --- | --- |\n| Lock | 45 s |\n| Bridge | 30 s |\n\nTail";
    open_markdown(&mut doc, md, md.find("Lock").unwrap());
    // Single-cell focus shows no useless "Cell xx" footer; only multi-cell ranges show table-bar-0.
    assert!(doc.inner.borrow().get_element_by_id("table-bar-0").is_none());
    // Alt+Down moves row down directly from the keyboard!
    chord(&mut doc, Key::Named(NamedKey::ArrowDown), Modifiers::ALT);
    assert!(
        markdown(&doc).contains("| Bridge | 30 s |\n| Lock | 45 s |"),
        "{}",
        markdown(&doc)
    );
    // Alt+Right moves column right directly from the keyboard!
    chord(&mut doc, Key::Named(NamedKey::ArrowRight), Modifiers::ALT);
    assert!(markdown(&doc).contains("| Take | Stop |"), "{}", markdown(&doc));
    // Only the hovered/active column & row show chevron menus (no floating toolbar or + buttons).
    assert!(doc.inner.borrow().get_element_by_id("col-menu-btn-0-1").is_some());
    assert!(doc.inner.borrow().get_element_by_id("col-menu-btn-0-0").is_none());
    assert!(doc.inner.borrow().get_element_by_id("row-menu-btn-0-2").is_some());
    assert!(doc.inner.borrow().get_element_by_id("row-menu-btn-0-1").is_none());
    assert!(doc.inner.borrow().get_element_by_id("move-col-left-0").is_none());
    assert!(doc.inner.borrow().get_element_by_id("move-row-up-0").is_none());
    assert!(doc.inner.borrow().get_element_by_id("add-column-0").is_none());
    assert!(doc.inner.borrow().get_element_by_id("add-row-0").is_none());
    // Column menu actions work cleanly!
    click(&mut doc, "col-menu-btn-0-1");
    click(&mut doc, "col-menu-move-left-0-1");
    assert!(markdown(&doc).contains("| Stop | Take |"));
    click(&mut doc, "col-menu-btn-0-0");
    click(&mut doc, "col-menu-move-right-0-0");
    assert!(markdown(&doc).contains("| Take | Stop |"));
    click(&mut doc, "col-menu-btn-0-1");
    click(&mut doc, "col-menu-move-left-0-1");
    assert!(markdown(&doc).contains("| Stop | Take |"));
    // Alignment icons live inside the in-header chevron menu.
    click(&mut doc, "cell-0-1-1");
    assert!(doc.inner.borrow().get_element_by_id("align-right-0").is_none());
    click(&mut doc, "col-menu-btn-0-1");
    click(&mut doc, "align-right-0");
    assert!(markdown(&doc).contains("| --- | ---: |"));
    click(&mut doc, "col-menu-btn-0-1");
    click(&mut doc, "align-center-0");
    assert!(markdown(&doc).contains("| --- | :---: |"));
    click(&mut doc, "col-menu-btn-0-1");
    click(&mut doc, "align-left-0");
    assert!(markdown(&doc).contains("| --- | :--- |"));
    click(&mut doc, "col-menu-btn-0-1");
    click(&mut doc, "col-menu-insert-right-0-1");
    assert!(markdown(&doc).contains("| Stop | Take | Column 3 |"));
    click(&mut doc, "col-menu-btn-0-2");
    click(&mut doc, "col-menu-delete-0-2");
    assert!(markdown(&doc).contains("| Stop | Take |"));
    click(&mut doc, "cell-0-1-0");
    click(&mut doc, "col-menu-btn-0-0");
    click(&mut doc, "col-menu-insert-left-0-0");
    assert!(markdown(&doc).contains("| Column 3 | Stop | Take |"));
    assert!(doc.inner.borrow().get_element_by_id("delete-column-0").is_none());
    click(&mut doc, "col-menu-btn-0-0");
    click(&mut doc, "col-menu-delete-0-0");
    assert!(markdown(&doc).contains("| Stop | Take |"));
    // Row chevron menu actions work cleanly!
    click(&mut doc, "cell-0-1-0");
    assert!(doc.inner.borrow().get_element_by_id("delete-row-0").is_none());
    click(&mut doc, "row-menu-btn-0-1");
    click(&mut doc, "row-menu-insert-above-0-1");
    assert!(markdown(&doc).contains("| --- | :--- |\n|  |  |\n| Bridge | 30 s |"));
    click(&mut doc, "row-menu-btn-0-1");
    click(&mut doc, "row-menu-delete-0-1");
    assert!(!markdown(&doc).contains("|  |  |"));
    click(&mut doc, "row-menu-btn-0-1");
    click(&mut doc, "row-menu-move-down-0-1");
    assert!(markdown(&doc).contains("| Lock | 45 s |\n| Bridge | 30 s |"));
    click(&mut doc, "row-menu-btn-0-2");
    click(&mut doc, "row-menu-move-up-0-2");
    assert!(markdown(&doc).contains("| Bridge | 30 s |\n| Lock | 45 s |"));
    click(&mut doc, "row-menu-btn-0-1");
    click(&mut doc, "row-menu-insert-below-0-1");
    assert!(markdown(&doc).contains("| Bridge | 30 s |\n|  |  |\n| Lock | 45 s |"));
    click(&mut doc, "cell-0-1-0");
    click(&mut doc, "row-menu-btn-0-1");
    click(&mut doc, "row-menu-delete-0-1");
    assert!(!markdown(&doc).contains("| Bridge | 30 s |"));
}

#[test]
fn rich_mode_has_no_global_toolbar_and_empty_line_shows_inline_icons() {
    let mut doc = editor();
    let md = "# Notes\n\nLeave **ten** seconds.\n\nNext line.";
    open_markdown(&mut doc, md, md.find("ten").unwrap() + 1);
    click(&mut doc, "rich-mode");
    // No global formatting toolbar bar.
    assert!(doc.inner.borrow().get_element_by_id("rich-toolbar").is_none());
    // Even with cursor inside **ten**, Rich mode keeps delimiters hidden!
    assert_eq!(value(&doc, "note-body"), "Leave ten seconds.\n\nNext line.");
    // On a non-empty line, inline empty-line icons are hidden.
    assert!(doc.inner.borrow().get_element_by_id("empty-line-tools").is_none());
    // Selecting text still brings up the floating FormatBar and formats without revealing delimiters.
    let sec = markdown(&doc).find("seconds").unwrap();
    open_body_at(&mut doc, [sec, sec + 7]);
    click(&mut doc, "format-1");
    assert_eq!(
        markdown(&doc),
        "# Notes\n\nLeave **ten** *seconds*.\n\nNext line."
    );
    assert_eq!(value(&doc, "note-body"), "Leave ten seconds.\n\nNext line.");
    // Moving caret onto the blank line reveals inline block-insert icons.
    let blank = markdown(&doc).find("\n\nNext").unwrap() + 1;
    open_body_at(&mut doc, [blank; 2]);
    assert!(doc.inner.borrow().get_element_by_id("empty-line-tools").is_some());
    // Clicking inline checklist icon inserts a task on that empty line and hides the empty-line bar.
    click(&mut doc, "empty-task");
    assert!(markdown(&doc).contains("\n- [ ] \nNext line."));
    assert!(doc.inner.borrow().get_element_by_id("empty-line-tools").is_none());
    // Undo back to blank line and test inline H2, quote, code block, and divider.
    click(&mut doc, "undo");
    open_body_at(&mut doc, [blank; 2]);
    assert!(doc.inner.borrow().get_element_by_id("empty-line-tools").is_some());
    click(&mut doc, "empty-quote");
    assert!(markdown(&doc).contains("\n> \nNext line."));
    click(&mut doc, "undo");
    open_body_at(&mut doc, [blank; 2]);
    click(&mut doc, "empty-h2");
    assert!(markdown(&doc).contains("\n## \nNext line."));
}

#[test]
fn live_mode_displays_bold_edits_striked_and_paints_visible_caret() {
    // Both JetBrains Mono and Geist must register weight 400 and 600 under their CSS family names.
    let mut fonts = crate::typefaces::notebook_fonts();
    for name in ["JetBrains Mono", "Geist"] {
        let id = fonts
            .collection
            .family_id(name)
            .unwrap_or_else(|| panic!("missing font family {name}"));
        let weights: Vec<f32> = fonts
            .collection
            .family(id)
            .unwrap()
            .fonts()
            .iter()
            .map(|f| f.weight().value())
            .collect();
        assert!(weights.contains(&400.0), "{name} missing 400: {weights:?}");
        assert!(weights.contains(&600.0), "{name} missing 600: {weights:?}");
    }

    let mut doc = editor();
    let md = "# Note\n\nSome **bold** and ~~striked~~ text.\n\n- [x] Done task.";
    open_markdown(&mut doc, md, 0);
    let bold_at = md.find("bold").unwrap();
    {
        let dom = doc.inner.borrow();
        let nid = dom.get_element_by_id(&format!("ink-{bold_at}")).unwrap();
        let st = dom.get_node(nid).unwrap().primary_styles().unwrap();
        assert_eq!(st.clone_font_weight().value(), 600.0);
    }

    // Click inside ~~striked~~: expands delimiters, lands on exact clicked char, paints visible caret above .md-ink, and edits cleanly.
    let disp_r = value(&doc, "note-body").find("riked").unwrap();
    let (x, y) = glyph_position(&doc, "note-body", disp_r);
    click_at(&mut doc, "note-body", Some((x + 0.5, y)));
    assert_eq!(
        value(&doc, "note-body"),
        "Some bold and ~~striked~~ text.\n\n-    Done task."
    );
    assert!(
        doc.inner.borrow().get_element_by_id("caret-ink").is_some(),
        "caret overlay should be visible on top of strikethrough ink"
    );
    press(&mut doc, Key::Character("X".into()), false);
    assert_eq!(
        markdown(&doc),
        "# Note\n\nSome **bold** and ~~stXriked~~ text.\n\n- [x] Done task."
    );
    assert!(doc.inner.borrow().get_element_by_id("caret-ink").is_some());

    // Click inside completed (strikethrough) task line: caret overlay is visible and text is editable.
    let disp_done = value(&doc, "note-body").find("one task").unwrap();
    let (dx, dy) = glyph_position(&doc, "note-body", disp_done);
    click_at(&mut doc, "note-body", Some((dx + 0.5, dy)));
    assert!(
        doc.inner.borrow().get_element_by_id("caret-ink").is_some(),
        "caret overlay should be visible on top of completed-task strikethrough ink"
    );
    press(&mut doc, Key::Character("Z".into()), false);
    assert_eq!(
        markdown(&doc),
        "# Note\n\nSome **bold** and ~~stXriked~~ text.\n\n- [x] DZone task."
    );
}

#[test]
fn slash_menu_filters_and_inserts_blocks_and_block_handle_reorders_blocks() {
    let mut doc = editor();
    let md = "# Notes\n\nFirst paragraph.\n\n\nSecond paragraph.";
    let blank = md.find("\n\n\nSecond").unwrap() + 2;
    open_markdown(&mut doc, md, blank);
    assert!(doc.inner.borrow().get_element_by_id("slash-menu").is_none());

    // Typing `/` on an empty line opens the slash menu and filters live.
    press(&mut doc, Key::Character("/".into()), false);
    assert!(doc.inner.borrow().get_element_by_id("slash-menu").is_some());
    press(&mut doc, Key::Character("i".into()), false);
    press(&mut doc, Key::Character("m".into()), false);
    assert!(doc.inner.borrow().get_element_by_id("slash-image").is_some());
    assert!(doc.inner.borrow().get_element_by_id("slash-table").is_none());
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert!(doc.inner.borrow().get_element_by_id("slash-menu").is_none());
    assert!(
        markdown(&doc).contains("![alt text](https://)"),
        "slash /im should insert image snippet: {}",
        markdown(&doc)
    );

    // Block handle on the left supports click-for-menu, drag-and-drop reorder, and Alt+Shift+Arrows.
    let md2 = "# Title\n\nAlpha block.\n\nBeta block.\n\nGamma block.";
    open_markdown(&mut doc, md2, md2.find("Alpha").unwrap());
    // Every paragraph inside the prose block has its own handle!
    assert!(doc.inner.borrow().get_element_by_id("block-handle-1").is_some());
    assert!(doc.inner.borrow().get_element_by_id("block-handle-2").is_some());
    assert!(doc.inner.borrow().get_element_by_id("block-handle-3").is_some());
    // Clicking opens the block menu.
    click(&mut doc, "block-handle-1");
    assert!(doc.inner.borrow().get_element_by_id("block-menu").is_some());
    click(&mut doc, "block-move-down");
    assert_eq!(
        markdown(&doc),
        "# Title\n\nBeta block.\n\nAlpha block.\n\nGamma block."
    );
    click(&mut doc, "undo");
    assert_eq!(markdown(&doc), md2);

    // Dragging `block-handle-3` ("Gamma block.") up above `block-handle-1` ("Alpha block.") reorders in one undoable edit!
    let h3 = bounds(&doc, "block-handle-3");
    let h1 = bounds(&doc, "block-handle-1");
    let make_ptr = |d: &DioxusDocument, x: f32, y: f32, held: bool| {
        let mut ptr = d
            .inner
            .borrow()
            .get_node(id(d, "block-handle-3"))
            .unwrap()
            .synthetic_click_event_data(Modifiers::empty());
        ptr.coords.client_x = x;
        ptr.coords.client_y = y;
        ptr.coords.page_x = x;
        ptr.coords.page_y = y;
        ptr.buttons = if held {
            MouseEventButtons::Primary
        } else {
            MouseEventButtons::None
        };
        ptr
    };
    let hx = (h3[0] + h3[2] * 0.5) as f32;
    let y_start = (h3[1] + h3[3] * 0.5) as f32;
    let y_above_h1 = (h1[1] - 4.0) as f32;
    doc.handle_ui_event(UiEvent::PointerDown(make_ptr(&doc, hx, y_start, true)));
    render(&mut doc);
    doc.handle_ui_event(UiEvent::PointerMove(make_ptr(&doc, hx, y_above_h1, true)));
    render(&mut doc);
    assert!(
        doc.inner.borrow().get_element_by_id("block-drop-line").is_some(),
        "dragging a block handle should display a drop indicator line"
    );
    doc.handle_ui_event(UiEvent::PointerUp(make_ptr(&doc, hx, y_above_h1, false)));
    render(&mut doc);
    assert!(doc.inner.borrow().get_element_by_id("block-drop-line").is_none());
    assert_eq!(
        markdown(&doc),
        "# Title\n\nGamma block.\n\nAlpha block.\n\nBeta block."
    );
    // Alt+Shift+ArrowDown moves the focused block down from the keyboard.
    chord(
        &mut doc,
        Key::Named(NamedKey::ArrowDown),
        Modifiers::ALT | Modifiers::SHIFT,
    );
    assert_eq!(
        markdown(&doc),
        "# Title\n\nAlpha block.\n\nGamma block.\n\nBeta block."
    );
    click(&mut doc, "undo");
    click(&mut doc, "undo");
    assert_eq!(markdown(&doc), md2);

    // Dragging a table and individual checklist items works even if the pointer drifts
    // outside `#paper` into the left window margin (full-window drag tracking).
    let md3 = "# Notes\n\n## Section\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n- [ ] First task\n- [x] Second task";
    open_markdown(&mut doc, md3, md3.find("First").unwrap());
    // Blocks: 0=# Notes, 1=## Section, 2=table, 3=- [ ] First task, 4=- [x] Second task.
    assert!(doc.inner.borrow().get_element_by_id("block-handle-2").is_some());
    assert!(doc.inner.borrow().get_element_by_id("block-handle-3").is_some());
    assert!(doc.inner.borrow().get_element_by_id("block-handle-4").is_some());
    // Drag second checklist item (4) above first checklist item (3), drifting to x = 4.0 (left margin).
    let h4 = bounds(&doc, "block-handle-4");
    let h3b = bounds(&doc, "block-handle-3");
    let h4x = (h4[0] + h4[2] * 0.5) as f32;
    let h4y = (h4[1] + h4[3] * 0.5) as f32;
    let above_h3 = (h3b[1] - 2.0) as f32;
    doc.handle_ui_event(UiEvent::PointerDown(make_ptr(&doc, h4x, h4y, true)));
    render(&mut doc);
    doc.handle_ui_event(UiEvent::PointerMove(make_ptr(&doc, 4.0, above_h3, true)));
    render(&mut doc);
    assert!(doc.inner.borrow().get_element_by_id("block-drop-line").is_some());
    doc.handle_ui_event(UiEvent::PointerUp(make_ptr(&doc, 4.0, above_h3, false)));
    render(&mut doc);
    assert_eq!(
        markdown(&doc),
        "# Notes\n\n## Section\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n- [x] Second task\n- [ ] First task"
    );
    // Drag table (2) above ## Section (1), drifting to x = 4.0 (left margin).
    let ht = bounds(&doc, "block-handle-2");
    let hs = bounds(&doc, "block-handle-1");
    let htx = (ht[0] + ht[2] * 0.5) as f32;
    let hty = (ht[1] + ht[3] * 0.5) as f32;
    let above_hs = (hs[1] - 4.0) as f32;
    doc.handle_ui_event(UiEvent::PointerDown(make_ptr(&doc, htx, hty, true)));
    render(&mut doc);
    doc.handle_ui_event(UiEvent::PointerMove(make_ptr(&doc, 4.0, above_hs, true)));
    render(&mut doc);
    assert!(doc.inner.borrow().get_element_by_id("block-drop-line").is_some());
    doc.handle_ui_event(UiEvent::PointerUp(make_ptr(&doc, 4.0, above_hs, false)));
    render(&mut doc);
    assert!(
        markdown(&doc).starts_with("# Notes\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n## Section"),
        "expected table moved above section, got: {}",
        markdown(&doc)
    );
}

#[test]
fn selecting_on_title_and_formatted_prose_keeps_delimiters_steady_without_wiggle() {
    let mut doc = editor();
    let title_rect = bounds(&doc, "note-title");
    let make_ptr = |d: &DioxusDocument, x: f32, y: f32, held: bool| {
        let mut ptr = d
            .inner
            .borrow()
            .get_node(id(d, "note-title"))
            .unwrap()
            .synthetic_click_event_data(Modifiers::empty());
        ptr.coords.client_x = x;
        ptr.coords.client_y = y;
        ptr.coords.page_x = x;
        ptr.coords.page_y = y;
        ptr.buttons = if held {
            MouseEventButtons::Primary
        } else {
            MouseEventButtons::None
        };
        ptr
    };

    // Dragging directly on an unfocused title expands `# ` once and selects without collapsing.
    let y_mid = (title_rect[1] + title_rect[3] * 0.5) as f32;
    let x_start = (title_rect[0] + 10.0) as f32;
    let x_end = (title_rect[0] + 180.0) as f32;
    doc.handle_ui_event(UiEvent::PointerDown(make_ptr(&doc, x_start, y_mid, true)));
    render(&mut doc);
    assert_eq!(value(&doc, "note-title"), "# Field notes");
    // Drag right, then stray vertically above the title box — `# ` must never collapse/wiggle!
    doc.handle_ui_event(UiEvent::PointerMove(make_ptr(&doc, x_end, y_mid, true)));
    render(&mut doc);
    assert_eq!(value(&doc, "note-title"), "# Field notes");
    doc.handle_ui_event(UiEvent::PointerMove(make_ptr(
        &doc,
        x_end,
        title_rect[1] as f32 - 8.0,
        true,
    )));
    render(&mut doc);
    assert_eq!(value(&doc, "note-title"), "# Field notes");
    doc.handle_ui_event(UiEvent::PointerUp(make_ptr(&doc, x_end, y_mid, false)));
    render(&mut doc);
    assert_eq!(value(&doc, "note-title"), "# Field notes");

    // Dragging on an already-focused title and straying outside must also keep `# ` steady.
    doc.handle_ui_event(UiEvent::PointerDown(make_ptr(
        &doc,
        x_start + 40.0,
        y_mid,
        true,
    )));
    render(&mut doc);
    doc.handle_ui_event(UiEvent::PointerMove(make_ptr(
        &doc,
        x_end,
        title_rect[1] as f32 - 6.0,
        true,
    )));
    render(&mut doc);
    assert_eq!(value(&doc, "note-title"), "# Field notes");
    doc.handle_ui_event(UiEvent::PointerUp(make_ptr(&doc, x_end, y_mid, false)));
    render(&mut doc);
    assert_eq!(value(&doc, "note-title"), "# Field notes");
}

#[test]
fn table_buttons_work_at_retina_and_hover() {
    let config = dioxus_native::DocumentConfig {
        font_ctx: Some(typefaces::notebook_fonts()),
        viewport: Some(Viewport {
            window_size: (1840, 1300),
            hidpi_scale: 2.0,
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut doc = DioxusDocument::new(VirtualDom::new(|| notebook(crate::SAMPLE)), config);
    doc.initial_build();
    render(&mut doc);

    let send_ptr = |d: &mut DioxusDocument, x: f32, y: f32, kind: &str| {
        let mut ptr = d
            .inner
            .borrow()
            .get_node(id(d, "paper"))
            .unwrap()
            .synthetic_click_event_data(Modifiers::empty());
        ptr.coords.client_x = x;
        ptr.coords.client_y = y;
        ptr.coords.page_x = x;
        ptr.coords.page_y = y;
        let ev = match kind {
            "move" => {
                ptr.buttons = MouseEventButtons::None;
                UiEvent::PointerMove(ptr)
            }
            "down" => {
                ptr.buttons = MouseEventButtons::Primary;
                UiEvent::PointerDown(ptr)
            }
            "up" => {
                ptr.buttons = MouseEventButtons::None;
                UiEvent::PointerUp(ptr)
            }
            _ => unreachable!(),
        };
        d.handle_ui_event(ev);
        render(d);
    };
    // Simulate a REAL mouse/trackpad click (including micro-jitter PointerMove while button held!).
    let real_click = |d: &mut DioxusDocument, name: &str| {
        let b = bounds(d, name);
        let cx = (b[0] + b[2] * 0.5) as f32;
        let cy = (b[1] + b[3] * 0.5) as f32;
        send_ptr(d, cx, cy, "move");
        send_ptr(d, cx, cy, "down");
        // Micro-jitter while button is held (previously triggered Blitz DragMode::Selecting!).
        let mut jitter = d
            .inner
            .borrow()
            .get_node(id(d, "paper"))
            .unwrap()
            .synthetic_click_event_data(Modifiers::empty());
        jitter.coords.client_x = cx + 0.5;
        jitter.coords.client_y = cy + 0.5;
        jitter.coords.page_x = cx + 0.5;
        jitter.coords.page_y = cy + 0.5;
        jitter.buttons = MouseEventButtons::Primary;
        d.handle_ui_event(UiEvent::PointerMove(jitter));
        render(d);
        send_ptr(d, cx + 0.5, cy + 0.5, "up");
    };

    // 1. Click cell-0-1-1, then verify real jittery clicks work on in-header chevron & menu items at 2x Retina!
    real_click(&mut doc, "cell-0-1-1");
    real_click(&mut doc, "col-menu-btn-0-1");
    assert!(
        doc.inner.borrow().get_element_by_id("col-menu-0-1").is_some(),
        "real click on col-menu-btn-0-1 must open popover"
    );
    real_click(&mut doc, "col-menu-move-left-0-1");
    assert!(
        markdown(&doc).contains("| Listen for | Stop | Take |"),
        "real click on col-menu-move-left-0-1 failed: {}",
        markdown(&doc)
    );
    real_click(&mut doc, "col-menu-btn-0-0");
    assert!(
        doc.inner.borrow().get_element_by_id("col-menu-0-0").is_some(),
        "real click on col-menu-btn-0-0 must open popover"
    );
    real_click(&mut doc, "col-menu-move-right-0-0");
    assert!(
        markdown(&doc).contains("| Stop | Listen for | Take |"),
        "real click on col-menu-move-right-0-0 failed"
    );
    // 2. Hovering a different cell moves the header chevron + row chevron without clicking!
    let b = bounds(&doc, "slot-0-2-2");
    send_ptr(
        &mut doc,
        (b[0] + b[2] * 0.5) as f32,
        (b[1] + b[3] * 0.5) as f32,
        "move",
    );
    assert!(
        doc.inner.borrow().get_element_by_id("col-menu-btn-0-2").is_some(),
        "hovering col 2 must reveal its in-header chevron"
    );
    assert!(
        doc.inner.borrow().get_element_by_id("col-menu-btn-0-1").is_none(),
        "hovering col 2 must hide col 1's chevron"
    );
    assert!(
        doc.inner.borrow().get_element_by_id("row-menu-btn-0-2").is_some(),
        "hovering row 2 must reveal row 2's chevron"
    );
    assert!(
        doc.inner.borrow().get_element_by_id("row-menu-btn-0-1").is_none(),
        "hovering row 2 must hide row 1's chevron"
    );
    real_click(&mut doc, "col-menu-btn-0-2");
    real_click(&mut doc, "col-menu-insert-right-0-2");
    assert!(markdown(&doc).contains("| Stop | Listen for | Take | Column 4 |"));
    real_click(&mut doc, "cell-0-2-0");
    real_click(&mut doc, "row-menu-btn-0-2");
    real_click(&mut doc, "row-menu-insert-below-0-2");
    assert_eq!(table::tables(&markdown(&doc))[0].rows.len(), 4);
}

#[test]
fn slash_menu_arrow_navigation_is_fast() {
    let mut doc = editor();
    open_markdown(&mut doc, "# Title\n\n\n\n## Section\n\n| A | B |\n| --- | --- |\n| 1 | 2 |", 9);
    press(&mut doc, Key::Character("/".into()), false);
    assert!(doc.inner.borrow().get_element_by_id("slash-menu").is_some());
    let start = std::time::Instant::now();
    for _ in 0..40 {
        press(&mut doc, Key::Named(NamedKey::ArrowDown), false);
    }
    let elapsed = start.elapsed();
    let per_step_ms = elapsed.as_secs_f64() * 1000.0 / 40.0;
    eprintln!("slash menu ArrowDown: {per_step_ms:.2} ms/step (40 steps in {elapsed:?})");
    assert!(
        per_step_ms < 25.0,
        "slash menu navigation too slow: {per_step_ms:.2} ms/step"
    );
}

#[test]
fn anchored_comments_create_reply_apply_undo_and_resolve() {
    let mut doc = editor();
    let sample = super::SAMPLE;
    open_markdown(&mut doc, sample, 0);

    // 1. Seed demo comments and verify Loro Cursor anchors survive edits above them.
    doc.vdom.in_scope(ScopeId::APP, || {
        let mut w = consume_context::<Writing>();
        let seeded = comments::seed_sample_comments(&w.note.peek());
        assert_eq!(seeded.len(), 2);
        w.comments.set(seeded);
        w.active_comment.set(Some(1));
        w.comments_open.set(true);
    });
    render(&mut doc);
    assert!(doc.inner.borrow().get_element_by_id("comments-pane").is_some());
    assert!(doc.inner.borrow().get_element_by_id("comment-thread-1").is_some());
    assert!(doc.inner.borrow().get_element_by_id("comment-hl-1").is_some());

    // Insert text above the table — Loro cursors keep seeded anchors pointing to the exact phrases.
    let insert_at = sample.find("Recorder").unwrap();
    doc.vdom.in_scope(ScopeId::APP, || {
        let mut w = consume_context::<Writing>();
        w.note
            .write()
            .replace(insert_at..insert_at, "Prefix line.\n\n")
            .unwrap();
    });
    render(&mut doc);
    // Apply seeded thread 1 proposal (inside table cell) even after the prefix shift!
    click(&mut doc, "comment-apply-1-1");
    assert!(
        markdown(&doc).contains("| North lock | Water through the gate (XY stereo, 80 Hz HPF) | 45 s |"),
        "applying cell proposal after doc edit above must hit exact cell: {}",
        markdown(&doc)
    );
    assert!(
        doc.inner.borrow().get_element_by_id("comment-undo-1-1").is_some(),
        "applied proposal must stay visible with Undo button"
    );

    // Undo the applied proposal — restores original table cell cleanly.
    click(&mut doc, "comment-undo-1-1");
    assert!(
        markdown(&doc).contains("| North lock | Water through the gate | 45 s |"),
        "undoing proposal must restore original text: {}",
        markdown(&doc)
    );

    // 2. Select text in prose and click Comment in the floating FormatBar.
    let body_text = {
        let dom = doc.inner.borrow();
        let node = dom.get_node(id(&doc, "note-body")).unwrap();
        node.element_data()
            .unwrap()
            .text_input_data()
            .unwrap()
            .editor
            .raw_text()
            .to_owned()
    };
    let start = body_text.find("Rain cover ready").expect("phrase in note-body");
    let end = start + "Rain cover ready".len();
    click(&mut doc, "note-body");
    select(&doc, "note-body", [start, end]);
    send_key(&mut doc, Key::Named(NamedKey::Shift), KeyState::Released, false);
    assert!(doc.inner.borrow().get_element_by_id("format-comment").is_some());
    click(&mut doc, "format-comment");
    assert!(doc.inner.borrow().get_element_by_id("new-comment-card").is_some());

    // Type a comment instruction in #new-comment-input and submit with Post.
    click(&mut doc, "new-comment-input");
    for ch in "replace with Rain cover sealed".chars() {
        press(&mut doc, Key::Character(ch.to_string()), false);
    }
    click(&mut doc, "new-comment-post");
    assert!(doc.inner.borrow().get_element_by_id("new-comment-card").is_none());
    assert!(doc.inner.borrow().get_element_by_id("comment-thread-3").is_some());
    assert!(doc.inner.borrow().get_element_by_id("comment-hl-3").is_some());

    // Apply the AI proposal from thread 3.
    click(&mut doc, "comment-apply-3-1");
    assert!(
        markdown(&doc).contains("Rain cover sealed"),
        "new comment AI proposal must update Markdown: {}",
        markdown(&doc)
    );

    // Reply in thread 3 -> appends follow-up message + new AI proposal.
    click(&mut doc, "comment-reply-3");
    for ch in "Shorten".chars() {
        press(&mut doc, Key::Character(ch.to_string()), false);
    }
    click(&mut doc, "comment-send-3");
    let msg_count = doc.vdom.in_scope(ScopeId::APP, || {
        consume_context::<Writing>()
            .comments
            .read()
            .iter()
            .find(|t| t.id == 3)
            .unwrap()
            .file
            .messages
            .len()
    });
    assert_eq!(msg_count, 4, "reply must append user + Claude messages");

    // Resolve thread 3 -> moves to Resolved tab, clears inline highlight when inactive.
    click(&mut doc, "comment-resolve-3");
    assert!(doc.inner.borrow().get_element_by_id("comment-thread-3").is_none());
    click(&mut doc, "filter-comments-resolved");
    assert!(doc.inner.borrow().get_element_by_id("comment-thread-3").is_some());
    click(&mut doc, "comment-delete-3");
    assert!(doc.inner.borrow().get_element_by_id("comment-thread-3").is_none());

    // Close and re-open via #toggle-comments.
    click(&mut doc, "toggle-comments");
    assert!(doc.inner.borrow().get_element_by_id("comments-pane").is_none());
    click(&mut doc, "toggle-comments");
    assert!(doc.inner.borrow().get_element_by_id("comments-pane").is_some());
}

#[test]
fn comment_proposals_go_stale_instead_of_overwriting_edited_text() {
    let mut doc = editor();
    open_markdown(&mut doc, super::SAMPLE, 0);
    doc.vdom.in_scope(ScopeId::APP, || {
        let mut w = consume_context::<Writing>();
        let seeded = comments::seed_sample_comments(&w.note.peek());
        w.comments.set(seeded);
        w.active_comment.set(Some(1));
        w.comments_open.set(true);
    });
    render(&mut doc);
    // Someone rewrites the commented cell after the suggestion was made.
    let at = markdown(&doc).find("Water through the gate").unwrap();
    doc.vdom.in_scope(ScopeId::APP, || {
        let mut w = consume_context::<Writing>();
        w.note.write().replace(at..at + "Water".len(), "Rain").unwrap();
    });
    render(&mut doc);
    assert!(
        doc.inner.borrow().get_element_by_id("comment-apply-1-1").is_none(),
        "a proposal made for other text cannot be applied"
    );
    let md = markdown(&doc);
    assert!(md.contains("| North lock | Rain through the gate | 45 s |"), "edited text kept: {md}");
    assert!(
        doc.inner.borrow().get_element_by_id("comment-ask-again-1-1").is_some(),
        "a stale proposal offers to ask again"
    );
}

#[test]
fn comment_files_round_trip_and_reattach_after_outside_edits() {
    let mut doc = editor();
    open_markdown(&mut doc, super::SAMPLE, 0);
    let saved = doc.vdom.in_scope(ScopeId::APP, || {
        let mut w = consume_context::<Writing>();
        let mut seeded = comments::seed_sample_comments(&w.note.peek());
        for (i, t) in seeded.iter_mut().enumerate() {
            t.file.id = format!("t{i}");
            for (j, m) in t.file.messages.iter_mut().enumerate() {
                m.id = format!("m{i}{j}");
            }
        }
        seeded[1].file.resolved = true;
        w.comments.set(seeded);
        w.comment_file_now("field-notes.md")
    });
    assert_eq!(saved.threads.len(), 2);
    assert_eq!(saved.threads[0].anchor.exact, "Water through the gate");
    // The page changed elsewhere: the agent added a line above both quotes.
    let edited = format!("Intro added by the agent.\n\n{}", super::SAMPLE);
    let mut other = editor();
    open_markdown(&mut other, &edited, 0);
    other.vdom.in_scope(ScopeId::APP, || {
        let w = consume_context::<Writing>();
        let json = saved.to_json();
        w.load_comment_file(margin_editor_core::comments::Sidecar::parse(&json).unwrap());
        let threads = w.comments.peek();
        let md = w.note.peek().markdown();
        assert_eq!(threads.len(), 2);
        for t in threads.iter() {
            let cursors = t.cursors.as_ref().expect("re-attached");
            let range = w.note.peek().resolve_anchor_in(&md, cursors).unwrap();
            assert_eq!(md[range].to_string(), t.file.anchor.exact);
        }
        assert_eq!(threads[0].file.id, "t0");
        assert!(threads[1].file.resolved);
        assert_eq!(threads[0].file.messages[1].proposal.as_ref().unwrap().new, "Water through the gate (XY stereo, 80 Hz HPF)");
        assert!(threads[0].seen.is_some(), "the agent already answered on this thread");
    });
}

#[test]
fn long_table_cell_keeps_caret_visible_while_scrolling_and_craft_handles_sit_on_edges() {
    use blitz_traits::events::{BlitzWheelDelta, BlitzWheelEvent};

    for scale in [1.0_f32, 2.0_f32] {
        let config = dioxus_native::DocumentConfig {
            font_ctx: Some(typefaces::notebook_fonts()),
            viewport: Some(Viewport {
                window_size: ((820.0 * scale) as u32, (560.0 * scale) as u32),
                hidpi_scale: scale,
                ..Default::default()
            }),
            ..Default::default()
        };
        let long_left = "Water rushing steadily through the north lock gate and all the way down the stone canal basin";
        let long_right = "1234567890-9876543210-1234567890-long-right-aligned-duration-value";
        let md = format!(
            "# Field notes\n\n| Stop | Listen for | Take |\n| :--- | :--- | ---: |\n| North lock | {long_left} | {long_right} |\n| Footbridge | Short | 30 s |\n\nTail"
        );
        let mut doc = DioxusDocument::new(VirtualDom::new(|| notebook(SAMPLE)), config);
        doc.initial_build();
        render(&mut doc);
        open_markdown(&mut doc, &md, md.find(long_left).unwrap());

        // 1. Craft-inspired triple-dot handles sit straddling the top edge of col 1 and left edge of row 1.
        let grid_rect = bounds(&doc, "grid-scroll-0");
        let col1_slot = bounds(&doc, "slot-0-0-1");
        let row1_slot = bounds(&doc, "slot-0-1-0");
        let col_btn = bounds(&doc, "col-menu-btn-0-1");
        let row_btn = bounds(&doc, "row-menu-btn-0-1");
        // Column pill straddles the top border of the table and is centered on column 1.
        assert!(
            col_btn[1] < grid_rect[1] && col_btn[1] + col_btn[3] > grid_rect[1],
            "col pill must straddle top edge: col_btn={col_btn:?}, grid={grid_rect:?}"
        );
        let col_btn_cx = col_btn[0] + col_btn[2] * 0.5;
        let col1_cx = col1_slot[0] + col1_slot[2] * 0.5;
        assert!(
            (col_btn_cx - col1_cx).abs() < 2.0,
            "col pill must be centered over col 1: btn_cx={col_btn_cx}, col_cx={col1_cx}"
        );
        // Row pill straddles the left border of row 1, centered vertically, clear of column 0 text (15px pad).
        assert!(
            row_btn[0] < grid_rect[0] && row_btn[0] + row_btn[2] > grid_rect[0],
            "row pill must straddle left edge: row_btn={row_btn:?}, grid={grid_rect:?}"
        );
        assert!(
            row_btn[0] + row_btn[2] < grid_rect[0] + 12.0,
            "row pill must stay clear of column 0 text: row_btn={row_btn:?}, grid={grid_rect:?}"
        );
        let row_btn_cy = row_btn[1] + row_btn[3] * 0.5;
        let row1_cy = row1_slot[1] + row1_slot[3] * 0.5;
        assert!(
            (row_btn_cy - row1_cy).abs() < 2.0,
            "row pill must be vertically centered on row 1: btn_cy={row_btn_cy}, row_cy={row1_cy}"
        );

        // 2. Typing and moving to the end of a long left-aligned cell scrolls horizontally and keeps #caret-ink inside the cell box!
        let cell_rect = bounds(&doc, "cell-0-1-1");
        let assert_caret_inside = |d: &DioxusDocument, cell_id: &str, label: &str| {
            let crect = bounds(d, cell_id);
            let krect = bounds(d, "caret-ink");
            assert!(
                krect[0] >= crect[0] - 0.5 && krect[0] + krect[2] <= crect[0] + crect[2] + 0.5,
                "scale={scale} {label}: caret-ink {krect:?} escaped cell {cell_id} {crect:?}"
            );
        };
        assert_caret_inside(&doc, "cell-0-1-1", "start of long left cell");
        // Move to end of the long cell.
        let end_pos = md.find(long_left).unwrap() + long_left.len();
        open_markdown(&mut doc, &md, end_pos);
        {
            let dom = doc.inner.borrow();
            let input = dom
                .get_node(id(&doc, "cell-0-1-1"))
                .unwrap()
                .element_data()
                .unwrap()
                .text_input_data()
                .unwrap();
            assert!(
                input.scroll_offset > 20.0,
                "long left cell must scroll horizontally at end (offset={})",
                input.scroll_offset
            );
        }
        assert_caret_inside(&doc, "cell-0-1-1", "end of long left cell");

        // Type extra characters at the end — caret must stay visible and never get lost off the right edge!
        for ch in " XYZ!".chars() {
            press(&mut doc, Key::Character(ch.to_string()), false);
            send_key(&mut doc, Key::Character(ch.to_string()), KeyState::Released, false);
            assert_caret_inside(&doc, "cell-0-1-1", "typing at end of long left cell");
        }
        // Arrow left several steps and verify caret stays visible inside cell.
        for _ in 0..8 {
            press(&mut doc, Key::Named(NamedKey::ArrowLeft), false);
            send_key(&mut doc, Key::Named(NamedKey::ArrowLeft), KeyState::Released, false);
            assert_caret_inside(&doc, "cell-0-1-1", "arrowing left in long left cell");
        }

        // 3. Wheel-scrolling the long cell far to the left hides the off-screen caret cleanly,
        // and the next arrow key brings both scroll_offset and #caret-ink right back into view.
        let cx = (cell_rect[0] + cell_rect[2] * 0.5) as f32;
        let cy = (cell_rect[1] + cell_rect[3] * 0.5) as f32;
        let mut hover_ptr = doc
            .inner
            .borrow()
            .get_node(id(&doc, "cell-0-1-1"))
            .unwrap()
            .synthetic_click_event_data(Modifiers::empty());
        hover_ptr.coords.client_x = cx;
        hover_ptr.coords.client_y = cy;
        hover_ptr.coords.page_x = cx;
        hover_ptr.coords.page_y = cy;
        hover_ptr.buttons = MouseEventButtons::None;
        doc.handle_ui_event(UiEvent::PointerMove(hover_ptr));
        render(&mut doc);
        let wheel = BlitzWheelEvent {
            delta: BlitzWheelDelta::Pixels(500.0, 0.0),
            coords: blitz_traits::events::PointerCoords {
                page_x: cx,
                page_y: cy,
                screen_x: cx,
                screen_y: cy,
                client_x: cx,
                client_y: cy,
            },
            buttons: MouseEventButtons::None,
            mods: Modifiers::empty(),
            element: Default::default(),
        };
        doc.handle_ui_event(UiEvent::Wheel(wheel));
        render(&mut doc);
        doc.vdom
            .in_scope(ScopeId::APP, || consume_context::<Writing>().refresh_boxes());
        render(&mut doc);
        assert!(
            doc.inner.borrow().get_element_by_id("caret-ink").is_none(),
            "caret scrolled completely off-screen by wheel must not dangle outside cell"
        );
        // Pressing ArrowRight snaps scroll_offset back so caret is immediately visible again.
        press(&mut doc, Key::Named(NamedKey::ArrowRight), false);
        send_key(&mut doc, Key::Named(NamedKey::ArrowRight), KeyState::Released, false);
        assert_caret_inside(&doc, "cell-0-1-1", "after recovering from wheel scroll");

        // 4. Right-aligned long cell must stay constrained inside its slot and keep its caret visible!
        let right_end = markdown(&doc).find(long_right).unwrap() + long_right.len();
        let current_md = markdown(&doc);
        open_markdown(&mut doc, &current_md, right_end);
        let slot_r = bounds(&doc, "slot-0-1-2");
        let cell_r = bounds(&doc, "cell-0-1-2");
        assert!(
            cell_r[2] <= slot_r[2],
            "long right-aligned cell ({cell_r:?}) must not overflow slot ({slot_r:?})"
        );
        assert_caret_inside(&doc, "cell-0-1-2", "end of long right-aligned cell");
        for ch in "99".chars() {
            press(&mut doc, Key::Character(ch.to_string()), false);
            send_key(&mut doc, Key::Character(ch.to_string()), KeyState::Released, false);
            assert_caret_inside(&doc, "cell-0-1-2", "typing at end of long right-aligned cell");
        }
    }
}

#[path = "tests/live_workflows.rs"]
mod live_workflows;

#[test]
fn personal_editor_persists_input_and_keeps_diagnostics_on_device() -> anyhow::Result<()> {
    use margin_editor_core::vault::{
        Vault,
        keys::{Identity, Recovery},
    };
    let tmp = tempfile::tempdir()?;
    let pass = tmp.path().join("password");
    std::fs::write(&pass, b"private editor test password")?;
    let path = tmp.path().join("vault");
    let vault = Vault::create(
        &path,
        Identity::from_recovery(&Recovery::new()),
        Some(&pass),
    )?;
    let starter = personal::open_note(&vault, None)?;
    assert!(!vault.shared(&starter)?);
    let field_notes = Note::from_doc(vault.load(&starter)?.0);
    assert_eq!(field_notes.markdown(), super::SAMPLE);
    let threads: Vec<comments::CommentThread> =
        serde_json::from_slice(&vault.private_value(&starter)?.unwrap())?;
    assert_eq!(threads.len(), 2);
    for thread in threads {
        let range = field_notes.resolve_anchor(thread.cursors.as_ref().unwrap()).unwrap();
        assert_eq!(&super::SAMPLE[range], thread.file.anchor.exact);
    }
    let room = vault.new_note("# Diary\n\nHello", true)?;
    let (note, seq) = vault.load(&room)?;
    let session = personal::Session {
        vault: vault.clone(),
        room: room.clone(),
        doc: note,
        seq,
        threads: vec![],
    };
    let dom = VirtualDom::new(|| notebook("")).with_root_context(session);
    let config = dioxus_native::DocumentConfig {
        font_ctx: Some(typefaces::notebook_fonts()),
        viewport: Some(Viewport {
            window_size: (820, 520),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut doc = DioxusDocument::new(dom, config);
    doc.initial_build();
    render(&mut doc);
    click(&mut doc, "note-body");
    select(&doc, "note-body", [5, 5]);
    press(&mut doc, Key::Character("!".into()), false);
    assert_eq!(markdown(&doc), "# Diary\n\nHello!");
    let reopened = Vault::open(&path, Some(&pass))?;
    assert_eq!(
        reopened.load(&room)?.0.get_text("markdown").to_string(),
        "# Diary\n\nHello!"
    );
    press(&mut doc, Key::Named(NamedKey::F8), false);
    click(&mut doc, "save-inspection");
    assert!(reopened.attachments(&room)?.is_empty());
    let files = reopened.attachments(&reopened.inbox())?;
    assert_eq!(files.len(), 1);
    let report = tmp.path().join("export.json");
    reopened.extract(&reopened.inbox(), &files[0].0, &report)?;
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(report)?)?;
    assert_eq!(json["current_markdown"], "# Diary\n\nHello!");
    drop(doc);

    // Launch the real app again, then switch notes through its menu.
    let other = reopened.new_note("# Elsewhere\n\nDifferent text", false)?;
    let launch = personal::open_note(&reopened, None)?;
    assert_eq!(launch, starter);
    personal::DEVICE
        .set((reopened.clone(), launch))
        .map_err(|_| anyhow::anyhow!("personal app already open"))?;
    let config = dioxus_native::DocumentConfig {
        font_ctx: Some(typefaces::notebook_fonts()),
        viewport: Some(Viewport {
            window_size: (820, 520),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut app = DioxusDocument::new(VirtualDom::new(super::app), config);
    app.initial_build();
    render(&mut app);
    assert_eq!(value(&app, "note-title"), "Field notes");
    assert_eq!(value(&app, "cell-0-1-0"), "North lock");
    assert!(app.inner.borrow().get_element_by_id("comment-thread-1").is_some());
    click(&mut app, "vault-notes");
    click(&mut app, &format!("vault-room-{other}"));
    assert_eq!(field(&app, id(&app, "note-body")), "Different text");
    assert_eq!(personal::open_note(&reopened, None)?, other);
    click(&mut app, "vault-notes");
    click(&mut app, &format!("vault-room-{room}"));
    assert_eq!(field(&app, id(&app, "note-body")), "Hello!");
    assert_eq!(personal::open_note(&reopened, None)?, room);
    Ok(())
}
