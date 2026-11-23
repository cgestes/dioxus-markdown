use parley::{FontContext, fontique::Blob};

pub(super) fn notebook_fonts() -> FontContext {
    let mut fonts = FontContext::new();
    for bytes in [
        include_bytes!("../../assets/fonts/instrument-serif-latin-400-normal.woff2").as_slice(),
        include_bytes!("../../assets/fonts/dm-sans-latin-400-normal.woff2"),
        include_bytes!("../../assets/fonts/dm-sans-latin-600-normal.woff2"),
        include_bytes!("../../assets/fonts/ibm-plex-mono-latin-400-normal.woff2"),
        include_bytes!("../../assets/fonts/ibm-plex-mono-latin-600-normal.woff2"),
        include_bytes!("../../assets/fonts/ibm-plex-mono-latin-400-italic.woff2"),
    ] {
        let data = blitz_dom::decode_font_bytes(bytes);
        fonts
            .collection
            .register_fonts(Blob::from(data.into_owned()), None);
    }
    fonts
}
