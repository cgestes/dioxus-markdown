//! What a paste becomes before the editor applies it.

/// Pasting a web address over a selection on one line links the selection
/// to it: `[selection](url)`. `None` when the paste goes in as text: the
/// clipboard is not one http(s) address, or the selection is empty, spans
/// lines, is itself an address or already holds a link.
pub fn link_over_selection(clipboard: &str, selected: &str) -> Option<String> {
    let url = clipboard.trim();
    if url.is_empty()
        || url.chars().any(char::is_whitespace)
        || !(url.starts_with("https://") || url.starts_with("http://"))
    {
        return None;
    }
    let sel = selected.trim();
    if sel.is_empty()
        || selected.contains('\n')
        || sel.starts_with("https://")
        || sel.starts_with("http://")
        || selected.contains("](")
    {
        return None;
    }
    Some(format!("[{selected}]({url})"))
}

#[cfg(test)]
mod tests {
    use super::link_over_selection;

    #[test]
    fn an_address_pasted_over_words_links_them() {
        assert_eq!(
            link_over_selection(" https://example.com/a \n", "the guide").as_deref(),
            Some("[the guide](https://example.com/a)")
        );
    }

    #[test]
    fn anything_else_pastes_as_text() {
        for (clipboard, selected) in [
            ("ftp://example.com", "words"),
            ("https://example.com two", "words"),
            ("https://example.com", "   "),
            ("https://example.com", "two\nlines"),
            ("https://example.com", "https://other.example"),
            ("https://example.com", "[a](b)"),
        ] {
            assert_eq!(link_over_selection(clipboard, selected), None, "{clipboard:?} over {selected:?}");
        }
    }
}
