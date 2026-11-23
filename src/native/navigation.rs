use super::document::{self, BlockKind, BodyStart};
use super::{Field, Writing};
use dioxus_native::prelude::*;

impl Writing {
    // One stop per heading/blank line/prose field or grid row, in document order.
    // Wrapped lines within a prose field come directly from its Parley layout.
    pub(super) fn stops(self) -> Vec<Field> {
        let md = self.contents.read();
        let parts = self.sections(&md);
        let blocks = self.blocks.read();
        let tables = self.tables.read();
        let mut result = vec![Field::Title];
        if parts.origin == BodyStart::AfterBlankLine {
            result.push(Field::TitleGap);
        }
        for i in 0..=blocks.len() {
            if document::gap(&md, parts.body, &blocks, i).is_some() {
                result.push(Field::Body(i));
            }
            if let Some(block) = blocks.get(i) {
                match block.kind {
                    BlockKind::Heading { .. } => result.push(Field::Heading(i)),
                    BlockKind::Table(t) => {
                        result.extend((0..tables[t].rows.len()).map(|r| Field::Cell(t, r, 0)))
                    }
                }
            }
        }
        result
    }

    pub fn grid_exit(self, t: usize, down: bool) {
        let stops = self.stops();
        let edge = if down {
            self.tables.read()[t].rows.len() - 1
        } else {
            0
        };
        let Some(i) = stops.iter().position(|f| *f == Field::Cell(t, edge, 0)) else {
            return;
        };
        let next = if down { i + 1 } else { i.saturating_sub(1) };
        if let Some(&field) = stops.get(next) {
            let bounds = self.range(field, &self.contents.read());
            self.open(field, [if down { bounds.start } else { bounds.end }; 2]);
        }
    }

    pub fn vertical(mut self, field: Field, down: bool, extend: bool) -> bool {
        let Some(handle) = self.root.peek().clone() else {
            return false;
        };
        let doc = handle.doc();
        let node = |f: Field| doc.get_node(doc.get_element_by_id(&f.id())?);
        let input = |f: Field| node(f)?.element_data()?.text_input_data();
        let Some(from) = input(field) else {
            return false;
        };
        let Some(layout) = from.editor.try_layout() else {
            return false;
        };
        let caret = from.editor.raw_selection().focus().geometry(layout, 0.0);
        let mid = ((caret.y0 + caret.y1) * 0.5) as f32;
        let line = layout
            .lines()
            .position(|l| mid < l.metrics().block_max_coord)
            .unwrap_or(layout.len().saturating_sub(1));
        let inside = if down {
            (line + 1 < layout.len()).then_some(line + 1)
        } else {
            line.checked_sub(1)
        };
        let origin_x = |f: Field| -> Option<f64> {
            let node = node(f)?;
            let rect = doc.get_client_bounding_rect(node.id)?;
            let l = node.final_layout();
            Some(rect.x + f64::from(l.padding.left + l.border.left))
        };
        let measured = origin_x(field).unwrap_or(0.0) + caret.x0 / f64::from(layout.scale())
            - if from.is_multiline {
                0.0
            } else {
                f64::from(from.scroll_offset)
            };
        let x = self.goal_x.peek().unwrap_or(measured);
        let (mut target, target_line) = if let Some(line) = inside {
            (field, Some(line))
        } else {
            let stops = self.stops();
            let row = match field {
                Field::Cell(t, r, _) => Field::Cell(t, r, 0),
                other => other,
            };
            let Some(i) = stops.iter().position(|f| *f == row) else {
                return false;
            };
            let next = if down {
                i.checked_add(1)
            } else {
                i.checked_sub(1)
            };
            let Some(next) = next.and_then(|i| stops.get(i)).copied() else {
                return true;
            };
            (next, None)
        };
        if let Field::Cell(t, r, _) = target {
            target = match field {
                Field::Cell(old, _, c) if old == t => Field::Cell(t, r, c),
                _ => self.column_at(&doc, t, r, x).unwrap_or(target),
            };
        }
        let Some(to) = input(target) else {
            return false;
        };
        let Some(layout) = to.editor.try_layout() else {
            return false;
        };
        let row = target_line.unwrap_or(if down {
            0
        } else {
            layout.len().saturating_sub(1)
        });
        let Some(line) = layout.get(row) else {
            return false;
        };
        let source = to.editor.raw_text();
        let line_range = line.text_range();
        let start = line_range.start.min(source.len());
        let end = start
            + source[start..line_range.end.min(source.len())]
                .trim_end_matches(['\r', '\n'])
                .len();
        let x_in_field = (x - origin_x(target).unwrap_or(0.0)) as f32
            + if to.is_multiline {
                0.0
            } else {
                to.scroll_offset
            };
        let y = (line.metrics().block_min_coord + line.metrics().block_max_coord) * 0.5;
        let cursor = parley::Cursor::from_point(layout, x_in_field * layout.scale(), y);
        let mut pos = cursor.index().clamp(start, end);
        if pos == end && end == line_range.end && end < source.len() && start < end {
            // A downstream caret at a soft wrap belongs to the next visual line.
            if let Some(cluster) = parley::Cluster::from_byte_index(layout, end - 1) {
                pos = cluster.text_range().start;
            }
        }
        drop(doc);
        self.goal_x.set(Some(x));
        let proj = self.projection(target);
        let abs = proj.display_to_source(pos);
        let anchor = if extend {
            self.absolute_selection().map_or(abs, |s| s[0])
        } else {
            abs
        };
        self.open(target, [anchor, abs]);
        true
    }
    pub(super) fn neighbor(self, field: Field, forward: bool) -> Option<Field> {
        let stops = self.stops();
        let mut row = field;
        if let Field::Cell(t, r, c) = field {
            let width = self.tables.read()[t].width();
            if forward && c + 1 < width {
                return Some(Field::Cell(t, r, c + 1));
            }
            if !forward && c > 0 {
                return Some(Field::Cell(t, r, c - 1));
            }
            row = Field::Cell(t, r, 0);
        }
        let i = stops.iter().position(|f| *f == row)?;
        let next = *stops.get(if forward { i + 1 } else { i.checked_sub(1)? })?;
        if let Field::Cell(t, r, _) = next
            && !forward
        {
            return Some(Field::Cell(t, r, self.tables.read()[t].width() - 1));
        }
        Some(next)
    }
    pub(super) fn horizontal(
        self,
        field: Field,
        [anchor, head]: super::ByteSelection,
        right: bool,
        extend: bool,
    ) -> bool {
        let md = self.contents.read();
        let selected = anchor.min(head)..anchor.max(head);
        let span = self.span.peek().is_some();
        let bounds = self.range(field, &md);
        if !extend && !selected.is_empty() {
            if !span {
                return false;
            }
            let pos = if right { selected.end } else { selected.start };
            self.open(self.field_at(pos), [pos; 2]);
        } else if head == if right { bounds.end } else { bounds.start } {
            let Some(target) = self.neighbor(field, right) else {
                return false;
            };
            let target_range = self.range(target, &md);
            let pos = if right {
                target_range.start
            } else {
                target_range.end
            };
            self.open(target, [if extend { anchor } else { pos }, pos]);
        } else if span && extend {
            let Some(handle) = self.root.peek().clone() else {
                return false;
            };
            let doc = handle.doc();
            let Some(input) = doc
                .get_element_by_id(&field.id())
                .and_then(|id| doc.get_node(id)?.element_data()?.text_input_data())
            else {
                return false;
            };
            let Some(layout) = input.editor.try_layout() else {
                return false;
            };
            let cursor = input.editor.raw_selection().focus();
            let next = if right {
                cursor.next_visual(layout)
            } else {
                cursor.previous_visual(layout)
            };
            drop(doc);
            let pos = self.projection(field).display_to_source(next.index());
            self.open(field, [anchor, pos]);
        } else {
            return false;
        }
        true
    }
}
