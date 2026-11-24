//! Top-level logical Markdown blocks for left-handle block actions (move, duplicate, delete).
use margin_editor_core::{edit::Splice, table};
use std::ops::Range;

/// Split a Markdown document into byte ranges of top-level logical blocks
/// (headings, tables, fenced code blocks, blockquotes, rules, lists, paragraphs).
pub fn split_blocks(md: &str) -> Vec<Range<usize>> {
    let tables = table::tables(md);
    let mut result: Vec<Range<usize>> = Vec::new();
    let len = md.len();
    let mut pos = 0;

    while pos < len {
        // Skip leading blank lines.
        let line_end = pos + md[pos..].find('\n').map_or(len - pos, |n| n + 1);
        let line_text = md[pos..line_end].trim_end_matches(['\r', '\n']);
        if line_text.trim().is_empty() {
            pos = line_end;
            continue;
        }

        // 1. GFM Table starting at `pos`.
        if let Some(t) = tables.iter().find(|t| t.range.start == pos) {
            result.push(t.range.clone());
            pos = t.range.end;
            if pos < len {
                let nl = if md[pos..].starts_with("\r\n") {
                    2
                } else if md[pos..].starts_with('\n') {
                    1
                } else {
                    0
                };
                pos += nl;
            }
            continue;
        }

        // 2. Fenced code block (``` or ~~~).
        let trimmed = line_text.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let fence_ch = trimmed.as_bytes()[0];
            let fence_len = trimmed.bytes().take_while(|b| *b == fence_ch).count();
            let block_start = pos;
            let mut cursor = line_end;
            let mut block_end = trim_eol(md, line_end);
            while cursor < len {
                let next_end = cursor + md[cursor..].find('\n').map_or(len - cursor, |n| n + 1);
                let next_text = md[cursor..next_end].trim_end_matches(['\r', '\n']);
                block_end = trim_eol(md, next_end);
                cursor = next_end;
                let t = next_text.trim();
                if t.len() >= fence_len && t.bytes().all(|b| b == fence_ch) {
                    break;
                }
            }
            result.push(block_start..block_end);
            pos = cursor;
            continue;
        }

        // 3. ATX Heading or Horizontal rule (single-line block).
        if is_atx_heading(line_text) || is_rule(line_text) {
            result.push(pos..trim_eol(md, line_end));
            pos = line_end;
            continue;
        }

        // 4. Paragraph / list / blockquote: collect consecutive non-blank lines
        // until a blank line, table, heading, fence, or rule.
        let block_start = pos;
        let mut block_end = trim_eol(md, line_end);
        let mut cursor = line_end;
        while cursor < len {
            if tables.iter().any(|t| t.range.start == cursor) {
                break;
            }
            let next_end = cursor + md[cursor..].find('\n').map_or(len - cursor, |n| n + 1);
            let next_text = md[cursor..next_end].trim_end_matches(['\r', '\n']);
            if next_text.trim().is_empty()
                || is_atx_heading(next_text)
                || is_rule(next_text)
                || next_text.trim_start().starts_with("```")
                || next_text.trim_start().starts_with("~~~")
            {
                break;
            }
            block_end = trim_eol(md, next_end);
            cursor = next_end;
        }
        result.push(block_start..block_end);
        pos = cursor;
    }

    result
}

fn trim_eol(md: &str, end: usize) -> usize {
    if md[..end].ends_with("\r\n") {
        end - 2
    } else if md[..end].ends_with('\n') {
        end - 1
    } else {
        end
    }
}

fn is_atx_heading(line: &str) -> bool {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return false;
    }
    let rest = &line[indent..];
    let hashes = rest.bytes().take_while(|b| *b == b'#').count();
    (1..=6).contains(&hashes)
        && (rest.len() == hashes || rest.as_bytes()[hashes].is_ascii_whitespace())
}

fn is_rule(line: &str) -> bool {
    let t = line.trim();
    if t.len() < 3 {
        return false;
    }
    let ch = t.as_bytes()[0];
    matches!(ch, b'-' | b'*' | b'_')
        && t.bytes().all(|b| b == ch || b == b' ' || b == b'\t')
        && t.bytes().filter(|b| *b == ch).count() >= 3
}

pub fn block_index_at(blocks: &[Range<usize>], pos: usize) -> Option<usize> {
    blocks
        .iter()
        .position(|r| pos >= r.start && pos <= r.end)
        .or_else(|| {
            blocks
                .iter()
                .enumerate()
                .min_by_key(|(_, r)| {
                    if pos < r.start {
                        r.start - pos
                    } else {
                        pos.saturating_sub(r.end)
                    }
                })
                .map(|(i, _)| i)
        })
}

/// Swap block `idx` with its previous (`!down`) or next (`down`) sibling.
pub fn move_block(
    md: &str,
    blocks: &[Range<usize>],
    idx: usize,
    down: bool,
) -> Option<(Vec<Splice>, usize)> {
    let target = if down {
        (idx + 1 < blocks.len()).then_some(idx + 2)?
    } else {
        idx.checked_sub(1)?
    };
    move_block_to(md, blocks, idx, target)
}

/// Move block `from` to insertion slot `to_slot` (`0..=blocks.len()`).
/// Slot `k` inserts before `blocks[k]`; slot `blocks.len()` inserts after the last block.
pub fn move_block_to(
    md: &str,
    blocks: &[Range<usize>],
    from: usize,
    to_slot: usize,
) -> Option<(Vec<Splice>, usize)> {
    if from >= blocks.len() || to_slot > blocks.len() || to_slot == from || to_slot == from + 1 {
        return None;
    }
    let lo = from.min(to_slot);
    let hi = from.max(to_slot.saturating_sub(1));
    let eol = table::eol(md);
    let blank = format!("{eol}{eol}");
    let mut texts: Vec<&str> = (lo..=hi).map(|i| &md[blocks[i].clone()]).collect();
    let seps: Vec<&str> = (lo..hi)
        .map(|i| {
            let s = &md[blocks[i].end..blocks[i + 1].start];
            if s.matches('\n').count() >= 2 {
                s
            } else {
                blank.as_str()
            }
        })
        .collect();
    let moved = texts.remove(from - lo);
    let dest = if to_slot < from { 0 } else { hi - lo };
    texts.insert(dest, moved);

    let mut replacement = String::new();
    let mut caret = blocks[lo].start;
    for (i, text) in texts.iter().enumerate() {
        if i == dest {
            caret = blocks[lo].start + replacement.len();
        }
        replacement.push_str(text);
        if let Some(sep) = seps.get(i) {
            replacement.push_str(sep);
        }
    }
    Some((
        vec![Splice::new(blocks[lo].start..blocks[hi].end, replacement)],
        caret,
    ))
}

/// Duplicate block `idx` immediately below itself separated by a blank line.
pub fn duplicate_block(
    md: &str,
    blocks: &[Range<usize>],
    idx: usize,
) -> Option<(Vec<Splice>, usize)> {
    let r = blocks.get(idx)?;
    let eol = table::eol(md);
    let text = &md[r.clone()];
    let insert = format!("{eol}{eol}{text}");
    let caret = r.end + eol.len() * 2;
    Some((vec![Splice::new(r.end..r.end, insert)], caret))
}

/// Delete block `idx` along with its redundant separating blank line.
pub fn delete_block(
    md: &str,
    blocks: &[Range<usize>],
    idx: usize,
) -> Option<(Vec<Splice>, usize)> {
    let r = blocks.get(idx)?;
    let (del_start, del_end) = if let Some(next) = blocks.get(idx + 1) {
        (r.start, next.start)
    } else if let Some(prev) = idx.checked_sub(1).and_then(|i| blocks.get(i)) {
        (prev.end, r.end)
    } else {
        (0, md.len())
    };
    let caret = del_start.min(md.len().saturating_sub(del_end - del_start));
    Some((vec![Splice::new(del_start..del_end, String::new())], caret))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(md: &str, splices: Vec<Splice>) -> String {
        let mut out = md.to_owned();
        for s in splices.into_iter().rev() {
            out.replace_range(s.range, &s.text);
        }
        out
    }

    #[test]
    fn move_duplicate_and_delete_top_level_blocks() {
        let md = "# Title\n\nFirst para\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n- [ ] Task";
        let blocks = split_blocks(md);
        assert_eq!(blocks.len(), 4);
        assert_eq!(&md[blocks[0].clone()], "# Title");
        assert_eq!(&md[blocks[1].clone()], "First para");
        assert_eq!(&md[blocks[2].clone()], "| A | B |\n| --- | --- |\n| 1 | 2 |");
        assert_eq!(&md[blocks[3].clone()], "- [ ] Task");

        // Move table up above First para.
        let (splices, _) = move_block(md, &blocks, 2, false).unwrap();
        let moved = apply(md, splices);
        assert_eq!(
            moved,
            "# Title\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\nFirst para\n\n- [ ] Task"
        );

        // Duplicate First para.
        let (splices, _) = duplicate_block(md, &blocks, 1).unwrap();
        let duped = apply(md, splices);
        assert!(duped.contains("First para\n\nFirst para"));

        // Delete table.
        let (splices, _) = delete_block(md, &blocks, 2).unwrap();
        let deleted = apply(md, splices);
        assert_eq!(deleted, "# Title\n\nFirst para\n\n- [ ] Task");

        // Drag-move task (index 3) to slot 1 (right below title, above First para).
        let (splices, caret) = move_block_to(md, &blocks, 3, 1).unwrap();
        let reordered = apply(md, splices);
        assert_eq!(
            reordered,
            "# Title\n\n- [ ] Task\n\nFirst para\n\n| A | B |\n| --- | --- |\n| 1 | 2 |"
        );
        assert_eq!(&reordered[caret..caret + 10], "- [ ] Task");

        // Drag-move First para (index 1) to end (slot 4).
        let (splices, caret) = move_block_to(md, &blocks, 1, 4).unwrap();
        let to_end = apply(md, splices);
        assert_eq!(
            to_end,
            "# Title\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n- [ ] Task\n\nFirst para"
        );
        assert_eq!(&to_end[caret..caret + 10], "First para");

        // No-op slots return None.
        assert!(move_block_to(md, &blocks, 1, 1).is_none());
        assert!(move_block_to(md, &blocks, 1, 2).is_none());
    }
}
