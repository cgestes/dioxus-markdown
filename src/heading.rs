//! A page's first heading and the title in it.

/// The text of the first `# ` heading.
pub fn h1(md: &str) -> Option<&str> {
    md.lines().find_map(|l| l.strip_prefix("# ")).map(str::trim)
}

/// A page's title: its first heading, up to a dash or colon.
pub fn page_title(md: &str) -> Option<&str> {
    h1(md).map(|h| split_heading(h).0).filter(|t| !t.is_empty())
}

/// Split a heading at its first em dash, colon or parenthesis:
/// `Weddings — Alex & Sam (2028)` → (`Weddings`, `Alex & Sam (2028)`).
pub fn split_heading(heading: &str) -> (&str, &str) {
    let found = [" — ", " – ", " - ", ": ", " ("]
        .iter()
        .filter_map(|sep| heading.find(sep).map(|at| (at, *sep)))
        .min();
    match found {
        // A parenthesis belongs to what it encloses.
        Some((at, " (")) => (heading[..at].trim(), heading[at..].trim()),
        Some((at, sep)) => (heading[..at].trim(), heading[at + sep.len()..].trim()),
        None => (heading.trim(), ""),
    }
}
