use parley::{
    FontContext,
    fontique::{Blob, FontInfoOverride, FontStyle, FontWeight},
};

pub(super) fn notebook_fonts() -> FontContext {
    let mut fonts = FontContext::new();
    let specs: &[(&[u8], &str, FontWeight, FontStyle)] = &[
        (
            include_bytes!("../../assets/fonts/geist-latin-400-normal.woff2"),
            "Geist",
            FontWeight::NORMAL,
            FontStyle::Normal,
        ),
        (
            include_bytes!("../../assets/fonts/geist-latin-500-normal.woff2"),
            "Geist",
            FontWeight::MEDIUM,
            FontStyle::Normal,
        ),
        (
            include_bytes!("../../assets/fonts/geist-latin-600-normal.woff2"),
            "Geist",
            FontWeight::SEMI_BOLD,
            FontStyle::Normal,
        ),
        (
            include_bytes!("../../assets/fonts/geist-latin-400-italic.woff2"),
            "Geist",
            FontWeight::NORMAL,
            FontStyle::Italic,
        ),
        (
            include_bytes!("../../assets/fonts/jetbrains-mono-latin-400-normal.woff2"),
            "JetBrains Mono",
            FontWeight::NORMAL,
            FontStyle::Normal,
        ),
        (
            include_bytes!("../../assets/fonts/jetbrains-mono-latin-500-normal.woff2"),
            "JetBrains Mono",
            FontWeight::MEDIUM,
            FontStyle::Normal,
        ),
        (
            include_bytes!("../../assets/fonts/jetbrains-mono-latin-600-normal.woff2"),
            "JetBrains Mono",
            FontWeight::SEMI_BOLD,
            FontStyle::Normal,
        ),
        (
            include_bytes!("../../assets/fonts/jetbrains-mono-latin-400-italic.woff2"),
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
