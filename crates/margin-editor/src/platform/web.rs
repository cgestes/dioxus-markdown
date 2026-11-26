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
use std::rc::Rc;
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

impl NodeHandle {
    pub fn doc(&self) -> Document {
        Document::default()
    }

    pub fn doc_mut(&self) -> Document {
        Document::default()
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
/// synchronous read reports nothing and paste stays the browser's own.
#[derive(Clone, Default)]
pub struct Shell;

impl Shell {
    pub fn get_clipboard_text(&self) -> Result<String, ()> {
        Err(())
    }

    pub fn set_clipboard_text(&self, text: String) -> Result<(), ()> {
        let clipboard = js_sys::Reflect::get(&window().navigator(), &"clipboard".into()).map_err(|_| ())?;
        let write = js_sys::Reflect::get(&clipboard, &"writeText".into()).map_err(|_| ())?;
        let write: js_sys::Function = write.dyn_into().map_err(|_| ())?;
        write.call1(&clipboard, &JsValue::from_str(&text)).map(|_| ()).map_err(|_| ())
    }
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

    pub fn set_focus_to(&mut self, id: NodeId) -> bool {
        let Some(el) = self.get_node(id).and_then(|n| n.element.dyn_ref::<HtmlElement>()) else {
            return false;
        };
        el.focus().is_ok()
    }

    pub fn with_text_input(&mut self, id: NodeId, edit: impl FnOnce(TextDriver)) {
        if let Some(node) = self.get_node(id) {
            edit(TextDriver { element: node.element.clone() });
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

    /// The browser lays out before it answers a geometry query.
    pub fn resolve(&mut self, _time: f64) {}

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

    /// The byte of this element's text under a point given from its content box.
    pub fn text_offset_at_point(&self, x: f32, y: f32) -> Option<usize> {
        let rect = self.element.get_bounding_client_rect();
        let l = self.final_layout();
        let (cx, cy) = (rect.x() as f32 + l.padding.left + l.border.left + x, rect.y() as f32 + l.padding.top + l.border.top + y);
        // WebKit and Blink answer `caretRangeFromPoint`; the standard is `caretPositionFromPoint`.
        let doc: JsValue = page().into();
        let call = |name: &str| -> Option<JsValue> {
            let f: js_sys::Function = js_sys::Reflect::get(&doc, &name.into()).ok()?.dyn_into().ok()?;
            f.call2(&doc, &cx.into(), &cy.into()).ok().filter(|v| !v.is_null() && !v.is_undefined())
        };
        let (container, offset) = if let Some(range) = call("caretRangeFromPoint") {
            (js_sys::Reflect::get(&range, &"startContainer".into()).ok()?, js_sys::Reflect::get(&range, &"startOffset".into()).ok()?)
        } else {
            let position = call("caretPositionFromPoint")?;
            (js_sys::Reflect::get(&position, &"offsetNode".into()).ok()?, js_sys::Reflect::get(&position, &"offset".into()).ok()?)
        };
        let container: web_sys::Node = container.dyn_into().ok()?;
        if !self.element.contains(Some(&container)) {
            return None;
        }
        // The text before the point, from the element's start.
        let before = page().create_range().ok()?;
        before.set_start(&self.element, 0).ok()?;
        before.set_end(&container, offset.as_f64()? as u32).ok()?;
        let prefix: String = before.to_string().into();
        Some(prefix.len())
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
    }

    #[derive(Clone, Debug, PartialEq)]
    struct LineData {
        range: Range<usize>,
        glyphs: Vec<Glyph>,
        metrics: LineMetrics,
        /// Where an empty line's caret sits (text alignment).
        start_x: f32,
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
        font_size: f32,
    }

    thread_local! {
        static MIRROR: RefCell<Option<HtmlElement>> = const { RefCell::new(None) };
        /// Layouts by a hash of text, font and width: a keystroke re-measures one
        /// field, not the note, and an unchanged field's layout is shared, not copied.
        static CACHE: RefCell<HashMap<u64, Rc<Layout>>> = RefCell::new(HashMap::new());
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
            if let Some(hit) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
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
            let finish = |lines: &mut Vec<LineData>, glyphs: &mut Vec<Glyph>, range: Range<usize>| {
                let top = lines.len() as f32 * line_height;
                lines.push(LineData {
                    range,
                    glyphs: std::mem::take(glyphs),
                    metrics: LineMetrics { block_min_coord: top, block_max_coord: top + line_height },
                    start_x,
                });
            };
            for (byte, ch) in text.char_indices() {
                let len16 = ch.len_utf16() as u32;
                if ch == '\n' {
                    let x = glyphs.last().map_or(start_x, |g| g.x + g.advance);
                    glyphs.push(Glyph { byte, len: 1, x, advance: 0.0 });
                    finish(&mut lines, &mut glyphs, line_start..byte + 1);
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
                    finish(&mut lines, &mut glyphs, line_start..byte);
                    (line_start, line_top) = (byte, None);
                }
                line_top.get_or_insert(top);
                let x = (r.left() - origin.left()) as f32;
                glyphs.push(Glyph { byte, len: ch.len_utf8(), x, advance: r.width() as f32 });
            }
            finish(&mut lines, &mut glyphs, line_start..text.len());
            let layout = Rc::new(Layout { lines, len: text.len(), font_size });
            CACHE.with(|c| {
                let mut cache = c.borrow_mut();
                if cache.len() > 512 {
                    cache.clear();
                }
                cache.insert(key, layout.clone());
            });
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
            std::iter::once(Run { range: self.text_range(), font_size: self.layout.font_size })
        }
    }

    pub struct Run {
        range: Range<usize>,
        font_size: f32,
    }

    impl Run {
        pub fn text_range(&self) -> Range<usize> {
            self.range.clone()
        }

        pub fn is_rtl(&self) -> bool {
            false
        }

        pub fn font_size(&self) -> f32 {
            self.font_size
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
            let index = l
                .glyphs
                .iter()
                .find(|g| g.advance == 0.0 || x < g.x + g.advance * 0.5)
                .map_or(l.range.end, |g| g.byte);
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

/// Tell the notebook when the focused field's caret or geometry moves, so its
/// overlays follow: selection changes, composition, scrolling and resizing.
pub fn use_field_watch(_root: Signal<Option<NodeHandle>>, on_event: impl FnMut(FieldEvent) + 'static) {
    use wasm_bindgen::closure::Closure;
    use_hook(move || {
        let on_event = std::rc::Rc::new(RefCell::new(on_event));
        let listen = |target: &web_sys::EventTarget, name: &'static str, capture: bool| {
            let on_event = on_event.clone();
            let handler = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                match name {
                    "compositionstart" => {
                        set_composing(true);
                        (on_event.borrow_mut())(FieldEvent::Composing);
                    }
                    "compositionend" => set_composing(false),
                    _ => {}
                }
                // A handler that edits the note re-enters through `selectionchange`.
                if let Ok(mut notify) = on_event.try_borrow_mut() {
                    notify(FieldEvent::Changed);
                }
            });
            let _ = target.add_event_listener_with_callback_and_bool(name, handler.as_ref().unchecked_ref(), capture);
            // The notebook lives as long as the page.
            handler.forget();
        };
        let doc: web_sys::EventTarget = page().into();
        let win: web_sys::EventTarget = window().into();
        for name in ["selectionchange", "compositionstart", "compositionend"] {
            listen(&doc, name, false);
        }
        listen(&doc, "scroll", true);
        listen(&win, "resize", false);
    });
}

/// The page's fonts, and the browser defaults the notebook's styles assume away.
#[component]
pub fn Head() -> dioxus::prelude::Element {
    rsx! {
        style { {margin_ui::typefaces::font_faces()} }
        style { {include_str!("web.css")} }
    }
}

/// Open the notebook in the page.
pub fn launch(_title: &str, _size: (f64, f64), root: fn() -> dioxus::prelude::Element) {
    dioxus::launch(root);
}
