//! A topic's page: its overview, discussions, questions, documents and checklists.
use super::*;

pub(super) fn tabs(t: &TopicView) -> Vec<(Tab, &'static str, usize)> {
    let open_q = t.questions.iter().filter(|q| q.open).count();
    [
        (Tab::Overview, "Overview", 0),
        (Tab::Discussions, "Discussions", t.threads.len()),
        (Tab::Calendar, "Calendar", t.agenda.len()),
        (Tab::Questions, "Questions", open_q),
        (Tab::Docs, "Docs", t.docs.len()),
        (Tab::Procedures, "Procedures", t.procedures.len()),
        (Tab::Lists, "Lists", t.lists.len()),
        (Tab::Code, "Code", t.code.len()),
    ]
    .into_iter()
    .filter(|(k, _, n)| *k == Tab::Overview || *n > 0 || (*k == Tab::Questions && !t.questions.is_empty()))
    .collect()
}

#[component]
pub(super) fn TopicPage(topic: TopicView) -> Element {
    let ui = use_context::<Ui>();
    let t = topic;
    let shown = tabs(&t);
    let tab = if shown.iter().any(|(k, _, _)| *k == (ui.tab)()) { (ui.tab)() } else { Tab::Overview };
    let parent = t
        .id
        .rsplit_once('/')
        .and_then(|(p, _)| ui.ws.read().as_ref()?.find(p).map(|p| p.title.clone()));
    let crumb = match (&parent, t.subtitle.is_empty()) {
        (Some(p), false) => format!("{p} / {}  ·  {}", t.title, t.subtitle),
        (Some(p), true) => format!("{p} / {}", t.title),
        (None, false) => t.subtitle.clone(),
        (None, true) => String::new(),
    };
    let sources: Vec<&str> = t.sources().iter().map(|s| s.name()).collect();
    let messages: u64 = t.threads.iter().map(|x| x.messages).sum();
    let meta = if sources.is_empty() {
        "No discussions yet".to_string()
    } else {
        format!("{} · {messages} messages", sources.join(" · "))
    };
    let today = ui.ws.read().as_ref().map(|w| w.today.clone()).unwrap_or_default();
    // With a doc open, SilverBullet opens that page; the header never moves.
    let web = match ((ui.pane)(), ui.live()) {
        (Pane::Doc(path), Some(live)) => Some(live.space.page_url(&path)),
        _ => t.web_url.clone(),
    };
    let selected = shown.iter().position(|(k, _, _)| *k == tab).unwrap_or(0);
    let keys: Vec<Tab> = shown.iter().map(|(k, _, _)| *k).collect();
    let items = shown
        .iter()
        .map(|(_, label, count)| TabItem { label: label.to_string(), count: *count })
        .collect::<Vec<_>>();
    rsx! {
        div { class: "crumb", "{crumb}" }
        div { class: "head",
            Tile { icon: t.icon, hue: t.hue, size: TileSize::Large }
            div { class: "grow",
                h1 { "{t.title}" }
                div { class: "meta", "{meta} · vault " span { class: "tag", "{t.vault_path}" } }
            }
            if let Some(url) = web {
                Button { label: "SilverBullet", icon: Icon::Open, text: "SilverBullet", onclick: move |_| open_url(&url) }
            }
            if ui.demo() {
                Button { label: "Note", icon: Icon::Plus, text: "Note", onclick: move |_| {} }
                Button { label: "Reminder", icon: Icon::Calendar, text: "Reminder", variant: Variant::Primary, onclick: move |_| {} }
            }
        }
        div { class: "tabs", Tabs { items, selected, onselect: move |i: usize| ui.set_tab(keys[i]) } }
        // A doc or a conversation replaces the tab's content; the header and
        // the tabs stay where they are.
        match (ui.pane)() {
        Pane::Doc(path) => rsx! { DocPane { key: "{path}", path } },
        Pane::Thread(key, title) => rsx! { TranscriptPane { key: "{key}", session_key: key, title } },
        Pane::Tab => match tab {
            Tab::Overview => rsx! { Overview { topic: t.clone() } },
            Tab::Discussions => rsx! { Discussions { topic: t.clone() } },
            Tab::Calendar => rsx! {
                Card { Rows {
                    for a in t.agenda.iter() { AgendaRow { today: today.clone(), agenda: a.clone(), topic: None, full: true } }
                } }
            },
            Tab::Questions => rsx! { Questions { topic: t.clone() } },
            Tab::Docs => rsx! { div { class: "grid3", for d in t.docs.iter() { DocEntry { doc: d.clone(), tile: true } } } },
            Tab::Procedures => rsx! { div { class: "two", for (i, p) in t.procedures.iter().enumerate() { ChecklistCard { list: p.clone(), topic: t.id.clone(), index: i, procedure: true } } } },
            Tab::Lists => rsx! { div { class: "two", for (i, l) in t.lists.iter().enumerate() { ChecklistCard { list: l.clone(), topic: t.id.clone(), index: i, procedure: false } } } },
            Tab::Code => rsx! {
                for s in t.code.iter() {
                    Card { CardTitle { text: s.file.clone(), icon: Icon::Code } pre { class: "code", "{s.code}" } }
                }
            },
        },
        }
    }
}

#[component]
pub(super) fn Overview(topic: TopicView) -> Element {
    let ui = use_context::<Ui>();
    let t = topic;
    let go = move |k: Tab| move |_| ui.set_tab(k);
    let open_q: Vec<&QuestionRow> = t.questions.iter().filter(|q| q.open).collect();
    let today = ui.ws.read().as_ref().map(|w| w.today.clone()).unwrap_or_default();
    rsx! {
        div { class: "ov",
            div { class: "ov-col l",
                if !t.brief.is_empty() {
                    Card { class: "brief",
                        CardTitle { text: "Brief", icon: Icon::Spark }
                        p { "{t.brief}" }
                        div { class: "by", {ui::icon(Icon::Spark)} "Kept up to date by the agent · {t.brief_by}" }
                    }
                }
                if let (Some(md), Some(path)) = (t.overview.clone(), t.overview_path.clone()) {
                    Card {
                        CardTitle { text: path.clone(), icon: Icon::File, action: "Edit →", onaction: move |_| ui.open(Pane::Doc(path.clone())) }
                        MarkdownView { text: md }
                    }
                }
                if !t.threads.is_empty() {
                    Card {
                        CardTitle { text: "Recent discussions", icon: Icon::Chat, action: "All →", onaction: go(Tab::Discussions) }
                        Rows { for th in t.threads.iter().take(3) { ThreadItem { key: "{th.key}", thread: th.clone(), topic_title: None } } }
                    }
                }
                if !t.procedures.is_empty() {
                    Card {
                        CardTitle { text: "Procedures", icon: Icon::Book, action: "Open →", onaction: go(Tab::Procedures) }
                        Rows {
                            for p in t.procedures.iter() {
                                Row { title: p.title.clone(), sub: rsx! { SubItem { text: p.meta.clone() } } }
                            }
                        }
                    }
                }
            }
            div { class: "ov-col r",
                Card {
                    if t.agenda.len() > 3 {
                        CardTitle { text: "Next up", icon: Icon::Calendar, action: format!("All {} →", t.agenda.len()), onaction: go(Tab::Calendar) }
                    } else {
                        CardTitle { text: "Next up", icon: Icon::Calendar }
                    }
                    if t.agenda.is_empty() { div { class: "empty small", "No reminders for this topic." } }
                    Rows { for a in t.agenda.iter().take(3) { AgendaRow { today: today.clone(), agenda: a.clone(), topic: None } } }
                }
                if !t.facts.is_empty() {
                    Card {
                        CardTitle { text: "Known facts", icon: Icon::Pin, note: format!("memory · topic:{}", t.id) }
                        div { class: "facts", for (k, v) in t.facts.iter() { div { span { "{k}" } "{v}" } } }
                    }
                }
                if !open_q.is_empty() {
                    Card {
                        CardTitle { text: "Open questions", icon: Icon::Question, action: format!("All {} →", open_q.len()), onaction: go(Tab::Questions) }
                        Rows {
                            for q in open_q.iter().take(3) {
                                Row { title: q.question.clone(), leading: rsx! { Badge { text: q.id.clone(), tone: Tone::Accent } },
                                    sub: (!model::blank(&q.ask_by)).then(|| rsx! { SubItem { text: format!("ask by {}", q.ask_by) } }) }
                            }
                        }
                    }
                }
                if !t.docs.is_empty() {
                    Card {
                        CardTitle { text: "Docs", icon: Icon::File, action: format!("All {} →", t.docs.len()), onaction: go(Tab::Docs) }
                        Rows { for d in t.docs.iter().take(4) { DocEntry { doc: d.clone(), tile: false } } }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn AgendaRow(agenda: Agenda, today: String, topic: Option<TopicTag>, #[props(default)] full: bool) -> Element {
    let a = agenda;
    rsx! {
        Row { title: a.title.clone(), leading: date_box(&a.sort, &today),
            snippet: (full && !a.notes.is_empty()).then(|| a.notes.clone()),
            trailing: full.then(|| a.tag.clone()),
            sub: rsx! {
                if let Some(t) = topic.as_ref() { SubItem { text: t.title.clone(), icon: t.icon } }
                if a.repeats { SubItem { text: "repeats", icon: Icon::Repeat } }
                if !a.deliver_to.is_empty() { SubItem { text: a.deliver_to.clone(), icon: Icon::Arrow } }
                if full && !a.detail.is_empty() { SubItem { text: a.detail.clone() } }
            }
        }
    }
}

/// A conversation row. Turns it carries expand in place; otherwise it opens
/// the transcript.
#[component]
pub(super) fn ThreadItem(thread: ThreadRow, topic_title: Option<String>) -> Element {
    let ui = use_context::<Ui>();
    let mut expanded = use_signal(|| false);
    let th = thread;
    let (key, title, inline) = (th.key.clone(), th.title.clone(), !th.turns.is_empty());
    rsx! {
        Row { title: th.title.clone(), dot: th.unread, snippet: th.snippet.clone(), trailing: th.when.clone(),
            leading: rsx! { ChannelGlyph { channel: channel(th.source) } },
            sub: rsx! {
                SubItem { text: format!("{} · {}", th.source.name(), th.place) }
                if th.messages > 0 { SubItem { text: format!("{} msgs", th.messages) } }
                if let Some(t) = topic_title { SubItem { text: t } }
            },
            onclick: move |_| if inline { expanded.toggle() } else { ui.open(Pane::Thread(key.clone(), title.clone())) },
            if expanded() {
                div { class: "turns", for (who, text) in th.turns.iter() { div { b { "{who} " } "{text}" } } }
            }
        }
    }
}

#[component]
pub(super) fn Discussions(topic: TopicView) -> Element {
    let ui = use_context::<Ui>();
    let filter = (ui.filter)();
    let set = move |f: Option<Source>| {
        let mut sig = ui.filter;
        sig.set(f);
    };
    let mut rows = topic.threads.iter().filter(|t| filter.is_none_or(|f| t.source == f)).peekable();
    let empty = rows.peek().is_none();
    rsx! {
        div { class: "chips",
            Chip { label: "All", selected: filter.is_none(), onclick: move |_| set(None) }
            for s in topic.sources() {
                Chip { label: s.name(), icon: Icon::for_channel(channel(s)), selected: filter == Some(s), onclick: move |_| set(Some(s)) }
            }
        }
        Card {
            Rows {
                for th in rows { ThreadItem { key: "{th.key}", thread: th.clone(), topic_title: None } }
            }
            if empty { div { class: "empty small", "Nothing from this source yet." } }
        }
    }
}

#[component]
pub(super) fn Questions(topic: TopicView) -> Element {
    let (open, answered): (Vec<&QuestionRow>, Vec<&QuestionRow>) = topic.questions.iter().partition(|q| q.open);
    let id = |q: &QuestionRow| rsx! { Badge { text: q.id.clone(), tone: Tone::Accent } };
    rsx! {
        Card {
            CardTitle { text: "Open · asked only when the conversation touches them", icon: Icon::Question }
            Rows {
                for q in open.iter() {
                    Row { title: q.question.clone(), leading: id(q), snippet: (!q.why.is_empty()).then(|| q.why.clone()),
                        sub: rsx! {
                            if !q.ask_when.is_empty() { SubItem { text: format!("ask when: {}", q.ask_when) } }
                            if !model::blank(&q.ask_by) { SubItem { text: format!("by {}", q.ask_by) } }
                        }
                    }
                }
            }
        }
        if !answered.is_empty() {
            div { class: "grp", "Answered" }
            Card {
                Rows {
                    for q in answered.iter() {
                        Row { title: q.question.clone(), leading: id(q), snippet: q.answer.clone() }
                    }
                }
            }
        }
    }
}

pub(super) fn kind_tone(kind: &str) -> Tone {
    match kind {
        "PDF" => Tone::Alert,
        "IMG" => Tone::Info,
        _ => Tone::Accent,
    }
}

/// A document: a tile in the Docs tab, a row in the overview.
#[component]
pub(super) fn DocEntry(doc: DocRow, tile: bool) -> Element {
    let ui = use_context::<Ui>();
    let path = doc.path.clone();
    let open = move |_| {
        if let Some(p) = path.clone() {
            ui.open(Pane::Doc(p));
        }
    };
    let kind = rsx! { Badge { text: doc.kind.clone(), tone: kind_tone(&doc.kind) } };
    if tile {
        return rsx! {
            Card { class: "doc", onclick: open,
                {kind}
                span { class: "nm", "{doc.name}" }
                span { class: "m", "{doc.meta}" }
            }
        };
    }
    rsx! {
        Row { title: doc.name.clone(), leading: kind, sub: rsx! { SubItem { text: doc.meta.clone() } }, onclick: open }
    }
}

#[component]
pub(super) fn ChecklistCard(list: Checklist, topic: String, index: usize, procedure: bool) -> Element {
    let ui = use_context::<Ui>();
    let note = if list.shopping { format!("{} · rolls up into Today › Shopping", list.meta) } else { list.meta.clone() };
    rsx! {
        Card { class: "proc",
            h4 { "{list.title}" }
            div { class: "m", "{note}" }
            for (j, (text, done)) in list.items.iter().enumerate() {
                CheckItem { text: text.clone(), checked: *done,
                    onclick: { let (topic, path) = (topic.clone(), list.path.clone()); move |_| toggle(ui, &topic, procedure, index, j, path.clone()) } }
            }
            if let Some(p) = list.path.clone() {
                Button { label: "Open in editor →", variant: Variant::Link, onclick: move |_| ui.open(Pane::Doc(p.clone())) }
            }
        }
    }
}

/// Toggle a checklist item. A list kept in a page is edited there; one
/// without a page changes in memory.
pub(super) fn toggle(ui: Ui, topic: &str, procedure: bool, index: usize, item: usize, path: Option<String>) {
    if let Some(p) = path {
        return ui.open(Pane::Doc(p));
    }
    let mut ws = ui.ws;
    let mut guard = ws.write();
    let Some(t) = guard.as_mut().and_then(|w| w.find_mut(topic)) else {
        return;
    };
    let lists = if procedure { &mut t.procedures } else { &mut t.lists };
    if let Some(entry) = lists.get_mut(index).and_then(|l| l.items.get_mut(item)) {
        entry.1 = !entry.1;
    }
}
