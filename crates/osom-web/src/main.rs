//! `osom-web`: Osom in a browser page (Dioxus Web), sharing `osom::Shell`
//! with the native macOS app. Always live (no Margin notebook, no demo data).
#[cfg(target_arch = "wasm32")]
mod page {
    use margin_ui::prelude::*;
    use osom::{Shell, Start};

    fn percent_decode(input: &str) -> String {
        let bytes = input.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%'
                && i + 2 < bytes.len()
                && let (Some(hi), Some(lo)) = ((bytes[i + 1] as char).to_digit(16), (bytes[i + 2] as char).to_digit(16))
            {
                out.push(((hi << 4) | lo) as u8);
                i += 3;
                continue;
            }
            out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    fn query_param(query: &str, name: &str) -> Option<String> {
        query
            .trim_start_matches('?')
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(k, _)| *k == name)
            .map(|(_, v)| percent_decode(v))
            .filter(|v| !v.is_empty())
    }

    pub fn main() {
        margin_runtime::launch("Osom", (1320.0, 860.0), app);
    }

    fn app() -> Element {
        let start = use_hook(|| {
            let query = web_sys::window()
                .and_then(|w| w.location().search().ok())
                .unwrap_or_default();
            Start::for_web(
                query_param(&query, "open").as_deref(),
                query_param(&query, "tab").as_deref(),
                query_param(&query, "doc").as_deref(),
            )
        });
        rsx! { Shell { start } }
    }
}

fn main() {
    #[cfg(target_arch = "wasm32")]
    page::main();
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("osom-web runs in a browser: dx serve --web --package osom-web");
}
