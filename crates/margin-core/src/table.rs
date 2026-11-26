use pulldown_cmark::{Alignment, Event, Options, Parser, Tag};
use std::ops::Range;

use crate::edit::Splice;

#[derive(Clone, PartialEq, Debug)]
pub struct Cell {
    pub slot: Range<usize>,
    pub text: Range<usize>,
}
#[derive(Clone, PartialEq, Debug)]
pub struct Row {
    pub start: usize,
    pub cells: Vec<Cell>,
    pub end: usize,
    pub trailing: bool,
}
#[derive(Clone, PartialEq, Debug)]
pub struct Table {
    pub range: Range<usize>,
    pub rows: Vec<Row>,
    pub delimiter: Row,
    pub align: Vec<Alignment>,
}

/// A rectangular selection uses visible cell coordinates, including the header at row zero.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CellRect {
    pub anchor: [usize; 2],
    pub head: [usize; 2],
}

impl CellRect {
    pub fn rows(self) -> Range<usize> {
        self.anchor[0].min(self.head[0])..self.anchor[0].max(self.head[0]) + 1
    }
    pub fn cols(self) -> Range<usize> {
        self.anchor[1].min(self.head[1])..self.anchor[1].max(self.head[1]) + 1
    }
    pub fn count(self) -> usize {
        self.rows().len() * self.cols().len()
    }
    pub fn step(self, row: usize, col: usize, backwards: bool) -> [usize; 2] {
        let rows = self.rows();
        let cols = self.cols();
        let flat = (row - rows.start) * cols.len() + col - cols.start;
        let next = (flat + if backwards { self.count() - 1 } else { 1 }) % self.count();
        [
            rows.start + next / cols.len(),
            cols.start + next % cols.len(),
        ]
    }
}

fn read_row(md: &str, range: Range<usize>) -> Row {
    let line = &md[range.clone()];
    let pipes: Vec<_> = line
        .match_indices('|')
        .filter(|(i, _)| *i == 0 || line.as_bytes()[i - 1] != b'\\')
        .map(|(i, _)| range.start + i)
        .collect();
    let mut separators = pipes.as_slice();
    let mut start = range.start;
    let mut end = range.end;
    if let Some((&pipe, rest)) = separators.split_first()
        && md[start..pipe].trim().is_empty()
    {
        start = pipe + 1;
        separators = rest;
    }
    let trailing = pipes
        .last()
        .is_some_and(|p| md[*p + 1..end].trim().is_empty());
    if let Some((&pipe, rest)) = separators.split_last()
        && trailing
    {
        end = pipe;
        separators = rest;
    }
    let mut cells = Vec::new();
    for right in separators.iter().copied().chain([end]) {
        let slot = start..right;
        // Reserve one ordinary padding space per edge. Extra spaces belong to the
        // editable value, including a space just typed at the end of a word.
        let raw = &md[slot.clone()];
        let l = start + usize::from(raw.starts_with([' ', '\t']));
        let r = right
            .saturating_sub(usize::from(raw.ends_with([' ', '\t'])))
            .max(l);
        cells.push(Cell { slot, text: l..r });
        start = right + 1;
    }
    Row {
        start: range.start,
        cells,
        end: range.end,
        trailing,
    }
}

pub fn tables(md: &str) -> Vec<Table> {
    let mut result = Vec::new();
    let mut depth = 0;
    for (event, span) in Parser::new_ext(md, Options::ENABLE_TABLES).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if depth == 0
                    && let Tag::Table(align) = tag
                {
                    let start = md[..span.start].rfind('\n').map_or(0, |i| i + 1);
                    let end = start + md[start..span.end].trim_end_matches(['\r', '\n']).len();
                    let mut at = start;
                    let mut lines = md[start..end].split('\n').map(|line| {
                        let row = read_row(md, at..at + line.trim_end_matches('\r').len());
                        at += line.len() + 1;
                        row
                    });
                    if let (Some(header), Some(delimiter)) = (lines.next(), lines.next()) {
                        let rows = std::iter::once(header).chain(lines).collect();
                        result.push(Table {
                            range: start..end,
                            rows,
                            delimiter,
                            align,
                        });
                    }
                }
                depth += 1;
            }
            Event::End(_) => depth -= 1,
            _ => {}
        }
    }
    result
}

pub fn delimiter_line(md: &str, table_start: usize) -> std::ops::RangeInclusive<usize> {
    let start = md
        .get(table_start..)
        .and_then(|s| s.find('\n'))
        .map_or(md.len(), |n| table_start + n + 1);
    let end = start + md[start..].find('\n').unwrap_or(md.len() - start);
    start..=end
}

pub fn eol(md: &str) -> &'static str {
    if md.contains("\r\n") { "\r\n" } else { "\n" }
}

pub fn decoded(raw: &str) -> String {
    raw.replace("\\|", "|")
}
pub fn encoded(value: &str) -> String {
    value.replace('|', "\\|")
}

/// A value as it can sit in one cell: on one line, pipes escaped.
pub fn cell_value(value: &str) -> String {
    encoded(&value.replace(['\r', '\n'], " "))
}

pub fn cell_to_source(raw: &str, byte: usize) -> usize {
    let mut display = 0;
    let mut at = 0;
    while at < raw.len() && display < byte {
        if raw[at..].starts_with("\\|") {
            at += 1;
        }
        let len = raw[at..].chars().next().unwrap().len_utf8();
        at += len;
        display += len;
    }
    at
}

impl Table {
    /// A cell's text, unescaped and trimmed.
    pub fn cell_text(&self, md: &str, row: usize, col: usize) -> Option<String> {
        let cell = self.rows.get(row)?.cells.get(col)?;
        Some(decoded(&md[cell.text.clone()]).trim().to_string())
    }

    /// Every row's cell texts; the header first.
    pub fn texts(&self, md: &str) -> Vec<Vec<String>> {
        (0..self.rows.len())
            .map(|r| (0..self.rows[r].cells.len()).filter_map(|c| self.cell_text(md, r, c)).collect())
            .collect()
    }

    pub fn width(&self) -> usize {
        self.align.len()
    }

    pub fn cell(&self, row: usize, col: usize) -> Range<usize> {
        let line = &self.rows[row];
        line.cells
            .get(col)
            .map_or(line.end..line.end, |c| c.text.clone())
    }

    pub fn position(&self, pos: usize) -> (usize, usize) {
        let row = self
            .rows
            .partition_point(|r| r.end < pos)
            .min(self.rows.len() - 1);
        let row = if pos <= self.delimiter.end { 0 } else { row };
        let col = self.rows[row]
            .cells
            .partition_point(|c| c.slot.end < pos)
            .min(self.width() - 1);
        (row, col)
    }

    pub fn edit_cell(&self, md: &str, row: usize, col: usize, value: &str) -> Splice {
        let line = &self.rows[row];
        let range = self.cell(row, col);
        if col >= line.cells.len() {
            let edge = if line.trailing { "" } else { " |" };
            let missing = "  |".repeat(col - line.cells.len());
            return Splice::new(range, format!("{edge}{missing} {} |", encoded(value)));
        }
        let slot = line.cells[col].slot.clone();
        let mut left = &md[slot.start..range.start];
        let mut right = &md[range.end..slot.end];
        if left.is_empty() && value.starts_with([' ', '\t']) {
            left = " ";
        }
        if right.is_empty() && value.ends_with([' ', '\t', '\\']) {
            right = " ";
        }
        let text = format!("{left}{}{right}", encoded(value));
        Splice::diff(&md[slot.clone()], &text, slot.start)
    }

    pub fn append_row(&self, md: &str) -> Splice {
        Splice::new(
            self.range.end..self.range.end,
            format!("{}|{}", eol(md), "  |".repeat(self.width())),
        )
    }

    /// Insert an empty data row at `row` (1..=rows.len()).
    pub fn insert_row(&self, md: &str, row: usize) -> Splice {
        let at = if row <= 1 {
            self.delimiter.end
        } else {
            self.rows[(row - 1).min(self.rows.len() - 1)].end
        };
        Splice::new(
            at..at,
            format!("{}|{}", eol(md), "  |".repeat(self.width())),
        )
    }

    /// Insert a new column at `col` (0..=width()).
    pub fn insert_column(&self, md: &str, col: usize) -> Vec<Splice> {
        if col >= self.width() {
            return self.append_column();
        }
        let mut edits = Vec::new();
        for (i, line) in std::iter::once(&self.rows[0])
            .chain([&self.delimiter])
            .chain(&self.rows[1..])
            .enumerate()
        {
            let value = match i {
                0 => format!("Column {}", self.width() + 1),
                1 => "---".to_owned(),
                _ => String::new(),
            };
            if let Some(cell) = line.cells.get(col) {
                let at = cell.slot.start;
                if col == 0 && at == line.start {
                    edits.push(Splice::new(at..at, format!("| {value} |")));
                } else {
                    edits.push(Splice::new(at..at, format!(" {value} |")));
                }
            } else {
                let at = line.end;
                let edge = if line.trailing { "" } else { " |" };
                let gap = "  |".repeat(col.saturating_sub(line.cells.len()));
                let text = if value.is_empty() {
                    format!("{edge}{gap}  |")
                } else {
                    format!("{edge}{gap} {value} |")
                };
                edits.push(Splice::new(at..at, text));
            }
        }
        let _ = md;
        edits
    }

    pub fn set_alignment(&self, md: &str, cols: Range<usize>, align: Alignment) -> Vec<Splice> {
        let marker = match align {
            Alignment::Left => ":---",
            Alignment::Center => ":---:",
            Alignment::Right => "---:",
            Alignment::None => "---",
        };
        let mut edits = Vec::new();
        for col in cols.start..cols.end.min(self.width()) {
            if let Some(cell) = self.delimiter.cells.get(col) {
                let raw = &md[cell.slot.clone()];
                let left = if raw.starts_with([' ', '\t']) { " " } else { "" };
                let right = if raw.ends_with([' ', '\t']) { " " } else { "" };
                let next = format!("{left}{marker}{right}");
                let edit = Splice::diff(raw, &next, cell.slot.start);
                if !edit.range.is_empty() || !edit.text.is_empty() {
                    edits.push(edit);
                }
            }
        }
        edits
    }

    pub fn move_row(&self, md: &str, row: usize, down: bool) -> Vec<Splice> {
        let other = if down {
            if row == 0 || row + 1 >= self.rows.len() {
                return Vec::new();
            }
            row + 1
        } else {
            if row <= 1 || row >= self.rows.len() {
                return Vec::new();
            }
            row - 1
        };
        let (first, second) = if row < other {
            (&self.rows[row], &self.rows[other])
        } else {
            (&self.rows[other], &self.rows[row])
        };
        vec![
            Splice::new(first.start..first.end, &md[second.start..second.end]),
            Splice::new(second.start..second.end, &md[first.start..first.end]),
        ]
    }

    pub fn move_column(&self, md: &str, col: usize, right: bool) -> Vec<Splice> {
        let other = if right {
            if col + 1 >= self.width() {
                return Vec::new();
            }
            col + 1
        } else {
            if col == 0 || col >= self.width() {
                return Vec::new();
            }
            col - 1
        };
        let (lo, hi) = (col.min(other), col.max(other));
        let mut edits = Vec::new();
        for (i, line) in std::iter::once(&self.rows[0])
            .chain([&self.delimiter])
            .chain(&self.rows[1..])
            .enumerate()
        {
            if let (Some(a), Some(b)) = (line.cells.get(lo), line.cells.get(hi)) {
                let va = &md[a.text.clone()];
                let vb = &md[b.text.clone()];
                if va != vb {
                    edits.push(Splice::new(a.text.clone(), vb));
                    edits.push(Splice::new(b.text.clone(), va));
                }
            } else if i > 1 && line.cells.get(lo).is_some() {
                // Short data row has `lo` but implicit empty `hi`: pad and move value.
                let a = &line.cells[lo];
                let va = &md[a.text.clone()];
                if !va.is_empty() {
                    let at = line.end;
                    let edge = if line.trailing { "" } else { " |" };
                    let gap = "  |".repeat(hi - line.cells.len());
                    edits.push(Splice::new(a.text.clone(), ""));
                    edits.push(Splice::new(at..at, format!("{edge}{gap} {va} |")));
                }
            }
        }
        edits.sort_by_key(|e| (e.range.start, e.range.end));
        edits
    }

    /// Add columns at the Markdown boundary, before any ignored overflow cells.
    fn extend_line(&self, row: &Row, values: &[String]) -> Splice {
        let tail = values.join(" | ");
        if let Some(cell) = row.cells.get(self.width() - 1) {
            let at = cell.slot.end;
            Splice::new(at..at, format!("| {tail} "))
        } else {
            let at = row.end;
            let edge = if row.trailing { "" } else { " |" };
            let gap = "  |".repeat(self.width() - row.cells.len());
            Splice::new(at..at, format!("{edge}{gap} {tail} |"))
        }
    }

    pub fn append_column(&self) -> Vec<Splice> {
        std::iter::once(&self.rows[0])
            .chain([&self.delimiter])
            .chain(&self.rows[1..])
            .enumerate()
            .map(|(i, row)| {
                let value = match i {
                    0 => format!("Column {}", self.width() + 1),
                    1 => "---".to_owned(),
                    _ => String::new(),
                };
                self.extend_line(row, &[value])
            })
            .collect()
    }

    /// Remove only this column's source from each physical line, including
    /// the alignment row. Surviving cells retain their Loro character IDs.
    pub fn delete_column(&self, md: &str, col: usize) -> Vec<Splice> {
        self.delete_columns(md, col..col.saturating_add(1))
    }

    pub fn delete_columns(&self, md: &str, cols: Range<usize>) -> Vec<Splice> {
        let mut edits = Vec::new();
        if cols.is_empty() || cols.end > self.width() || cols.len() == self.width() {
            return edits;
        }
        for line in std::iter::once(&self.rows[0])
            .chain([&self.delimiter])
            .chain(&self.rows[1..])
        {
            let first = cols.start;
            let end = cols.end.min(line.cells.len());
            if first >= end {
                continue;
            }
            let survivors = line.cells.len() - (end - first);
            if survivors == 0 {
                edits.push(Splice::diff(&md[line.start..line.end], "|  |", line.start));
            } else if survivors == 1 && line.cells[0].slot.start == line.start && !line.trailing {
                let keep = &line.cells[if first == 0 { end } else { 0 }].slot;
                let text = &md[keep.clone()];
                let left = if text.starts_with([' ', '\t']) {
                    "|"
                } else {
                    "| "
                };
                let right = if text.ends_with([' ', '\t']) {
                    "|"
                } else {
                    " |"
                };
                edits.push(Splice::new(line.start..keep.start, left));
                edits.push(Splice::new(keep.end..line.end, right));
            } else {
                let range = if let Some(next) = line.cells.get(end) {
                    line.cells[first].slot.start..next.slot.start
                } else {
                    line.cells[first - 1].slot.end..line.cells[end - 1].slot.end
                };
                edits.push(Splice::new(range, ""));
            }
        }
        edits
    }

    pub fn delete_rows(&self, rows: Range<usize>) -> Option<Splice> {
        if rows.start == 0 || rows.is_empty() || rows.end > self.rows.len() {
            return None;
        }
        let previous = if rows.start == 1 {
            &self.delimiter
        } else {
            &self.rows[rows.start - 1]
        };
        Some(Splice::new(previous.end..self.rows[rows.end - 1].end, ""))
    }

    pub fn copy_rect(&self, md: &str, rect: CellRect) -> String {
        rect.rows()
            .map(|r| {
                rect.cols()
                    .map(|c| decoded(&md[self.cell(r, c)]))
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Constrain a paste to the selection. A single clipboard value fills it.
    pub fn paste_rect(&self, md: &str, rect: CellRect, value: &str) -> Vec<Splice> {
        let rows = rect.rows();
        let cols = rect.cols();
        if value.is_empty() {
            return rows
                .flat_map(|r| cols.clone().map(move |c| self.cell(r, c)))
                .filter(|range| !range.is_empty())
                .map(|range| Splice::new(range, ""))
                .collect();
        }
        let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
        let text = normalized.strip_suffix('\n').unwrap_or(&normalized);
        let matrix = if !text.contains(['\n', '\t']) {
            vec![vec![text; cols.len()]; rows.len()]
        } else {
            text.split('\n')
                .take(rows.len())
                .map(|line| line.split('\t').take(cols.len()).collect())
                .collect()
        };
        self.paste_matrix(md, rows.start, cols.start, &matrix)
    }

    /// Preserve existing CRDT characters between edited cells, even when the
    /// clipboard rectangle crosses untouched Markdown padding and other columns.
    pub fn paste(&self, md: &str, row: usize, col: usize, text: &str) -> Vec<Splice> {
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let matrix: Vec<Vec<&str>> = normalized
            .strip_suffix('\n')
            .unwrap_or(&normalized)
            .split('\n')
            .map(|r| r.split('\t').collect())
            .collect();
        self.paste_matrix(md, row, col, &matrix)
    }

    fn paste_matrix(&self, md: &str, row: usize, col: usize, matrix: &[Vec<&str>]) -> Vec<Splice> {
        let width = self
            .width()
            .max(col + matrix.iter().map(Vec::len).max().unwrap_or(0));
        let value = |r: usize, c: usize| {
            matrix
                .get(r.checked_sub(row)?)?
                .get(c.checked_sub(col)?)
                .copied()
        };
        let mut edits = Vec::new();
        for (r, line) in self.rows.iter().enumerate() {
            for c in 0..line.cells.len().min(self.width()) {
                if let Some(v) = value(r, c) {
                    let edit = self.edit_cell(md, r, c, v);
                    if !edit.range.is_empty() || !edit.text.is_empty() {
                        edits.push(edit);
                    }
                }
            }
            // Include implicit cells only as far as the paste actually reaches.
            let existing = line.cells.len().min(self.width());
            let filled = r
                .checked_sub(row)
                .and_then(|i| matrix.get(i))
                .map_or(0, |v| col + v.len());
            let end = if width > self.width() { width } else { filled };
            if existing < end {
                let mut new = Vec::new();
                for c in existing..end {
                    new.push(value(r, c).map(encoded).unwrap_or_else(|| {
                        if r == 0 && c >= self.width() {
                            format!("Column {}", c + 1)
                        } else {
                            String::new()
                        }
                    }));
                }
                if existing == self.width() {
                    edits.push(self.extend_line(line, &new));
                } else {
                    let at = line.end;
                    let edge = if line.trailing { "" } else { " |" };
                    edits.push(Splice::new(at..at, format!("{edge} {} |", new.join(" | "))));
                }
            }
        }
        if width > self.width() {
            edits.push(self.extend_line(
                &self.delimiter,
                &vec!["---".to_owned(); width - self.width()],
            ));
        }
        let mut rows = String::new();
        for r in self.rows.len()..row + matrix.len() {
            rows.push_str(eol(md));
            rows.push('|');
            for c in 0..width {
                rows.push(' ');
                rows.push_str(&value(r, c).map(encoded).unwrap_or_default());
                rows.push_str(" |");
            }
        }
        if !rows.is_empty() {
            edits.push(Splice::new(self.range.end..self.range.end, rows));
        }
        edits.sort_by_key(|e| (e.range.start, e.range.end));
        edits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remove(md: &str, col: usize) -> String {
        let grid = tables(md).remove(0);
        let mut out = md.to_owned();
        let changes = grid.delete_column(md, col);
        assert!(
            changes
                .windows(2)
                .all(|e| e[0].range.end <= e[1].range.start)
        );
        for edit in changes.into_iter().rev() {
            out.replace_range(edit.range, &edit.text);
        }
        out
    }

    #[test]
    fn remove_column_keeps_other_cells_line_endings_and_short_rows() {
        let note = "## Table\r\n\r\n| Stop | Time | Notes |\r\n| :--- | ---: | :---: |\r\n| A\\|B | 3 | **🌿** |\r\n| Short |\r\n\r\nAfter";
        let next = remove(note, 1);
        assert_eq!(
            next,
            "## Table\r\n\r\n| Stop | Notes |\r\n| :--- | :---: |\r\n| A\\|B | **🌿** |\r\n| Short |\r\n\r\nAfter"
        );
        let last = remove(&next, 0);
        assert_eq!(
            last,
            "## Table\r\n\r\n| Notes |\r\n| :---: |\r\n| **🌿** |\r\n|  |\r\n\r\nAfter"
        );
        assert_eq!(tables(&last)[0].rows.len(), 3);
        assert_eq!(remove(&last, 0), last);
    }

    #[test]
    fn remove_column_from_a_borderless_pair_preserves_the_surviving_characters() {
        let note = "Place | Time\n:--- | ---:\nA\\| | Noon";
        let grid = tables(note).remove(0);
        for col in [0, 1] {
            let changes = grid.delete_column(note, col);
            for r in 0..grid.rows.len() {
                let keep = grid.cell(r, 1 - col);
                assert!(
                    changes
                        .iter()
                        .all(|e| e.range.end <= keep.start || e.range.start >= keep.end)
                );
            }
            assert_eq!(tables(&remove(note, col))[0].width(), 1);
        }
        assert_eq!(remove(note, 1), "| Place |\n| :--- |\n| A\\| |");
        assert_eq!(remove(note, 0), "| Time |\n| ---: |\n| Noon |");
    }

    fn apply(md: &str, changes: Vec<Splice>) -> String {
        let mut out = md.to_owned();
        for edit in changes.into_iter().rev() {
            out.replace_range(edit.range, &edit.text);
        }
        out
    }

    #[test]
    fn insert_move_and_align_table_rows_and_columns() {
        let md = "| A | B |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |";
        let g = tables(md).remove(0);
        let with_row = apply(md, vec![g.insert_row(md, 1)]);
        assert_eq!(
            with_row,
            "| A | B |\n| --- | --- |\n|  |  |\n| 1 | 2 |\n| 3 | 4 |"
        );
        let with_col = apply(md, g.insert_column(md, 1));
        assert_eq!(
            with_col,
            "| A | Column 3 | B |\n| --- | --- | --- |\n| 1 |  | 2 |\n| 3 |  | 4 |"
        );
        let aligned = apply(md, g.set_alignment(md, 1..2, Alignment::Right));
        assert_eq!(aligned, "| A | B |\n| --- | ---: |\n| 1 | 2 |\n| 3 | 4 |");
        let moved_row = apply(md, g.move_row(md, 1, true));
        assert_eq!(
            moved_row,
            "| A | B |\n| --- | --- |\n| 3 | 4 |\n| 1 | 2 |"
        );
        let moved_col = apply(md, g.move_column(md, 0, true));
        assert_eq!(
            moved_col,
            "| B | A |\n| --- | --- |\n| 2 | 1 |\n| 4 | 3 |"
        );
    }
}
