//! What every Margin crate shares, with no I/O and no interface: edits to
//! Markdown source (`callout`, `checkbox`, `edit`, `format`, `heading`,
//! `table`), the comment file and what an agent is asked about a comment
//! (`comments`), the interfaces to an agent (`agent`) and to versioned files
//! on a server (`files`), and dates. It builds for native and for wasm.
pub mod agent;
pub mod callout;
pub mod checkbox;
pub mod comments;
pub mod dates;
pub mod edit;
pub mod files;
pub mod format;
pub mod heading;
pub mod table;
