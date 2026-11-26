//! `margin ui`: every component of the design system in each variant and
//! state, on the real fonts and tokens. The living reference for
//! `docs/design-system.md`.
use super::*;
use crate::prelude::*;

const SAMPLE_MD: &str = "# Car — maintenance\n\nOil every **15,000 km** or a year; see `car/log.md` and [the garage](https://example.com).\n\n## Schedule\n\n- [x] Cabin filter, 2026-04\n- [ ] Brakes: check at next service\n\n> Expired = ~135 € fine.\n\n| Item | Interval | Next due |\n| --- | --- | ---: |\n| Oil + filter | 1 year | 2027-04 |\n| CT | 2 years | 2027-06 |\n";

fn section(title: &str, children: Element) -> Element {
    rsx! {
        section { class: "gallery-section",
            h2 { class: "gallery-title", "{title}" }
            div { class: "gallery-body", {children} }
        }
    }
}

/// The gallery window.
#[component]
pub fn Gallery() -> Element {
    let noop = move |_| {};
    let text = use_signal(String::new);
    let filled = use_signal(|| "Front-left sensor failing".to_string());
    let mut tab = use_signal(|| 1usize);
    let mut segment = use_signal(|| 1usize);
    let mut checked = use_signal(|| true);
    let segments = ["Rich", "Live", "Split", "Source"]
        .iter()
        .enumerate()
        .map(|(i, l)| Segment { id: ["g-rich", "g-live", "g-split", "g-source"][i], label: l, tip: None, selected: i == segment() })
        .collect::<Vec<_>>();
    rsx! {
        style { "{CSS}" }
        style { "{GALLERY_CSS}" }
        main { class: "gallery",
            h1 { class: "gallery-head", "Margin design system" }
            p { class: "gallery-lede", "Tokens from assets/theme.css, styles from assets/ui.css. Features compose these components; they add layout only." }
            {section("Buttons", rsx! {
                div { class: "gallery-line",
                    for (v, name) in [(Variant::Default, "Default"), (Variant::Primary, "Primary"), (Variant::Ghost, "Ghost"), (Variant::Link, "Link →")] {
                        Button { label: name.to_string(), variant: v, onclick: noop }
                    }
                    Button { label: "SilverBullet", icon: Icon::Open, text: "SilverBullet".to_string(), onclick: noop }
                    Button { label: "Reminder", icon: Icon::Calendar, text: "Reminder".to_string(), variant: Variant::Primary, onclick: noop }
                    Button { label: "Undo", icon: Icon::Undo, variant: Variant::Icon, onclick: noop }
                    Button { label: "Comments", icon: Icon::Comment, variant: Variant::Icon, selected: Some(true), onclick: noop }
                    Button { label: "Save", variant: Variant::Primary, disabled: true, onclick: noop }
                    Button { label: "Disabled", disabled: true, onclick: noop }
                }
            })}
            {section("Chips, segmented switch, tabs", rsx! {
                div { class: "gallery-line",
                    Chip { label: "All", selected: true, onclick: noop }
                    Chip { label: "Telegram", icon: Icon::Telegram, onclick: noop }
                    Chip { label: "Slack", icon: Icon::Slack, onclick: noop }
                    Chip { label: "Summarize this week", onclick: noop }
                }
                div { class: "gallery-line",
                    Segmented { label: "Markdown view", items: segments, onselect: move |i| segment.set(i) }
                }
                Tabs { items: vec![
                        TabItem { label: "Overview".into(), count: 0 },
                        TabItem { label: "Discussions".into(), count: 3 },
                        TabItem { label: "Calendar".into(), count: 1 },
                        TabItem { label: "Docs".into(), count: 2 },
                    ], selected: tab(), onselect: move |i| tab.set(i) }
            })}
            {section("Text entry and checkboxes", rsx! {
                div { class: "gallery-grid",
                    TextField { value: text, placeholder: "Search topics" }
                    TextField { value: filled, placeholder: "Quote" }
                    Composer { placeholder: "Ask about Car…", onsend: move |_| {} }
                    Composer { placeholder: "Comment or ask the agent to edit…", send_label: "Post".to_string(), onsend: move |_| {} }
                }
                div { class: "gallery-line",
                    Checkbox { checked: false }
                    Checkbox { checked: true }
                    CheckItem { text: "Ripe mangoes", checked: checked(), onclick: move |_| checked.toggle() }
                    CheckItem { text: "Fresh basil", checked: true, tag: "Nikki".to_string(), onclick: noop }
                }
            })}
            {section("Cards and rows", rsx! {
                div { class: "gallery-grid",
                    div { class: "gallery-nav",
                        NavItem { label: "Today", leading: icon(Icon::Calendar), count: (4, Tone::Accent), selected: true, onclick: noop }
                        NavItem { label: "Unfiled", leading: icon(Icon::Inbox), count: (3, Tone::Alert), onclick: noop }
                        NavItem { label: "House", hint: "Plumber · Thu", leading: rsx! { Tile { icon: Icon::Home, hue: Hue::Clay } }, dot: true, onclick: noop }
                        NavItem { label: "Hot water", hint: "chauffe-eau", indent: true, leading: rsx! { Tile { icon: Icon::Drop, hue: Hue::Clay } }, onclick: noop }
                    }
                    Card {
                        CardTitle { text: "Next up", icon: Icon::Calendar, action: "All 3 →".to_string(), onaction: move |_| {} }
                        Rows {
                            Row { title: "Book the contrôle technique", leading: rsx! { DateBox { top: "MAR", day: "25" } },
                                sub: rsx! { SubItem { icon: Icon::Repeat, text: "repeats" } SubItem { icon: Icon::Arrow, text: "Telegram car group" } } }
                            Row { title: "Borland Lodge: check the payment", leading: rsx! { DateBox { top: "THU", day: "26" } },
                                trailing: "wedding-q2".to_string() }
                        }
                    }
                    Card {
                        CardTitle { text: "Recent discussions", icon: Icon::Chat, note: "memory · topic:car".to_string() }
                        Rows {
                            Row { title: "its a mazda 3 - dt-868-mz", leading: rsx! { SourceGlyph { source: Source::Slack } },
                                snippet: "agent: Got it. The reminders will keep pinging…".to_string(),
                                sub: rsx! { SubItem { text: "Slack · #car" } SubItem { text: "36 msgs" } }, trailing: "13 h".to_string(), onclick: move |_| {} }
                            Row { title: "Telegram car group", leading: rsx! { SourceGlyph { source: Source::Telegram } }, dot: true,
                                snippet: "agent: Gmail access works now.".to_string(), trailing: "7 h".to_string(), onclick: move |_| {} }
                            Row { title: "maintenance.md", leading: rsx! { Badge { text: "MD", tone: Tone::Accent } }, snippet: "Car · 4 KB".to_string() }
                        }
                    }
                }
            })}
            {section("Marks", rsx! {
                div { class: "gallery-line",
                    for hue in Hue::ALL { Tile { icon: Icon::Car, hue } }
                    Tile { icon: Icon::Home, hue: Hue::Clay, size: TileSize::Large }
                    Tile { icon: Icon::Heart, hue: Hue::Plum, size: TileSize::Small }
                    SourceGlyph { source: Source::Telegram }
                    SourceGlyph { source: Source::Slack }
                    SourceGlyph { source: Source::Chat }
                    Avatar { ai: true }
                    Avatar { initial: "Y" }
                    DateBox { top: "THU", day: "26" }
                }
                div { class: "gallery-line",
                    for tone in Tone::ALL { Badge { text: tone.class().to_string(), tone } }
                    Badge { text: "Q1", tone: Tone::Accent }
                    Badge { text: "PDF", tone: Tone::Alert }
                }
                div { class: "gallery-grid",
                    Banner { text: "SilverBullet: HTTP 502", tone: Tone::Alert }
                    Banner { text: "Comment file saved with the page.", tone: Tone::Info }
                }
            })}
            {section("Messages", rsx! {
                div { class: "gallery-grid",
                    div { class: "gallery-stack",
                        Message { author: "you", body: "Is 60 °C the right setpoint?" }
                        Message { author: "luc", ai: true, body: "Yes, keep it at **60 °C or above**; see `house/systems.md`.", tools: vec!["file_read".to_string()] }
                        Message { author: "luc", ai: true, pending: true, body: "" }
                        Message { author: "luc", ai: true, failed: true, body: "luc couldn’t answer: the connection closed" }
                    }
                    div { class: "gallery-stack",
                        Message { author: "you", body: "Shorten this to a few words.", style: MessageStyle::Thread }
                        Message { author: "scribe", ai: true, body: "The rest of the bullet gives the context, so the lead-in only names the part.", style: MessageStyle::Thread, tools: vec!["file_read".to_string()] }
                        Message { author: "scribe", ai: true, pending: true, body: "The rest of the bullet gives", style: MessageStyle::Thread }
                        Message { author: "agent", ai: true, body: "A transcript turn on its own card.", at: "2026-11-24 14:56".to_string(), style: MessageStyle::Card }
                    }
                }
            })}
            {section("Menus, popovers, toolbars", rsx! {
                div { class: "gallery-grid gallery-menus",
                    Menu { label: "Column menu",
                        MenuSection { label: "Column B" }
                        MenuRow {
                            Button { label: "Align left", icon: Icon::AlignLeft, variant: Variant::Icon, selected: Some(true), onclick: noop }
                            Button { label: "Align center", icon: Icon::AlignCenter, variant: Variant::Icon, onclick: noop }
                            Button { label: "Align right", icon: Icon::AlignRight, variant: Variant::Icon, onclick: noop }
                        }
                        MenuSeparator {}
                        MenuItem { label: "Move left", badge: "←", disabled: true, onclick: noop }
                        MenuItem { label: "Move right", badge: "→", active: true, onclick: noop }
                        MenuItem { label: "Insert right", badge: "+", onclick: noop }
                        MenuSeparator {}
                        MenuItem { label: "Delete column", badge: "×", danger: true, onclick: noop }
                    }
                    Menu { label: "Insert block", role: "listbox",
                        MenuItem { label: "Heading 1", badge: "H1", hint: "#", option: true, active: true, onclick: noop }
                        MenuItem { label: "Checklist", icon: Icon::Task, hint: "- [ ]", option: true, onclick: noop }
                        MenuItem { label: "Table", icon: Icon::Table, hint: "| |", option: true, onclick: noop }
                    }
                    Popover { label: "Popover",
                        div { class: "gallery-pop", "A popover holds any small surface: a picker, a preview, a confirmation." }
                    }
                }
                div { class: "gallery-line",
                    Toolbar { label: "Dock", look: ToolbarLook::Dock,
                        Button { label: "Insert table", icon: Icon::Table, variant: Variant::Icon, onclick: noop }
                        ToolbarSep {}
                        Button { label: "Undo", icon: Icon::Undo, variant: Variant::Icon, onclick: noop }
                        Button { label: "Redo", icon: Icon::Redo, variant: Variant::Icon, disabled: true, onclick: noop }
                    }
                    Toolbar { label: "Format", look: ToolbarLook::Floating,
                        Button { label: "Bold", icon: Icon::Bold, variant: Variant::Icon, selected: Some(true), onclick: noop }
                        Button { label: "Italic", icon: Icon::Italic, variant: Variant::Icon, onclick: noop }
                        Button { label: "Code", icon: Icon::Code, variant: Variant::Icon, onclick: noop }
                        Button { label: "Link", icon: Icon::Link, variant: Variant::Icon, onclick: noop }
                        ToolbarSep {}
                        Button { label: "Comment", icon: Icon::Comment, text: "Comment", variant: Variant::Ghost, onclick: noop }
                    }
                    Toolbar { label: "Insert", look: ToolbarLook::Inline,
                        Button { label: "Heading 1", text: "H1", variant: Variant::Icon, onclick: noop }
                        Button { label: "Heading 2", text: "H2", variant: Variant::Icon, onclick: noop }
                        ToolbarSep {}
                        Button { label: "Checklist", icon: Icon::Task, variant: Variant::Icon, onclick: noop }
                        Button { label: "Quote", icon: Icon::Quote, variant: Variant::Icon, onclick: noop }
                        Button { label: "Table", icon: Icon::Table, variant: Variant::Icon, onclick: noop }
                    }
                }
            })}
            {section("Markdown", rsx! {
                div { class: "gallery-grid",
                    Card { CardTitle { text: "page", icon: Icon::File } MarkdownView { text: SAMPLE_MD } }
                    Card { CardTitle { text: "compact", icon: Icon::Chat } MarkdownView { text: SAMPLE_MD, compact: true } }
                }
            })}
            {section("Icons", rsx! {
                div { class: "gallery-icons",
                    for i in Icon::ALL { span { class: "gallery-icon", title: "{i:?}", {icon(i)} } }
                }
            })}
        }
    }
}

/// Layout of the gallery page only.
const GALLERY_CSS: &str = "
.gallery { display: flex; flex-direction: column; gap: var(--space-5); padding: var(--space-5) 32px 48px; background: var(--page); color: var(--ink); font: var(--font-ui); line-height: 1.45; }
.gallery-head { margin: 0; font: var(--font-h1); letter-spacing: var(--track-h1); }
.gallery-lede { margin: 0; color: var(--muted); }
.gallery-section { display: flex; flex-direction: column; gap: var(--space-3); }
.gallery-title { margin: 0; color: var(--faint); font: var(--font-label); letter-spacing: var(--track-label); text-transform: uppercase; }
.gallery-body { display: flex; flex-direction: column; gap: var(--space-3); }
.gallery-line { display: flex; flex-wrap: wrap; align-items: center; gap: var(--space-2); }
.gallery-grid { display: flex; flex-wrap: wrap; align-items: flex-start; gap: var(--space-4); }
.gallery-grid > * { flex: 1 1 360px; min-width: 0; }
.gallery-stack { display: flex; flex-direction: column; gap: var(--space-3); padding: var(--space-3); border: 1px solid var(--line); border-radius: var(--radius-l); background: var(--surface); }
.gallery-nav { display: flex; flex-direction: column; gap: 2px; flex: 0 0 220px; padding: 10px; border-radius: var(--radius-l); background: var(--rail); }
.gallery-icons { display: flex; flex-wrap: wrap; gap: var(--space-2); color: var(--ink); }
.gallery-icon { display: flex; align-items: center; justify-content: center; width: 32px; height: 32px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--surface); }
.gallery-icon .i { width: 16px; height: 16px; }
.gallery-menus > * { flex: 0 1 220px; }
.gallery-pop { max-width: 200px; padding: var(--space-2); color: var(--muted); }
";

#[cfg(test)]
mod tests {
    use super::*;

    /// Paint the gallery to `target/ui-gallery.png`.
    /// `cargo test -p margin-ui ui_gallery_shot -- --ignored`
    #[test]
    #[ignore = "writes a screenshot"]
    fn ui_gallery_shot() {
        use blitz_dom::Document;
        use blitz_traits::shell::Viewport;
        use dioxus_native::DioxusDocument;
        let (w, h) = (1180u32, 2200u32);
        let config = dioxus_native::DocumentConfig {
            font_ctx: Some(crate::typefaces::notebook_fonts()),
            viewport: Some(Viewport { window_size: (w, h), ..Default::default() }),
            ..Default::default()
        };
        let mut doc = DioxusDocument::new(VirtualDom::new(Gallery), config);
        doc.initial_build();
        for _ in 0..6 {
            doc.poll(None);
            doc.inner.borrow_mut().resolve(0.0);
        }
        let pixels = anyrender::render_to_buffer::<anyrender_vello_cpu::VelloCpuImageRenderer, _>(
            |scene| blitz_paint::paint_scene(scene, &mut doc.inner.borrow_mut(), 1.0, w, h, 0, 0),
            w,
            h,
        );
        let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).expect("crates/<name> sits two levels below the workspace").join("target");
        std::fs::create_dir_all(&target).unwrap();
        let path = target.join("ui-gallery.png");
        let mut enc = png::Encoder::new(std::fs::File::create(&path).unwrap(), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&pixels).unwrap();
        println!("{}", path.display());
    }
}
