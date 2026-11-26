#[cfg(not(target_arch = "wasm32"))]
use parley::{
    FontContext,
    fontique::{Blob, FontInfoOverride, FontStyle, FontWeight},
};

/// The proportional family of prose, headings and the interface (`--sans`,
/// `--body`, `--display` in assets/theme.css).
pub const PROSE_FAMILY: &str = "Geist";

/// Width of `text` set in the prose family at `size`, `weight` and slant,
/// measured with the notebook's own fonts. Rich mode paints bold and italic
/// over a field laid out in the regular face; this is what lets each styled
/// run be fitted to exactly the width beneath it.
#[cfg(not(target_arch = "wasm32"))]
pub fn prose_width(text: &str, size: f32, weight: f32, italic: bool) -> Option<f32> {
    use parley::{FontFamily, LayoutContext, StyleProperty};
    use std::cell::RefCell;
    use std::collections::HashMap;
    type Key = (String, u32, u32, bool);
    thread_local! {
        static CX: RefCell<(FontContext, LayoutContext<()>, HashMap<Key, f32>)> =
            RefCell::new((notebook_fonts(), LayoutContext::new(), HashMap::new()));
    }
    CX.with(|cx| {
        let (fonts, layouts, cache) = &mut *cx.borrow_mut();
        let key = (text.to_owned(), size.to_bits(), weight.to_bits(), italic);
        if let Some(w) = cache.get(&key) {
            return Some(*w);
        }
        let mut builder = layouts.ranged_builder(fonts, text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named(PROSE_FAMILY)));
        builder.push_default(StyleProperty::FontSize(size));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(weight)));
        if italic {
            builder.push_default(StyleProperty::FontStyle(FontStyle::Italic));
        }
        let mut layout = builder.build(text);
        layout.break_all_lines(None);
        let width = layout.full_width();
        if cache.len() > 4096 {
            cache.clear();
        }
        cache.insert(key, width);
        (width > 0.0).then_some(width)
    })
}

/// The browser measures with the same faces through a canvas.
#[cfg(target_arch = "wasm32")]
pub fn prose_width(text: &str, size: f32, weight: f32, italic: bool) -> Option<f32> {
    use wasm_bindgen::JsCast;
    thread_local! {
        static CX: Option<web_sys::CanvasRenderingContext2d> = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.create_element("canvas").ok())
            .and_then(|c| c.dyn_into::<web_sys::HtmlCanvasElement>().ok())
            .and_then(|c| c.get_context("2d").ok().flatten())
            .and_then(|c| c.dyn_into().ok());
    }
    CX.with(|cx| {
        let cx = cx.as_ref()?;
        let slant = if italic { "italic " } else { "" };
        cx.set_font(&format!("{slant}{weight} {size}px \"{PROSE_FAMILY}\""));
        let width = cx.measure_text(text).ok()?.width() as f32;
        (width > 0.0).then_some(width)
    })
}

/// The notebook's faces: family, weight, whether italic, and the file under the
/// crate's root. Each renderer reads the table through its own `face!`.
macro_rules! faces {
    () => {
        [
            face!("Geist", 400, false, "/assets/fonts/geist-latin-400-normal.woff2"),
            face!("Geist", 500, false, "/assets/fonts/geist-latin-500-normal.woff2"),
            face!("Geist", 600, false, "/assets/fonts/geist-latin-600-normal.woff2"),
            face!("Geist", 400, true, "/assets/fonts/geist-latin-400-italic.woff2"),
            face!("JetBrains Mono", 400, false, "/assets/fonts/jetbrains-mono-latin-400-normal.woff2"),
            face!("JetBrains Mono", 500, false, "/assets/fonts/jetbrains-mono-latin-500-normal.woff2"),
            face!("JetBrains Mono", 600, false, "/assets/fonts/jetbrains-mono-latin-600-normal.woff2"),
            face!("JetBrains Mono", 400, true, "/assets/fonts/jetbrains-mono-latin-400-italic.woff2"),
        ]
    };
}

/// `@font-face` rules for the notebook's faces, served as bundled assets.
#[cfg(target_arch = "wasm32")]
pub fn font_faces() -> String {
    use crate::prelude::*;
    macro_rules! face {
        ($family:tt, $weight:tt, $italic:tt, $file:tt) => {
            ($family, $weight, $italic, asset!($file))
        };
    }
    faces!()
        .iter()
        .map(|(family, weight, italic, src)| {
            let style = if *italic { "italic" } else { "normal" };
            format!("@font-face{{font-family:\"{family}\";font-weight:{weight};font-style:{style};font-display:swap;src:url(\"{src}\") format(\"woff2\")}}")
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn notebook_fonts() -> FontContext {
    macro_rules! face {
        ($family:tt, $weight:tt, $italic:tt, $file:tt) => {
            ($family, $weight as f32, $italic, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), $file)).as_slice())
        };
    }
    let mut fonts = FontContext::new();
    for (family, weight, italic, bytes) in faces!() {
        let data = blitz_dom::decode_font_bytes(bytes);
        fonts.collection.register_fonts(
            Blob::from(data.into_owned()),
            Some(FontInfoOverride {
                family_name: Some(family),
                weight: Some(FontWeight::new(weight)),
                style: Some(if italic { FontStyle::Italic } else { FontStyle::Normal }),
                ..Default::default()
            }),
        );
    }
    fonts
}
