//! Browser: the calls `blitz.rs` re-exports, answered from the DOM.
//!
//! The notebook asks its renderer where text is: the caret's rectangle, the
//! byte under a point, a field's lines. Natively Parley has laid the text out
//! and answers directly. A browser keeps a textarea's layout to itself, so a
//! hidden mirror element with the field's font, width and wrapping is measured
//! instead (`text::Layout::measure`). Everything is in CSS pixels: `scale()` is 1.
use super::FieldEvent;
use crate::prelude::*;
use dioxus::web::WebEventExt;
use std::cell::{Cell, OnceCell, RefCell};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Element, HtmlElement, HtmlInputElement, HtmlTextAreaElement};

pub type NodeId = usize;

fn window() -> web_sys::Window {
    web_sys::window().expect("a browser window")
}

fn page() -> web_sys::Document {
    window().document().expect("a document")
}

/// The page's document. A browser has one; the handle only says it is mounted.
#[derive(Clone, PartialEq)]
pub struct NodeHandle;

thread_local! {
    /// Whether a pass is running (`shared_pass`), and the view of the page its readers share.
    static PASS: Cell<bool> = const { Cell::new(false) };
    static SHARED: RefCell<Option<Rc<Document>>> = const { RefCell::new(None) };
}

/// Run one burst of reads against a single view of the page, so that each
/// `doc()` in it finds what the previous one already read from the DOM. Any
/// `doc_mut()` ends the sharing: what was read is stale once the page changes.
fn shared_pass(pass: impl FnOnce()) {
    let nested = PASS.replace(true);
    pass();
    PASS.set(nested);
    if !nested {
        SHARED.take();
    }
}

impl NodeHandle {
    pub fn doc(&self) -> DocRef {
        if !PASS.get() {
            return DocRef(Rc::default());
        }
        DocRef(SHARED.with_borrow_mut(|shared| shared.get_or_insert_with(Rc::default).clone()))
    }

    pub fn doc_mut(&self) -> DocMut {
        SHARED.take();
        DocMut(Document::default())
    }
}

/// A view of the page to read from, as `NodeHandle::doc` lends it.
pub struct DocRef(Rc<Document>);

impl Deref for DocRef {
    type Target = Document;

    fn deref(&self) -> &Document {
        &self.0
    }
}

/// A view of the page that can also change it, as `NodeHandle::doc_mut` lends it.
pub struct DocMut(Document);

impl Deref for DocMut {
    type Target = Document;

    fn deref(&self) -> &Document {
        &self.0
    }
}

impl DerefMut for DocMut {
    fn deref_mut(&mut self) -> &mut Document {
        &mut self.0
    }
}

impl Drop for DocMut {
    fn drop(&mut self) {
        SHARED.take();
    }
}

/// The document an element was mounted into.
pub fn mounted(event: &MountedEvent) -> Option<NodeHandle> {
    event.try_as_web_event().map(|_| NodeHandle)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoundingRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollBehavior {
    Instant,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

pub struct Viewport {
    size: (f32, f32),
    scale: f32,
    /// The viewport in device pixels.
    pub window_size: (u32, u32),
}

impl Viewport {
    pub fn logical_size(&self) -> (f32, f32) {
        self.size
    }

    /// Device pixels per CSS pixel.
    pub fn scale(&self) -> f32 {
        self.scale
    }
}

pub struct Hit {
    pub node_id: NodeId,
}

/// A scrollbar the pointer is on. The browser handles its own scrollbars, so
/// the notebook never sees one.
pub struct Scrollbar {
    pub node_id: NodeId,
}

/// The system clipboard. Reading it is asynchronous in a browser, so a
/// synchronous read reports nothing: pasted text arrives with the page's
/// paste event instead, which the notebook handles (`onpaste`).
#[derive(Clone, Default)]
pub struct Shell;

impl Shell {
    pub fn get_clipboard_text(&self) -> Result<String, ()> {
        Err(())
    }

    /// Start writing `text` to the clipboard. `Ok` only says the write began:
    /// the browser answers later. Code that must know it worked (a cut, which
    /// then deletes) uses `copy_then`.
    pub fn set_clipboard_text(&self, text: String) -> Result<(), ()> {
        write_clipboard(&text).map(|_| ()).ok_or(())
    }
}

/// The browser's clipboard write, as a promise.
fn write_clipboard(text: &str) -> Option<js_sys::Promise> {
    let clipboard = js_sys::Reflect::get(&window().navigator(), &"clipboard".into()).ok()?;
    let write: js_sys::Function = js_sys::Reflect::get(&clipboard, &"writeText".into()).ok()?.dyn_into().ok()?;
    write.call1(&clipboard, &JsValue::from_str(text)).ok()?.dyn_into().ok()
}

/// Put `text` on the clipboard, then run `then`, only once the browser says
/// the text is there: it can refuse (no permission, the page not focused).
pub fn copy_then(_root: &NodeHandle, text: String, then: impl FnOnce() + 'static) {
    let Some(written) = write_clipboard(&text) else {
        return;
    };
    spawn(async move {
        if wasm_bindgen_futures::JsFuture::from(written).await.is_ok() {
            then();
        }
    });
}

/// A view of the page for one burst of queries. Nodes are looked up on demand
/// and stay valid for as long as this value lives.
#[derive(Default)]
pub struct Document {
    nodes: elsa::FrozenVec<Box<Node>>,
    pub shell_provider: Shell,
}

impl Document {
    fn intern(&self, element: Element) -> NodeId {
        for i in 0..self.nodes.len() {
            if self.nodes[i].element == element {
                return i;
            }
        }
        // Parents first, so a node's `parent` is already here.
        let parent = element.parent_element().map(|p| self.intern(p));
        let id = self.nodes.len();
        self.nodes.push(Box::new(Node { id, parent, element, data: OnceCell::new(), layout: OnceCell::new() }));
        id
    }

    pub fn get_element_by_id(&self, id: &str) -> Option<NodeId> {
        page().get_element_by_id(id).map(|e| self.intern(e))
    }

    pub fn get_node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id)
    }

    pub fn get_client_bounding_rect(&self, id: NodeId) -> Option<BoundingRect> {
        let r = self.get_node(id)?.element.get_bounding_client_rect();
        Some(BoundingRect { x: r.x(), y: r.y(), width: r.width(), height: r.height() })
    }

    pub fn get_focussed_node_id(&self) -> Option<NodeId> {
        let active = page().active_element()?;
        (active.tag_name() != "BODY").then(|| self.intern(active))
    }

    /// Forget what was read from the page: it is about to differ.
    fn forget(&mut self) {
        for node in self.nodes.as_mut() {
            node.data.take();
            node.layout.take();
        }
    }

    pub fn set_focus_to(&mut self, id: NodeId) -> bool {
        let Some(el) = self.get_node(id).and_then(|n| n.element.dyn_ref::<HtmlElement>()) else {
            return false;
        };
        let focused = el.focus().is_ok();
        self.forget();
        focused
    }

    pub fn with_text_input(&mut self, id: NodeId, edit: impl FnOnce(TextDriver)) {
        if let Some(node) = self.get_node(id) {
            edit(TextDriver { element: node.element.clone() });
            self.forget();
        }
    }

    /// The deepest element under a point in the viewport.
    pub fn element_from_point(&self, x: f32, y: f32) -> Option<NodeId> {
        page().element_from_point(x, y).map(|e| self.intern(e))
    }

    /// The deepest element under a point given in page coordinates.
    pub fn hit(&self, x: f32, y: f32) -> Option<Hit> {
        let scroll = self.viewport_scroll();
        let node_id = self.element_from_point(x - scroll.x as f32, y - scroll.y as f32)?;
        Some(Hit { node_id })
    }

    pub fn viewport_scroll(&self) -> Point {
        let w = window();
        Point { x: w.scroll_x().unwrap_or(0.0), y: w.scroll_y().unwrap_or(0.0) }
    }

    pub fn viewport(&self) -> Viewport {
        let w = window();
        let size = |v: Result<JsValue, JsValue>| v.ok().and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let (width, height) = (size(w.inner_width()), size(w.inner_height()));
        let scale = w.device_pixel_ratio() as f32;
        Viewport { size: (width, height), scale, window_size: ((width * scale) as u32, (height * scale) as u32) }
    }

    pub fn try_root_element(&self) -> Option<&Node> {
        let root = page().document_element()?;
        self.get_node(self.intern(root))
    }

    pub fn scroll_by(&mut self, id: NodeId, dx: f64, dy: f64, _behavior: ScrollBehavior) {
        let Some(node) = self.get_node(id) else {
            return;
        };
        if node.parent.is_none() {
            window().scroll_by_with_x_and_y(dx, dy);
        } else {
            node.element.scroll_by_with_x_and_y(dx, dy);
        }
    }

    /// The browser lays out before it answers a geometry query, so the next
    /// reads only have to be fresh ones.
    pub fn resolve(&mut self, _time: f64) {
        self.forget();
    }

    pub fn hovered_scrollbar(&self) -> Option<Scrollbar> {
        None
    }

    pub fn scrollbar_drag_target(&self) -> Option<Scrollbar> {
        None
    }
}

pub struct QualName {
    pub local: String,
}

pub struct ElementData {
    pub name: QualName,
    pub id: Option<String>,
    element: Element,
    input: OnceCell<Option<TextInputData>>,
}

impl ElementData {
    /// The field's text and selection, read the first time they are asked for:
    /// most nodes are only looked up for their place on the page.
    pub fn text_input_data(&self) -> Option<&TextInputData> {
        self.input.get_or_init(|| TextInputData::read(&self.element)).as_ref()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edges {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

/// An element's box: the part of a Taffy layout the notebook reads.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoxLayout {
    pub padding: Edges,
    pub border: Edges,
    content: (f32, f32),
}

impl BoxLayout {
    pub fn content_box_width(&self) -> f32 {
        self.content.0
    }

    pub fn content_box_height(&self) -> f32 {
        self.content.1
    }
}

pub struct Node {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    element: Element,
    data: OnceCell<ElementData>,
    layout: OnceCell<BoxLayout>,
}

fn px(style: &web_sys::CssStyleDeclaration, property: &str) -> f32 {
    style
        .get_property_value(property)
        .ok()
        .and_then(|v| v.trim_end_matches("px").parse().ok())
        .unwrap_or(0.0)
}

fn is_active(element: &Element) -> bool {
    page().active_element().is_some_and(|active| &active == element)
}

impl Node {
    pub fn element_data(&self) -> Option<&ElementData> {
        Some(self.data.get_or_init(|| {
            let id = self.element.id();
            ElementData {
                name: QualName { local: self.element.tag_name().to_ascii_lowercase() },
                id: (!id.is_empty()).then_some(id),
                element: self.element.clone(),
                input: OnceCell::new(),
            }
        }))
    }

    pub fn final_layout(&self) -> &BoxLayout {
        self.layout.get_or_init(|| {
            let Ok(Some(style)) = window().get_computed_style(&self.element) else {
                return BoxLayout::default();
            };
            let edges = |kind: &str, suffix: &str| Edges {
                left: px(&style, &format!("{kind}-left{suffix}")),
                right: px(&style, &format!("{kind}-right{suffix}")),
                top: px(&style, &format!("{kind}-top{suffix}")),
                bottom: px(&style, &format!("{kind}-bottom{suffix}")),
            };
            let (padding, border) = (edges("padding", ""), edges("border", "-width"));
            let rect = self.element.get_bounding_client_rect();
            BoxLayout {
                padding,
                border,
                content: (
                    rect.width() as f32 - padding.left - padding.right - border.left - border.right,
                    rect.height() as f32 - padding.top - padding.bottom - border.top - border.bottom,
                ),
            }
        })
    }

    /// A single-line input centres its line in the content box; a textarea starts at the top.
    pub fn text_input_v_centering_offset(&self, _scale: f64) -> f64 {
        match self.element_data().and_then(|data| data.text_input_data()) {
            Some(input) if !input.is_multiline => {
                let line = input.editor.try_layout().map_or(0.0, |l| l.height());
                f64::from((self.final_layout().content_box_height() - line).max(0.0)) * 0.5
            }
            _ => 0.0,
        }
    }

    pub fn scroll_offset(&self) -> Point {
        Point { x: f64::from(self.element.scroll_left()), y: f64::from(self.element.scroll_top()) }
    }

    pub fn is_focussed(&self) -> bool {
        is_active(&self.element)
    }
}

thread_local! {
    /// Set between `compositionstart` and `compositionend`: an input method owns the text.
    static COMPOSING: Cell<bool> = const { Cell::new(false) };
}

fn set_composing(composing: bool) {
    COMPOSING.with(|c| c.set(composing));
}

/// The two elements that hold editable text.
enum TextField<'a> {
    Area(&'a HtmlTextAreaElement),
    Input(&'a HtmlInputElement),
}

/// The same call on either element.
macro_rules! either {
    ($field:expr, $f:ident => $call:expr) => {
        match $field {
            TextField::Area($f) => $call,
            TextField::Input($f) => $call,
        }
    };
}

impl<'a> TextField<'a> {
    fn of(element: &'a Element) -> Option<Self> {
        if let Some(area) = element.dyn_ref() {
            return Some(Self::Area(area));
        }
        let input: &HtmlInputElement = element.dyn_ref()?;
        // A checkbox or a button is an input that holds no text.
        matches!(input.type_().as_str(), "text" | "search" | "url" | "").then_some(Self::Input(input))
    }

    fn value(&self) -> String {
        either!(self, f => f.value())
    }

    /// The selection's start and end in UTF-16 units, and whether its focus is the start.
    fn selection(&self) -> (usize, usize, bool) {
        either!(self, f => (
            f.selection_start().ok().flatten().unwrap_or(0) as usize,
            f.selection_end().ok().flatten().unwrap_or(0) as usize,
            f.selection_direction().ok().flatten().is_some_and(|d| d == "backward"),
        ))
    }

    fn select(&self, start: u32, end: u32, direction: &str) {
        let _ = either!(self, f => f.set_selection_range_with_direction(start, end, direction));
    }

    fn replace_selection(&self, text: &str) {
        let _ = either!(self, f => f.set_range_text(text));
    }
}

pub struct TextInputData {
    pub editor: Editor,
    pub is_multiline: bool,
    pub scroll_offset: f32,
}

impl TextInputData {
    fn read(element: &Element) -> Option<Self> {
        let field = TextField::of(element)?;
        let text = field.value();
        let (start, end, backward) = field.selection();
        let (start, end) = (text::utf8_offset(&text, start), text::utf8_offset(&text, end));
        let (anchor, focus) = if backward { (end, start) } else { (start, end) };
        // A textarea scrolls down its lines, a one-line input along its line.
        let (is_multiline, scroll_offset) = match field {
            TextField::Area(_) => (true, element.scroll_top()),
            TextField::Input(_) => (false, element.scroll_left()),
        };
        Some(Self {
            editor: Editor {
                selection: text::Selection::new(text::Cursor::at(anchor), text::Cursor::at(focus)),
                composing: is_active(element) && COMPOSING.with(Cell::get),
                element: element.clone(),
                text,
                layout: OnceCell::new(),
            },
            is_multiline,
            scroll_offset: scroll_offset as f32,
        })
    }
}

/// A text field's content and selection, as Parley's editor reports them.
pub struct Editor {
    text: String,
    selection: text::Selection,
    composing: bool,
    element: Element,
    layout: OnceCell<Option<Rc<text::Layout>>>,
}

impl Editor {
    pub fn raw_text(&self) -> &str {
        &self.text
    }

    pub fn raw_selection(&self) -> &text::Selection {
        &self.selection
    }

    pub fn is_composing(&self) -> bool {
        self.composing
    }

    pub fn try_layout(&self) -> Option<&text::Layout> {
        self.layout.get_or_init(|| text::Layout::measure(&self.element, &self.text)).as_deref()
    }

    /// The caret's rectangle, `width` wide.
    pub fn cursor_geometry(&self, width: f32) -> Option<text::Rect> {
        Some(self.selection.focus().geometry(self.try_layout()?, width))
    }
}

/// Changes a field's selection, as the closure of `with_text_input` expects.
pub struct TextDriver {
    element: Element,
}

impl TextDriver {
    /// Replace the selected text, as typing would.
    pub fn insert_or_replace_selection(&mut self, text: &str) {
        if let Some(field) = TextField::of(&self.element) {
            field.replace_selection(text);
        }
    }

    pub fn select_byte_range(&mut self, anchor: usize, focus: usize) {
        let Some(field) = TextField::of(&self.element) else {
            return;
        };
        let value = field.value();
        let (a, f) = (text::utf16_offset(&value, anchor) as u32, text::utf16_offset(&value, focus) as u32);
        if f < a { field.select(f, a, "backward") } else { field.select(a, f, "forward") }
    }
}

/// A text field's laid-out text: lines, glyph clusters, carets and selections.
pub mod text {
    use super::{page, px, window};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::hash::{DefaultHasher, Hash, Hasher};
    use std::ops::Range;
    use std::rc::Rc;
    use wasm_bindgen::JsCast;
    use web_sys::{Element, HtmlElement, HtmlTextAreaElement};

    /// UTF-16 code units (the DOM's offsets) to a UTF-8 byte offset.
    pub(super) fn utf8_offset(text: &str, utf16: usize) -> usize {
        let mut units = 0;
        for (i, ch) in text.char_indices() {
            if units >= utf16 {
                return i;
            }
            units += ch.len_utf16();
        }
        text.len()
    }

    pub(super) fn utf16_offset(text: &str, utf8: usize) -> usize {
        text[..text.floor_char_boundary(utf8)].encode_utf16().count()
    }

    /// Which line a caret at a soft wrap belongs to. The DOM reports the next line's start.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum Affinity {
        Downstream,
    }

    /// `x0, y0` to `x1, y1`, as Parley reports geometry.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct Rect {
        pub x0: f64,
        pub y0: f64,
        pub x1: f64,
        pub y1: f64,
    }

    impl Rect {
        pub fn width(&self) -> f64 {
            self.x1 - self.x0
        }

        pub fn height(&self) -> f64 {
            self.y1 - self.y0
        }
    }

    #[derive(Clone, Debug, PartialEq)]
    struct Glyph {
        byte: usize,
        len: usize,
        x: f32,
        advance: f32,
        /// A space a line can wrap at.
        space: bool,
    }

    #[derive(Clone, Debug, PartialEq)]
    struct LineData {
        range: Range<usize>,
        glyphs: Vec<Glyph>,
        metrics: LineMetrics,
        /// Where an empty line's caret sits (text alignment).
        start_x: f32,
        /// The line ends with a line break; its glyph is the line's last.
        hard_break: bool,
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct LineMetrics {
        pub block_min_coord: f32,
        pub block_max_coord: f32,
    }

    #[derive(Clone, Debug, PartialEq)]
    pub struct Layout {
        lines: Vec<LineData>,
        len: usize,
    }

    thread_local! {
        static MIRROR: RefCell<Option<HtmlElement>> = const { RefCell::new(None) };
        /// Each field's latest layout, by element id, with a hash of the text, font
        /// and width it was measured from: a keystroke re-measures one field, not
        /// the note, and the memory held is one layout per field.
        static CACHE: RefCell<HashMap<String, (u64, Rc<Layout>)>> = RefCell::new(HashMap::new());
    }

    /// Measure every field again: the fonts they were measured with have changed.
    pub(super) fn forget_layouts() {
        CACHE.with_borrow_mut(HashMap::clear);
    }

    /// Text properties that decide where glyphs and line breaks fall.
    const COPIED: [&str; 17] = [
        "font-family", "font-size", "font-weight", "font-style", "font-stretch", "font-variant-numeric",
        "font-kerning", "font-feature-settings", "letter-spacing", "word-spacing", "line-height",
        "text-transform", "text-indent", "tab-size", "text-align", "word-break", "overflow-wrap",
    ];

    impl Layout {
        /// Lay `text` out the way `field` does and record where every character is.
        pub(super) fn measure(field: &Element, text: &str) -> Option<Rc<Layout>> {
            let multiline = field.has_type::<HtmlTextAreaElement>();
            let style = window().get_computed_style(field).ok()??;
            let rect = field.get_bounding_client_rect();
            let width = rect.width() as f32
                - px(&style, "padding-left") - px(&style, "padding-right")
                - px(&style, "border-left-width") - px(&style, "border-right-width");
            if rect.width() == 0.0 && rect.height() == 0.0 {
                return None;
            }
            let mut css = String::from("position:absolute;top:0;left:-99999px;visibility:hidden;pointer-events:none;margin:0;padding:0;border:0;box-sizing:content-box;");
            for property in COPIED {
                if let Ok(value) = style.get_property_value(property) {
                    css.push_str(&format!("{property}:{value};"));
                }
            }
            css.push_str(if multiline { "white-space:pre-wrap;" } else { "white-space:pre;" });
            let key = {
                let mut hasher = DefaultHasher::new();
                (text, &css, if multiline { width.to_bits() } else { 0 }).hash(&mut hasher);
                hasher.finish()
            };
            let id = field.id();
            if let Some(hit) = CACHE.with_borrow(|c| c.get(&id).filter(|(of, _)| *of == key).map(|(_, layout)| layout.clone())) {
                return Some(hit);
            }
            if multiline {
                css.push_str(&format!("width:{width}px;"));
            }
            let mirror = MIRROR.with(|m| {
                m.borrow_mut()
                    .get_or_insert_with(|| {
                        let div: HtmlElement = page().create_element("div").unwrap().unchecked_into();
                        div.set_attribute("aria-hidden", "true").ok();
                        page().body().unwrap().append_child(&div).ok();
                        div
                    })
                    .clone()
            });
            mirror.set_attribute("style", &css).ok()?;
            // A trailing break only shows its empty line when something follows it.
            mirror.set_text_content(Some(&format!("{text}\u{200b}")));
            let node = mirror.first_child()?;
            let origin = mirror.get_bounding_client_rect();
            let font_size = px(&style, "font-size");
            let line_height = match px(&style, "line-height") {
                h if h > 0.0 => h,
                _ => (font_size * 1.2).max(1.0),
            };
            let align = style.get_property_value("text-align").unwrap_or_default();
            let start_x = match align.as_str() {
                "right" | "end" => width,
                "center" => width * 0.5,
                _ => 0.0,
            };
            let range = page().create_range().ok()?;
            let mut lines: Vec<LineData> = Vec::new();
            let mut glyphs: Vec<Glyph> = Vec::new();
            let (mut line_start, mut line_top, mut units) = (0usize, None::<f32>, 0u32);
            let finish = |lines: &mut Vec<LineData>, glyphs: &mut Vec<Glyph>, range: Range<usize>, hard_break: bool| {
                let top = lines.len() as f32 * line_height;
                lines.push(LineData {
                    range,
                    glyphs: std::mem::take(glyphs),
                    metrics: LineMetrics { block_min_coord: top, block_max_coord: top + line_height },
                    start_x,
                    hard_break,
                });
            };
            for (byte, ch) in text.char_indices() {
                let len16 = ch.len_utf16() as u32;
                if ch == '\n' {
                    let x = glyphs.last().map_or(start_x, |g| g.x + g.advance);
                    glyphs.push(Glyph { byte, len: 1, x, advance: 0.0, space: false });
                    finish(&mut lines, &mut glyphs, line_start..byte + 1, true);
                    (line_start, line_top) = (byte + 1, None);
                    units += len16;
                    continue;
                }
                range.set_start(&node, units).ok()?;
                range.set_end(&node, units + len16).ok()?;
                let r = range.get_bounding_client_rect();
                units += len16;
                let top = (r.top() - origin.top()) as f32;
                // A character that lands lower than its line started a wrapped line.
                if line_top.is_some_and(|t| top > t + line_height * 0.5) {
                    finish(&mut lines, &mut glyphs, line_start..byte, false);
                    (line_start, line_top) = (byte, None);
                }
                line_top.get_or_insert(top);
                let x = (r.left() - origin.left()) as f32;
                glyphs.push(Glyph { byte, len: ch.len_utf8(), x, advance: r.width() as f32, space: ch.is_whitespace() });
            }
            finish(&mut lines, &mut glyphs, line_start..text.len(), false);
            let layout = Rc::new(Layout { lines, len: text.len() });
            CACHE.with_borrow_mut(|c| c.insert(id, (key, layout.clone())));
            Some(layout)
        }

        /// Layout units per CSS pixel. The DOM already answers in CSS pixels.
        pub fn scale(&self) -> f32 {
            1.0
        }

        pub fn len(&self) -> usize {
            self.lines.len()
        }

        pub fn height(&self) -> f32 {
            self.lines.last().map_or(0.0, |l| l.metrics.block_max_coord)
        }

        pub fn get(&self, index: usize) -> Option<Line<'_>> {
            (index < self.lines.len()).then_some(Line { layout: self, index })
        }

        pub fn lines(&self) -> impl DoubleEndedIterator<Item = Line<'_>> {
            (0..self.lines.len()).map(move |index| Line { layout: self, index })
        }

        /// The line a caret at `byte` is drawn on. At a soft wrap it is the next line's start.
        fn line_of(&self, byte: usize) -> usize {
            self.lines
                .iter()
                .position(|l| byte < l.range.end)
                .unwrap_or(self.lines.len().saturating_sub(1))
        }

        fn x_at(&self, line: usize, byte: usize) -> f32 {
            let l = &self.lines[line];
            match l.glyphs.iter().find(|g| g.byte >= byte) {
                Some(g) => g.x,
                None => l.glyphs.last().map_or(l.start_x, |g| g.x + g.advance),
            }
        }
    }

    #[derive(Clone, Copy)]
    pub struct Line<'a> {
        layout: &'a Layout,
        index: usize,
    }

    impl<'a> Line<'a> {
        pub fn metrics(&self) -> &'a LineMetrics {
            &self.layout.lines[self.index].metrics
        }

        pub fn text_range(&self) -> Range<usize> {
            self.layout.lines[self.index].range.clone()
        }

        /// One left-to-right run per line: the mirror does not expose bidi runs.
        pub fn runs(&self) -> impl Iterator<Item = Run> {
            std::iter::once(Run { range: self.text_range() })
        }
    }

    pub struct Run {
        range: Range<usize>,
    }

    impl Run {
        pub fn text_range(&self) -> Range<usize> {
            self.range.clone()
        }

        pub fn is_rtl(&self) -> bool {
            false
        }
    }

    pub struct Cluster<'a> {
        layout: &'a Layout,
        line: usize,
        glyph: usize,
    }

    impl<'a> Cluster<'a> {
        pub fn from_byte_index(layout: &'a Layout, byte: usize) -> Option<Self> {
            let line = layout.lines.iter().position(|l| byte < l.range.end)?;
            let glyph = layout.lines[line].glyphs.iter().position(|g| byte < g.byte + g.len)?;
            Some(Self { layout, line, glyph })
        }

        fn glyph(&self) -> &'a Glyph {
            &self.layout.lines[self.line].glyphs[self.glyph]
        }

        pub fn visual_offset(&self) -> Option<f32> {
            Some(self.glyph().x)
        }

        pub fn advance(&self) -> f32 {
            self.glyph().advance
        }

        pub fn text_range(&self) -> Range<usize> {
            let g = self.glyph();
            g.byte..g.byte + g.len
        }

        pub fn line(&self) -> Line<'a> {
            Line { layout: self.layout, index: self.line }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Cursor {
        index: usize,
    }

    impl Cursor {
        pub(super) fn at(index: usize) -> Self {
            Self { index }
        }

        pub fn from_byte_index(layout: &Layout, index: usize, _affinity: Affinity) -> Self {
            Self { index: index.min(layout.len) }
        }

        /// The caret position nearest a point in the layout.
        pub fn from_point(layout: &Layout, x: f32, y: f32) -> Self {
            let Some(line) = layout
                .lines
                .iter()
                .position(|l| y < l.metrics.block_max_coord)
                .or(layout.lines.len().checked_sub(1))
            else {
                return Self { index: 0 };
            };
            let l = &layout.lines[line];
            let wrapped = !l.hard_break && line + 1 < layout.lines.len();
            // Past the line's text the caret sits before its line break, if it
            // has one. A wrapped line's end is the next line's start, where the
            // caret would be drawn, so it sits before the spaces it wrapped at
            // (or before its last letter, when a word was broken).
            let (text, end) = match l.glyphs.split_last() {
                Some((line_break, text)) if l.hard_break => (text, line_break.byte),
                Some((last, _)) if wrapped => {
                    let words = l.glyphs.iter().rposition(|g| !g.space).map_or(0, |i| i + 1);
                    let end = l.glyphs.get(words).map_or(last.byte, |space| space.byte);
                    (&l.glyphs[..words], end)
                }
                _ => (&l.glyphs[..], l.range.end),
            };
            let index = text.iter().find(|g| x < g.x + g.advance * 0.5).map_or(end, |g| g.byte);
            Self { index }
        }

        pub fn index(&self) -> usize {
            self.index
        }

        pub fn geometry(&self, layout: &Layout, width: f32) -> Rect {
            if layout.lines.is_empty() {
                return Rect::default();
            }
            let line = layout.line_of(self.index);
            let x = f64::from(layout.x_at(line, self.index));
            let m = layout.lines[line].metrics;
            Rect { x0: x, y0: f64::from(m.block_min_coord), x1: x + f64::from(width), y1: f64::from(m.block_max_coord) }
        }

        fn step(&self, layout: &Layout, forward: bool) -> Self {
            let mut bounds = layout.lines.iter().flat_map(|l| l.glyphs.iter().map(|g| g.byte)).chain([layout.len]);
            let index = if forward {
                bounds.find(|b| *b > self.index).unwrap_or(layout.len)
            } else {
                bounds.take_while(|b| *b < self.index).last().unwrap_or(0)
            };
            Self { index }
        }

        pub fn next_visual(&self, layout: &Layout) -> Self {
            self.step(layout, true)
        }

        pub fn previous_visual(&self, layout: &Layout) -> Self {
            self.step(layout, false)
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Selection {
        anchor: Cursor,
        focus: Cursor,
    }

    impl Selection {
        pub fn new(anchor: Cursor, focus: Cursor) -> Self {
            Self { anchor, focus }
        }

        pub fn anchor(&self) -> Cursor {
            self.anchor
        }

        pub fn focus(&self) -> Cursor {
            self.focus
        }

        pub fn text_range(&self) -> Range<usize> {
            self.anchor.index.min(self.focus.index)..self.anchor.index.max(self.focus.index)
        }

        pub fn is_collapsed(&self) -> bool {
            self.anchor.index == self.focus.index
        }

        /// One rectangle per line the selection touches, with the line's index.
        pub fn geometry(&self, layout: &Layout) -> Vec<(Rect, usize)> {
            let selected = self.text_range();
            let mut rects = Vec::new();
            for (index, l) in layout.lines.iter().enumerate() {
                let (start, end) = (selected.start.max(l.range.start), selected.end.min(l.range.end));
                if start >= end && !(selected.is_empty() && l.range.contains(&start)) {
                    continue;
                }
                // Past the line's last character `x_at` is the line's end.
                let (x0, x1) = (layout.x_at(index, start), layout.x_at(index, end));
                rects.push((
                    Rect {
                        x0: f64::from(x0),
                        y0: f64::from(l.metrics.block_min_coord),
                        x1: f64::from(x1.max(x0)),
                        y1: f64::from(l.metrics.block_max_coord),
                    },
                    index,
                ));
            }
            rects
        }
    }
}

/// Keep a single-line field's caret in view. The browser scrolls its own fields.
pub fn keep_caret_in_view(_doc: &mut Document, _id: NodeId) {}

/// Whether the field under a press already has its caret there. A browser
/// places a touch's caret on the mouse events it sends after the finger lifts,
/// later than the pointer events the notebook handles.
pub fn press_places_caret(press: &PointerData) -> bool {
    press.pointer_type() != "touch"
}

/// The notebook placed a touch's caret in field `id` at the press: keep the
/// browser from placing its own when the finger lifts. (Past the end of a
/// wrapped line the browser's is the next line's start.) Only while the field
/// has the focus: a tap that focuses it is the browser's.
pub fn keep_touch_caret(doc: &Document, id: NodeId) {
    let Some(node) = doc.get_node(id) else { return };
    let field = node.element.clone();
    let options = web_sys::AddEventListenerOptions::new();
    options.set_once(true);
    options.set_passive(false);
    let keep = Closure::once_into_js(move |e: web_sys::Event| {
        // A touch that became a scroll ends uncancelable: nothing to keep.
        if e.cancelable() && page().active_element().as_ref() == Some(&field) {
            e.prevent_default();
        }
    });
    let _ = node.element.add_event_listener_with_callback_and_add_event_listener_options("touchend", keep.unchecked_ref(), &options);
}

/// Whether moving this pressed pointer drags a selection. In a browser a
/// moving finger scrolls the page, and the press is then taken away.
pub fn move_selects(pointer: &PointerData) -> bool {
    pointer.pointer_type() != "touch"
}

/// Whether a click follows this release. A browser always sends one.
pub fn click_follows_release(_release: &PointerData) -> bool {
    true
}

/// The width at which the comments pane stops sitting beside the note and
/// covers it (`comments.css`).
const PHONE_WIDTH: f64 = 720.0;

/// Whether the window is as narrow as a phone's screen.
pub fn phone_sized() -> bool {
    window().inner_width().ok().and_then(|w| w.as_f64()).is_some_and(|w| w <= PHONE_WIDTH)
}

/// Whether shortcuts use ⌘: the page is open on a Mac, an iPhone or an iPad.
pub fn apple_keys() -> bool {
    window().navigator().platform().is_ok_and(|p| p.starts_with("Mac") || p.starts_with("iP"))
}

/// Open a link in a new tab.
pub fn open_url(url: &str) {
    let _ = window().open_with_url_and_target(url, "_blank");
}

/// The browser tab, for its title.
#[derive(Clone, PartialEq)]
pub struct Window;

impl Window {
    pub fn set_title(&self, title: &str) {
        page().set_title(title);
    }
}

pub fn use_window() -> Option<Window> {
    Some(Window)
}

type Handler = Closure<dyn FnMut(web_sys::Event)>;

/// DOM listeners that last as long as this value: dropping it removes them.
#[derive(Default)]
struct Listeners(Vec<(web_sys::EventTarget, &'static str, bool, Handler)>);

impl Listeners {
    fn add(&mut self, target: &web_sys::EventTarget, name: &'static str, capture: bool, handler: impl FnMut(web_sys::Event) + 'static) {
        let handler = Handler::new(handler);
        let _ = target.add_event_listener_with_callback_and_bool(name, handler.as_ref().unchecked_ref(), capture);
        self.0.push((target.clone(), name, capture, handler));
    }
}

impl Drop for Listeners {
    fn drop(&mut self) {
        for (target, name, capture, handler) in &self.0 {
            let _ = target.remove_event_listener_with_callback_and_bool(name, handler.as_ref().unchecked_ref(), *capture);
        }
    }
}

/// A task that runs once on the next animation frame, however often it is asked for.
struct NextFrame {
    task: Closure<dyn FnMut()>,
    request: Rc<Cell<Option<i32>>>,
}

impl NextFrame {
    fn new(mut task: impl FnMut() + 'static) -> Self {
        let request = Rc::new(Cell::new(None));
        let asked = request.clone();
        let task = Closure::<dyn FnMut()>::new(move || {
            asked.set(None);
            task();
        });
        Self { task, request }
    }

    fn request(&self) {
        if self.request.get().is_none() {
            self.request.set(window().request_animation_frame(self.task.as_ref().unchecked_ref()).ok());
        }
    }
}

impl Drop for NextFrame {
    fn drop(&mut self) {
        if let Some(request) = self.request.take() {
            let _ = window().cancel_animation_frame(request);
        }
    }
}

/// Tell the notebook when the focused field's caret or geometry moves, so its
/// overlays follow: selection changes, composition, scrolling, resizing, and
/// fonts arriving. One change sends several of these events, so they are
/// reported once, on the next frame, and the notebook's reads in that report
/// share one view of the page. The listeners go when the component does.
pub fn use_field_watch(_root: Signal<Option<NodeHandle>>, on_event: impl FnMut(FieldEvent) + 'static) {
    use_hook(move || {
        let on_event = Rc::new(RefCell::new(on_event));
        let changed = Rc::new(NextFrame::new({
            let on_event = on_event.clone();
            move || shared_pass(|| (on_event.borrow_mut())(FieldEvent::Changed))
        }));
        let notify = |before: fn()| {
            let changed = changed.clone();
            move |_: web_sys::Event| {
                before();
                changed.request();
            }
        };
        let mut listeners = Listeners::default();
        let doc: web_sys::EventTarget = page().into();
        listeners.add(&doc, "selectionchange", false, notify(|| {}));
        listeners.add(&doc, "compositionstart", false, {
            let on_event = on_event.clone();
            let changed = changed.clone();
            move |_| {
                set_composing(true);
                (on_event.borrow_mut())(FieldEvent::Composing);
                changed.request();
            }
        });
        listeners.add(&doc, "compositionend", false, notify(|| set_composing(false)));
        listeners.add(&doc, "scroll", true, notify(|| {}));
        listeners.add(&window().into(), "resize", false, {
            let on_event = on_event.clone();
            let changed = changed.clone();
            move |_| {
                (on_event.borrow_mut())(FieldEvent::ViewportChanged);
                changed.request();
            }
        });
        // A face that arrives after its text was measured moves every glyph.
        listeners.add(&page().fonts().into(), "loadingdone", false, notify(text::forget_layouts));
        Rc::new(listeners)
    });
}

/// Inputs waiting for the next frame's report.
#[derive(Clone)]
struct InputTrace {
    pending: Rc<RefCell<Vec<String>>>,
    report: Rc<NextFrame>,
}

impl InputTrace {
    fn push(&self, label: String) {
        let mut pending = self.pending.borrow_mut();
        if pending.len() < super::TRACE_BATCH {
            pending.push(label);
            self.report.request();
        }
    }
}

fn event_field(event: &web_sys::Event, name: &str) -> JsValue {
    js_sys::Reflect::get(event, &name.into()).unwrap_or(JsValue::UNDEFINED)
}

/// The modifiers held during a key or pointer event, as the inspector labels them.
fn held(event: &web_sys::Event) -> String {
    [("metaKey", "Meta+"), ("ctrlKey", "Ctrl+"), ("altKey", "Alt+"), ("shiftKey", "Shift+")]
        .into_iter()
        .filter_map(|(key, name)| event_field(event, key).is_truthy().then_some(name))
        .collect()
}

/// Report what the user did, for the inspector: each frame's key presses,
/// composition, pointer buttons, scrolling and resizing, as short labels.
pub fn use_input_trace(on_inputs: impl FnMut(Vec<String>) + 'static) {
    use_hook(move || {
        let on_inputs = RefCell::new(on_inputs);
        let pending = Rc::new(RefCell::new(vec!["Opened inspector".to_owned()]));
        let report = Rc::new(NextFrame::new({
            let pending = pending.clone();
            move || {
                let inputs = std::mem::take(&mut *pending.borrow_mut());
                if !inputs.is_empty() {
                    (on_inputs.borrow_mut())(inputs);
                }
            }
        }));
        report.request();
        let trace = InputTrace { pending, report };
        let mut listeners = Listeners::default();
        let mut record = |target: &web_sys::EventTarget, name: &'static str, label: Box<dyn Fn(&web_sys::Event) -> Option<String>>| {
            let trace = trace.clone();
            listeners.add(target, name, true, move |event| {
                if let Some(label) = label(&event) {
                    trace.push(label);
                }
            });
        };
        let doc: web_sys::EventTarget = page().into();
        record(&doc, "keydown", Box::new(|e| {
            let key = event_field(e, "key").as_string()?;
            // The inspector's own keys and bare modifiers are not inputs to the note.
            if matches!(key.as_str(), "F8" | "F9" | "Meta" | "Control" | "Alt" | "Shift") {
                return None;
            }
            let repeat = if event_field(e, "repeat").is_truthy() { " (repeat)" } else { "" };
            Some(format!("{}{key:?}{repeat}", held(e)))
        }));
        record(&doc, "compositionupdate", Box::new(|e| Some(format!("IME preedit {:?}", event_field(e, "data").as_string()?))));
        record(&doc, "compositionend", Box::new(|e| Some(format!("IME commit {:?}", event_field(e, "data").as_string()?))));
        for (name, state) in [("pointerdown", "pressed"), ("pointerup", "released")] {
            record(&doc, name, Box::new(move |e| {
                let at = |axis: &str| event_field(e, axis).as_f64().unwrap_or(0.0);
                let pointer = event_field(e, "pointerType").as_string().unwrap_or_default();
                Some(format!("{}{pointer} {state} at {:.0},{:.0} px", held(e), at("clientX"), at("clientY")))
            }));
        }
        record(&doc, "wheel", Box::new(|e| {
            Some(format!("Scroll {:.0},{:.0}", event_field(e, "deltaX").as_f64()?, event_field(e, "deltaY").as_f64()?))
        }));
        let win: web_sys::EventTarget = window().into();
        record(&win, "resize", Box::new(|_| {
            let size = |v: Result<JsValue, JsValue>| v.ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
            Some(format!("Resize {:.0}×{:.0} px", size(window().inner_width()), size(window().inner_height())))
        }));
        // A field's focus passes through the window on its way down: only the window's own counts.
        for (name, focused) in [("focus", true), ("blur", false)] {
            record(&win, name, Box::new(move |e| {
                (e.target() == Some(window().into())).then(|| format!("Window focused {focused}"))
            }));
        }
        Rc::new(listeners)
    });
}

/// Save an inspection report: the browser downloads it. Says where it went.
pub fn save_report(bytes: &[u8]) -> std::io::Result<String> {
    let refused = |_| std::io::Error::other("the browser refused the download");
    let name = format!("margin-report-{}.json", js_sys::Date::now() as u64);
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("application/json");
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options).map_err(refused)?;
    // The URL is left for the page to release: revoking it here can cancel the download.
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(refused)?;
    let link: HtmlElement = page().create_element("a").map_err(refused)?.unchecked_into();
    link.set_attribute("href", &url).map_err(refused)?;
    link.set_attribute("download", &name).map_err(refused)?;
    link.click();
    Ok(format!("{name} to your downloads"))
}

/// The page's fonts, and the browser defaults the notebook's styles assume away.
#[component]
pub fn Head() -> dioxus::prelude::Element {
    rsx! {
        margin_ui::typefaces::FontFaces {}
        style { {include_str!("web.css")} }
    }
}
