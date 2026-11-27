//! Osom: the Topics window, and the commands that run without it
//! (`osom setup`, `osom pair`, `osom --print`, `osom comment-test`).
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    use margin_ui::prelude::*;
    use osom::{Shell, Start, commands};

    margin_runtime::restart::answer_build_probe("osom");
    let cli = osom::cli::init();
    if let Some(code) = commands::run(cli) {
        std::process::exit(code);
    }
    fn app() -> Element {
        rsx! { Shell { start: Start::from_cli(osom::cli::current().topics_args()) } }
    }
    margin_runtime::launch("Osom", (1320.0, 860.0), app);
}

#[cfg(target_arch = "wasm32")]
fn main() {}
