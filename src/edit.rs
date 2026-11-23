//! Source edits use UTF-8 byte offsets in the Markdown.
use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub struct Splice {
    pub range: Range<usize>,
    pub text: String,
}

impl Splice {
    pub fn new(range: Range<usize>, text: impl Into<String>) -> Self {
        Self {
            range,
            text: text.into(),
        }
    }

    pub fn diff(before: &str, after: &str, offset: usize) -> Self {
        let (removed, inserted) = text_change(before, after);
        Self::new(offset + removed.start..offset + removed.end, inserted)
    }
}

/// Return the smallest single UTF-8 splice between two native editor values.
pub fn text_change<'a>(before: &str, after: &'a str) -> (Range<usize>, &'a str) {
    let prefix = before
        .chars()
        .zip(after.chars())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum::<usize>();
    let suffix = before[prefix..]
        .chars()
        .rev()
        .zip(after[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum::<usize>();
    (
        prefix..before.len() - suffix,
        &after[prefix..after.len() - suffix],
    )
}
