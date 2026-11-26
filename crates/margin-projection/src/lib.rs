//! Markdown as it is shown, with no UI. For editing: how a note's bytes
//! project onto the text a field shows (and edits map back), the title and
//! body, the blocks the handles move, list and quote continuation, the style
//! flags of each span, what a link's card says, and what a paste becomes.
//! For reading: lines of styled runs, and tables (`reading`). Both highlight
//! code (`syntax`). It builds for native and for wasm; `margin-editor` draws it.
pub mod blocks;
pub mod document;
pub mod link_preview;
pub mod lists;
pub mod paste;
pub mod projection;
pub mod reading;
pub mod style;
pub mod syntax;
