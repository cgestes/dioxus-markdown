//! `margin ui`: every component of the design system in each variant and
//! state, on the real fonts and tokens. The living reference for
//! `docs/design-system.md`.
use super::*;
use crate::prelude::*;

const ROW: &str = "flex flex-wrap items-center gap-2";
/// Responsive multi-column grid used by gallery sections (including host sections).
pub const GRID: &str = "flex flex-wrap items-start gap-4 [&>*]:flex-[1_1_360px] [&>*]:min-w-0";

/// One titled group of samples.
pub fn section(title: &str, children: Element) -> Element {
    rsx! {
        section { class: "flex flex-col gap-3",
            h2 { class: "m-0 text-faint type-label uppercase", "{title}" }
            div { class: "flex flex-col gap-3", {children} }
        }
    }
}

/// The gallery window. Children are sections the host adds for what it
/// builds from these components (`section`): they go before the icons.
#[component]
pub fn Gallery(children: Element) -> Element {
    let noop = move |_| {};
    let text = use_signal(String::new);
    let filled = use_signal(|| "Front-left sensor failing".to_string());
    let mut tab = use_signal(|| 1usize);
    let mut segment = use_signal(|| 1usize);
    let mut checked = use_signal(|| true);
    let mut combo_open = use_signal(|| false);
    let mut combo_lang = use_signal(|| "rust".to_string());
    let combo_items = vec![
        ComboboxItem { value: String::new(), label: "Plain text".into(), hint: None, keywords: vec!["plain".into()], id: None },
        ComboboxItem { value: "rust".into(), label: "Rust".into(), hint: Some("rust".into()), keywords: vec!["rs".into()], id: None },
        ComboboxItem { value: "ts".into(), label: "TypeScript".into(), hint: Some("ts".into()), keywords: vec!["typescript".into()], id: None },
        ComboboxItem { value: "python".into(), label: "Python".into(), hint: Some("python".into()), keywords: vec!["py".into()], id: None },
    ];
    let combo_label = combo_items
        .iter()
        .find(|i| i.value == *combo_lang.read())
        .map_or_else(|| combo_lang.read().clone(), |i| i.label.clone());
    let segments = ["Rich", "Live", "Source"]
        .iter()
        .enumerate()
        .map(|(i, l)| Segment { id: ["g-rich", "g-live", "g-source"][i], label: l, tip: None, selected: i == segment() })
        .collect::<Vec<_>>();
    rsx! {
        style { "{CSS}" }
        main { class: "flex flex-col gap-6 px-8 pt-6 pb-12 bg-page text-ink type-ui leading-[1.45]",
            h1 { class: "m-0 type-h1", "Margin design system" }
            p { class: "m-0 text-muted", "Tokens from assets/theme.css and Tailwind v4 utilities compiled into margin_ui::CSS. Features compose these components; they add layout only." }
            {section("Buttons", rsx! {
                div { class: ROW,
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
                div { class: ROW,
                    Chip { label: "All", selected: true, onclick: noop }
                    Chip { label: "Telegram", icon: Icon::Telegram, onclick: noop }
                    Chip { label: "Slack", icon: Icon::Slack, onclick: noop }
                    Chip { label: "Summarize this week", onclick: noop }
                }
                div { class: ROW,
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
                div { class: GRID,
                    TextField { value: text, placeholder: "Search topics" }
                    TextField { value: filled, placeholder: "Quote" }
                    Composer { placeholder: "Ask about Car…", onsend: move |_| {} }
                    Composer { placeholder: "Comment or ask the agent to edit…", send_label: "Post".to_string(), onsend: move |_| {} }
                }
                div { class: ROW,
                    Checkbox { checked: false }
                    Checkbox { checked: true }
                    CheckItem { text: "Ripe mangoes", checked: checked(), onclick: move |_| checked.toggle() }
                    CheckItem { text: "Fresh basil", checked: true, tag: "Nikki".to_string(), onclick: noop }
                }
            })}
            {section("Cards and rows", rsx! {
                div { class: GRID,
                    div { class: "flex flex-col gap-0.5 !flex-[0_0_220px] p-2.5 rounded-lg bg-rail",
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
                            Row { title: "its a mazda 3 - dt-868-mz", leading: rsx! { ChannelGlyph { channel: Channel::Slack } },
                                snippet: "agent: Got it. The reminders will keep pinging…".to_string(),
                                sub: rsx! { SubItem { text: "Slack · #car" } SubItem { text: "36 msgs" } }, trailing: "13 h".to_string(), onclick: move |_| {} }
                            Row { title: "Telegram car group", leading: rsx! { ChannelGlyph { channel: Channel::Telegram } }, dot: true,
                                snippet: "agent: Gmail access works now.".to_string(), trailing: "7 h".to_string(), onclick: move |_| {} }
                            Row { title: "maintenance.md", leading: rsx! { Badge { text: "MD", tone: Tone::Accent } }, snippet: "Car · 4 KB".to_string() }
                        }
                    }
                }
            })}
            {section("Marks", rsx! {
                div { class: ROW,
                    for hue in Hue::ALL { Tile { icon: Icon::Car, hue } }
                    Tile { icon: Icon::Home, hue: Hue::Clay, size: TileSize::Large }
                    Tile { icon: Icon::Heart, hue: Hue::Plum, size: TileSize::Small }
                    ChannelGlyph { channel: Channel::Telegram }
                    ChannelGlyph { channel: Channel::Slack }
                    ChannelGlyph { channel: Channel::Chat }
                    Avatar { ai: true }
                    Avatar { initial: "Y" }
                    DateBox { top: "THU", day: "26" }
                }
                div { class: ROW,
                    for tone in Tone::ALL { Badge { text: tone.class().to_string(), tone } }
                    Badge { text: "Q1", tone: Tone::Accent }
                    Badge { text: "PDF", tone: Tone::Alert }
                }
                div { class: GRID,
                    Banner { text: "SilverBullet: HTTP 502", tone: Tone::Alert }
                    Banner { text: "Comment file saved with the page.", tone: Tone::Info }
                }
            })}
            {section("Messages", rsx! {
                div { class: GRID,
                    div { class: "flex flex-col gap-3 p-3 border border-solid border-line rounded-lg bg-surface",
                        Message { author: "you", body: "Is 60 °C the right setpoint?" }
                        Message { author: "luc", ai: true, body: "Yes, keep it at 60 °C or above; see house/systems.md.", tools: vec!["file_read".to_string()] }
                        Message { author: "luc", ai: true, pending: true, body: "" }
                        Message { author: "luc", ai: true, failed: true, body: "luc couldn’t answer: the connection closed" }
                    }
                    div { class: "flex flex-col gap-3 p-3 border border-solid border-line rounded-lg bg-surface",
                        Message { author: "you", body: "Shorten this to a few words.", style: MessageStyle::Thread }
                        Message { author: "scribe", ai: true, body: "The rest of the bullet gives the context, so the lead-in only names the part.", style: MessageStyle::Thread, tools: vec!["file_read".to_string()] }
                        Message { author: "scribe", ai: true, pending: true, body: "The rest of the bullet gives", style: MessageStyle::Thread }
                        Message { author: "agent", ai: true, body: "A transcript turn on its own card.", at: "2026-11-24 14:56".to_string(), style: MessageStyle::Card }
                    }
                }
            })}
            {section("Menus, popovers, toolbars", rsx! {
                div { class: "flex flex-wrap items-start gap-4 [&>*]:flex-[0_1_220px] [&>*]:min-w-0",
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
                        div { class: "max-w-[200px] p-2 text-muted", "A popover holds any small surface: a picker, a preview, a confirmation." }
                    }
                    Combobox {
                        label: "Code language",
                        value_label: combo_label,
                        selected: combo_lang(),
                        items: combo_items,
                        open: combo_open(),
                        placeholder: "Search language…".to_owned(),
                        allow_custom: true,
                        ontoggle: move |next| combo_open.set(next),
                        onselect: move |val| {
                            combo_lang.set(val);
                            combo_open.set(false);
                        },
                    }
                }
                div { class: ROW,
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
            {children}
            {section("Icons", rsx! {
                div { class: "flex flex-wrap gap-2 text-ink",
                    for i in Icon::ALL { span { class: "flex items-center justify-center w-8 h-8 border border-solid border-line rounded-md bg-surface", title: "{i:?}", {icon(i)} } }
                }
            })}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Paint the gallery to `target/ui-gallery.png`: this crate's components,
    /// without the sections a host adds (`margin ui` shows those too).
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
        let mut doc = DioxusDocument::new(VirtualDom::new(|| rsx! { Gallery {} }), config);
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
