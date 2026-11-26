//! The style flags of a display token: one bit per kind of Markdown span or
//! marker, so a token inside bold link text carries `STRONG | LINK`.
use margin_core::callout::CalloutKind;
use crate::syntax::HighlightKind;

pub const STRONG: u32 = 1;
pub const ITALIC: u32 = 2;
pub const CODE: u32 = 4;
pub const LINK: u32 = 8;
pub const STRIKE: u32 = 16;
pub const HEADING: u32 = 32;
pub const QUOTE: u32 = 64;
pub const RULE: u32 = 128;
pub const PUNCTUATION: u32 = 256;
pub const BULLET: u32 = 512;
pub const QUOTE_MARK: u32 = 2048;
pub const DONE: u32 = 4096;
pub const CODE_BLOCK: u32 = 8192;
pub const CALLOUT_TITLE: u32 = 1 << 15;
pub const CALLOUT_NOTE: u32 = 1 << 16;
pub const CALLOUT_TIP: u32 = 1 << 17;
pub const CALLOUT_IMPORTANT: u32 = 1 << 18;
pub const CALLOUT_WARNING: u32 = 1 << 19;
pub const CALLOUT_CAUTION: u32 = 1 << 20;
pub const HIGHLIGHT: u32 = 1 << 21;
/// 4-bit syntax token kind stored in bits 24..28 (`0` = none, `1..=10` = `HighlightKind`).
pub const SYNTAX_SHIFT: u32 = 24;
pub const SYNTAX_MASK: u32 = 0xF << SYNTAX_SHIFT;
pub const CALLOUT_ANY: u32 =
    CALLOUT_NOTE | CALLOUT_TIP | CALLOUT_IMPORTANT | CALLOUT_WARNING | CALLOUT_CAUTION;

pub const fn syntax_style(kind: HighlightKind) -> u32 {
    (kind as u32) << SYNTAX_SHIFT
}

pub const fn syntax_kind(style: u32) -> HighlightKind {
    HighlightKind::from_u8(((style & SYNTAX_MASK) >> SYNTAX_SHIFT) as u8)
}

pub fn callout_style(kind: CalloutKind) -> u32 {
    match kind {
        CalloutKind::Note => CALLOUT_NOTE,
        CalloutKind::Tip => CALLOUT_TIP,
        CalloutKind::Important => CALLOUT_IMPORTANT,
        CalloutKind::Warning => CALLOUT_WARNING,
        CalloutKind::Caution => CALLOUT_CAUTION,
    }
}

pub fn callout_kind(style: u32) -> Option<CalloutKind> {
    CalloutKind::ALL
        .into_iter()
        .find(|&kind| style & callout_style(kind) != 0)
}

/// The inline spans (and their punctuation) that Live mode reveals around the caret.
pub const INLINE: u32 = STRONG | ITALIC | CODE | LINK | STRIKE | HIGHLIGHT | PUNCTUATION;
