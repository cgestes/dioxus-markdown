//! Icons: one set for the editor, the comments and the Topics window.
//! Lucide-style strokes on a 24-unit grid; `Grip` and `Sparkle` are filled.
use super::Source;
use dioxus_native::prelude::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Icon {
    // Editor
    Bold,
    Italic,
    Strike,
    Code,
    Link,
    List,
    Task,
    Quote,
    Rule,
    Image,
    Grip,
    ChevronDown,
    DotsHorizontal,
    DotsVertical,
    AlignLeft,
    AlignCenter,
    AlignRight,
    Comment,
    Bug,
    Table,
    Undo,
    Redo,
    // Topics and comments
    Car,
    Rings,
    Home,
    Drop,
    Gutter,
    Sliders,
    Users,
    Heart,
    Sprout,
    Person,
    #[default]
    Folder,
    Calendar,
    Inbox,
    Search,
    Plus,
    Repeat,
    Arrow,
    File,
    Spark,
    /// The agent's mark, filled.
    Sparkle,
    Telegram,
    Slack,
    Chat,
    Book,
    Pin,
    Send,
    Question,
    Back,
    Open,
    Check,
    Pencil,
    Copy,
    Trash,
    Clock,
    Info,
    Important,
    AlertTriangle,
    AlertOctagon,
}

impl Icon {
    /// Every icon, for the gallery.
    pub const ALL: [Icon; 60] = [
        Icon::Bold,
        Icon::Italic,
        Icon::Strike,
        Icon::Code,
        Icon::Link,
        Icon::List,
        Icon::Task,
        Icon::Quote,
        Icon::Rule,
        Icon::Image,
        Icon::Grip,
        Icon::ChevronDown,
        Icon::DotsHorizontal,
        Icon::DotsVertical,
        Icon::AlignLeft,
        Icon::AlignCenter,
        Icon::AlignRight,
        Icon::Comment,
        Icon::Bug,
        Icon::Table,
        Icon::Undo,
        Icon::Redo,
        Icon::Car,
        Icon::Rings,
        Icon::Home,
        Icon::Drop,
        Icon::Gutter,
        Icon::Sliders,
        Icon::Users,
        Icon::Heart,
        Icon::Sprout,
        Icon::Person,
        Icon::Folder,
        Icon::Calendar,
        Icon::Inbox,
        Icon::Search,
        Icon::Plus,
        Icon::Repeat,
        Icon::Arrow,
        Icon::File,
        Icon::Spark,
        Icon::Sparkle,
        Icon::Telegram,
        Icon::Slack,
        Icon::Chat,
        Icon::Book,
        Icon::Pin,
        Icon::Send,
        Icon::Question,
        Icon::Back,
        Icon::Open,
        Icon::Check,
        Icon::Pencil,
        Icon::Copy,
        Icon::Trash,
        Icon::Clock,
        Icon::Info,
        Icon::Important,
        Icon::AlertTriangle,
        Icon::AlertOctagon,
    ];

    /// The glyph of a conversation's source.
    pub fn for_source(source: Source) -> Self {
        match source {
            Source::Telegram => Self::Telegram,
            Source::Slack => Self::Slack,
            Source::Chat | Source::Other => Self::Chat,
        }
    }

    /// The glyph of a GFM/Obsidian callout.
    pub const fn for_callout(kind: margin_editor_core::callout::CalloutKind) -> Self {
        use margin_editor_core::callout::CalloutKind;
        match kind {
            CalloutKind::Note => Self::Info,
            CalloutKind::Tip => Self::Spark,
            CalloutKind::Important => Self::Important,
            CalloutKind::Warning => Self::AlertTriangle,
            CalloutKind::Caution => Self::AlertOctagon,
        }
    }

    fn path(self) -> &'static str {
        match self {
            Self::Bold => "M6 4h8a4 4 0 0 1 0 8H6zm0 8h9a4 4 0 0 1 0 8H6z",
            Self::Italic => "M19 4h-9m4 16H5M15 4 9 20",
            Self::Strike => {
                "M17.5 6.5C16.5 4.8 14.8 4 12 4c-3.5 0-5.4 1.7-5.4 4 0 1.5.8 2.6 2.3 3.3M4 12h16m-5.8 1.4c2 .8 3 1.7 3 3.3 0 2.2-2 3.7-5.4 3.7-2.5 0-4.6-.8-6-2.5"
            }
            Self::Code => "m8 7-5 5 5 5m8-10 5 5-5 5",
            Self::Link => {
                "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"
            }
            Self::List => "M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01",
            Self::Task => "m9 11 3 3L22 4M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11",
            Self::Quote => {
                "M3 21c3 0 7-1 7-8V5c0-1.25-.756-2.017-2-2H4c-1.25 0-2 .75-2 1.972V11c0 1.25.75 2 2 2 1 0 1 0 1 1v1c0 1-1 2-2 2s-1 .008-1 1.031V20c0 1 0 1 1 1Zm12 0c3 0 7-1 7-8V5c0-1.25-.757-2.017-2-2h-4c-1.25 0-2 .75-2 1.972V11c0 1.25.75 2 2 2h.75c0 2.25.25 4-2.75 4v3c0 1 0 1 1 1Z"
            }
            Self::Rule => "M4 12h16",
            Self::Image => {
                "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Zm3.5 5.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3ZM21 15l-5-5L5 21"
            }
            Self::Grip | Self::DotsHorizontal | Self::DotsVertical => "",
            Self::ChevronDown => "m6 9 6 6 6-6",
            Self::AlignLeft => "M4 6h16M4 12h10M4 18h14",
            Self::AlignCenter => "M4 6h16M7 12h10M5 18h14",
            Self::AlignRight => "M4 6h16M10 12h10M6 18h14",
            Self::Comment => "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z",
            Self::Bug => {
                "m8 2 1.88 1.88M14.12 3.88 16 2M9 7.13v-1a3.003 3.003 0 1 1 6 0v1M12 20c-3.3 0-6-2.7-6-6v-3a4 4 0 0 1 4-4h4a4 4 0 0 1 4 4v3c0 3.3-2.7 6-6 6ZM12 20v-9M6.53 9C4.6 8.8 3 7.1 3 5M6 13H2M3 21c0-2.1 1.7-3.9 3.8-4M20.97 5c0 2.1-1.6 3.8-3.5 4M22 13h-4M17.2 17c2.1.1 3.8 1.9 3.8 4"
            }
            Self::Table => "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2ZM3 9h18M9 3v18",
            Self::Undo => "M9 14 4 9l5-5M4 9h10.5a5.5 5.5 0 0 1 0 11H11",
            Self::Redo => "m15 14 5-5-5-5M20 9H9.5a5.5 5.5 0 0 0 0 11H13",
            Self::Car => {
                "M5 17H3v-4l2-5h14l2 5v4h-2M5 13h14M9.5 17h5M5.5 17a2 2 0 1 0 4 0a2 2 0 1 0-4 0M14.5 17a2 2 0 1 0 4 0a2 2 0 1 0-4 0"
            }
            Self::Rings => "M4 14.5a5 5 0 1 0 10 0a5 5 0 1 0-10 0M10 14.5a5 5 0 1 0 10 0a5 5 0 1 0-10 0M10 6.5 12 4l2 2.5",
            Self::Home => "M4 11 12 4l8 7M6 9.5V20h12V9.5M10 20v-5h4v5",
            Self::Drop => "M12 3.5s6 6.4 6 10.8a6 6 0 0 1-12 0c0-4.4 6-10.8 6-10.8z",
            Self::Gutter => "M3 7h18l-2.5 4h-13zM16 11v9",
            Self::Sliders => "M6 4v16M12 4v16M18 4v16M4 9h4M10 15h4M16 7h4",
            Self::Users => {
                "M5.8 8a3.2 3.2 0 1 0 6.4 0a3.2 3.2 0 1 0-6.4 0M3 20c0-3.3 2.7-5.5 6-5.5s6 2.2 6 5.5M14.6 9a2.4 2.4 0 1 0 4.8 0a2.4 2.4 0 1 0-4.8 0M16.5 14.6c2.6.2 4.5 2.1 4.5 5.4"
            }
            Self::Heart => "M12 20s-7.5-4.6-7.5-10.2A4.2 4.2 0 0 1 12 7.4a4.2 4.2 0 0 1 7.5 2.4C19.5 15.4 12 20 12 20z",
            Self::Sprout => "M12 20v-8M12 12c0-4 3-6 7-6 0 4-3 6-7 6zM12 14c0-3-2.5-5-6-5 0 3 2.5 5 6 5z",
            Self::Person => "M8.5 8a3.5 3.5 0 1 0 7 0a3.5 3.5 0 1 0-7 0M5 20c0-3.6 3.1-6 7-6s7 2.4 7 6",
            Self::Folder => {
                "M3 6.5A1.5 1.5 0 0 1 4.5 5H9l2 2h8.5A1.5 1.5 0 0 1 21 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5z"
            }
            Self::Calendar => "M6 5h12a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2zM4 10h16M9 3v4M15 3v4",
            Self::Inbox => "M4 13 6.5 5h11L20 13v6H4zM4 13h5l1 2h4l1-2h5",
            Self::Search => "M5 11a6 6 0 1 0 12 0a6 6 0 1 0-12 0M20 20l-4.5-4.5",
            Self::Plus => "M12 5v14M5 12h14",
            Self::Repeat => "m17 3 3 3-3 3M4 12v-1a5 5 0 0 1 5-5h11M7 21l-3-3 3-3M20 12v1a5 5 0 0 1-5 5H4",
            Self::Arrow => "M5 12h14M13 6l6 6-6 6",
            Self::File => "M7 3h7l5 5v13H7zM14 3v5h5",
            Self::Spark => "M12 3v5M12 16v5M3 12h5M16 12h5M6.3 6.3l2.5 2.5M15.2 15.2l2.5 2.5M6.3 17.7l2.5-2.5M15.2 8.8l2.5-2.5",
            Self::Sparkle => "M12 1.8 14.55 9.45 22.2 12 14.55 14.55 12 22.2 9.45 14.55 1.8 12 9.45 9.45Z",
            Self::Telegram => "m21 4-18 7.4 6.6 2.1L12 20l3.3-4.4L20.2 19zM9.6 13.5l6.2-5.3",
            Self::Slack => "M5 9h14M5 15h14M10 4 8 20M16 4l-2 16",
            Self::Chat => "M4 5h16v11h-9l-5 4v-4H4zM8 10h8",
            Self::Book => "M5 4h11a3 3 0 0 1 3 3v13H8a3 3 0 0 1-3-3zM5 17a3 3 0 0 1 3-3h11",
            Self::Pin => "M12 16v5M9 3h6l-1 6 3 3v2H7v-2l3-3z",
            Self::Send => "M12 19V5M6 11l6-6 6 6",
            Self::Question => "M9.1 9a3 3 0 0 1 5.8 1c0 2-3 3-3 3M12 17h.01M3 12a9 9 0 1 0 18 0a9 9 0 1 0-18 0",
            Self::Back => "M19 12H5M11 18l-6-6 6-6",
            Self::Open => "M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5",
            Self::Check => "m5 12.5 4.5 4.5L19 7.5",
            Self::Pencil => "M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z",
            Self::Copy => "M9 9h10a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H9a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2ZM5 15H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v1",
            Self::Trash => "M3 6h18M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6M10 11v6M14 11v6",
            Self::Clock => "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20ZM12 6v6l4 2",
            Self::Info => "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20ZM12 16v-4M12 8h.01",
            Self::Important => "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2ZM12 7v4M12 15h.01",
            Self::AlertTriangle => "m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3ZM12 9v4M12 17h.01",
            Self::AlertOctagon => "M7.86 2h8.28L22 7.86v8.28L16.14 22H7.86L2 16.14V7.86ZM12 8v4M12 16h.01",
        }
    }

    /// The bare `svg`, styled by `class` (its size comes from CSS).
    pub fn svg(self, class: &'static str) -> Element {
        match self {
            Self::Grip => rsx! {
                svg { class, width: "16", height: "16", view_box: "0 0 24 24", fill: "currentColor",
                    circle { cx: "9", cy: "6", r: "1.6" }
                    circle { cx: "15", cy: "6", r: "1.6" }
                    circle { cx: "9", cy: "12", r: "1.6" }
                    circle { cx: "15", cy: "12", r: "1.6" }
                    circle { cx: "9", cy: "18", r: "1.6" }
                    circle { cx: "15", cy: "18", r: "1.6" }
                }
            },
            Self::DotsHorizontal => rsx! {
                svg { class, width: "16", height: "16", view_box: "0 0 24 24", fill: "currentColor",
                    circle { cx: "6", cy: "12", r: "1.8" }
                    circle { cx: "12", cy: "12", r: "1.8" }
                    circle { cx: "18", cy: "12", r: "1.8" }
                }
            },
            Self::DotsVertical => rsx! {
                svg { class, width: "16", height: "16", view_box: "0 0 24 24", fill: "currentColor",
                    circle { cx: "12", cy: "6", r: "1.8" }
                    circle { cx: "12", cy: "12", r: "1.8" }
                    circle { cx: "12", cy: "18", r: "1.8" }
                }
            },
            Self::Sparkle => rsx! {
                svg { class, width: "16", height: "16", view_box: "0 0 24 24", fill: "currentColor", path { d: self.path() } }
            },
            _ => rsx! {
                svg { class, width: "16", height: "16", view_box: "0 0 24 24", fill: "none",
                    stroke: "currentColor", stroke_width: "1.7", stroke_linecap: "round", stroke_linejoin: "round",
                    path { d: self.path() }
                }
            },
        }
    }

    /// The editor's toolbar form: the icon in a `tool-icon` box.
    pub fn draw(self) -> Element {
        rsx! { span { class: "tool-icon", aria_hidden: "true", {self.svg("")} } }
    }
}

/// An inline icon (`svg.i`), sized by its container's CSS.
pub fn icon(i: Icon) -> Element {
    i.svg("i")
}
