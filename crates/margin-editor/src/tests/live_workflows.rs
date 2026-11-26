//! User journeys through the Live window's input dispatcher. Keep expected
//! Markdown and selections independent of the editor's navigation code.
use super::*;
use crate::inspector;
use std::path::{Path, PathBuf};

struct Session {
    name: String,
    doc: DioxusDocument,
    actions: Vec<String>,
}

impl Session {
    fn writing<T>(&self, f: impl FnOnce(Writing) -> T) -> T {
        self.doc
            .vdom
            .in_scope(ScopeId::APP, || f(consume_context::<Writing>()))
    }

    fn state(&self) -> inspector::State {
        self.writing(inspector::snapshot)
    }

    fn markdown(&self) -> String {
        self.writing(|w| w.note.peek().markdown())
    }

    fn click_at_text(&mut self, name: &str, byte: usize) {
        self.actions
            .push(format!("Click {name} at UTF-8 byte {byte}"));
        let (x, y) = {
            let dom = self.doc.inner.borrow();
            let i = id(&self.doc, name);
            let node = dom.get_node(i).unwrap();
            let f = node.element_data().unwrap().text_input_data().unwrap();
            assert!(f.editor.raw_text().is_char_boundary(byte));
            let layout = f.editor.try_layout().unwrap();
            let scale = f64::from(layout.scale());
            let caret = parley::Cursor::from_byte_index(layout, byte, parley::Affinity::Downstream)
                .geometry(layout, 0.);
            let l = node.final_layout();
            let mut x = f64::from(l.padding.left + l.border.left) + caret.x0 / scale + 0.1;
            let mut y = f64::from(l.padding.top + l.border.top)
                + node.text_input_v_centering_offset(scale)
                + (caret.y0 + caret.y1) / (2. * scale);
            if f.is_multiline {
                y -= f64::from(f.scroll_offset);
            } else {
                x -= f64::from(f.scroll_offset);
            }
            let b = dom.get_client_bounding_rect(i).unwrap();
            let (w, h) = dom.viewport().logical_size();
            assert!(
                (0.0..f64::from(w)).contains(&(b.x + x))
                    && (0.0..f64::from(h)).contains(&(b.y + y)),
                "{name} must be reachable on screen"
            );
            (x as f32, y as f32)
        };
        click_at(&mut self.doc, name, Some((x, y)));
    }

    fn hover(&mut self, name: &str) {
        self.actions.push(format!("Hover {name}"));
        let element = id(&self.doc, name);
        let mut pointer = {
            let dom = self.doc.inner.borrow();
            let mut p = dom.get_node(element).unwrap().synthetic_click_event_data(Modifiers::empty());
            let r = dom.get_client_bounding_rect(element).unwrap();
            let scroll = dom.viewport_scroll();
            p.coords.client_x = (r.x + r.width / 2.) as f32;
            p.coords.client_y = (r.y + r.height / 2.) as f32;
            p.coords.page_x = p.coords.client_x + scroll.x as f32;
            p.coords.page_y = p.coords.client_y + scroll.y as f32;
            p
        };
        pointer.buttons = MouseEventButtons::None;
        self.doc.handle_ui_event(UiEvent::PointerMove(pointer));
        render(&mut self.doc);
    }

    fn button(&mut self, name: &str) {
        self.actions.push(format!("Click {name}"));
        click(&mut self.doc, name);
    }

    fn key(&mut self, key: Key, modifiers: Modifiers) {
        self.actions.push(format!("{key} {modifiers:?}"));
        // Blitz delegates ordinary backward deletion to AppKit on macOS.
        // Winit delivers the text-system selector before KeyboardInput.
        #[cfg(target_os = "macos")]
        if key == Key::Named(NamedKey::Backspace) && modifiers.is_empty() {
            self.doc
                .handle_ui_event(UiEvent::AppleStandardKeybinding("deleteBackward:".into()));
            render(&mut self.doc);
        }
        let text = match &key {
            Key::Character(s) if (modifiers - Modifiers::SHIFT).is_empty() => {
                Some(s.as_str().into())
            }
            _ => None,
        };
        for state in [KeyState::Pressed, KeyState::Released] {
            let event = BlitzKeyEvent {
                key: key.clone(),
                code: Code::Unidentified,
                modifiers,
                location: Location::Standard,
                is_auto_repeating: false,
                is_composing: false,
                state,
                text: text.clone(),
            };
            self.doc.handle_ui_event(match state {
                KeyState::Pressed => UiEvent::KeyDown(event),
                KeyState::Released => UiEvent::KeyUp(event),
            });
            render(&mut self.doc);
        }
    }

    fn named(&mut self, key: NamedKey) {
        self.key(Key::Named(key), Modifiers::empty());
    }
    fn type_char(&mut self, ch: char) {
        self.key(Key::Character(ch.to_string()), Modifiers::empty());
    }
    fn commit(&mut self, s: &str) {
        self.actions.push(format!("IME commit {s:?}"));
        commit_text(&mut self.doc, s);
    }

    fn expect(&self, md: &str, field: &str, selection: ByteSelection) {
        let state = self.state();
        assert_eq!(
            self.markdown(),
            md,
            "Loro text after {:?}",
            self.actions.last()
        );
        assert_eq!(state.focused.as_deref(), Some(field), "native focus");
        assert_eq!(
            state.editor_field, state.focused,
            "editor and native focus disagree"
        );
        assert_eq!(
            state.field_matches_source,
            Some(true),
            "native text buffer differs from source"
        );
        assert_eq!(
            state.markdown_bytes,
            Some(selection),
            "directed Markdown selection"
        );
        assert!(selection.iter().all(|pos| md.is_char_boundary(*pos)));
    }

    fn pixels(&self) -> (u32, u32, Vec<u8>) {
        let d = self.doc.inner.borrow();
        let (w, h) = d.viewport().window_size;
        let scale = d.viewport().scale_f64();
        drop(d);
        let pixels = anyrender::render_to_buffer::<anyrender_vello_cpu::VelloCpuImageRenderer, _>(
            |scene| {
                blitz_paint::paint_scene(scene, &mut self.doc.inner.borrow_mut(), scale, w, h, 0, 0)
            },
            w,
            h,
        );
        (w, h, pixels)
    }

    fn visible_blank_caret(&self, field: &str) {
        let (top, bottom, view_top, view_bottom) = painted_caret(&self.doc, field);
        assert!(
            bottom > top && top >= view_top - 1. && bottom <= view_bottom + 1.,
            "clipped caret"
        );
        let state = self.state();
        let [x, ..] = state.caret_rect.expect("native caret geometry");
        let scale = f64::from(state.scale);
        let (w, h, pixels) = self.pixels();
        let x = (x * scale).floor() as usize;
        let end = (bottom * scale).floor().min(f64::from(h)) as usize;
        assert!(x + 10 < w as usize && end > 2);
        // A dark vertical stroke with clear paper beside it, inside this blank line.
        let rows = ((top * scale).ceil().max(0.) as usize + 1..end - 1)
            .filter(|y| {
                let green = |cx| pixels[(y * w as usize + cx) * 4 + 1];
                (x.saturating_sub(1)..=x + 2).any(|cx| green(cx) < 170) && green(x + 9) > 190
            })
            .count();
        assert!(
            rows >= (5. * scale) as usize,
            "{field}: only {rows} rows of visible caret pixels"
        );
    }

    fn artifact(&self, root: &Path, checkpoint: &str, error: Option<&str>) -> std::io::Result<()> {
        let dir = root.join(&self.name);
        std::fs::create_dir_all(&dir)?;
        let base = dir.join(format!("{:03}-{checkpoint}", self.actions.len()));
        let (w, h, pixels) = self.pixels();
        let mut enc = png::Encoder::new(std::fs::File::create(base.with_extension("png"))?, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()?.write_image_data(&pixels)?;
        let record = serde_json::json!({"case":self.name,"checkpoint":checkpoint,
            "state":self.state(),"actions":self.actions,"markdown":self.markdown(),"error":error});
        std::fs::write(
            base.with_extension("json"),
            serde_json::to_vec_pretty(&record)?,
        )
    }

    fn capture(&self, checkpoint: &str) {
        if let Some(dir) = std::env::var_os("MARGIN_TEST_ARTIFACTS") {
            self.artifact(Path::new(&dir), checkpoint, None).unwrap();
        }
    }
}

fn run(
    name: &str,
    initial: &'static str,
    logical: (u32, u32),
    scale: u32,
    steps: impl FnOnce(&mut Session),
) {
    let config = dioxus_native::DocumentConfig {
        font_ctx: Some(typefaces::notebook_fonts()),
        viewport: Some(Viewport {
            window_size: (logical.0 * scale, logical.1 * scale),
            hidpi_scale: scale as f32,
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut doc = DioxusDocument::new(
        VirtualDom::new_with_props(|md: &'static str| notebook(md), initial),
        config,
    );
    doc.initial_build();
    render(&mut doc);
    let mut session = Session {
        name: name.into(),
        doc,
        actions: vec![],
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| steps(&mut session)));
    if let Err(panic) = result {
        let error = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("workflow panicked");
        let root = std::env::var_os("MARGIN_TEST_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).expect("crates/<name> sits two levels below the workspace").join("target/editor-tests/failures")
            });
        eprintln!(
            "Failed {name} at input {}: {:?}\nArtifacts: {}",
            session.actions.len(),
            session.actions.last(),
            root.join(name).display()
        );
        // An artifact failure should never hide the original assertion.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Err(err) = session.artifact(&root, "failure", Some(error)) {
                eprintln!("Could not write artifacts: {err}");
            }
        }));
        std::panic::resume_unwind(panic);
    }
}

#[test]
fn title_and_lists_keep_a_painted_caret_before_the_next_keystroke() {
    const MD: &str = "# Café\n\n- [x] Pack\n\n## Next";
    for scale in [1, 2] {
        run(&format!("return-{scale}x"), MD, (680, 440), scale, |s| {
            s.click_at_text("note-title", "Café".len());
            s.expect(MD, "note-title", ["# Café".len(); 2]);
            s.named(NamedKey::Enter);
            let mut md = MD.replacen('\n', "\n\n", 1);
            let at = "# Café\n".len();
            s.expect(&md, "title-gap", [at; 2]);
            s.visible_blank_caret("title-gap");
            s.capture("after-title-enter");
            s.commit("Été");
            md.insert_str(at, "Été");
            s.expect(&md, "note-body", [at + "Été".len(); 2]);
            let body = value(&s.doc, "note-body");
            s.click_at_text("note-body", body.find("Pack").unwrap() + 4);
            s.named(NamedKey::Enter);
            let at = md.find("Pack").unwrap() + 4;
            md.insert_str(at, "\n- [ ] ");
            s.expect(&md, "note-body", [at + "\n- [ ] ".len(); 2]);
            s.visible_blank_caret("note-body");
            s.capture("after-checklist-enter");
            s.type_char('A');
            md.insert(at + "\n- [ ] ".len(), 'A');
            s.expect(&md, "note-body", [at + "\n- [ ] A".len(); 2]);
        });
    }
}

#[test]
fn table_pointer_edit_tab_delete_and_undo() {
    const MD: &str = "# Table\n\n| A | B |\n| --- | --- |\n| old | six |\n\nTail";
    run("table-column", MD, (740, 480), 1, |s| {
        s.click_at_text("cell-0-1-1", 1);
        let at = MD.find("six").unwrap() + 1;
        s.expect(MD, "cell-0-1-1", [at; 2]);
        s.type_char('!');
        let edited = MD.replace("six", "s!ix");
        s.expect(&edited, "cell-0-1-1", [at + 1; 2]);
        s.key(Key::Named(NamedKey::Tab), Modifiers::SHIFT);
        s.expect(
            &edited,
            "cell-0-1-0",
            [edited.find("old").unwrap(), edited.find("old").unwrap() + 3],
        );
        s.button("col-menu-btn-0-0");
        s.button("col-menu-delete-0-0");
        let one = "# Table\n\n| B |\n| --- |\n| s!ix |\n\nTail";
        let keep = one.find("s!ix").unwrap();
        s.expect(one, "cell-0-1-0", [keep, keep + 4]);
        s.capture("removed-column");
        s.key(Key::Character("z".into()), Modifiers::META);
        assert_eq!(s.markdown(), edited);
        assert_eq!(s.state().field_matches_source, Some(true));
        s.capture("undo-column");
        let [_, _, width, _] = bounds(&s.doc, "grid-scroll-0");
        s.actions
            .push("Click the right rail beside the column button".into());
        click_at(&mut s.doc, "grid-scroll-0", Some((width as f32 + 9., 8.)));
        s.expect(&edited, "cell-0-0-1", [edited.find("B |").unwrap() + 1; 2]);
        s.button("col-menu-btn-0-1");
        s.button("col-menu-insert-right-0-1");
        let wider =
            "# Table\n\n| A | B | Column 3 |\n| --- | --- | --- |\n| old | s!ix |  |\n\nTail";
        let col = wider.find("Column 3").unwrap();
        s.expect(wider, "cell-0-0-2", [col, col + 8]);
        s.click_at_text("cell-0-1-0", 0);
        s.button("row-menu-btn-0-1");
        s.button("row-menu-insert-below-0-1");
        let taller = wider.replace("\n\nTail", "\n|  |  |  |\n\nTail");
        assert_eq!(s.markdown(), taller);
        assert_eq!(s.state().focused.as_deref(), Some("cell-0-2-0"));
        s.commit("West");
        let filled = taller.replace("\n|  |  |  |", "\n| West |  |  |");
        s.expect(&filled, "cell-0-2-0", [filled.find("West").unwrap() + 4; 2]);
        s.capture("added-row-and-column");
    });
}

#[test]
fn seeded_basic_typing_matches_a_plain_text_model_after_each_input() {
    let seeds = match std::env::var("MARGIN_TEST_SEED") {
        Ok(s) => vec![
            s.parse::<u64>()
                .expect("MARGIN_TEST_SEED must be an unsigned integer"),
        ],
        Err(_) => vec![17, 631, 20261122, 90210],
    };
    for seed in seeds {
        run(
            &format!("typing-seed-{seed}"),
            "# Note\n\nHello",
            (560, 420),
            1,
            |s| {
                let prefix = "# Note\n\n";
                let mut text = String::from("Hello");
                let mut at = text.len();
                let mut rng = seed;
                s.click_at_text("note-body", at);
                s.expect(
                    &format!("{prefix}{text}"),
                    "note-body",
                    [prefix.len() + at; 2],
                );
                for _ in 0..60 {
                    rng = rng
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    let choice = (rng >> 32) % 10;
                    let previous = text[..at].char_indices().next_back().map(|(i, _)| i);
                    let next = text[at..].chars().next().map(|ch| at + ch.len_utf8());
                    match (choice, previous, next) {
                        (0, Some(i), _) => {
                            s.named(NamedKey::ArrowLeft);
                            at = i;
                        }
                        (1, _, Some(i)) => {
                            s.named(NamedKey::ArrowRight);
                            at = i;
                        }
                        (2, Some(i), _) => {
                            s.named(NamedKey::Backspace);
                            text.replace_range(i..at, "");
                            at = i;
                        }
                        (3, _, Some(i)) => {
                            s.named(NamedKey::Delete);
                            text.replace_range(at..i, "");
                        }
                        (4, _, _) => {
                            s.named(NamedKey::Enter);
                            text.insert(at, '\n');
                            at += 1;
                        }
                        _ => {
                            let ch = ['é', 'a', '界', '🎧', ' '][(rng % 5) as usize];
                            s.type_char(ch);
                            text.insert(at, ch);
                            at += ch.len_utf8();
                        }
                    }
                    s.expect(
                        &format!("{prefix}{text}"),
                        "note-body",
                        [prefix.len() + at; 2],
                    );
                }
                s.capture("after-60-inputs");
            },
        );
    }
}

#[test]
fn arrows_cross_each_blank_line_heading_and_table_row_once() {
    const MD: &str =
        "# One\n\nA\n\n\n## Two\n\n| Place | Time |\n| --- | --- |\n| Lock | Noon |\n\nEnd";
    run("vertical-navigation", MD, (680, 540), 1, |s| {
        s.click_at_text("note-title", 0);
        // Move to offset 0 (before `#`) so goal_x sits on the left margin.
        s.named(NamedKey::ArrowLeft);
        s.named(NamedKey::ArrowLeft);
        let stops = [
            ("note-title", 2),
            ("title-gap", 6),
            ("note-body", 7),
            ("note-body", 9),
            ("note-body", 10),
            ("note-heading-0", MD.find("Two").unwrap()),
            ("note-body-1", MD.find("\n\n|").unwrap() + 1),
            ("cell-0-0-0", MD.find("Place").unwrap()),
            ("cell-0-1-0", MD.find("Lock").unwrap()),
            ("note-body-2", MD.find("\n\nEnd").unwrap() + 1),
            ("note-body-2", MD.find("End").unwrap()),
        ];
        for &(field, at) in &stops[1..] {
            s.named(NamedKey::ArrowDown);
            s.expect(MD, field, [at; 2]);
        }
        s.capture("after-down");
        for &(field, at) in stops[..stops.len() - 1].iter().rev() {
            s.named(NamedKey::ArrowUp);
            s.expect(MD, field, [at; 2]);
        }
    });
}

#[test]
fn compact_toolbar_and_table_stay_in_place_while_editing() {
    const MD: &str = "# Log\n\n| Stop | Listen for | Time |\n| --- | --- | ---: |\n| Lock | Water through the gate | 45 s |";
    run("compact-live", MD, (375, 540), 1, |s| {
        let toolbar = [
            "live-mode",
            "source-mode",
            "insert-table",
            "undo",
            "redo",
        ];
        let mut end = 0.;
        for id in toolbar {
            let [x, _, width, _] = bounds(&s.doc, id);
            assert!(
                x >= end && width >= 24. && x + width <= 375.,
                "unreachable toolbar control {id}"
            );
            end = x + width;
        }
        let columns = ["cell-0-1-0", "cell-0-1-1", "cell-0-1-2"];
        let before = columns.map(|id| bounds(&s.doc, id));
        let [_, grid_y, grid_width, _] = bounds(&s.doc, "grid-scroll-0");
        let row_center = before[2][1] + before[2][3] * 0.5 - grid_y;
        s.actions
            .push("Click the right border beside the duration".into());
        click_at(
            &mut s.doc,
            "grid-scroll-0",
            Some(((grid_width - 0.5) as f32, row_center as f32)),
        );
        s.expect(MD, columns[2], [MD.find("45 s").unwrap() + 4; 2]);
        assert!(
            before[1][2] > before[2][2],
            "the longer description gets more room"
        );
        s.click_at_text(columns[1], 0);
        // The table stays inside the window, and a column's handle sits over its column.
        let [x, _, width, _] = bounds(&s.doc, "grid-0");
        assert!(x + width <= 375., "the table reaches past the window: {x}+{width}");
        let centre = |id: &str| {
            let [x, _, width, _] = bounds(&s.doc, id);
            x + width * 0.5
        };
        let (handle, column) = (centre("col-menu-btn-0-1"), centre("slot-0-0-1"));
        assert!((handle - column).abs() < 2., "handle at {handle} over a column centred at {column}");
        s.commit("Rushing along the edge of the path. ");
        assert!(
            s.markdown()
                .contains("Rushing along the edge of the path. Water through the gate")
        );
        // Columns hold still while typing. The edited row grows to fit its wrapped text,
        // and the row's other cells stay vertically centred in it.
        let steady = |s: &Session| {
            let after = columns.map(|id| bounds(&s.doc, id));
            for (b, a) in before.iter().zip(&after) {
                assert_eq!([b[0], b[2]], [a[0], a[2]], "a column moved while typing");
            }
            assert!(
                after[1][3] > before[1][3],
                "the longer description wraps onto more lines"
            );
            let row = bounds(&s.doc, "slot-0-1-0")[3];
            let strip = s.writing(|w| w.row_heights.peek()[0][1]);
            assert_eq!(strip, row, "the row menu strip follows the grown row");
        };
        steady(s);
        s.named(NamedKey::F8);
        let [x, y, width, height] = bounds(&s.doc, "save-inspection");
        assert!(x >= 0. && x + width <= 375.);
        assert!(y + height <= bounds(&s.doc, "note-title")[1]);
        steady(s);
        s.capture("compact-table");
    });
}

#[test]
fn inspector_preserves_geometry_selection_and_saves_the_current_state() {
    const MD: &str = "# Note\n\nCafé\n\n## Stops\n\n| A | B |\n| --- | --- |\n| One | Two |";
    run("inspector", MD, (820, 520), 1, |s| {
        s.click_at_text("note-body", "Café".len());
        let at = MD.find("Café").unwrap() + "Café".len();
        let fields = [
            "note-title",
            "title-gap",
            "note-body",
            "note-heading-0",
            "cell-0-1-1",
            "undo",
        ];
        let before = fields.map(|f| bounds(&s.doc, f));
        s.named(NamedKey::F8);
        assert_eq!(
            fields.map(|f| bounds(&s.doc, f)),
            before,
            "inspector changed editor layout"
        );
        s.expect(MD, "note-body", [at; 2]);
        s.key(Key::Named(NamedKey::ArrowLeft), Modifiers::SHIFT);
        s.expect(MD, "note-body", [at, at - "é".len()]);
        s.commit("è");
        let edited = MD.replace("Café", "Cafè");
        s.expect(&edited, "note-body", [at; 2]);
        s.capture("field-outlines");
        s.button("save-inspection");
        let saved = s
            .doc
            .inner
            .borrow()
            .get_node(id(&s.doc, "inspect-save-status"))
            .unwrap()
            .text_content();
        let path = Path::new(
            saved
                .trim()
                .strip_prefix("Saved ")
                .expect("visible report location"),
        );
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(report["initial_markdown"], MD);
        assert_eq!(report["current_markdown"], edited);
        assert_eq!(report["current_state"]["focused"], "note-body");
        assert_eq!(
            report["current_state"]["markdown_bytes"],
            serde_json::json!([at, at])
        );
        s.expect(&edited, "note-body", [at; 2]);
        s.named(NamedKey::F8);
        assert!(
            s.doc
                .inner
                .borrow()
                .get_element_by_id("save-inspection")
                .is_none()
        );
    });
}

#[test]
fn checklist_toggling_and_literal_typing_share_the_clicked_position() {
    const MD: &str = "# Tasks\n\n- [ ] Pack";
    run("checkbox-and-literal", MD, (620, 400), 1, |s| {
        let pack = value(&s.doc, "note-body").find("Pack").unwrap();
        s.click_at_text("note-body", pack);
        s.named(NamedKey::ArrowLeft);
        s.named(NamedKey::ArrowLeft);
        let at = MD.find(']').unwrap();
        s.expect(MD, "note-body", [at; 2]);
        s.type_char('x');
        let checked = MD.replace("[ ]", "[x]");
        s.expect(&checked, "note-body", [at; 2]);
        s.type_char(' ');
        s.expect(MD, "note-body", [at; 2]);
        s.named(NamedKey::F2);
        s.type_char('q');
        let raw = MD.replace("[ ]", "[ q]");
        s.expect(&raw, "note-body", [at + 1; 2]);
        s.capture("inside-literal-brackets");
        s.named(NamedKey::Escape);
        s.expect(&raw, "note-body", [at + 1; 2]);
    });
}

#[test]
fn rectangular_selection_cycles_copies_clears_and_deletes_only_its_columns() {
    const MD: &str = "# Ranges\n\n| A | B | C | D |\n| --- | :--- | ---: | --- |\n| one | Café | 🌿 | keep |\n| two | a\\|b | c | last |\n\nOutside";
    run("cell-range", MD, (850, 560), 1, |s| {
        s.click_at_text("cell-0-2-2", 0);
        click_with(&mut s.doc, "cell-0-1-1", None, Modifiers::SHIFT);
        let rect = s
            .writing(|w| *w.cells.peek())
            .expect("Shift-click selects cells")
            .1;
        assert_eq!(rect.count(), 4);
        let clipboard = std::sync::Arc::new(TestClipboard::default());
        s.doc.inner.borrow_mut().shell_provider = clipboard.clone();
        s.key(Key::Character("c".into()), Modifiers::META);
        assert_eq!(*clipboard.0.lock().unwrap(), "Café\t🌿\na|b\tc");
        s.capture("four-cells");
        s.named(NamedKey::Tab);
        assert_focus(&s.doc, "cell-0-1-2", [0, "🌿".len()]);
        s.key(Key::Named(NamedKey::Tab), Modifiers::SHIFT);
        assert_focus(&s.doc, "cell-0-1-1", [0, "Café".len()]);
        s.key(Key::Named(NamedKey::Tab), Modifiers::SHIFT);
        assert_focus(&s.doc, "cell-0-2-2", [0, 1]);
        assert_eq!(s.markdown(), MD);
        s.named(NamedKey::Delete);
        assert_eq!(
            s.markdown(),
            MD.replace("Café", "")
                .replace("🌿", "")
                .replace("a\\|b", "")
                .replace("| c |", "|  |")
        );
        s.key(Key::Character("z".into()), Modifiers::META);
        assert_eq!(s.markdown(), MD);
        s.click_at_text("cell-0-1-1", 0);
        s.key(
            Key::Named(NamedKey::ArrowRight),
            Modifiers::ALT | Modifiers::SHIFT,
        );
        s.key(Key::Named(NamedKey::ArrowDown), Modifiers::SHIFT);
        s.capture("range-menu");
        s.button("delete-column-0");
        assert_eq!(
            s.markdown(),
            "# Ranges\n\n| A | D |\n| --- | --- |\n| one | keep |\n| two | last |\n\nOutside"
        );
        s.key(Key::Character("z".into()), Modifiers::META);
        assert_eq!(s.markdown(), MD);
        s.click_at_text("cell-0-1-1", 0);
        click_with(&mut s.doc, "cell-0-2-2", None, Modifiers::SHIFT);
        *clipboard.0.lock().unwrap() = "q|v\tz\textra\r\nl\tm\textra".into();
        s.key(Key::Character("v".into()), Modifiers::META);
        assert_eq!(
            s.markdown(),
            MD.replace("| Café | 🌿 |", "| q\\|v | z |")
                .replace("| a\\|b | c |", "| l | m |")
        );
        s.key(Key::Character("z".into()), Modifiers::META);
        s.click_at_text("cell-0-1-1", 0);
        click_with(&mut s.doc, "cell-0-2-2", None, Modifiers::SHIFT);
        s.button("delete-row-0");
        assert_eq!(
            s.markdown(),
            MD.replace(
                "\n| one | Café | 🌿 | keep |\n| two | a\\|b | c | last |",
                ""
            )
        );
        s.key(Key::Character("z".into()), Modifiers::META);
        assert_eq!(s.markdown(), MD);
    });
}

#[test]
fn floating_format_bar_keeps_text_selection_focus_and_undo_in_prose_and_cells() {
    const MD: &str = "# Pages\n\nCafé au port\n\n| Name | Time |\n| --- | --- |\n| Été | Noon |";
    run("selection-tools", MD, (840, 530), 1, |s| {
        s.click_at_text("note-body", 5);
        select(&s.doc, "note-body", [5, 0]);
        send_key(
            &mut s.doc,
            Key::Named(NamedKey::Shift),
            KeyState::Released,
            false,
        );
        s.capture("text-toolbar");
        s.button("format-0");
        assert_eq!(s.markdown(), MD.replace("Café", "**Café**"));
        assert_focus(&s.doc, "note-body", [7, 2]);
        s.key(Key::Character("b".into()), Modifiers::META);
        assert_eq!(s.markdown(), MD);
        assert_focus(&s.doc, "note-body", [5, 0]);
        s.click_at_text("cell-0-1-0", 2);
        select(&s.doc, "cell-0-1-0", [0, "Été".len()]);
        send_key(
            &mut s.doc,
            Key::Named(NamedKey::Shift),
            KeyState::Released,
            false,
        );
        s.button("format-1");
        assert_eq!(s.markdown(), MD.replace("Été", "*Été*"));
        assert_focus(&s.doc, "cell-0-1-0", [1, 1 + "Été".len()]);
        s.key(Key::Character("z".into()), Modifiers::META);
        assert_eq!(s.markdown(), MD);
    });
}

#[test]
fn ux_polish_showcase_renders_rich_markdown_and_table_controls() {
    const MD: &str = "# Field notes\n\nRecorder packed [ ]. Rain cover ready [x]. Visit [canal](https://example.com).\n\n\n> Morning fog along the lock gate.\n\n---\n\n## Listening stops\n\n| Stop | Listen for | Take |\n| :--- | :--- | ---: |\n| North lock | **Water** through the gate | 45 s |\n| Footbridge | Steps on `steel` deck | 30 s |\n\n- [ ] Leave **ten seconds** of silence.\n- [x] Set the recorder to `48 kHz`.";
    run("ux-showcase", MD, (900, 680), 1, |s| {
        s.capture("live-resting-tight");
        s.click_at_text("note-title", 3);
        s.capture("live-inline-heading-hashes");
        s.click_at_text("note-heading-0", 4);
        s.capture("live-inline-h2-hashes");
        s.click_at_text("cell-0-1-1", 2);
        s.capture("live-active-span-and-table-bar");
        s.button("col-menu-btn-0-1");
        s.capture("live-table-column-menu");
        s.button("row-menu-btn-0-1");
        s.capture("live-table-row-menu");
        let blank = s.markdown().find("\n\n\n>").unwrap() + 2;
        open_body_at(&mut s.doc, [blank; 2]);
        press(&mut s.doc, Key::Character("/".into()), false);
        s.capture("live-slash-menu");
        press(&mut s.doc, Key::Named(NamedKey::Escape), false);
        s.button("rich-mode");
        open_body_at(&mut s.doc, [blank; 2]);
        s.capture("rich-mode-empty-line-icons");
        s.button("live-mode");
        s.writing(|mut w| {
            let mut seeded = comments::seed_sample_comments(&w.note.peek());
            // The showcase's cell is bold; anchor thread 1 to it.
            if !seeded.iter().any(|t| t.id == 1) {
                let md = w.note.peek().markdown();
                let quote = "**Water** through the gate";
                seeded.extend(comments::sample_thread(
                    &w.note.peek(),
                    &md,
                    1,
                    quote,
                    "Specify the mic setup so this take is reproducible?",
                    "Good call — noting the stereo pattern and high-pass filter keeps the log actionable:",
                    "Update table cell",
                    "**Water** through the gate (XY stereo, 80 Hz HPF)",
                ));
                seeded.sort_by_key(|t| t.id);
            }
            let first = seeded.first().map(|t| t.id);
            w.comments.set(seeded);
            w.active_comment.set(first);
            w.comments_open.set(true);
        });
        render(&mut s.doc);
        s.capture("live-comments-pane");
    });
}

#[test]
fn only_one_menu_is_open_at_a_time() {
    const MD: &str = "Intro\n\n> [!NOTE]\n> Callout body\n\n\n| Stop | Take |\n| --- | --- |\n| North | 45 s |";
    run("one-menu", MD, (900, 680), 1, |s| {
        let shown = |s: &Session, id: &str| s.doc.inner.borrow().get_element_by_id(id).is_some();
        let blank = MD.find("\n\n\n").unwrap() + 2;
        let callout_start = MD.find("> [!NOTE]").unwrap();
        s.click_at_text("cell-0-1-0", 2);
        s.button("row-menu-btn-0-1");
        assert!(shown(s, "row-menu-0-1"));
        // The caret moves without a click (a command, a test), then `/` opens
        // the slash menu: the row menu closes.
        open_body_at(&mut s.doc, [blank; 2]);
        s.type_char('/');
        assert!(shown(s, "slash-menu"), "the slash menu opens");
        assert!(!shown(s, "row-menu-0-1"), "the row menu closed");
        // And the other way: the row menu replaces the slash menu.
        s.hover("cell-0-1-1");
        s.button("row-menu-btn-0-1");
        assert!(shown(s, "row-menu-0-1"), "the row menu opens");
        assert!(!shown(s, "slash-menu"), "the slash menu closed");
        // Menus of the same table replace each other too.
        s.button("col-menu-btn-0-1");
        assert!(shown(s, "col-menu-0-1") && !shown(s, "row-menu-0-1"));
        // Opening the callout type picker closes the table menu, and vice versa.
        s.button(&format!("callout-badge-{callout_start}"));
        assert!(shown(s, "callout-menu"), "callout menu opens");
        assert!(!shown(s, "col-menu-0-1"), "table menu closed");
        s.hover("cell-0-1-1");
        s.button("row-menu-btn-0-1");
        assert!(shown(s, "row-menu-0-1"), "row menu opens");
        assert!(!shown(s, "callout-menu"), "callout menu closed");
    });
}

#[test]
fn link_card_visual_showcase() {
    const MD: &str = "# Field notes\n\nLorem ipsum *labore et dolore* magna aliqua. Ut enim ad minim veniam, ~~quis nostrud~~ `ullamco laboris` commodo consequat. Duis aute irure dolor in [Work Log](brain/log \"Append-only audit trail. Add one dated entry per turn that creates, edits, or restructures content. The knowledge-base skill describes what to log and the entry shape.\") in voluptate velit esse.\n\nVisit the [Canal guide](https://example.com/canal/guide \"Field guide for acoustic lock recordings along the northern canal.\") or jump to [Listening stops](#listening-stops).\n\n## Listening stops\n\nMorning fog along the lock gate before the first barge passes.\n\n- North lock — water through the gate\n- Footbridge — steps on the steel deck";
    run("link-card-showcase", MD, (860, 560), 2, |s| {
        s.button("rich-mode");
        let work_pos = MD.find("Work Log").unwrap() + 2;
        open_body_at(&mut s.doc, [work_pos; 2]);
        assert!(s.doc.inner.borrow().get_element_by_id("link-card").is_some());
        s.capture("rich-internal-link-card");

        s.button("link-card-edit");
        assert!(
            s.doc
                .inner
                .borrow()
                .get_element_by_id("link-edit-label")
                .is_some()
        );
        s.capture("rich-link-edit-form");

        s.button("link-edit-cancel");
        let ext_pos = MD.find("Canal guide").unwrap() + 2;
        open_body_at(&mut s.doc, [ext_pos; 2]);
        assert!(s.doc.inner.borrow().get_element_by_id("link-card").is_some());
        s.capture("rich-external-link-card");

        s.button("live-mode");
        open_body_at(&mut s.doc, [work_pos; 2]);
        assert!(s.doc.inner.borrow().get_element_by_id("link-card").is_some());
        s.capture("live-internal-link-card");
    });

    // Also verify placement in a tall scrolled document (testing viewport scroll + flip-above).
    const LONG_MD: &str = "# Long note\n\nLine 01\n\nLine 02\n\nLine 03\n\nLine 04\n\nLine 05\n\nLine 06\n\nLine 07\n\nLine 08\n\nLine 09\n\nLine 10\n\nLine 11\n\nLine 12\n\nSee [Work Log](brain/log \"Append-only audit trail.\") here.\n\nLine 14\n\nLine 15\n\nLine 16\n\nLine 17\n\nLine 18";
    run("link-card-scrolled", LONG_MD, (860, 480), 2, |s| {
        s.button("rich-mode");
        let work_pos = LONG_MD.find("Work Log").unwrap() + 2;
        open_body_at(&mut s.doc, [work_pos; 2]);
        assert!(s.doc.inner.borrow().viewport_scroll().y > 0.0);
        assert!(s.doc.inner.borrow().get_element_by_id("link-card").is_some());
        let card = bounds(&s.doc, "link-card");
        let caret = s.state().caret_rect.expect("caret geometry");
        assert!(
            card[1] >= 40.0 && card[1] + card[3] <= 480.0,
            "scrolled link card must remain inside viewport: card={card:?} caret={caret:?}"
        );
        let gap_above = (caret[1] - (card[1] + card[3])).abs();
        let gap_below = (card[1] - (caret[1] + caret[3])).abs();
        assert!(
            gap_above < 24.0 || gap_below < 24.0,
            "scrolled link card must anchor right beside link: card={card:?} caret={caret:?}"
        );
        s.capture("scrolled-rich-link-card");
    });
}

#[test]
fn callouts_visual_showcase() {
    const MD: &str = "# Field guide\n\n> [!NOTE]\n> Useful context with **bold** and `48 kHz` audio notes.\n\n> [!TIP] Quick shortcut\n> Type `/callout` or `/warning` on an empty line to insert a callout.\n\n> [!IMPORTANT]\n> Untouched Markdown bytes stay exact in Loro.\n\n> [!WARNING]\n> Check the lock gate before stepping onto the footbridge.\n\n> [!CAUTION]\n> High water level after heavy rain.";
    run("callouts-showcase", MD, (860, 720), 2, |s| {
        s.button("rich-mode");
        s.capture("rich-callouts");

        let note_start = MD.find("> [!NOTE]").unwrap();
        s.button(&format!("callout-badge-{note_start}"));
        assert!(s.doc.inner.borrow().get_element_by_id("callout-menu").is_some());
        s.capture("rich-callout-type-picker");
        s.named(NamedKey::Escape);

        s.button("live-mode");
        let tip_pos = MD.find("[!TIP]").unwrap() + 3;
        open_body_at(&mut s.doc, [tip_pos; 2]);
        s.capture("live-callouts-editing-tag");
    });
}
