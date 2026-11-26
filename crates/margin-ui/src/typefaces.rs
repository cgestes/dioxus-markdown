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

pub fn notebook_fonts() -> FontContext {
    let mut fonts = FontContext::new();
    let specs: &[(&[u8], &str, FontWeight, FontStyle)] = &[
        (
            include_bytes!("../assets/fonts/geist-latin-400-normal.woff2"),
            "Geist",
            FontWeight::NORMAL,
            FontStyle::Normal,
        ),
        (
            include_bytes!("../assets/fonts/geist-latin-500-normal.woff2"),
            "Geist",
            FontWeight::MEDIUM,
            FontStyle::Normal,
        ),
        (
            include_bytes!("../assets/fonts/geist-latin-600-normal.woff2"),
            "Geist",
            FontWeight::SEMI_BOLD,
            FontStyle::Normal,
        ),
        (
            include_bytes!("../assets/fonts/geist-latin-400-italic.woff2"),
            "Geist",
            FontWeight::NORMAL,
            FontStyle::Italic,
        ),
        (
            include_bytes!("../assets/fonts/jetbrains-mono-latin-400-normal.woff2"),
            "JetBrains Mono",
            FontWeight::NORMAL,
            FontStyle::Normal,
        ),
        (
            include_bytes!("../assets/fonts/jetbrains-mono-latin-500-normal.woff2"),
            "JetBrains Mono",
            FontWeight::MEDIUM,
            FontStyle::Normal,
        ),
        (
            include_bytes!("../assets/fonts/jetbrains-mono-latin-600-normal.woff2"),
            "JetBrains Mono",
            FontWeight::SEMI_BOLD,
            FontStyle::Normal,
        ),
        (
            include_bytes!("../assets/fonts/jetbrains-mono-latin-400-italic.woff2"),
            "JetBrains Mono",
            FontWeight::NORMAL,
            FontStyle::Italic,
        ),
    ];
    for &(bytes, family, weight, style) in specs {
        let data = blitz_dom::decode_font_bytes(bytes);
        fonts.collection.register_fonts(
            Blob::from(data.into_owned()),
            Some(FontInfoOverride {
                family_name: Some(family),
                weight: Some(weight),
                style: Some(style),
                ..Default::default()
            }),
        );
    }
    fonts
}
