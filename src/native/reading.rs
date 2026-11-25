//! Hit testing in the read-only projection always targets the source input.
use super::{Field, Source, report, selection};
use crate::ui;
use dioxus_native::prelude::*;
use margin_editor_core::{
    checkbox::{Input, Mode},
    preview::{self, Block, Passage},
};

fn passage_id(from: usize, row: usize, col: usize) -> String {
    format!("reading-cell-{from}-{row}-{col}")
}

fn hit_text(doc: &blitz_dom::BaseDocument, id: &str, passage: &Passage, x: f64, y: f64) -> usize {
    let Some(node_id) = doc.get_element_by_id(id) else {
        return passage.from;
    };
    let Some(node) = doc.get_node(node_id) else {
        return passage.from;
    };
    let Some(rect) = doc.get_client_bounding_rect(node_id) else {
        return passage.from;
    };
    let l = node.final_layout();
    let x = (x - rect.x) as f32 - l.padding.left - l.border.left;
    let y = (y - rect.y) as f32 - l.padding.top - l.border.top;
    passage.source_at(node.text_offset_at_point(x, y).unwrap_or(0))
}

fn nearest(
    count: usize,
    point: f64,
    bounds: impl Fn(usize) -> Option<(f64, f64)>,
) -> Option<usize> {
    (0..count)
        .filter_map(|i| {
            let (start, size) = bounds(i)?;
            Some((i, (start - point).max(point - start - size).max(0.0)))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

fn read_pointer(source: Source, blocks: Memo<Vec<Block>>, event: PointerEvent) {
    if event.trigger_button() != Some(dioxus_html::input_data::MouseButton::Primary) {
        return;
    }
    let p = event.client_coordinates();
    let Some(h) = source.writing.root.peek().clone() else {
        return;
    };
    let doc = h.doc();
    let Some(mut id) = doc.element_from_point(p.x as f32, p.y as f32) else {
        return;
    };
    let mut box_at = None;
    let block = loop {
        let Some(n) = doc.get_node(id) else { return };
        if let Some(el) = n.element_data() {
            if &*el.name.local == "button" {
                return;
            }
            if let Some(name) = el.id.as_deref() {
                if let Some(at) = name
                    .strip_prefix("reading-box-")
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    box_at = Some(at);
                }
                if let Some(i) = name
                    .strip_prefix("reading-block-")
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    break i;
                }
            }
        }
        let Some(parent) = n.parent else { return };
        id = parent;
    };
    let model = blocks.peek();
    let (pos, cell) = match &model[block] {
        Block::Text { passage } => (
            hit_text(&doc, &format!("reading-block-{block}"), passage, p.x, p.y),
            None,
        ),
        Block::Table { from, rows, .. } => {
            let rect = |r, c| {
                doc.get_client_bounding_rect(doc.get_element_by_id(&passage_id(*from, r, c))?)
            };
            let Some(r) = nearest(rows.len(), p.y, |r| rect(r, 0).map(|b| (b.y, b.height))) else {
                return;
            };
            let frame = doc
                .get_element_by_id(&format!("reading-scroll-{from}"))
                .and_then(|n| doc.get_client_bounding_rect(n));
            let x = frame.map_or(p.x, |f| p.x.clamp(f.x, f.x + f.width));
            let Some(c) = nearest(rows[r].len(), x, |c| rect(r, c).map(|b| (b.x, b.width))) else {
                return;
            };
            (
                hit_text(&doc, &passage_id(*from, r, c), &rows[r][c], x, p.y),
                Some((*from, r, c)),
            )
        }
    };
    drop(doc);
    drop(model);
    event.prevent_default();
    if let Some(open) = box_at
        && !event
            .modifiers()
            .intersects(Modifiers::ALT | Modifiers::SHIFT)
    {
        let mut writing = source.writing;
        source.read();
        report(
            writing.note.write().edit_marker(
                open + 1..open + 1,
                Mode::Structured,
                Input::Type(' '),
            ),
            writing.message,
        );
        source.place([open + 1; 2]);
        return;
    }
    let anchor = if event.modifiers().shift() {
        selection(source.writing.root, Field::Source).map_or(pos, |s| s[0])
    } else {
        pos
    };
    if let Some((start, r, c)) = cell {
        source.cell(start, r, c, Some([anchor, pos]));
    } else {
        source.place([anchor, pos]);
    }
}

/// The preview's spans, with the checkbox ids `read_pointer` toggles.
fn runs(p: &Passage) -> Element {
    crate::ui::runs(p, true)
}

// Follow the source caret when its reading line leaves the preview viewport.
// Scrolling the preview by hand does not change the source selection.
fn follow(source: Source, blocks: &[Block], head: usize) {
    let Some(handle) = source.writing.root.peek().clone() else {
        return;
    };
    let mut doc = handle.doc_mut();
    let Some(frame) = doc.get_element_by_id("reading-page") else {
        return;
    };
    let target = blocks.iter().enumerate().find_map(|(i, b)| match b {
        Block::Text { passage } if (passage.from..=passage.to).contains(&head) => {
            Some(format!("reading-block-{i}"))
        }
        Block::Table { from, to, rows, .. } if (*from..=*to).contains(&head) => {
            let cell = rows.iter().enumerate().find_map(|(r, row)| {
                row.iter()
                    .position(|p| (p.from..=p.to).contains(&head))
                    .map(|c| passage_id(*from, r, c))
            });
            Some(cell.unwrap_or_else(|| format!("reading-block-{i}")))
        }
        _ => None,
    });
    let Some(target) = target
        .and_then(|s| doc.get_element_by_id(&s))
        .and_then(|id| doc.get_client_bounding_rect(id))
    else {
        return;
    };
    let Some(view) = doc.get_client_bounding_rect(frame) else {
        return;
    };
    if target.y + target.height < view.y || target.y > view.y + view.height {
        doc.scroll_by(
            frame,
            0.,
            target.y - view.y - 12.,
            blitz_dom::ScrollBehavior::Instant,
        );
    }
}

#[component]
pub(super) fn Reading(source: Source) -> Element {
    let blocks = use_memo(move || preview::document(&source.writing.contents.read()));
    let range = *source.caret.read();
    let head = range[1];
    let tables = source.writing.tables.read();
    use_effect(move || follow(source, &blocks.read(), source.caret.read()[1]));
    rsx! {
        section {class:"split-preview", aria_label:"Formatted preview",
            div {class:"pane-label","Preview"}
            div {id:"reading-page",class:"reading-page ui-md", onpointerdown:move|e|read_pointer(source,blocks,e),
                for (i,block) in blocks.read().iter().enumerate() {
                    match block {
                        Block::Text{passage:p} => rsx! {
                            div {key:"p-{i}",id:"reading-block-{i}",class:"read-line {p.kind}", style:if p.depth>0 {format!("padding-left:{}px",p.depth*18)}, {runs(p)} }
                        },
                        Block::Table{from,to,align,widths,rows} => {
                            let start=*from;
                            let can_delete = align.len()>1 && tables.iter().find(|t|t.range.start==start)
                                .and_then(|t|source.selected_cell(t,range)).is_some();
                            let last = rows.len().saturating_sub(1);
                            rsx! {div {key:"t-{i}",id:"reading-block-{i}",class:"grid-block read-table",class:if (*from..=*to).contains(&head) {"engaged"},
                                // The same table as the editor's and every read-only view's.
                                div {class:"ledger read-ledger",
                                    div {class:"grid-scroll",id:"reading-scroll-{from}",role:"table",
                                        for (r,row) in rows.iter().enumerate() {
                                            div {class:"grid-row",class:if r==last {"ledger-last"},role:"row",
                                                for (c,p) in row.iter().enumerate() {
                                                    div {class:"ledger-cell read-cell",class:if r==0 {"ledger-head column-head"} else {"ledger-data"},role:if r==0{"columnheader"}else{"cell"}, id:passage_id(start,r,c),text_align:align[c],flex_grow:widths[c].to_string(), {runs(p)} }
                                                }
                                            }
                                        }
                                    }
                                }
                                div {class:"grid-edge col", ui::Button {id:format!("reading-column-{start}"),variant:ui::Variant::Icon,label:"Add column",icon:ui::Icon::Plus,onclick:move |_|source.grow(start,true,0)}}
                                if can_delete {div {class:"column-action",
                                    ui::Button {id:format!("reading-delete-column-{start}"),variant:ui::Variant::Icon,danger:true,label:"Delete column",text:"−",onclick:move |_|source.delete_column(start)}
                                }}
                                div {class:"grid-edge row", ui::Button {id:format!("reading-row-{start}"),variant:ui::Variant::Icon,label:"Add row",icon:ui::Icon::Plus,onclick:move |_|source.grow(start,false,0)}}
                            }}
                        }
                    }
                }
            }
        }
    }
}
