//! Margin's design system: the components every surface is built from.
//!
//! Tokens (colours, type, radii, spacing) live in `assets/theme.css`; the
//! component styles in `assets/ui.css`, scoped by `ui-*` classes. Features
//! compose these components and add only layout: no button, card, row,
//! Markdown or checkbox styles of their own. `margin ui` shows every
//! component in every state (see `docs/design-system.md`).
//!
//! This crate depends on no other workspace crate: it knows no Markdown and
//! no app. The editor crate draws Markdown (`margin_editor::MarkdownView`) in
//! these styles and hands it to the components that show it.
/// Dioxus for the renderer this build targets: Blitz natively, the DOM in a
/// browser. Components import this, so the same `rsx!` serves both.
pub mod prelude {
    #[cfg(target_arch = "wasm32")]
    pub use dioxus::prelude::*;
    #[cfg(not(target_arch = "wasm32"))]
    pub use dioxus_native::prelude::*;
}

mod button;
mod combobox;
mod content;
mod field;
pub mod gallery;
mod icon;
mod menu;
mod message;
pub mod typefaces;

pub use button::{Button, Chip, Segment, Segmented, TabItem, Tabs, Variant};
pub use combobox::{Combobox, ComboboxItem};
pub use content::{Avatar, Badge, Banner, Card, CardTitle, Channel, ChannelGlyph, DateBox, NavItem, Row, Rows, SubItem, Tile, TileSize};
pub use field::{CheckItem, Checkbox, Composer, TextField};
pub use icon::{Icon, icon};
pub use menu::{Menu, MenuItem, MenuRow, MenuSection, MenuSeparator, Popover, Toolbar, ToolbarLook, ToolbarSep};
pub use message::{Message, MessageStyle};

/// Tokens, component styles, and compiled Tailwind v4 utilities, for every window.
/// Rebuild with `./scripts/build-css` after changing CSS or Tailwind classes.
pub const CSS: &str = include_str!("../assets/tailwind.css");

/// A tint for tiles and glyphs (`--hue-*` tokens).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hue {
    Slate,
    Rose,
    Clay,
    Ochre,
    Plum,
    #[default]
    Pine,
}

impl Hue {
    pub const ALL: [Hue; 6] = [Hue::Slate, Hue::Rose, Hue::Clay, Hue::Ochre, Hue::Plum, Hue::Pine];

    pub fn class(self) -> &'static str {
        match self {
            Self::Slate => "hue-slate",
            Self::Rose => "hue-rose",
            Self::Clay => "hue-clay",
            Self::Ochre => "hue-ochre",
            Self::Plum => "hue-plum",
            Self::Pine => "hue-pine",
        }
    }
}

/// What a badge or banner says about its content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tone {
    #[default]
    Neutral,
    Accent,
    Ok,
    Info,
    Warn,
    Alert,
}

impl Tone {
    pub const ALL: [Tone; 6] = [Tone::Neutral, Tone::Accent, Tone::Ok, Tone::Info, Tone::Warn, Tone::Alert];

    pub fn class(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Accent => "accent",
            Self::Ok => "ok",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Alert => "alert",
        }
    }
}
