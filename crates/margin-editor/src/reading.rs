//! Read-only Markdown, in the notebook's reading projection
//! (`preview::document`): the same lines, spans and tables everywhere. The
//! styles are the design system's (`.ui-md`); the Markdown is read here, so
//! the design system knows none.
use crate::prelude::*;
use crate::ui::{self, Card, CardTitle, Icon, Message, icon};
use margin_editor_core::callout::CalloutKind;
use margin_editor_core::preview::{self, Block, Passage};

/// The glyph of a GFM/Obsidian callout, in the editor and in reading.
pub(crate) const fn callout_icon(kind: CalloutKind) -> Icon {
    match kind {
        CalloutKind::Note => Icon::Info,
        CalloutKind::Tip => Icon::Spark,
        CalloutKind::Important => Icon::Important,
        CalloutKind::Warning => Icon::AlertTriangle,
        CalloutKind::Caution => Icon::AlertOctagon,
    }
}

fn callout_badge_icon(style: &str) -> Icon {
    let kind = CalloutKind::ALL
        .into_iter()
        .find(|k| style.contains(k.css_class()))
        .unwrap_or(CalloutKind::Note);
    callout_icon(kind)
}

/// A passage's spans. A task box looks like the editor's checkbox; its glyph
/// stays as hidden text, so the passage's text still matches its source runs.
fn runs(p: &Passage) -> Element {
    rsx! {
        for run in &p.runs {
            if run.style == "box" {
                span { class: "box", class: if run.text.starts_with('☑') { "checked" },
                    "{run.text}"
                    if run.text.starts_with('☑') { span { class: "marker-tick" } }
                }
            } else if run.style.starts_with("callout-badge") {
                span { class: "{run.style}",
                    {icon(callout_badge_icon(&run.style))}
                    span { "{run.text}" }
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
                            {runs(passage)}
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
                                                    {runs(p)}
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

const SAMPLE_PAGE: &str = "# Car — maintenance\n\nOil every **15,000 km** or a year; see `car/log.md` and [the garage](https://example.com).\n\n## Schedule\n\n- [x] Cabin filter, 2026-04\n- [ ] Brakes: check at next service\n\n> Expired = ~135 € fine.\n\n| Item | Interval | Next due |\n| --- | --- | ---: |\n| Oil + filter | 1 year | 2027-04 |\n| CT | 2 years | 2027-06 |\n";
const SAMPLE_REPLY: &str = "Yes, keep it at **60 °C or above**; see `house/systems.md`.";

/// `margin ui`: the design system's gallery, with the Markdown view added:
/// a page, the compact form, and a message whose body is Markdown.
#[component]
pub fn Gallery() -> Element {
    rsx! {
        ui::gallery::Gallery {
            {ui::gallery::section("Markdown", rsx! {
                div { class: "gallery-grid",
                    Card { CardTitle { text: "page", icon: Icon::File } MarkdownView { text: SAMPLE_PAGE } }
                    Card { CardTitle { text: "compact", icon: Icon::Chat } MarkdownView { text: SAMPLE_PAGE, compact: true } }
                    div { class: "gallery-stack",
                        Message { author: "luc", ai: true, body: SAMPLE_REPLY, tools: vec!["file_read".to_string()],
                            rich: rsx! { MarkdownView { text: SAMPLE_REPLY, compact: true } } }
                    }
                }
            })}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{open, text};

    const BODY: &str = "Keep it at **60 °C**.";

    fn messages() -> Element {
        let rich = || rsx! { MarkdownView { text: BODY, compact: true } };
        rsx! {
            div { id: "done", Message { author: "luc", ai: true, body: BODY, rich: rich() } }
            div { id: "pending", Message { author: "luc", ai: true, pending: true, body: BODY, rich: rich() } }
            div { id: "failed", Message { author: "luc", ai: true, failed: true, body: BODY, rich: rich() } }
            div { id: "plain", Message { author: "luc", ai: true, body: BODY } }
        }
    }

    #[test]
    fn a_finished_message_reads_as_markdown_and_any_other_as_its_plain_text() {
        let doc = open(VirtualDom::new(messages));
        let done = text(&doc, "done");
        assert!(done.contains("Keep it at 60 °C.") && !done.contains("**"), "{done}");
        for id in ["pending", "failed", "plain"] {
            assert!(text(&doc, id).contains(BODY), "{id}: {}", text(&doc, id));
        }
    }

    #[test]
    fn the_gallery_adds_markdown_to_the_design_systems_components() {
        let doc = open(VirtualDom::new(|| rsx! { div { id: "gallery", Gallery {} } }));
        let shown = text(&doc, "gallery");
        // The design system's own sections, then this crate's.
        assert!(shown.contains("Buttons") && shown.contains("Icons"), "the components");
        assert!(shown.contains("Car — maintenance") && !shown.contains("# Car"), "the page, read as Markdown");
        assert!(shown.contains("60 °C or above") && !shown.contains("**60"), "the message body, read as Markdown");
    }
}
