//! The editor's model of a note's Markdown, with no UI: how its bytes project
//! onto the text a field shows (and edits map back), the title and body, the
//! blocks the handles move, list and quote continuation, the style flags of
//! each span, and what a link's card says. It builds for native and for wasm;
//! `margin-editor` draws it.
pub mod blocks;
pub mod document;
pub mod link_preview;
pub mod lists;
pub mod projection;
pub mod style;
