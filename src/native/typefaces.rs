use parley::{
    FontContext,
    fontique::{Blob, FontInfoOverride, FontStyle, FontWeight},
};

pub(super) fn notebook_fonts() -> FontContext {
    let mut fonts = FontContext::new();
    let specs: &[(&[u8], &str, FontWeight, FontStyle)] = &[
        (
            include_bytes!("../../assets/fonts/instrument-serif-latin-400-normal.woff2"),
            "Instrument Serif",
            FontWeight::NORMAL,
            FontStyle::Normal,
        ),
        (
            include_bytes!("../../assets/fonts/dm-sans-latin-400-normal.woff2"),
            "DM Sans",
            FontWeight::NORMAL,
            FontStyle::Normal,
        ),
        (
            include_bytes!("../../assets/fonts/dm-sans-latin-600-normal.woff2"),
            "DM Sans",
            FontWeight::SEMI_BOLD,
            FontStyle::Normal,
        ),
        (
            include_bytes!("../../assets/fonts/ibm-plex-mono-latin-400-normal.woff2"),
            "IBM Plex Mono",
            FontWeight::NORMAL,
            FontStyle::Normal,
        ),
        (
            include_bytes!("../../assets/fonts/ibm-plex-mono-latin-600-normal.woff2"),
            "IBM Plex Mono",
            FontWeight::SEMI_BOLD,
            FontStyle::Normal,
        ),
        (
            include_bytes!("../../assets/fonts/ibm-plex-mono-latin-400-italic.woff2"),
            "IBM Plex Mono",
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
