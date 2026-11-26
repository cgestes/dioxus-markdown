//! The typefaces every window uses (Geist and JetBrains Mono), bundled.
#[cfg(not(target_arch = "wasm32"))]
use parley::{
    FontContext,
    fontique::{Blob, FontInfoOverride, FontStyle, FontWeight},
};

/// The bundled faces: family, weight, whether italic, and the file under the
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

/// `@font-face` rules for the bundled faces, served as assets.
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

/// The bundled faces as a font context: what a Blitz window, or a test that
/// renders one, lays text out with.
#[cfg(not(target_arch = "wasm32"))]
pub fn fonts() -> FontContext {
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
