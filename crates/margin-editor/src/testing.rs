//! Drive a rendered document from tests: render it, find elements, click,
//! select, type and read fields. The editor's own tests use it, and so do the
//! tests of the apps that embed the editor.
use blitz_dom::Document;
use blitz_traits::events::{BlitzImeEvent, BlitzKeyEvent, BlitzPointerId, KeyState, MouseEventButtons, UiEvent};
use blitz_traits::shell::Viewport;
use dioxus_native::prelude::*;
use dioxus_native::{DioxusDocument, NodeId};

/// Build and render `dom` the way the tests see the editor: the notebook's
/// fonts in an 820 × 520 window.
pub fn open(dom: VirtualDom) -> DioxusDocument {
    let config = dioxus_native::DocumentConfig {
        font_ctx: Some(margin_ui::typefaces::fonts()),
        viewport: Some(Viewport {
            window_size: (820, 520),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut doc = DioxusDocument::new(dom, config);
    doc.initial_build();
    render(&mut doc);
    doc
}

pub fn render(doc: &mut DioxusDocument) {
    for _ in 0..8 {
        let changed = doc.poll(None);
        doc.inner.borrow_mut().resolve(0.0);
        if !changed {
            return;
        }
    }
    panic!("editor did not settle");
}

pub fn id(doc: &DioxusDocument, name: &str) -> NodeId {
    doc.inner
        .borrow()
        .get_element_by_id(name)
        .unwrap_or_else(|| panic!("missing {name}"))
}

pub fn click(doc: &mut DioxusDocument, name: &str) {
    click_at(doc, name, None);
}

pub fn click_at(doc: &mut DioxusDocument, name: &str, point: Option<(f32, f32)>) {
    click_with(doc, name, point, Modifiers::empty());
}

pub fn click_with(
    doc: &mut DioxusDocument,
    name: &str,
    point: Option<(f32, f32)>,
    modifiers: Modifiers,
) {
    press_with(doc, name, point, modifiers, BlitzPointerId::Mouse);
}

/// A finger's tap on the middle of the element with id `name`.
pub fn tap(doc: &mut DioxusDocument, name: &str) {
    press_with(doc, name, None, Modifiers::empty(), BlitzPointerId::Finger(1));
}

fn press_with(
    doc: &mut DioxusDocument,
    name: &str,
    point: Option<(f32, f32)>,
    modifiers: Modifiers,
    pointer_id: BlitzPointerId,
) {
    let element = id(doc, name);
    let mut pointer = doc
        .inner
        .borrow()
        .get_node(element)
        .unwrap()
        .synthetic_click_event_data(modifiers);
    pointer.id = pointer_id;
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

pub fn select(doc: &DioxusDocument, name: &str, [anchor, focus]: [usize; 2]) {
    let id = id(doc, name);
    doc.inner
        .borrow_mut()
        .with_text_input(id, |mut e| e.select_byte_range(anchor, focus));
}

pub fn assert_focus(doc: &DioxusDocument, name: &str, offsets: [usize; 2]) {
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

pub fn send_key(doc: &mut DioxusDocument, key: Key, state: KeyState, repeat: bool) {
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

pub fn press(doc: &mut DioxusDocument, key: Key, repeat: bool) {
    send_key(doc, key, KeyState::Pressed, repeat);
}

pub fn backspace_text(doc: &mut DioxusDocument) {
    #[cfg(target_os = "macos")]
    {
        doc.handle_ui_event(UiEvent::AppleStandardKeybinding("deleteBackward:".into()));
        render(doc);
    }
    press(doc, Key::Named(NamedKey::Backspace), false);
}

pub fn field(doc: &DioxusDocument, id: NodeId) -> String {
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

pub fn commit_text(doc: &mut DioxusDocument, text: &str) {
    doc.handle_ui_event(UiEvent::Ime(BlitzImeEvent::Commit(text.into())));
    render(doc);
}

pub fn value(doc: &DioxusDocument, name: &str) -> String {
    field(doc, id(doc, name))
}

pub fn bounds(doc: &DioxusDocument, name: &str) -> [f64; 4] {
    let dom = doc.inner.borrow();
    let r = dom.get_client_bounding_rect(id(doc, name)).unwrap();
    [r.x, r.y, r.width, r.height]
}

/// The text content of the element with id `name`.
pub fn text(doc: &DioxusDocument, name: &str) -> String {
    doc.inner.borrow().get_node(id(doc, name)).unwrap().text_content()
}

/// Whether the document has an element with id `name`.
pub fn has(doc: &DioxusDocument, name: &str) -> bool {
    doc.inner.borrow().get_element_by_id(name).is_some()
}

/// An attribute of the element with id `name`.
pub fn attribute(doc: &DioxusDocument, name: &str, attribute: &str) -> Option<String> {
    doc.inner.borrow().get_node(id(doc, name)).unwrap().attr(attribute.into()).map(str::to_owned)
}

/// Poll the document until `done` holds or `timeout` passes: for work that
/// finishes off the UI thread (a worker, an agent's reply).
pub fn settle(doc: &mut DioxusDocument, timeout: std::time::Duration, done: impl Fn(&DioxusDocument) -> bool) {
    let started = std::time::Instant::now();
    while !done(doc) && started.elapsed() < timeout {
        doc.poll(None);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// The id of the first text field that shows `text`.
pub fn field_showing(doc: &DioxusDocument, text: &str) -> Option<String> {
    let dom = doc.inner.borrow();
    dom.query_selector_all("textarea").ok()?.into_iter().find_map(|node| {
        let node = dom.get_node(node)?;
        let shown = node.element_data()?.text_input_data()?.editor.raw_text().contains(text);
        shown.then(|| node.attr("id".into()).map(str::to_owned)).flatten()
    })
}

/// Comment on `quote` as a reader does: select it where the notebook shows
/// it, press Comment in the format bar, write `request`, post. What follows
/// shows in the page, and reaches the app through its `NoteStore` (and, on a
/// page, its `PageFiles`).
pub fn comment_on(doc: &mut DioxusDocument, quote: &str, request: &str) {
    let name = field_showing(doc, quote).unwrap_or_else(|| panic!("no field shows {quote:?}"));
    let start = value(doc, &name).find(quote).unwrap();
    click(doc, &name);
    select(doc, &name, [start, start + quote.len()]);
    send_key(doc, Key::Named(NamedKey::Shift), KeyState::Released, false);
    click(doc, "format-comment");
    click(doc, "new-comment-input");
    commit_text(doc, request);
    click(doc, "new-comment-post");
}
