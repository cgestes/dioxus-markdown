//! Read-only Markdown, in the notebook's reading projection
//! (`preview::document`): the same lines, spans and tables everywhere.
use dioxus_native::prelude::*;
use margin_editor_core::preview::{self, Block, Passage};

/// A passage's spans. `ids` adds the checkbox ids the Split preview uses to
/// toggle them; read-only views leave them out so ids stay unique.
///
/// A task box looks like the editor's checkbox. Its glyph stays as hidden
/// text so the Split preview's text offsets still match the source.
pub fn runs(p: &Passage, ids: bool) -> Element {
    rsx! {
        for run in &p.runs {
            if run.style == "box" {
                span { class: "box", class: if run.text.starts_with('☑') { "checked" },
                    id: ids.then(|| format!("reading-box-{}", run.from)),
                    "{run.text}"
                    if run.text.starts_with('☑') { span { class: "marker-tick" } }
                }
            } else {
                span { class: "{run.style}", "{run.text}" }
            }
        }
    }
}

/// Markdown to read: a topic's page (`page`, the default) or a message body
/// (`compact`). Tables look like the editor's. A component, so unchanged
/// text is not parsed again.
#[component]
pub fn MarkdownView(text: String, #[props(default)] compact: bool) -> Element {
    let blocks = preview::document(&text);
    rsx! {
        div { class: "ui-md", class: if compact { "compact" },
            for (i, block) in blocks.iter().enumerate() {
                match block {
                    Block::Text { passage } => rsx! {
                        div { key: "{i}", class: "read-line {passage.kind}",
                            style: if passage.depth > 0 { format!("padding-left:{}px", passage.depth * 18) },
                            {runs(passage, false)}
                        }
                    },
                    Block::Table { align, widths, rows, .. } => {
                        let last = rows.len().saturating_sub(1);
                        rsx! {
                            div { key: "{i}", class: "grid-block ledger read-ledger",
                                div { class: "grid-scroll", role: "table",
                                    for (r, row) in rows.iter().enumerate() {
                                        div { class: "grid-row", class: if r == last { "ledger-last" }, role: "row",
                                            for (c, p) in row.iter().enumerate() {
                                                div { class: "ledger-cell read-cell",
                                                    class: if r == 0 { "ledger-head" } else { "ledger-data" },
                                                    text_align: align.get(c).copied().unwrap_or("left"),
                                                    flex_grow: widths.get(c).copied().unwrap_or(1).max(1).to_string(),
                                                    {runs(p, false)}
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
