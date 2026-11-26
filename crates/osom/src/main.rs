//! Osom: the Topics window, and the commands that run without it
//! (`osom setup`, `osom pair`, `osom --print`, `osom comment-test`).
use dioxus_native::prelude::*;
use osom::topics::{Shell, Start, commands};

fn main() {
    let cli = osom::cli::init();
    if let Some(code) = commands::run(cli) {
        std::process::exit(code);
    }
    margin_editor::launch("Osom", (1320.0, 860.0), app);
}

fn app() -> Element {
    rsx! { Shell { start: Start::from_cli(osom::cli::current().topics_args()) } }
}
