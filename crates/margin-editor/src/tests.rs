const SAMPLE: &str = "# Field notes\n\nRecorder packed [ ]. Rain cover ready [x].\n\n- [ ] Pack the windscreen\n- [x] Charge the recorder\n\nThe literal example is `[ ]`.";

use crate::*;
use crate::testing::*;
use blitz_dom::Document;
use blitz_traits::events::{BlitzImeEvent, BlitzKeyEvent, KeyState, MouseEventButtons, UiEvent};
use blitz_traits::shell::Viewport;
use dioxus_native::{DioxusDocument, NodeId};

fn editor() -> DioxusDocument {
    let config = dioxus_native::DocumentConfig {
        font_ctx: Some(typefaces::fonts()),
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

fn hover(doc: &mut DioxusDocument, name: &str) {
    let element = id(doc, name);
    let mut pointer = doc
        .inner
        .borrow()
        .get_node(element)
        .unwrap()
        .synthetic_click_event_data(Modifiers::empty());
    {
        let dom = doc.inner.borrow();
        let rect = dom.get_client_bounding_rect(element).unwrap();
        let scroll = dom.viewport_scroll();
        let x = (rect.x + rect.width * 0.5) as f32;
        let y = (rect.y + rect.height * 0.5) as f32;
        pointer.coords.page_x = x + scroll.x as f32;
        pointer.coords.client_x = x;
        pointer.coords.screen_x = x;
        pointer.coords.page_y = y + scroll.y as f32;
        pointer.coords.client_y = y;
        pointer.coords.screen_y = y;
    }
    doc.handle_ui_event(UiEvent::PointerMove(pointer));
    render(doc);
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
    assert!(has(&doc, &first_id));
    assert!(has(&doc, &other_id));

    // Keyboard cursor reveals only the adjacent checkbox.
    for pos in [open - 1, open, open + 1, open + 2, open + 3, open + 4, other + 1] {
        open_body_at(&mut doc, [pos; 2]);
        assert_eq!(
            has(&doc, &first_id),
            !(open..=open + 3).contains(&pos),
            "first token at {pos}"
        );
        assert_eq!(
            has(&doc, &other_id),
            !(other..=other + 3).contains(&pos),
            "second token at {pos}"
        );
    }

    // Mouse-clicking a resting checkbox toggles it while keeping the togglebox visible!
    open_body_at(&mut doc, [7; 2]);
    assert!(has(&doc, &first_id));
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
    assert!(has(&doc, &first_id), "mouse click keeps togglebox visible");
    // Clicking again toggles it back off cleanly!
    click_at(&mut doc, "note-body", Some(click_pt));
    assert_eq!(&markdown(&doc)[open..open + 3], "[ ]");
    assert!(has(&doc, &first_id));
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
    // ArrowDown on the last line moves to the end of the line (and Shift+ArrowDown selects to end);
    // ArrowUp on the first line moves to the start of the line (and Shift+ArrowUp selects to start).
    open_markdown(&mut doc, md, md.find("End").unwrap() + 1);
    assert_focus(&doc, "note-body-2", [2; 2]);
    chord(&mut doc, Key::Named(NamedKey::ArrowDown), Modifiers::SHIFT);
    assert_focus(&doc, "note-body-2", [2, 4]);
    open_markdown(&mut doc, md, md.find("End").unwrap() + 1);
    press(&mut doc, Key::Named(NamedKey::ArrowDown), false);
    assert_focus(&doc, "note-body-2", [4; 2]);
    press(&mut doc, Key::Named(NamedKey::ArrowDown), false);
    assert_focus(&doc, "note-body-2", [4; 2]);
    open_markdown(&mut doc, md, 4);
    assert_focus(&doc, "note-title", [4; 2]);
    chord(&mut doc, Key::Named(NamedKey::ArrowUp), Modifiers::SHIFT);
    assert_focus(&doc, "note-title", [4, 0]);
    open_markdown(&mut doc, md, 4);
    press(&mut doc, Key::Named(NamedKey::ArrowUp), false);
    assert_focus(&doc, "note-title", [0; 2]);
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

/// In Rich mode the fences of a code block are hidden; Backspace beside
/// them never joins a fence line to the text next to it.
#[test]
fn backspace_beside_a_hidden_fence_keeps_the_code_block() {
    let mut doc = editor();
    let md = "# Note\n\nBefore\n```rust\nfn main() {}\n```\nAfter";
    open_markdown(&mut doc, md, md.find("After").unwrap());
    click(&mut doc, "rich-mode");
    for at in [md.find("After").unwrap(), md.find("fn main").unwrap()] {
        let local = body_display(&doc, at);
        select(&doc, "note-body", [local; 2]);
        backspace_text(&mut doc);
        assert_eq!(markdown(&doc), md, "Backspace at {at}");
        assert!(value(&doc, "note-body").contains("fn main() {}\nAfter"), "the field shows the note again");
    }
}

/// A finger leaves no hover: after a tap, the selected cell's column and
/// row show their handles, and typing does not hide them. A mouse press
/// goes back to showing them on hover only.
#[test]
fn a_tapped_cell_shows_its_column_and_row_handles() {
    let mut doc = editor();
    let md = "# Note\n\n| A | B | C |\n| --- | --- | --- |\n| one | two | three |\n\nBody";
    open_markdown(&mut doc, md, 2);
    tap(&mut doc, "cell-0-1-2");
    assert!(has(&doc, "col-menu-btn-0-2") && has(&doc, "row-menu-btn-0-1"));
    commit_text(&mut doc, "!");
    assert!(has(&doc, "col-menu-btn-0-2") && has(&doc, "row-menu-btn-0-1"), "typing keeps them");
    // The header row has no row handle.
    tap(&mut doc, "cell-0-0-1");
    assert!(has(&doc, "col-menu-btn-0-1") && !has(&doc, "row-menu-btn-0-1"));
    // A mouse press: the handles follow the hover again, and typing hides them.
    click(&mut doc, "cell-0-1-0");
    commit_text(&mut doc, "!");
    assert!(!has(&doc, "col-menu-btn-0-0") && !has(&doc, "row-menu-btn-0-1"));
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
    // Navigating into a table with the keyboard keeps the `...` handles hidden.
    assert!(doc.inner.borrow().get_element_by_id("col-menu-btn-0-0").is_none());
    press(&mut doc, Key::Named(NamedKey::ArrowDown), false);
    assert!(doc.inner.borrow().get_element_by_id("row-menu-btn-0-1").is_none());
    // Hovering a cell reveals the column and row `...` handles without shifting layout.
    hover(&mut doc, "slot-0-1-0");
    assert!(doc.inner.borrow().get_element_by_id("col-menu-btn-0-0").is_some());
    assert!(doc.inner.borrow().get_element_by_id("row-menu-btn-0-1").is_some());
    assert_eq!(bounds(&doc, "note-body-1"), earlier);
    // Typing in the cell hides the `...` handles again; moving the mouse brings them back.
    commit_text(&mut doc, "!");
    assert!(doc.inner.borrow().get_element_by_id("col-menu-btn-0-0").is_none());
    assert!(doc.inner.borrow().get_element_by_id("row-menu-btn-0-1").is_none());
    hover(&mut doc, "slot-0-1-0");
    assert!(doc.inner.borrow().get_element_by_id("col-menu-btn-0-0").is_some());
    assert!(doc.inner.borrow().get_element_by_id("row-menu-btn-0-1").is_some());
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
fn a_click_in_the_empty_part_of_a_tall_row_stays_in_that_row() {
    let mut doc = editor();
    let long = "Water through the gate, then the weir, then the long reach under the footbridge where the swans keep their distance";
    let md = format!("# Stops\n\n| Stop | Listen for | Take |\n| --- | --- | ---: |\n| Lock | {long} | 45 s |\n| Bridge | Steps | 30 s |\n\nTail");
    open_markdown(&mut doc, &md, 2);
    let row = bounds(&doc, "slot-0-1-0");
    let cell = bounds(&doc, "cell-0-1-0");
    assert!(row[3] > cell[3] * 2.0, "the row is taller than its short cell: row={row:?}, cell={cell:?}");
    // Just inside the row's top and bottom edges, far from its vertically centred text.
    for dy in [4.0, row[3] as f32 - 4.0] {
        click_at(&mut doc, "slot-0-1-0", Some((20.0, dy)));
        assert_eq!(
            doc.inner.borrow().get_focussed_node_id(),
            Some(id(&doc, "cell-0-1-0")),
            "a click {dy}px into the row"
        );
    }
}

#[test]
fn a_word_wider_than_its_column_breaks_and_its_row_grows() {
    let mut doc = editor();
    let word = "a".repeat(200);
    let md = format!("# Hashes\n\n| Name | Digest | Size |\n| --- | --- | ---: |\n| First | {word} | 4 kB |\n| Second | short | 2 kB |\n\nTail");
    open_markdown(&mut doc, &md, md.find(&word).unwrap() + word.len());
    let lines = {
        let dom = doc.inner.borrow();
        let cell = dom.get_node(id(&doc, "cell-0-1-1")).unwrap();
        cell.element_data().unwrap().text_input_data().unwrap().editor.try_layout().unwrap().len()
    };
    assert!(lines > 1, "the word breaks inside its column");
    let (long, short) = (bounds(&doc, "slot-0-1-1"), bounds(&doc, "slot-0-2-1"));
    assert!(long[3] > short[3] * 1.5, "the row grows with its broken word: {long:?} against {short:?}");
    // The caret after the word's last letter is drawn inside the cell.
    let (cell, caret) = (bounds(&doc, "cell-0-1-1"), bounds(&doc, "caret-ink"));
    assert!(
        caret[0] >= cell[0] - 0.5 && caret[0] + caret[2] <= cell[0] + cell[2] + 0.5,
        "caret {caret:?} inside cell {cell:?}"
    );
}

#[test]
fn a_chord_of_enter_in_a_cell_leaves_its_text_alone() {
    let mut doc = editor();
    let md = "# Stops\n\n| Stop | Take |\n| --- | --- |\n| North lock | 45 s |";
    open_markdown(&mut doc, md, md.find("lock").unwrap());
    for modifiers in [Modifiers::META, Modifiers::CONTROL, Modifiers::ALT] {
        chord(&mut doc, Key::Named(NamedKey::Enter), modifiers);
        assert_eq!(markdown(&doc), md, "{modifiers:?}+Enter");
    }
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
            .in_scope(ScopeId::APP, || *consume_context::<Writing>().grid.cells.peek())
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
        consume_context::<Writing>().grid.cells.peek().is_none()
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
            font_ctx: Some(typefaces::fonts()),
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

/// An editor in Source view holding `md`, with the caret at its start.
fn source_document(md: &str) -> DioxusDocument {
    let mut doc = editor();
    click(&mut doc, "source-mode");
    select(&doc, "markdown", [0, SAMPLE.len()]);
    commit_text(&mut doc, md);
    select(&doc, "markdown", [0; 2]);
    assert_eq!(value(&doc, "markdown"), md);
    doc
}

#[test]
fn source_uses_one_input_across_blank_lines_headings_tables_and_mode_changes() {
    let md = "# A\n\n## B\n\n| X | Y |\n| --- | --- |\n| É | Q |\n\nTail";
    let mut doc = source_document(md);
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
    // Leaving for Live and coming back keeps the selection.
    click(&mut doc, "live-mode");
    assert!(doc.inner.borrow().get_element_by_id("markdown").is_none());
    click(&mut doc, "source-mode");
    assert_focus(&doc, "markdown", [end, 2]);
    commit_text(&mut doc, "Hi");
    assert_eq!(value(&doc, "markdown"), format!("# Hi{}", &md[end..]));
    click(&mut doc, "undo");
    assert_eq!(value(&doc, "markdown"), md);
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
    for mode in ["live-mode", "source-mode"] {
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
fn delete_selected_column_is_scoped_undoable_and_keeps_a_caret() {
    let md = "| Keep | Value |\n| --- | --- |\n| α | 8 |\n\n| A | B | C |\n| :--- | --- | ---: |\n| a\\|b | 🌿 | c |\n| Only |";
    let start = md.find("| A |").unwrap();
    let mut doc = source_document(md);
    click(&mut doc, "live-mode");
    let remove = "col-menu-delete-1-1";
    assert!(doc.inner.borrow().get_element_by_id(remove).is_none());
    click(&mut doc, "cell-1-1-1");
    // Destructive delete lives in the column menu, not the floating bar.
    assert!(doc.inner.borrow().get_element_by_id("delete-column-1").is_none());
    click(&mut doc, "col-menu-btn-1-1");
    click(&mut doc, remove);
    let expected = md
        .replace("| A | B | C |", "| A | C |")
        .replace("| :--- | --- | ---: |", "| :--- | ---: |")
        .replace("| 🌿 | c |", "| c |");
    assert_focus(&doc, "cell-1-1-1", [0, 1]);
    hover(&mut doc, "slot-1-1-1");
    click(&mut doc, "col-menu-btn-1-1");
    click(&mut doc, remove);
    hover(&mut doc, "slot-1-1-0");
    click(&mut doc, "col-menu-btn-1-0");
    assert_eq!(
        doc.inner
            .borrow()
            .get_node(id(&doc, "col-menu-delete-1-0"))
            .unwrap()
            .attr("aria-disabled".into()),
        Some("true")
    );
    click(&mut doc, "source-mode");
    let last = value(&doc, "markdown");
    assert_eq!(&last[..start], &md[..start]);
    assert_eq!(table::tables(&last)[1].width(), 1);
    click(&mut doc, "undo");
    assert_eq!(value(&doc, "markdown"), expected);
    click(&mut doc, "undo");
    assert_eq!(value(&doc, "markdown"), md);
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
    // Keyboard navigation hides the `...` handles; hovering a cell reveals only that column & row.
    assert!(doc.inner.borrow().get_element_by_id("col-menu-btn-0-1").is_none());
    assert!(doc.inner.borrow().get_element_by_id("row-menu-btn-0-2").is_none());
    hover(&mut doc, "slot-0-2-1");
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
    hover(&mut doc, "slot-0-2-0");
    click(&mut doc, "col-menu-btn-0-0");
    click(&mut doc, "col-menu-move-right-0-0");
    assert!(markdown(&doc).contains("| Take | Stop |"));
    hover(&mut doc, "slot-0-2-1");
    click(&mut doc, "col-menu-btn-0-1");
    click(&mut doc, "col-menu-move-left-0-1");
    assert!(markdown(&doc).contains("| Stop | Take |"));
    // Alignment icons live inside the in-header chevron menu.
    click(&mut doc, "cell-0-1-1");
    assert!(doc.inner.borrow().get_element_by_id("align-right-0").is_none());
    click(&mut doc, "col-menu-btn-0-1");
    click(&mut doc, "align-right-0");
    assert!(markdown(&doc).contains("| --- | ---: |"));
    hover(&mut doc, "slot-0-1-1");
    click(&mut doc, "col-menu-btn-0-1");
    click(&mut doc, "align-center-0");
    assert!(markdown(&doc).contains("| --- | :---: |"));
    hover(&mut doc, "slot-0-1-1");
    click(&mut doc, "col-menu-btn-0-1");
    click(&mut doc, "align-left-0");
    assert!(markdown(&doc).contains("| --- | :--- |"));
    hover(&mut doc, "slot-0-1-1");
    click(&mut doc, "col-menu-btn-0-1");
    click(&mut doc, "col-menu-insert-right-0-1");
    assert!(markdown(&doc).contains("| Stop | Take | Column 3 |"));
    hover(&mut doc, "slot-0-1-2");
    click(&mut doc, "col-menu-btn-0-2");
    click(&mut doc, "col-menu-delete-0-2");
    assert!(markdown(&doc).contains("| Stop | Take |"));
    click(&mut doc, "cell-0-1-0");
    click(&mut doc, "col-menu-btn-0-0");
    click(&mut doc, "col-menu-insert-left-0-0");
    assert!(markdown(&doc).contains("| Column 3 | Stop | Take |"));
    assert!(doc.inner.borrow().get_element_by_id("delete-column-0").is_none());
    hover(&mut doc, "slot-0-1-0");
    click(&mut doc, "col-menu-btn-0-0");
    click(&mut doc, "col-menu-delete-0-0");
    assert!(markdown(&doc).contains("| Stop | Take |"));
    // Row chevron menu actions work cleanly!
    click(&mut doc, "cell-0-1-0");
    assert!(doc.inner.borrow().get_element_by_id("delete-row-0").is_none());
    click(&mut doc, "row-menu-btn-0-1");
    click(&mut doc, "row-menu-insert-above-0-1");
    assert!(markdown(&doc).contains("| --- | :--- |\n|  |  |\n| Bridge | 30 s |"));
    hover(&mut doc, "slot-0-1-0");
    click(&mut doc, "row-menu-btn-0-1");
    click(&mut doc, "row-menu-delete-0-1");
    assert!(!markdown(&doc).contains("|  |  |"));
    hover(&mut doc, "slot-0-1-0");
    click(&mut doc, "row-menu-btn-0-1");
    click(&mut doc, "row-menu-move-down-0-1");
    assert!(markdown(&doc).contains("| Lock | 45 s |\n| Bridge | 30 s |"));
    hover(&mut doc, "slot-0-2-0");
    click(&mut doc, "row-menu-btn-0-2");
    click(&mut doc, "row-menu-move-up-0-2");
    assert!(markdown(&doc).contains("| Bridge | 30 s |\n| Lock | 45 s |"));
    hover(&mut doc, "slot-0-1-0");
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
    let mut fonts = crate::typefaces::fonts();
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
        font_ctx: Some(typefaces::fonts()),
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
    let b0 = bounds(&doc, "slot-0-1-0");
    send_ptr(
        &mut doc,
        (b0[0] + b0[2] * 0.5) as f32,
        (b0[1] + b0[3] * 0.5) as f32,
        "move",
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

/// A read-only notebook is read, selected and commented on, never changed:
/// keys, a committed IME text and a checkbox leave the note and the field as
/// they were, and the agent's proposal waits for an editor.
#[test]
fn a_read_only_notebook_takes_comments_but_no_edits() {
    fn reader() -> Element {
        notebook_view(SAMPLE, NotebookOptions { view: View::Rich, read_only: true, ..Default::default() })
    }
    let config = dioxus_native::DocumentConfig {
        font_ctx: Some(typefaces::fonts()),
        viewport: Some(Viewport { window_size: (820, 520), ..Default::default() }),
        ..Default::default()
    };
    let mut doc = DioxusDocument::new(VirtualDom::new(reader), config);
    doc.initial_build();
    render(&mut doc);
    assert!(has(&doc, "toggle-comments"));
    for gone in ["rich-mode", "insert-table", "undo", "toggle-debug"] {
        assert!(!has(&doc, gone), "{gone} is for editing");
    }

    let open = SAMPLE.find("[ ]").unwrap();
    open_body_at(&mut doc, [open + 1; 2]);
    let field = field_showing(&doc, "Recorder packed").unwrap();
    let shown = value(&doc, &field);
    press(&mut doc, Key::Character(" ".into()), false);
    press(&mut doc, Key::Character("q".into()), false);
    click(&mut doc, &field);
    commit_text(&mut doc, "typed");
    press(&mut doc, Key::Named(NamedKey::Backspace), false);
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert_eq!(markdown(&doc), SAMPLE);
    assert_eq!(value(&doc, &field), shown);

    // Selecting offers Comment alone, centred on the selection.
    let quote = "Pack the windscreen";
    let name = field_showing(&doc, quote).unwrap();
    let start = value(&doc, &name).find(quote).unwrap();
    click(&mut doc, &name);
    select(&doc, &name, [start, start + quote.len()]);
    send_key(&mut doc, Key::Named(NamedKey::Shift), KeyState::Released, false);
    assert!(has(&doc, "format-comment") && !has(&doc, "format-0"), "only Comment");
    let [field_x, ..] = bounds(&doc, &name);
    let (first, last) = (glyph_position(&doc, &name, start).0, glyph_position(&doc, &name, start + quote.len()).0);
    let middle = field_x + f64::from(first + last) / 2.0;
    let [bar_x, _, bar_w, _] = bounds(&doc, "format-bar");
    assert!((bar_x + bar_w / 2.0 - middle).abs() < 8.0, "bar {bar_x}+{bar_w} is centred on {middle}");

    comment_on(&mut doc, quote, "Shorten this to a few words.");
    let answered = |doc: &DioxusDocument| has(doc, "comment-thread-1") && text(doc, "comment-thread-1").contains("Open the page in the editor");
    settle(&mut doc, std::time::Duration::from_secs(2), answered);
    assert!(answered(&doc), "the stand-in answered with a proposal: {}", text(&doc, "comment-thread-1"));
    assert!(!has(&doc, "comment-apply-1-1"), "no Apply in a read-only notebook");
    assert_eq!(markdown(&doc), SAMPLE);
}

#[test]
fn anchored_comments_create_reply_apply_undo_and_resolve() {
    let mut doc = editor();
    let sample = crate::SAMPLE;
    open_markdown(&mut doc, sample, 0);

    // 1. Seed demo comments and verify Loro Cursor anchors survive edits above them.
    doc.vdom.in_scope(ScopeId::APP, || {
        let mut w = consume_context::<Writing>();
        let seeded = comments::seed_sample_comments(&w.note.peek());
        assert_eq!(seeded.len(), 2);
        w.discussion.threads.set(seeded);
        w.discussion.active.set(Some(1));
        w.discussion.open.set(true);
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
            .discussion.threads
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
    open_markdown(&mut doc, crate::SAMPLE, 0);
    doc.vdom.in_scope(ScopeId::APP, || {
        let mut w = consume_context::<Writing>();
        let seeded = comments::seed_sample_comments(&w.note.peek());
        w.discussion.threads.set(seeded);
        w.discussion.active.set(Some(1));
        w.discussion.open.set(true);
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
    open_markdown(&mut doc, crate::SAMPLE, 0);
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
        w.discussion.threads.set(seeded);
        w.comment_file_now("field-notes.md")
    });
    assert_eq!(saved.threads.len(), 2);
    assert_eq!(saved.threads[0].anchor.exact, "Water through the gate");
    // The page changed elsewhere: the agent added a line above both quotes.
    let edited = format!("Intro added by the agent.\n\n{}", crate::SAMPLE);
    let mut other = editor();
    open_markdown(&mut other, &edited, 0);
    other.vdom.in_scope(ScopeId::APP, || {
        let w = consume_context::<Writing>();
        let json = saved.to_json();
        w.load_comment_file(margin_core::comments::Sidecar::parse(&json).unwrap());
        let threads = w.discussion.threads.peek();
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
fn long_table_cell_wraps_around_its_caret_and_craft_handles_sit_on_edges() {
    use blitz_traits::events::{BlitzWheelDelta, BlitzWheelEvent};

    for scale in [1.0_f32, 2.0_f32] {
        let config = dioxus_native::DocumentConfig {
            font_ctx: Some(typefaces::fonts()),
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
        hover(&mut doc, "slot-0-1-1");

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

        // 2. A long left-aligned cell wraps instead of scrolling: its row grows, the field never
        // scrolls sideways, and #caret-ink stays inside the cell box wherever the caret goes.
        let cell_rect = bounds(&doc, "cell-0-1-1");
        let short_cell = bounds(&doc, "cell-0-2-1");
        assert!(
            cell_rect[3] > short_cell[3] * 1.5,
            "scale={scale} long left cell must wrap onto more lines: long={cell_rect:?}, short={short_cell:?}"
        );
        let scroll_offset = |d: &DioxusDocument, cell_id: &str| {
            let dom = d.inner.borrow();
            dom.get_node(id(d, cell_id))
                .unwrap()
                .element_data()
                .unwrap()
                .text_input_data()
                .unwrap()
                .scroll_offset
        };
        let assert_caret_inside = |d: &DioxusDocument, cell_id: &str, label: &str| {
            let crect = bounds(d, cell_id);
            let krect = bounds(d, "caret-ink");
            assert!(
                krect[0] >= crect[0] - 0.5
                    && krect[0] + krect[2] <= crect[0] + crect[2] + 0.5
                    && krect[1] >= crect[1] - 0.5
                    && krect[1] + krect[3] <= crect[1] + crect[3] + 0.5,
                "scale={scale} {label}: caret-ink {krect:?} escaped cell {cell_id} {crect:?}"
            );
            assert_eq!(scroll_offset(d, cell_id), 0.0, "scale={scale} {label}: a wrapped cell never scrolls");
        };
        assert_caret_inside(&doc, "cell-0-1-1", "start of long left cell");
        // Move to end of the long cell: the caret is on its last wrapped line.
        let end_pos = md.find(long_left).unwrap() + long_left.len();
        open_markdown(&mut doc, &md, end_pos);
        assert_caret_inside(&doc, "cell-0-1-1", "end of long left cell");
        let first_line_caret = cell_rect[1];
        assert!(
            bounds(&doc, "caret-ink")[1] > first_line_caret + short_cell[3] * 0.5,
            "scale={scale} the caret at the end of a wrapped cell sits below its first line"
        );

        // Type extra characters at the end: the caret stays visible inside the growing cell.
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

        // 3. A sideways wheel over the cell has nothing to scroll: the caret stays put, in view.
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
        assert_caret_inside(&doc, "cell-0-1-1", "after a sideways wheel");
        press(&mut doc, Key::Named(NamedKey::ArrowRight), false);
        send_key(&mut doc, Key::Named(NamedKey::ArrowRight), KeyState::Released, false);
        assert_caret_inside(&doc, "cell-0-1-1", "arrowing after a sideways wheel");

        // 4. A long right-aligned cell wraps inside its slot too and keeps its caret visible.
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

#[test]
fn link_card_preview_copy_edit_remove_dismiss_and_anchor_jump() {
    const MD: &str = "# Field notes\n\nSee [Work Log](brain/log \"Append-only audit trail. Add one dated entry per turn.\") and [canal guide](https://example.com/guide) or jump to [Listening stops](#listening-stops).\n\n## Listening stops\n\nMorning fog along the lock gate.\n\n| Stop | Notes |\n| :--- | :--- |\n| [North lock](locks/north) | 45 s |";
    let config = dioxus_native::DocumentConfig {
        font_ctx: Some(typefaces::fonts()),
        viewport: Some(Viewport {
            window_size: (820, 560),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut doc = DioxusDocument::new(VirtualDom::new(|| notebook(MD)), config);
    doc.initial_build();
    render(&mut doc);
    let clipboard = std::sync::Arc::new(TestClipboard::default());
    doc.inner.borrow_mut().shell_provider = clipboard.clone();

    let text_of = |doc: &DioxusDocument, elem: &str| -> String {
        let dom = doc.inner.borrow();
        let node_id = dom.get_element_by_id(elem).unwrap();
        let mut out = String::new();
        fn walk(dom: &blitz_dom::BaseDocument, id: NodeId, out: &mut String) {
            if let Some(node) = dom.get_node(id) {
                if let Some(t) = node.text_data() {
                    out.push_str(&t.content);
                }
                for &child in &node.children {
                    walk(dom, child, out);
                }
            }
        }
        walk(&dom, node_id, &mut out);
        out
    };

    let place_caret = |doc: &mut DioxusDocument, pos: usize| {
        doc.vdom.in_scope(ScopeId::APP, || {
            let w = consume_context::<Writing>();
            w.open(w.field_at(pos), [pos; 2]);
        });
        render(doc);
    };

    // 1. Resting cursor outside links shows no link card.
    place_caret(&mut doc, MD.find("See ").unwrap() + 1);
    assert!(doc.inner.borrow().get_element_by_id("link-card").is_none());

    // 2. Caret inside internal page link opens #link-card with target, title, folder, excerpt, backlinks.
    let work_pos = MD.find("Work Log").unwrap() + 2;
    place_caret(&mut doc, work_pos);
    assert!(
        doc.inner.borrow().get_element_by_id("link-card").is_some(),
        "caret inside link should open #link-card"
    );
    let card_text = text_of(&doc, "link-card");
    assert!(card_text.contains("brain/log"), "card text: {card_text}");
    assert!(card_text.contains("Work Log"), "card text: {card_text}");
    assert!(card_text.contains("brain"), "card text: {card_text}");
    assert!(
        card_text.contains("Append-only audit trail"),
        "card text: {card_text}"
    );
    assert!(card_text.contains("1 backlink"), "card text: {card_text}");

    // 3. Copy button copies the link target URL/path to clipboard.
    click(&mut doc, "link-card-copy");
    assert_eq!(clipboard.0.lock().unwrap().as_str(), "brain/log");

    // 4. Non-empty selection inside a link prefers FormatBar over LinkCard.
    let sel_start = MD.find("Work Log").unwrap();
    open_body_at(&mut doc, [sel_start, sel_start + 4]);
    assert!(doc.inner.borrow().get_element_by_id("format-bar").is_some());
    assert!(doc.inner.borrow().get_element_by_id("link-card").is_none());

    // 5. Escape dismisses the card until caret leaves and re-enters the link.
    place_caret(&mut doc, work_pos);
    assert!(doc.inner.borrow().get_element_by_id("link-card").is_some());
    press(&mut doc, Key::Named(NamedKey::Escape), false);
    assert!(doc.inner.borrow().get_element_by_id("link-card").is_none());
    // Still dismissed while moving within the same link.
    place_caret(&mut doc, work_pos + 2);
    assert!(doc.inner.borrow().get_element_by_id("link-card").is_none());
    // Leaving the link and returning reopens it.
    place_caret(&mut doc, MD.find("See ").unwrap() + 1);
    place_caret(&mut doc, work_pos);
    assert!(doc.inner.borrow().get_element_by_id("link-card").is_some());

    // 6. Edit button opens inline form, saves in one undoable Loro edit, and Undo restores.
    click(&mut doc, "link-card-edit");
    assert!(doc.inner.borrow().get_element_by_id("link-edit-label").is_some());
    assert!(doc.inner.borrow().get_element_by_id("link-edit-url").is_some());
    doc.vdom.in_scope(ScopeId::APP, || {
        let w = consume_context::<Writing>();
        w.save_edited_link("Daily Log", "brain/daily-log");
    });
    render(&mut doc);
    assert_eq!(
        markdown(&doc),
        MD.replace(
            "[Work Log](brain/log \"Append-only audit trail. Add one dated entry per turn.\")",
            "[Daily Log](brain/daily-log \"Append-only audit trail. Add one dated entry per turn.\")",
        )
    );
    click(&mut doc, "undo");
    assert_eq!(markdown(&doc), MD);

    // 7. Remove button unwraps [label](url) -> label in one undoable edit, and Undo restores.
    place_caret(&mut doc, work_pos);
    assert!(doc.inner.borrow().get_element_by_id("link-card").is_some());
    click(&mut doc, "link-card-remove");
    assert_eq!(
        markdown(&doc),
        MD.replace(
            "[Work Log](brain/log \"Append-only audit trail. Add one dated entry per turn.\")",
            "Work Log",
        )
    );
    assert!(doc.inner.borrow().get_element_by_id("link-card").is_none());
    click(&mut doc, "undo");
    assert_eq!(markdown(&doc), MD);

    // 8. In-document heading anchor resolves heading & prose excerpt, and clicking target jumps to it.
    let anchor_pos = MD.find("#listening-stops").unwrap() - 3;
    place_caret(&mut doc, anchor_pos);
    let anchor_text = text_of(&doc, "link-card");
    assert!(anchor_text.contains("#listening-stops"), "{anchor_text}");
    assert!(anchor_text.contains("Listening stops"), "{anchor_text}");
    assert!(
        anchor_text.contains("Morning fog along the lock gate"),
        "{anchor_text}"
    );
    click(&mut doc, "link-card-open");
    assert_eq!(
        doc.inner.borrow().get_focussed_node_id(),
        Some(id(&doc, "note-heading-0"))
    );

    // 9. Rich mode & table cell links also open #link-card cleanly.
    click(&mut doc, "rich-mode");
    let cell_pos = MD.find("North lock").unwrap() + 2;
    place_caret(&mut doc, cell_pos);
    assert!(doc.inner.borrow().get_element_by_id("link-card").is_some());
    let cell_card = text_of(&doc, "link-card");
    assert!(cell_card.contains("locks/north"), "{cell_card}");
    assert!(cell_card.contains("North lock"), "{cell_card}");
    assert!(cell_card.contains("locks"), "{cell_card}");

    // 10. External URL link shows host and External link metadata.
    let ext_pos = MD.find("canal guide").unwrap() + 2;
    place_caret(&mut doc, ext_pos);
    let ext_card = text_of(&doc, "link-card");
    assert!(ext_card.contains("example.com/guide"), "{ext_card}");
    assert!(ext_card.contains("canal guide"), "{ext_card}");
    assert!(ext_card.contains("External link"), "{ext_card}");

    // 11. Round-tripping from Source mode into a link also opens #link-card immediately.
    open_markdown(&mut doc, MD, work_pos);
    assert!(
        doc.inner.borrow().get_element_by_id("link-card").is_some(),
        "switching from Source mode into a link should open #link-card"
    );

    // 12. Verify popover placement: sits cleanly below the full link span (even when expanded
    // across lines in Live mode) and tracks viewport scroll accurately.
    let card_rect = bounds(&doc, "link-card");
    let body_rect = bounds(&doc, "note-body");
    assert!(
        card_rect[1] > body_rect[1] + 20.0 && card_rect[0] >= 8.0,
        "link card should sit below the link in Live mode: card={card_rect:?} body={body_rect:?}"
    );
}

#[test]
fn enter_continues_and_exits_blockquotes_in_live_rich_and_source() {
    let mut doc = editor();
    click(&mut doc, "note-body");
    select(&doc, "note-body", [0, value(&doc, "note-body").len()]);

    // 1. Enter continues a blockquote; Enter on empty `> ` exits cleanly with a blank line.
    commit_text(&mut doc, "> First line");
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert_eq!(value(&doc, "note-body"), "> First line\n> ");
    commit_text(&mut doc, "Second line");
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert_eq!(value(&doc, "note-body"), "> First line\n> Second line\n> ");
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    commit_text(&mut doc, "Outside");
    assert_eq!(
        value(&doc, "note-body"),
        "> First line\n> Second line\n\nOutside"
    );

    // 2. Nested blockquotes peel one level at a time, and Backspace strips `> `.
    let text = value(&doc, "note-body");
    select(&doc, "note-body", [0, text.len()]);
    commit_text(&mut doc, "> > Deep");
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert_eq!(value(&doc, "note-body"), "> > Deep\n> > ");
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert_eq!(value(&doc, "note-body"), "> > Deep\n> ");
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert_eq!(value(&doc, "note-body"), "> > Deep\n\n");

    // 3. List item inside a blockquote continues with both `> ` and list prefix.
    let text = value(&doc, "note-body");
    select(&doc, "note-body", [0, text.len()]);
    commit_text(&mut doc, "> - Item");
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert_eq!(value(&doc, "note-body"), "> - Item\n> - ");
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert_eq!(value(&doc, "note-body"), "> - Item\n> ");

    // 4. `> ` inside a fenced code block stays literal.
    let text = value(&doc, "note-body");
    select(&doc, "note-body", [0, text.len()]);
    commit_text(&mut doc, "```text\n> literal\n```");
    move_body(&mut doc, ["```text\n> literal".len(); 2]);
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert_eq!(value(&doc, "note-body"), "```text\n> literal\n\n```");

    // 5. Also works in Source mode.
    click(&mut doc, "source-mode");
    let src_len = value(&doc, "markdown").len();
    select(&doc, "markdown", [0, src_len]);
    commit_text(&mut doc, "# Title\n\n> Quoted");
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert_eq!(markdown(&doc), "# Title\n\n> Quoted\n> ");
}

#[test]
fn callouts_render_insert_via_slash_and_empty_bar_and_continue_on_enter() {
    const MD: &str = "# Field notes\n\n> [!NOTE]\n> Recorder packed with **fresh** batteries.\n\n> [!TIP] Quick check\n> Press `/` on an empty line.\n\n> [!WARNING]\n> Check the lock gate.\n\n";
    let config = dioxus_native::DocumentConfig {
        font_ctx: Some(typefaces::fonts()),
        viewport: Some(Viewport {
            window_size: (860, 620),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut doc = DioxusDocument::new(VirtualDom::new(|| notebook(MD)), config);
    doc.initial_build();
    render(&mut doc);

    // 1. All three callouts produce backdrop boxes in the editor.
    for tag in ["[!NOTE]", "[!TIP]", "[!WARNING]"] {
        let start = MD.find(tag).unwrap() - 2;
        assert!(
            doc.inner
                .borrow()
                .get_element_by_id(&format!("callout-{start}"))
                .is_some(),
            "missing callout backdrop for {tag}"
        );
    }

    // 2. Typing `/warning` on the trailing empty line inserts a warning callout and places caret on body line.
    let end = MD.len();
    open_body_at(&mut doc, [end; 2]);
    for ch in "/warn".chars() {
        press(&mut doc, Key::Character(ch.to_string()), false);
    }
    assert!(doc.inner.borrow().get_element_by_id("slash-warning").is_some());
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert!(
        markdown(&doc).ends_with("> [!WARNING]\n> "),
        "markdown was: {:?}",
        markdown(&doc)
    );

    // 3. Typing body + Enter continues the callout; second Enter exits cleanly.
    commit_text(&mut doc, "Watch the current");
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert!(markdown(&doc).ends_with("> [!WARNING]\n> Watch the current\n> "));
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    commit_text(&mut doc, "After callout");
    assert!(
        markdown(&doc).ends_with("> [!WARNING]\n> Watch the current\n\nAfter callout"),
        "markdown was: {:?}",
        markdown(&doc)
    );

    // 4. Empty-line Callout icon inserts a Note callout on an empty line, and clicking its badge switches type.
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert!(doc.inner.borrow().get_element_by_id("empty-callout").is_some());
    click(&mut doc, "empty-callout");
    assert!(
        markdown(&doc).ends_with("> [!NOTE]\n> "),
        "markdown was: {:?}",
        markdown(&doc)
    );
    let note_start = markdown(&doc).rfind("> [!NOTE]").unwrap();
    click(&mut doc, &format!("callout-badge-{note_start}"));
    assert!(doc.inner.borrow().get_element_by_id("callout-menu").is_some());
    for kind_id in [
        "callout-pick-note",
        "callout-pick-tip",
        "callout-pick-important",
        "callout-pick-warning",
        "callout-pick-caution",
    ] {
        assert!(
            doc.inner.borrow().get_element_by_id(kind_id).is_some(),
            "missing callout type option {kind_id}"
        );
    }
    click(&mut doc, "callout-pick-caution");
    assert!(
        markdown(&doc).ends_with("> [!CAUTION]\n> "),
        "markdown after badge switch was: {:?}",
        markdown(&doc)
    );
    click(&mut doc, "undo");
    assert!(markdown(&doc).ends_with("> [!NOTE]\n> "));

    // 5. The reading projection classifies callouts with kind and header badge.
    let blocks = margin_projection::reading::read(MD);
    assert!(blocks.iter().any(|b| matches!(
        b,
        margin_projection::reading::Block::Text { passage }
            if passage.kind.contains("callout-note") && passage.kind.contains("callout-head")
    )));
    assert!(blocks.iter().any(|b| matches!(
        b,
        margin_projection::reading::Block::Text { passage }
            if passage.kind.contains("callout-tip")
                && passage.runs.iter().any(|r| r.text.contains("Quick check"))
    )));
}

#[test]
fn highlights_bare_urls_smart_url_paste_and_code_block_copy() {
    const MD: &str = "# Notes\n\nCheck ==key phrase== and https://example.com/docs today.\n\n> Line one\n> Line two\n\n```rust\nfn main() {}\n```";
    let config = dioxus_native::DocumentConfig {
        font_ctx: Some(typefaces::fonts()),
        viewport: Some(Viewport {
            window_size: (860, 620),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut doc = DioxusDocument::new(VirtualDom::new(|| notebook(MD)), config);
    doc.initial_build();
    render(&mut doc);
    let clipboard = std::sync::Arc::new(TestClipboard::default());
    doc.inner.borrow_mut().shell_provider = clipboard.clone();

    // 1. Highlight delimiters collapse tightly at rest in Live/Rich mode and read-only preview splits highlight runs.
    assert!(!value(&doc, "note-body").contains("=="));
    let preview_blocks = margin_projection::reading::read(MD);
    assert!(preview_blocks.iter().any(|b| matches!(
        b,
        margin_projection::reading::Block::Text { passage }
            if passage
                .runs
                .iter()
                .any(|r| r.text == "key phrase" && r.style.contains("highlight"))
    )));

    // 2. Bare URL autolink opens the Link Card (without a Remove button since it has no wrapper).
    let url_pos = MD.find("https://example.com/docs").unwrap() + 10;
    open_body_at(&mut doc, [url_pos; 2]);
    assert!(doc.inner.borrow().get_element_by_id("link-card").is_some());
    assert!(doc.inner.borrow().get_element_by_id("link-card-remove").is_none());

    // 3. Fenced code block renders a card + Copy button that copies only the code body.
    let code_start = MD.find("```rust").unwrap();
    assert!(
        doc.inner
            .borrow()
            .get_element_by_id(&format!("code-block-{code_start}"))
            .is_some()
    );
    click(&mut doc, &format!("code-copy-{code_start}"));
    assert_eq!(clipboard.0.lock().unwrap().as_str(), "fn main() {}");

    // 3b. Syntax highlighting emits semantic `.tok-*` ink overlays when enabled.
    let fn_at = MD.find("fn main").unwrap();
    let main_at = MD.find("main()").unwrap();
    let (fn_class, main_class) = {
        let inner = doc.inner.borrow();
        let class_of = |offset: usize| {
            inner
                .get_element_by_id(&format!("ink-{offset}"))
                .and_then(|id| inner.get_node(id))
                .and_then(|n| n.attr("class".into()))
                .unwrap_or("")
                .to_owned()
        };
        (class_of(fn_at), class_of(main_at))
    };
    // Native builds always highlight (margin-ui turns it on).
    {
        assert!(
            fn_class.contains("code-block") && fn_class.contains("tok-keyword"),
            "expected `fn` ink to have code-block + tok-keyword, got: {fn_class:?}"
        );
        assert!(
            main_class.contains("code-block") && main_class.contains("tok-function"),
            "expected `main` ink to have code-block + tok-function, got: {main_class:?}"
        );
    }

    // 3c. Caret inside a code block keeps a constant width across every character
    //     (never oscillating between 1px and 2px on fractional monospace advances),
    //     and closing fences blend into the code-block surface.
    let close_fence_at = MD.rfind("```").unwrap();
    let close_fence_class = {
        let inner = doc.inner.borrow();
        inner
            .get_element_by_id(&format!("ink-{close_fence_at}"))
            .and_then(|id| inner.get_node(id))
            .and_then(|n| n.attr("class".into()))
            .unwrap_or("")
            .to_owned()
    };
    assert!(
        close_fence_class.contains("code-block") && close_fence_class.contains("punctuation"),
        "expected closing fence to carry code-block + punctuation, got: {close_fence_class:?}"
    );
    open_body_at(&mut doc, [fn_at; 2]);
    let expected_w = bounds(&doc, "caret-ink")[2];
    assert!(
        (expected_w - 2.0).abs() < 0.01,
        "expected crisp 2px caret width, got {expected_w}"
    );
    for step in 1..=8 {
        press(&mut doc, Key::Named(NamedKey::ArrowRight), false);
        let w = bounds(&doc, "caret-ink")[2];
        assert!(
            (w - expected_w).abs() < 0.01,
            "caret width changed at step {step}: got {w}, expected {expected_w}"
        );
    }

    // 3d. Rich mode hides ``` fences and offers a searchable language combobox.
    click(&mut doc, "rich-mode");
    assert!(
        !value(&doc, "note-body").contains("```"),
        "Rich mode should hide fenced code block backticks: {:?}",
        value(&doc, "note-body")
    );
    assert!(value(&doc, "note-body").contains("fn main() {}"));
    click(&mut doc, &format!("code-lang-{code_start}"));
    assert!(
        doc.inner.borrow().get_element_by_id("code-lang-menu").is_some(),
        "clicking language pill must open the searchable combobox"
    );
    assert_eq!(
        doc.inner.borrow().get_focussed_node_id(),
        doc.inner.borrow().get_element_by_id("code-lang-search"),
        "opening language combobox must focus the search input"
    );
    let search_rect = bounds(&doc, "code-lang-search");
    let ph_rect = bounds(&doc, "code-lang-ph");
    assert!(
        (ph_rect[0] - search_rect[0]).abs() <= 2.0
            && ph_rect[1] >= search_rect[1] - 1.0
            && ph_rect[1] + ph_rect[3] <= search_rect[1] + search_rect[3] + 1.0,
        "placeholder must align inside search input: ph={ph_rect:?}, search={search_rect:?}"
    );
    let ph_cy = ph_rect[1] + ph_rect[3] * 0.5;
    let search_cy = search_rect[1] + search_rect[3] * 0.5;
    assert!(
        (ph_cy - search_cy).abs() <= 2.0,
        "placeholder must be vertically centered in search input: ph_cy={ph_cy}, search_cy={search_cy}"
    );
    commit_text(&mut doc, "py");
    assert!(
        doc.inner
            .borrow()
            .get_element_by_id("code-lang-pick-python")
            .is_some(),
        "typing 'py' should filter to Python"
    );
    press(&mut doc, Key::Named(NamedKey::Enter), false);
    assert!(
        doc.inner.borrow().get_element_by_id("code-lang-menu").is_none(),
        "pressing Enter should select the language and close the combobox"
    );
    assert!(
        markdown(&doc).contains("```python\nfn main() {}\n```"),
        "selecting Python should update the fence info: {}",
        markdown(&doc)
    );
    click(&mut doc, "undo");
    assert!(markdown(&doc).contains("```rust\nfn main() {}\n```"));
    // A typed language is kept to one word without fence characters: a
    // backtick would turn the opening fence into inline code.
    click(&mut doc, &format!("code-lang-{code_start}"));
    commit_text(&mut doc, "c` x");
    click(&mut doc, "ui-combobox-pick-custom");
    assert!(markdown(&doc).contains("```c\nfn main() {}\n```"), "{}", markdown(&doc));
    click(&mut doc, "undo");
    assert!(markdown(&doc).contains("```rust\nfn main() {}\n```"));
    click(&mut doc, &format!("code-lang-{code_start}"));
    assert!(doc.inner.borrow().get_element_by_id("code-lang-menu").is_some());
    press(&mut doc, Key::Named(NamedKey::Escape), false);
    assert!(doc.inner.borrow().get_element_by_id("code-lang-menu").is_none());
    click(&mut doc, "live-mode");

    // 4. Smart URL paste onto selected text wraps it as `[selection](url)` in one undoable edit.
    *clipboard.0.lock().unwrap() = "https://margin.example/guide".into();
    let today_start = MD.find("today").unwrap();
    open_body_at(&mut doc, [today_start, today_start + "today".len()]);
    doc.handle_ui_event(UiEvent::KeyDown(BlitzKeyEvent {
        key: Key::Character("v".into()),
        code: Code::Unidentified,
        modifiers: Modifiers::META,
        location: Location::Standard,
        is_auto_repeating: false,
        is_composing: false,
        state: KeyState::Pressed,
        text: None,
    }));
    render(&mut doc);
    assert_eq!(
        markdown(&doc),
        MD.replace("today", "[today](https://margin.example/guide)")
    );
    click(&mut doc, "undo");
    assert_eq!(markdown(&doc), MD);

    // 5. Cmd+Shift+H toggles `==highlight==` around selected text — both wrap and unwrap,
    //    including right beside multi-byte UTF-8 characters (no char-boundary panic).
    let toggle_highlight_key = |doc: &mut DioxusDocument| {
        doc.handle_ui_event(UiEvent::KeyDown(BlitzKeyEvent {
            key: Key::Character("h".into()),
            code: Code::Unidentified,
            modifiers: Modifiers::META | Modifiers::SHIFT,
            location: Location::Standard,
            is_auto_repeating: false,
            is_composing: false,
            state: KeyState::Pressed,
            text: None,
        }));
        render(doc);
    };
    open_body_at(&mut doc, [today_start, today_start + "today".len()]);
    toggle_highlight_key(&mut doc);
    assert_eq!(markdown(&doc), MD.replace("today", "==today=="));
    // Pressing Cmd+Shift+H again unwraps `==today==` back to `today`.
    toggle_highlight_key(&mut doc);
    assert_eq!(markdown(&doc), MD);

    // 6. Multi-byte UTF-8 neighbors around toggle_highlight, indented closing code fence body_range,
    //    and inline_fields() covering highlights and bare URLs in headings & table cells.
    const EDGE_MD: &str = "# Notes\n\n## ==Bright==\n\nSee —café— here.\n\n| Link | Note |\n| :--- | :--- |\n| https://example.com/cell | ==Hot== |\n\n```rust\nlet x = 1;\n  ```";
    let mut edge_doc = DioxusDocument::new(
        VirtualDom::new(|| notebook(EDGE_MD)),
        dioxus_native::DocumentConfig {
            font_ctx: Some(typefaces::fonts()),
            viewport: Some(Viewport {
                window_size: (860, 620),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    edge_doc.initial_build();
    render(&mut edge_doc);
    edge_doc.inner.borrow_mut().shell_provider = clipboard.clone();

    // Selecting `café` right between 3-byte em-dashes (`—café—`) wraps and unwraps without panicking.
    let cafe_start = EDGE_MD.find("café").unwrap();
    edge_doc.vdom.in_scope(ScopeId::APP, || {
        let w = consume_context::<Writing>();
        w.open(w.field_at(cafe_start), [cafe_start, cafe_start + "café".len()]);
    });
    render(&mut edge_doc);
    toggle_highlight_key(&mut edge_doc);
    assert_eq!(markdown(&edge_doc), EDGE_MD.replace("café", "==café=="));
    toggle_highlight_key(&mut edge_doc);
    assert_eq!(markdown(&edge_doc), EDGE_MD);

    // Heading highlight and table cell bare URL / highlight are discovered by inline_fields() and painted.
    edge_doc.vdom.in_scope(ScopeId::APP, || {
        let w = consume_context::<Writing>();
        let fields = w.inline_fields();
        assert!(fields.contains(&Field::Heading(0)), "heading highlight missing: {fields:?}");
        assert!(fields.contains(&Field::Cell(0, 1, 0)), "cell bare URL missing: {fields:?}");
        assert!(fields.contains(&Field::Cell(0, 1, 1)), "cell highlight missing: {fields:?}");
    });

    // Indented closing code fence copies exact code body without trailing whitespace/newline.
    let indented_code_start = EDGE_MD.find("```rust").unwrap();
    click(&mut edge_doc, &format!("code-copy-{indented_code_start}"));
    assert_eq!(clipboard.0.lock().unwrap().as_str(), "let x = 1;");
}

mod live_workflows;



/// A store that keeps the discussion the notebook saves, as an app does.
#[derive(Clone, Default)]
struct KeepsDiscussion(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);

impl NoteStore for KeepsDiscussion {
    fn use_persistence(&self, _note: Signal<Note>) -> Signal<String> {
        use_signal(String::new)
    }

    fn save_discussion(&self, _previous: &[u8], current: &[u8]) -> anyhow::Result<()> {
        *self.0.borrow_mut() = current.to_vec();
        Ok(())
    }
}

impl KeepsDiscussion {
    fn threads(&self) -> Vec<CommentThread> {
        serde_json::from_slice(&self.0.borrow()).unwrap_or_default()
    }
}

/// The test driver comments as a reader does, and the stand-in agent's
/// answer reaches the app through its store. Osom's live test does the same
/// against the real agent and a page of the space.
#[test]
fn a_comment_made_through_the_page_reaches_the_store_with_its_answer() {
    let kept = KeepsDiscussion::default();
    let mut doc = open(VirtualDom::new_with_props(
        |props: NotebookWithProps| notebook_view(SAMPLE, NotebookOptions { store: Some(props.store), ..Default::default() }),
        NotebookWithProps { store: Store::new(kept.clone()) },
    ));
    let quote = "Pack the windscreen";
    comment_on(&mut doc, quote, "Shorten this to a few words.");
    let answered = |k: &KeepsDiscussion| k.threads().first().is_some_and(|t| t.file.messages.len() == 2);
    settle(&mut doc, std::time::Duration::from_secs(2), |_| answered(&kept));
    let threads = kept.threads();
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].file.anchor.exact, quote);
    let [asked, answer] = &threads[0].file.messages[..] else { panic!("your comment and the answer") };
    assert!(!asked.ai && asked.body == "Shorten this to a few words.");
    assert!(answer.ai && !answer.body.is_empty(), "the stand-in answered");
}

#[derive(Props, Clone, PartialEq)]
struct NotebookWithProps {
    store: Store,
}

#[component]
fn NotebookIn(view: View) -> Element {
    notebook_view(SAMPLE, NotebookOptions { view, ..Default::default() })
}

#[test]
fn a_notebook_opens_in_the_view_it_is_asked_for() {
    assert!(View::default() == View::Live);
    for (view, selected) in [(View::Rich, "rich-mode"), (View::Live, "live-mode"), (View::Source, "source-mode")] {
        let doc = open(VirtualDom::new_with_props(NotebookIn, NotebookInProps { view }));
        for mode in ["rich-mode", "live-mode", "source-mode"] {
            let on = attribute(&doc, mode, "aria-pressed").as_deref() == Some("true");
            assert_eq!(on, mode == selected, "{mode} when opened as {selected}");
        }
        // Source is one field of Markdown; Rich and Live show the note's fields.
        assert_eq!(has(&doc, "markdown"), selected == "source-mode");
        assert_eq!(has(&doc, "note-title"), selected != "source-mode");
    }
}

#[test]
fn the_host_supplies_its_menu_button_and_its_pane_bar() {
    // Alone, the notebook has neither.
    let alone = editor();
    assert!(!has(&alone, "notes-menu") && !has(&alone, "doc-back"));

    // The app's menu: the bar starts with a Notes button that toggles it.
    fn with_menu() -> Element {
        let menu = use_signal(|| false);
        rsx! {
            div { id: "menu-state", "{menu}" }
            {notebook_view(SAMPLE, NotebookOptions { host: Host { menu: Some(menu), ..Default::default() }, ..Default::default() })}
        }
    }
    let mut doc = open(VirtualDom::new(with_menu));
    assert_eq!(text(&doc, "menu-state"), "false");
    click(&mut doc, "notes-menu");
    assert_eq!(text(&doc, "menu-state"), "true");

    // A pane of the app's window: the bar starts with the app's way back.
    // Being a pane does not choose the view: it is still Live unless asked.
    fn as_pane() -> Element {
        let back = HostAction { label: "Docs".into(), icon: Icon::Back, onclick: EventHandler::new(|_| {}) };
        let bar = HostBar { back: Some(back), crumb: "notes.md".into(), actions: vec![] };
        notebook_view(SAMPLE, NotebookOptions { host: Host { bar: Some(bar), ..Default::default() }, ..Default::default() })
    }
    let pane = open(VirtualDom::new(as_pane));
    assert!(has(&pane, "doc-back") && !has(&pane, "notes-menu"));
    assert_eq!(attribute(&pane, "live-mode", "aria-pressed").as_deref(), Some("true"));

    // A page inside the app's own view has no way back.
    fn in_place() -> Element {
        let bar = HostBar { back: None, crumb: String::new(), actions: vec![] };
        notebook_view(SAMPLE, NotebookOptions { host: Host { bar: Some(bar), ..Default::default() }, ..Default::default() })
    }
    assert!(!has(&open(VirtualDom::new(in_place)), "doc-back"));
}
