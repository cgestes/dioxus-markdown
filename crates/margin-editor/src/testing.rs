//! Drive a rendered document from tests: render it, find elements, click,
//! select, type and read fields. The editor's own tests use it, and so do the
//! tests of the apps that embed the editor.
use blitz_dom::Document;
use blitz_traits::events::{BlitzImeEvent, BlitzKeyEvent, KeyState, MouseEventButtons, UiEvent};
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

fn writing(doc: &DioxusDocument) -> crate::Writing {
    doc.vdom.in_scope(ScopeId::APP, consume_context::<crate::Writing>)
}

/// Comment on `range` of the note as its reader would: quote it and ask
/// `request`. Here and below, the notebook is the document's root.
pub fn ask_about(doc: &DioxusDocument, range: std::ops::Range<usize>, quote: &str, request: &str) {
    doc.vdom.in_scope(ScopeId::APP, || {
        let mut w = consume_context::<crate::Writing>();
        w.discussion.draft.set(Some(crate::comments::DraftComment { range, quote: quote.into() }));
        w.submit_draft_comment(request);
    });
}

/// The note's comment threads, as the notebook holds them.
pub fn threads(doc: &DioxusDocument) -> Vec<crate::CommentThread> {
    writing(doc).discussion.threads.peek().clone()
}

/// Where the comment file of a page stands.
#[derive(Clone, Debug, PartialEq)]
pub struct CommentFileState {
    /// Read from the server, or known to be absent.
    pub loaded: bool,
    pub saving: bool,
    pub error: Option<String>,
}

pub fn comment_file(doc: &DioxusDocument) -> CommentFileState {
    let file = writing(doc).discussion.file;
    let file = file.peek();
    CommentFileState { loaded: file.is_loaded(), saving: file.is_saving(), error: file.error.clone() }
}
