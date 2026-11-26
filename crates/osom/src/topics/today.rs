//! The Today and Inbox views: what is coming up across every topic.
use super::*;

pub(super) fn coming_days(w: &Workspace, days: i64) -> std::ops::Range<i64> {
    dates::parse_date(&w.today).map(|today| today..today + days).unwrap_or_default()
}

pub(super) fn upcoming_key_dates(w: &Workspace, days: i64) -> impl Iterator<Item = &KeyDate> {
    let window = coming_days(w, days);
    w.key_dates.iter().filter(move |entry| window.contains(&entry.day))
}

/// Reminders due in the window, each paired with its owning topic.
pub(super) fn upcoming_reminders(w: &Workspace, days: i64) -> impl Iterator<Item = (&Agenda, &TopicView)> {
    let window = coming_days(w, days);
    w.all()
        .flat_map(|topic| topic.agenda.iter().map(move |reminder| (reminder, topic)))
        .filter(move |(reminder, _)| {
            reminder.primary
                && dates::parse_date(&reminder.sort).is_some_and(|day| window.contains(&day))
        })
}

pub(super) fn upcoming(w: &Workspace, days: i64) -> Vec<(Agenda, TopicTag)> {
    let mut rows: Vec<_> = upcoming_reminders(w, days)
        .map(|(reminder, topic)| (reminder.clone(), TopicTag::from(topic)))
        .collect();
    rows.sort_by(|a, b| a.0.sort.cmp(&b.0.sort));
    rows
}

/// An item of Today's shopping list.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ShoppingItem {
    text: String,
    done: bool,
    topic: TopicTag,
    list: usize,
    item: usize,
    path: Option<String>,
}

/// What Today shows, derived from the workspace.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct TodayData {
    pub(super) key_dates: Vec<KeyDate>,
    pub(super) agenda: Vec<(Agenda, TopicTag)>,
    reminders_unavailable: bool,
    /// Unread threads when there are some, else the most recent ones.
    threads: Vec<(ThreadRow, String)>,
    unread: bool,
    shopping: Vec<ShoppingItem>,
    label: String,
    today: String,
    topics: usize,
}

pub(super) fn today_data(w: &Workspace) -> TodayData {
    let mut threads: Vec<(ThreadRow, String)> = w
        .all()
        .flat_map(|t| t.threads.iter().map(move |x| (x.clone(), t.title.clone())))
        .collect();
    threads.sort_by(|a, b| b.0.sort.cmp(&a.0.sort));
    let mut seen = std::collections::HashSet::new();
    threads.retain(|x| seen.insert(x.0.key.clone()));
    let unread = threads.iter().any(|x| x.0.unread);
    if unread {
        threads.retain(|x| x.0.unread);
    } else {
        threads.truncate(6);
    }
    let shopping = w
        .all()
        .flat_map(|t| {
            t.lists.iter().enumerate().filter(|(_, l)| l.shopping).flat_map(move |(i, l)| {
                l.items.iter().enumerate().map(move |(j, (text, done))| ShoppingItem {
                    text: text.clone(),
                    done: *done,
                    topic: TopicTag::from(t),
                    list: i,
                    item: j,
                    path: l.path.clone(),
                })
            })
        })
        .collect();
    TodayData {
        key_dates: upcoming_key_dates(w, 21).cloned().collect(),
        agenda: upcoming(w, 21),
        reminders_unavailable: w.reminders_unavailable,
        threads,
        unread,
        shopping,
        label: calendar::long_date(&w.today),
        today: w.today.clone(),
        topics: w.topics.len(),
    }
}

#[component]
pub(super) fn Today() -> Element {
    let ui = use_context::<Ui>();
    let data = use_memo(move || ui.ws.read().as_ref().map(today_data));
    let Some(d) = data() else { return rsx! {} };
    rsx! {
        div { class: CRUMB, "{d.label}" }
        div { class: "flex items-center gap-3.5", div { class: "flex-auto min-w-0",
            h1 { class: PAGE_H1, "Today" }
            div { class: PAGE_META,
                if !d.reminders_unavailable { "{d.agenda.len()} reminders in the next three weeks · " }
                "{d.threads.len()} recent discussions · across {d.topics} topics"
            }
        } }
        div { class: "flex flex-row items-start gap-3.5 mt-4 max-[1080px]:flex-col max-[1080px]:items-stretch",
            div { class: "flex flex-col gap-3.5 min-w-0 flex-[1.25_1_0]",
                if !d.key_dates.is_empty() {
                    Card {
                        CardTitle { text: "Key dates", icon: Icon::Pin, action: "Open Planning →",
                            onaction: move |_| ui.open(Pane::Doc(model::PLANNING_PAGE.into())) }
                        Rows {
                            for entry in &d.key_dates {
                                div { class: "ui-row py-2 border-0 border-t border-solid border-line", MarkdownView { text: entry.markdown.clone(), compact: true } }
                            }
                        }
                    }
                }
                Card {
                    CardTitle { text: "Coming up", icon: Icon::Calendar }
                    if d.reminders_unavailable { div { class: EMPTY_SMALL, "Reminders are unavailable." } }
                    else if d.agenda.is_empty() { div { class: EMPTY_SMALL, "Nothing due in the next three weeks." } }
                    Rows {
                        for (a, t) in d.agenda.iter() {
                            AgendaRow { today: d.today.clone(), agenda: a.clone(), topic: Some(t.clone()) }
                        }
                    }
                }
                if !d.shopping.is_empty() {
                    Card {
                        CardTitle { text: "Shopping", icon: Icon::List, note: "rolled up from topic lists" }
                        for s in d.shopping.iter().cloned() {
                            CheckItem { text: s.text.clone(), checked: s.done, tag: s.topic.title.clone(),
                                onclick: move |_| toggle(ui, &s.topic.id, false, s.list, s.item, s.path.clone()) }
                        }
                    }
                }
            }
            div { class: "flex flex-col gap-3.5 min-w-0 flex-[1_1_0]",
                Card {
                    CardTitle { text: if d.unread { "New since yesterday" } else { "Recent discussions" }, icon: Icon::Chat }
                    Rows { for (th, title) in d.threads.iter() { ThreadItem { key: "{th.key}", thread: th.clone(), topic_title: Some(title.clone()) } } }
                }
            }
        }
    }
}

#[component]
pub(super) fn Inbox() -> Element {
    let ui = use_context::<Ui>();
    let ws = ui.ws.read();
    let Some(w) = ws.as_ref() else { return rsx! {} };
    let filed = ui.filed.read();
    let file = move |key: &str, to: &str| {
        let (key, to) = (key.to_string(), to.to_string());
        move |_| {
            let mut f = ui.filed;
            f.write().insert(key.clone(), to.clone());
        }
    };
    rsx! {
        div { class: CRUMB, " " }
        div { class: "flex items-center gap-3.5", div { class: "flex-auto min-w-0",
            h1 { class: PAGE_H1, "{w.inbox_title}" }
            div { class: PAGE_META, "{w.inbox_note}" }
        } }
        div { class: "flex flex-col gap-3.5 [&_.ui-row-title]:text-[15px]",
            for item in w.inbox.iter() {
                Card { key: "{item.key}",
                    Row { title: item.text.clone(), trailing: item.when.clone(),
                        leading: rsx! { ChannelGlyph { channel: channel(item.source) } },
                        sub: rsx! { SubItem { text: format!("{} · {}", item.source.name(), item.place) } },
                        if let Some(to) = filed.get(&item.key) {
                            div { class: "mt-2 text-accent text-[12px]", if to == "none" { "Dismissed — not a topic." } else { "Filed to {w.find(to).map(|t| t.title.clone()).unwrap_or_default()} ✓" } }
                        } else if !item.suggestions.is_empty() {
                            div { class: "flex flex-wrap gap-1.5 mt-2",
                                for (k, (id, score)) in item.suggestions.iter().enumerate() {
                                    if let Some(t) = w.find(id) {
                                        Button { label: format!("File to {}", t.title), text: format!("{} · {score}", t.title), icon: t.icon,
                                            variant: if k == 0 { Variant::Primary } else { Variant::Default }, onclick: file(&item.key, id) }
                                    }
                                }
                                Button { label: "Not a topic", variant: Variant::Ghost, onclick: file(&item.key, "none") }
                            }
                        } else {
                            div { class: "flex flex-wrap gap-1.5 mt-2",
                                Button { label: "Read",
                                    onclick: { let (key, place) = (item.key.clone(), item.place.clone()); move |_| ui.open(Pane::Thread(key.clone(), place.clone())) } }
                            }
                        }
                    }
                }
            }
            if w.inbox.is_empty() { div { class: EMPTY, "Every chat is filed under a topic." } }
        }
    }
}
