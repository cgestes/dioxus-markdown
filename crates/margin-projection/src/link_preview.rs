//! Pure URL classification, Markdown summary extraction, anchor resolution,
//! and link formatting helpers for the link popover card.
use crate::document::Sections;
use crate::projection::ParsedLink;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachedPage {
    pub title: Option<String>,
    pub excerpt: Option<String>,
    pub found: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenTarget {
    ExternalUrl(String),
    HeadingOffset(usize),
    /// A page on the server the note is kept on.
    Page(String),
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkPreview {
    pub is_internal: bool,
    pub display_target: String,
    pub title: String,
    pub folder_or_host: Option<String>,
    pub excerpt: Option<String>,
    pub meta_left: Option<String>,
    pub meta_right: Option<String>,
    pub open_target: OpenTarget,
}

pub fn is_external_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("mailto:")
        || lower.starts_with("tel:")
        || lower.starts_with("ftp://")
        || lower.starts_with("www.")
}

fn normalized_external_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.to_ascii_lowercase().starts_with("www.") {
        format!("https://{trimmed}")
    } else {
        trimmed.to_owned()
    }
}

fn normalize_internal_path(url: &str) -> String {
    let base = url.trim().split('#').next().unwrap_or("").trim();
    let base = base.trim_start_matches("./").trim_start_matches('/');
    base.strip_suffix(".md").unwrap_or(base).to_owned()
}

pub fn page_path(url: &str) -> Option<String> {
    let norm = normalize_internal_path(url);
    if norm.is_empty() || norm.contains("..") {
        return None;
    }
    Some(format!("{norm}.md"))
}

pub fn page_cache_key(page_root: &str, path: &str) -> String {
    format!("{page_root}\0{path}")
}

fn display_target(url: &str, is_internal: bool) -> String {
    let trimmed = url.trim();
    if !is_internal {
        let stripped = trimmed
            .strip_prefix("https://")
            .or_else(|| trimmed.strip_prefix("http://"))
            .or_else(|| trimmed.strip_prefix("mailto:"))
            .unwrap_or(trimmed);
        return stripped.trim_end_matches('/').to_owned();
    }
    if trimmed.starts_with('#') {
        return trimmed.to_owned();
    }
    let clean = normalize_internal_path(trimmed);
    match (clean.is_empty(), trimmed.split_once('#')) {
        (true, Some((_, frag))) => format!("#{frag}"),
        (false, Some((_, frag))) => format!("{clean}#{frag}"),
        (true, None) => trimmed.to_owned(),
        (false, None) => clean,
    }
}

fn extract_folder(target: &str) -> Option<String> {
    let base = target.split('#').next().unwrap_or(target);
    let (dir, _) = base.rsplit_once('/')?;
    let dir = dir.trim_matches('/');
    (!dir.is_empty()).then(|| dir.to_owned())
}

fn extract_host(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if let Some(rest) = trimmed.strip_prefix("mailto:") {
        let addr = rest.split('?').next().unwrap_or(rest).trim();
        return (!addr.is_empty()).then(|| addr.to_owned());
    }
    let after_scheme = trimmed
        .split_once("://")
        .map_or(trimmed, |(_, rest)| rest);
    let authority = after_scheme.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host).trim();
    (!host.is_empty()).then(|| host.to_owned())
}

fn prettify_stem(target: &str) -> String {
    let base = target.split('#').next().unwrap_or(target);
    let stem = base.rsplit('/').next().unwrap_or(base);
    let stem = stem.strip_suffix(".md").unwrap_or(stem);
    let words: Vec<String> = stem
        .split(['-', '_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();
    if words.is_empty() {
        target.to_owned()
    } else {
        words.join(" ")
    }
}

fn slugify(text: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in text.trim().chars() {
        if ch.is_alphanumeric() {
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
            prev_dash = false;
        } else if (ch.is_whitespace() || ch == '-' || ch == '_') && !out.is_empty() && !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_end_matches('-').to_owned()
}

fn strip_inline_markdown(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            if let Some(close_bracket) = line[i + 1..].find(']') {
                let label_end = i + 1 + close_bracket;
                if line[label_end + 1..].starts_with('(')
                    && let Some(close_paren) = line[label_end + 2..].find(')')
                {
                    out.push_str(&line[i + 1..label_end]);
                    i = label_end + 2 + close_paren + 1;
                    continue;
                }
            }
        }
        let ch = line[i..].chars().next().unwrap();
        if !matches!(ch, '*' | '_' | '`' | '~') {
            out.push(ch);
        }
        i += ch.len_utf8();
    }
    out
}

fn truncate_excerpt(text: &str, max_chars: usize) -> Option<String> {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    let count = collapsed.chars().count();
    if count <= max_chars {
        return Some(collapsed);
    }
    let mut cut = collapsed
        .char_indices()
        .nth(max_chars)
        .map_or(collapsed.len(), |(idx, _)| idx);
    if let Some(space) = collapsed[..cut].rfind(' ')
        && space > cut / 2
    {
        cut = space;
    }
    Some(format!("{}…", collapsed[..cut].trim_end_matches([',', ';', ':'])))
}

pub fn extract_markdown_summary(md: &str) -> (Option<String>, Option<String>) {
    let mut lines = md.lines().peekable();
    // Skip optional YAML frontmatter.
    if lines.peek().is_some_and(|l| l.trim() == "---") {
        lines.next();
        for l in lines.by_ref() {
            if l.trim() == "---" || l.trim() == "..." {
                break;
            }
        }
    }
    let mut title = None;
    let mut body_parts = Vec::new();
    let mut body_len = 0usize;
    for raw in lines {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            if !body_parts.is_empty() {
                break;
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('#') {
            let heading = rest.trim_start_matches('#').trim();
            if title.is_none() && !heading.is_empty() {
                title = Some(strip_inline_markdown(heading));
            }
            continue;
        }
        if trimmed.starts_with("```")
            || trimmed.starts_with("~~~")
            || trimmed.starts_with('|')
            || trimmed.chars().all(|c| matches!(c, '-' | '*' | '_' | ' '))
        {
            continue;
        }
        let clean = trimmed
            .trim_start_matches("> ")
            .trim_start_matches("- [ ] ")
            .trim_start_matches("- [x] ")
            .trim_start_matches("- [X] ")
            .trim_start_matches("- ")
            .trim_start_matches("* ");
        let plain = strip_inline_markdown(clean);
        if !plain.is_empty() {
            body_len += plain.len();
            body_parts.push(plain);
            if body_len > 200 {
                break;
            }
        }
    }
    let excerpt = truncate_excerpt(&body_parts.join(" "), 168);
    (title, excerpt)
}

fn resolve_heading_anchor(md: &str, fragment: &str) -> Option<(usize, String, Option<String>)> {
    let want = slugify(fragment.trim_start_matches('#'));
    if want.is_empty() {
        return None;
    }
    let mut offset = 0;
    let mut matched = None;
    for raw_line in md.split_inclusive('\n') {
        let line = raw_line.trim_end_matches(['\r', '\n']);
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            let hashes = trimmed.bytes().take_while(|b| *b == b'#').count();
            if (1..=6).contains(&hashes) {
                let text = trimmed[hashes..].trim();
                if !text.is_empty() && slugify(text) == want {
                    matched = Some((offset, strip_inline_markdown(text), offset + raw_line.len()));
                    break;
                }
            }
        }
        offset += raw_line.len();
    }
    let (start, heading, after) = matched?;
    let (_, excerpt) = extract_markdown_summary(&md[after.min(md.len())..]);
    Some((start, heading, excerpt))
}

fn count_doc_backlinks(links: &[ParsedLink], target_url: &str) -> usize {
    let norm = normalize_internal_path(target_url);
    let frag = target_url.trim().starts_with('#');
    let count = links
        .iter()
        .filter(|l| {
            if frag {
                l.url.trim().eq_ignore_ascii_case(target_url.trim())
            } else {
                !is_external_url(&l.url) && normalize_internal_path(&l.url) == norm
            }
        })
        .count();
    count.max(1)
}

fn format_backlinks(count: usize) -> String {
    if count == 1 {
        "1 backlink".to_owned()
    } else {
        format!("{count} backlinks")
    }
}

fn fallback_title(raw_label: &str, stem_source: &str) -> String {
    if !raw_label.is_empty() {
        raw_label.to_owned()
    } else {
        prettify_stem(stem_source)
    }
}

fn anchor_preview(
    md: &str,
    all_links: &[ParsedLink],
    url: &str,
    raw_label: &str,
    title_attr: Option<String>,
) -> LinkPreview {
    let note_title = {
        let t = Sections::read(md).title(md).trim();
        (!t.is_empty()).then(|| t.to_owned())
    };
    let backlinks = format_backlinks(count_doc_backlinks(all_links, url));
    let (title, excerpt, meta_left, open_target) =
        if let Some((offset, heading, excerpt)) = resolve_heading_anchor(md, url) {
            (
                heading,
                excerpt.or(title_attr),
                "Section in this note",
                OpenTarget::HeadingOffset(offset),
            )
        } else {
            (
                fallback_title(raw_label, url.trim_start_matches('#')),
                title_attr,
                "Anchor link",
                OpenTarget::None,
            )
        };
    LinkPreview {
        is_internal: true,
        display_target: url.to_owned(),
        title,
        folder_or_host: note_title,
        excerpt,
        meta_left: Some(meta_left.to_owned()),
        meta_right: Some(backlinks),
        open_target,
    }
}

fn external_preview(url: &str, raw_label: &str, title_attr: Option<String>) -> LinkPreview {
    let host = extract_host(url);
    let disp = display_target(url, false);
    let title = if !raw_label.is_empty()
        && !raw_label.eq_ignore_ascii_case(url)
        && !raw_label.eq_ignore_ascii_case(&disp)
    {
        raw_label.to_owned()
    } else {
        host.clone().unwrap_or_else(|| disp.clone())
    };
    let excerpt =
        title_attr.or_else(|| (disp != title).then(|| truncate_excerpt(url, 168)).flatten());
    LinkPreview {
        is_internal: false,
        display_target: disp,
        title,
        folder_or_host: host,
        excerpt,
        meta_left: Some("External link".to_owned()),
        meta_right: Some("Open in browser".to_owned()),
        open_target: OpenTarget::ExternalUrl(normalized_external_url(url)),
    }
}

fn internal_preview(
    all_links: &[ParsedLink],
    url: &str,
    raw_label: &str,
    title_attr: Option<String>,
    page_root: Option<&str>,
    page_cache: &HashMap<String, CachedPage>,
) -> LinkPreview {
    let disp = display_target(url, true);
    let folder = extract_folder(&disp);
    let backlinks = format_backlinks(count_doc_backlinks(all_links, url));
    let linked_page = page_path(url);
    let cached = page_root
        .zip(linked_page.as_deref())
        .and_then(|(root, path)| page_cache.get(&page_cache_key(root, path)));

    let (title, excerpt, meta_left) = match cached {
        Some(page) if page.found => (
            page.title
                .clone()
                .unwrap_or_else(|| fallback_title(raw_label, &disp)),
            page.excerpt.clone().or(title_attr),
            "Space page",
        ),
        Some(_) => (fallback_title(raw_label, &disp), title_attr, "Uncreated page"),
        None => (
            fallback_title(raw_label, &disp),
            title_attr,
            if page_root.is_some() {
                "Space page"
            } else {
                "Linked page"
            },
        ),
    };

    let open_target = match linked_page {
        Some(path) if page_root.is_some() => OpenTarget::Page(path),
        _ => OpenTarget::None,
    };

    LinkPreview {
        is_internal: true,
        display_target: disp,
        title,
        folder_or_host: folder,
        excerpt,
        meta_left: Some(meta_left.to_owned()),
        meta_right: Some(backlinks),
        open_target,
    }
}

pub fn build_preview(
    md: &str,
    all_links: &[ParsedLink],
    link: &ParsedLink,
    page_root: Option<&str>,
    page_cache: &HashMap<String, CachedPage>,
) -> LinkPreview {
    let raw_label = md
        .get(link.label_range.clone())
        .map(str::trim)
        .unwrap_or("");
    let url = link.url.trim();
    let title_attr = truncate_excerpt(&link.title, 168);

    if url.starts_with('#') {
        anchor_preview(md, all_links, url, raw_label, title_attr)
    } else if is_external_url(url) {
        external_preview(url, raw_label, title_attr)
    } else {
        internal_preview(all_links, url, raw_label, title_attr, page_root, page_cache)
    }
}

pub fn format_markdown_link(
    label: &str,
    url: &str,
    title: &str,
    was_autolink: bool,
) -> String {
    let url = url.trim();
    let label = label.trim();
    if url.is_empty() {
        return label.to_owned();
    }
    if was_autolink && (label.is_empty() || label == url) && title.is_empty() {
        return format!("<{url}>");
    }
    let text = if label.is_empty() { url } else { label };
    if title.is_empty() {
        format!("[{text}]({url})")
    } else {
        let escaped = title.replace('"', "\\\"");
        format!("[{text}]({url} \"{escaped}\")")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_targets_and_blocks_parent_traversal() {
        assert_eq!(display_target("https://example.com/guide/", false), "example.com/guide");
        assert_eq!(display_target("./brain/log.md#today", true), "brain/log#today");
        assert_eq!(display_target("#listening-stops", true), "#listening-stops");
        assert_eq!(page_path("./brain/log.md#top"), Some("brain/log.md".into()));
        assert_eq!(page_path("../secrets/keys"), None);
        assert_eq!(page_path("#only-anchor"), None);
    }

    #[test]
    fn extracts_summary_past_frontmatter_and_resolves_headings() {
        let md = "---\ntags: [audio]\n---\n# Field *Log*\n\nFirst **entry** with [link](url).\n\n## Listening stops\n\nMorning fog.\n";
        let (title, excerpt) = extract_markdown_summary(md);
        assert_eq!(title.as_deref(), Some("Field Log"));
        assert_eq!(excerpt.as_deref(), Some("First entry with link."));

        let (offset, heading, section_excerpt) =
            resolve_heading_anchor(md, "#listening-stops").expect("anchor");
        assert_eq!(&md[offset..offset + "## Listening stops".len()], "## Listening stops");
        assert_eq!(heading, "Listening stops");
        assert_eq!(section_excerpt.as_deref(), Some("Morning fog."));
    }

    #[test]
    fn formats_links_and_preserves_autolinks() {
        assert_eq!(
            format_markdown_link("https://example.com", "https://example.com", "", true),
            "<https://example.com>"
        );
        assert_eq!(
            format_markdown_link("Guide", "https://example.com", "A \"quoted\" note", false),
            "[Guide](https://example.com \"A \\\"quoted\\\" note\")"
        );
        assert_eq!(format_markdown_link("Keep text", "", "", false), "Keep text");
    }
}
