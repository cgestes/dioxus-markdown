//! The Topics window: the agent's space as a notebook, with its live adapter, demo data and CLI.
//! Topics: the agent's space as a notebook window.
//!
//! `osom` reads the live space (SilverBullet, over the tailnet) and
//! the agent's conversations (ZeroClaw). `osom --demo` shows the same
//! window with sample data and a stand-in agent. Both build a
//! `topics_view::Workspace` (see `topics_live`); everything here renders it
//! from `ui` components, so the two modes share one interface.
pub mod commands;
mod demo;
mod live;
mod view;
use margin_editor::CommentAgent;
use margin_editor::support::page::PageSource;
use crate::topics::live::{Backend, Live, load};
use crate::topics::view::*;
use margin_ui as ui;
use margin_ui::{
    self, Badge, Banner, Button, Card, CardTitle, CheckItem, Chip, Composer, DateBox, MarkdownView, Message,
    MessageStyle, NavItem, Row, Rows, ChannelGlyph, SubItem, TabItem, Tabs, TextField, Tile, TileSize, Tone, Variant,
};
use margin_editor::support::worker::{batch, turn, work};
use margin_editor::{Comments, HostAction, HostBar, NotebookOptions};
use dioxus_native::prelude::*;
use margin_editor_core::dates;
use margin_hub::{self as hub, Ask, ChatEvent, Endpoints, Page};
use osom_topics as model;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

/// The window's layout; components bring their own styles (`ui::CSS`).
const CSS: &str = include_str!("topics.css");

// ── Start ───────────────────────────────────────────────────────────────────

/// Where the window opens:
/// `osom [--demo] [--open <topic|today|inbox>] [--tab <name>] [--doc <path>]`.
#[derive(Clone, Debug, PartialEq)]
pub struct Start {
    demo: bool,
    place: Place,
    tab: Tab,
    doc: Option<String>,
}

impl Start {
    /// The window's start, from the parsed `osom` command line.
    pub fn from_cli(args: &crate::cli::TopicsArgs) -> Self {
        Self::build(args.demo, args.open.as_deref(), args.tab.unwrap_or(Tab::Overview), args.doc.as_deref())
    }

    fn build(demo: bool, open: Option<&str>, tab: Tab, doc: Option<&str>) -> Self {
        let place = match open.filter(|s| !s.is_empty()) {
            Some("today") => Place::Today,
            Some("inbox") => Place::Inbox,
            Some(id) => Place::Topic(id.into()),
            None if demo => Place::Topic("house".into()),
            None => Place::Today,
        };
        Self {
            demo,
            place,
            tab,
            doc: doc.filter(|d| !d.is_empty()).map(String::from),
        }
    }

    #[cfg(test)]
    fn new(demo: bool, open: Option<&str>, tab: Option<&str>, doc: Option<&str>) -> Self {
        use clap::ValueEnum;
        let tab = tab
            .and_then(|s| Tab::from_str(s, true).ok())
            .unwrap_or(Tab::Overview);
        Self::build(demo, open, tab, doc)
    }
}

/// Open `url` in the browser on a thread of its own, so the click returns at once.
fn open_url(url: &str) {
    let url = url.to_owned();
    std::thread::spawn(move || {
        let _ = webbrowser::open(&url);
    });
}

// ── Window state ────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
enum Place {
    Today,
    Inbox,
    Topic(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Tab {
    Overview,
    Discussions,
    Calendar,
    Questions,
    Docs,
    Procedures,
    Lists,
    Code,
}

#[derive(Clone, Debug, PartialEq)]
enum Pane {
    Tab,
    Doc(String),
    Thread(String, String),
}

/// The conversation in the chat panel. The reply being written is apart,
/// in `Ui::pending`, so a streamed chunk re-renders that reply only.
#[derive(Clone, Debug, Default)]
struct ChatState {
    /// Session these lines belong to.
    session: String,
    lines: Vec<ChatLine>,
    loading: bool,
    /// A tool the agent asks to run: (tool, summary).
    approval: Option<(String, String)>,
    /// Answers the agent's approval request for the turn in flight.
    reply: Option<std::sync::mpsc::Sender<bool>>,
    error: Option<String>,
}

#[derive(Clone, Copy)]
struct Ui {
    /// What the window talks to; `None` when Osom is not set up.
    backend: Signal<Option<Backend>>,
    ws: Signal<Option<Workspace>>,
    place: Signal<Place>,
    tab: Signal<Tab>,
    pane: Signal<Pane>,
    filter: Signal<Option<Source>>,
    filed: Signal<HashMap<String, String>>,
    error: Signal<Option<String>>,
    refresh: Callback<()>,
    chat: Signal<ChatState>,
    /// The agent's reply while it streams in.
    pending: Signal<Option<ChatLine>>,
    /// Transcripts by session key, read with the workspace.
    turns: Signal<HashMap<String, Vec<ChatLine>>>,
    /// Pages read with the workspace, by path: a doc opens from here at once.
    pages: Signal<HashMap<String, Page>>,
}

impl Ui {
    fn live(&self) -> Option<Live> {
        self.backend.read().as_ref()?.live().cloned()
    }

    fn demo(&self) -> bool {
        matches!(*self.backend.read(), Some(Backend::Demo))
    }

    /// The current topic's title and chat context.
    fn topic_brief(&self) -> Option<(String, String)> {
        let Place::Topic(id) = (self.place)() else {
            return None;
        };
        let ws = self.ws.read();
        let t = ws.as_ref()?.find(&id)?;
        Some((t.title.clone(), t.chat_context.clone()))
    }

    fn go(mut self, place: Place) {
        if place == Place::Today
            && *self.place.peek() == Place::Today
            && *self.pane.peek() == Pane::Tab
        {
            return;
        }
        self.place.set(place);
        self.tab.set(Tab::Overview);
        self.open(Pane::Tab);
        self.filter.set(None);
    }

    fn open(mut self, pane: Pane) {
        if pane == Pane::Tab && *self.place.peek() == Place::Today {
            self.refresh.call(());
        }
        self.pane.set(pane);
    }

    /// Show a tab of the topic page; any open doc or conversation closes.
    fn set_tab(mut self, tab: Tab) {
        self.tab.set(tab);
        self.pane.set(Pane::Tab);
    }

    fn session(&self) -> String {
        match (self.place)() {
            Place::Topic(id) => model::chat_session(&id),
            _ => model::GENERAL_CHAT.into(),
        }
    }

    async fn load_workspaces(mut self, mut requests: watch::Receiver<()>) {
        requests.mark_changed(); // Load once when the window opens.
        while requests.changed().await.is_ok() {
            let Some(live) = self.live() else { continue };
            let result = work(move || load(&live)).await;
            // Requests received during the read become one follow-up load.
            if requests.has_changed().unwrap_or(true) {
                continue;
            }
            match result {
                Ok(loaded) => {
                    self.ws.set(Some(loaded.ws));
                    self.turns.set(loaded.turns);
                    self.pages.set(loaded.pages);
                    self.error.set(loaded.warning);
                }
                Err(e) => self.error.set(Some(e)),
            }
        }
    }
}

/// A topic as shown beside an entry that belongs to it.
#[derive(Clone, Debug, PartialEq)]
struct TopicTag {
    id: String,
    title: String,
    icon: Icon,
}

impl From<&TopicView> for TopicTag {
    fn from(t: &TopicView) -> Self {
        Self {
            id: t.id.clone(),
            title: t.title.clone(),
            icon: t.icon,
        }
    }
}

// ── Root ────────────────────────────────────────────────────────────────────

#[component]
pub fn Shell(start: Start) -> Element {
    // No connection in the demo, or when Osom is not set up.
    let boot = use_hook(|| {
        if start.demo {
            return Ok(Backend::Demo);
        }
        Endpoints::load().map(|e| Backend::Live(Live::new(e)))
    });
    let (request, refreshes) = use_hook(|| watch::channel(()));
    let ui = Ui {
        ws: use_signal(|| boot.as_ref().ok().and_then(Backend::initial)),
        error: use_signal(|| boot.clone().err()),
        backend: use_signal(|| boot.clone().ok()),
        place: use_signal(|| start.place.clone()),
        tab: use_signal(|| start.tab),
        pane: use_signal(|| start.doc.clone().map_or(Pane::Tab, Pane::Doc)),
        filter: use_signal(|| None),
        filed: use_signal(HashMap::new),
        refresh: use_callback(move |()| request.send_replace(())),
        chat: use_signal(ChatState::default),
        pending: use_signal(|| None),
        turns: use_signal(HashMap::new),
        pages: use_signal(HashMap::new),
    };
    use_context_provider(|| ui);
    use_future(move || ui.load_workspaces(refreshes.clone()));
    // The chat panel follows the current place.
    use_effect(move || {
        let session = ui.session();
        let mut chat = ui.chat;
        if chat.peek().session == session {
            return;
        }
        let key = model::session_key(&session);
        let known = if ui.demo() {
            let ws = ui.ws.peek();
            match (ui.place)() {
                Place::Topic(id) => ws.as_ref().and_then(|w| w.find(&id)).map(|t| t.chat.clone()),
                _ => ws.as_ref().map(|w| w.general_chat.clone()),
            }
        } else {
            ui.turns.read().get(&key).cloned()
        };
        let fetch = known.is_none();
        chat.set(ChatState {
            session: session.clone(),
            lines: known.unwrap_or_default(),
            ..Default::default()
        });
        let Some(live) = ui.live().filter(|l| fetch && l.zc.paired()) else {
            return;
        };
        chat.write().loading = true;
        spawn(async move {
            let result = work(move || live.zc.messages(&key)).await;
            if chat.peek().session != session {
                return;
            }
            let mut c = chat.write();
            c.loading = false;
            match result {
                Ok(messages) => c.lines = model::transcript(&messages),
                Err(e) => c.error = Some(e),
            }
        });
    });
    rsx! {
        style { "{ui::CSS}" }
        style { "{CSS}" }
        div { class: "topics",
            Sidebar {}
            main { class: "main",
                if let Some(e) = ui.error.read().clone() { Banner { text: e, tone: Tone::Alert } }
                Center {}
            }
            ChatPanel {}
        }
    }
}

/// `https://box.tail0b.ts.net:8443` → `box :8443`.
fn host(url: &str) -> String {
    let hostport = url.split("://").nth(1).unwrap_or(url);
    let (host, port) = hostport.split_once(':').unwrap_or((hostport, ""));
    let name = host.split('.').next().unwrap_or(host);
    if port.is_empty() { name.into() } else { format!("{name} :{port}") }
}

fn date_box(iso: &str, today: &str) -> Element {
    let (top, day) = dates::chip(iso, today).unwrap_or_default();
    rsx! { DateBox { top, day } }
}

// ── Sidebar ─────────────────────────────────────────────────────────────────

#[component]
fn Sidebar() -> Element {
    let ui = use_context::<Ui>();
    let query = use_signal(String::new);
    let today_count = use_memo(move || {
        ui.ws.read().as_ref().map_or(0, |w| {
            upcoming_reminders(w, 7).count() + upcoming_key_dates(w, 7).count()
        })
    });
    let q = query.read().trim().to_lowercase();
    let ws = ui.ws.read();
    let place = (ui.place)();
    let inbox_left = ws.as_ref().map_or(0, |w| {
        let filed = ui.filed.read();
        w.inbox.iter().filter(|i| !filed.contains_key(&i.key)).count()
    });
    let row = |t: &TopicView, child: bool| {
        let id = t.id.clone();
        let on = place == Place::Topic(id.clone());
        let hint = if child { t.subtitle.clone() } else { t.next.clone() };
        let (icon, hue) = (t.icon, t.hue);
        rsx! {
            NavItem { key: "{t.id}", label: t.title.clone(), hint, dot: t.unread, selected: on, indent: child,
                leading: rsx! { Tile { icon, hue } },
                onclick: move |_| ui.go(Place::Topic(id.clone())) }
        }
    };
    let footer = match (ui.live(), ws.as_ref()) {
        (Some(l), _) => {
            let paired = l.zc.paired();
            rsx! {
                span { class: "sdot ok" } strong { "SilverBullet" } br {}
                {host(&l.endpoints.space)} br {}
                span { class: if paired { "sdot ok" } else { "sdot bad" } } strong { "ZeroClaw" } " · agent " strong { "{l.endpoints.agent}" } br {}
                if paired { {format!("{} · paired", host(&l.endpoints.zeroclaw))} } else { "not paired" }
            }
        }
        (None, Some(w)) => rsx! {
            strong { "ZeroClaw" } " · agent " strong { "{w.agent}" } br {}
            "{w.host}" br {}
            "demo data · not connected"
        },
        (None, None) => rsx! {},
    };
    rsx! {
        aside { class: "side",
            div { class: "search",
                {ui::icon(Icon::Search)}
                TextField { value: query, placeholder: "Search topics" }
            }
            NavItem { label: "Today", selected: place == Place::Today, count: (today_count(), Tone::Accent),
                leading: ui::icon(Icon::Calendar), onclick: move |_| ui.go(Place::Today) }
            NavItem { label: ws.as_ref().map(|w| w.inbox_title.clone()).unwrap_or_default(), selected: place == Place::Inbox,
                count: (inbox_left, Tone::Alert), leading: ui::icon(Icon::Inbox), onclick: move |_| ui.go(Place::Inbox) }
            div { class: "lbl", "Topics" }
            if let Some(w) = ws.as_ref() {
                if q.is_empty() {
                    for t in w.topics.iter() {
                        {row(t, false)}
                        for c in t.children.iter() { {row(c, true)} }
                    }
                } else {
                    for t in w.all().filter(|t| [&t.title, &t.subtitle, &t.id].iter().any(|s| s.to_lowercase().contains(&q))) {
                        {row(t, false)}
                    }
                }
            } else if ui.error.read().is_none() {
                div { class: "loading", "Loading the space…" }
            }
            div { class: "sfoot",
                {footer}
                Button { label: "Refresh", variant: Variant::Ghost, class: "refresh", onclick: ui.refresh }
            }
        }
    }
}

fn coming_days(w: &Workspace, days: i64) -> std::ops::Range<i64> {
    dates::parse_date(&w.today).map(|today| today..today + days).unwrap_or_default()
}

fn upcoming_key_dates(w: &Workspace, days: i64) -> impl Iterator<Item = &KeyDate> {
    let window = coming_days(w, days);
    w.key_dates.iter().filter(move |entry| window.contains(&entry.day))
}

/// Reminders due in the window, each paired with its owning topic.
fn upcoming_reminders(w: &Workspace, days: i64) -> impl Iterator<Item = (&Agenda, &TopicView)> {
    let window = coming_days(w, days);
    w.all()
        .flat_map(|topic| topic.agenda.iter().map(move |reminder| (reminder, topic)))
        .filter(move |(reminder, _)| {
            reminder.primary
                && dates::parse_date(&reminder.sort).is_some_and(|day| window.contains(&day))
        })
}

fn upcoming(w: &Workspace, days: i64) -> Vec<(Agenda, TopicTag)> {
    let mut rows: Vec<_> = upcoming_reminders(w, days)
        .map(|(reminder, topic)| (reminder.clone(), TopicTag::from(topic)))
        .collect();
    rows.sort_by(|a, b| a.0.sort.cmp(&b.0.sort));
    rows
}

// ── Center ──────────────────────────────────────────────────────────────────

#[component]
fn Center() -> Element {
    let ui = use_context::<Ui>();
    if ui.ws.read().is_none() {
        return rsx! { div { class: "empty", if ui.error.read().is_some() { "Nothing to show yet." } else { "Loading the space…" } } };
    }
    match ((ui.pane)(), (ui.place)()) {
        // Under a topic, they open below its header (see `TopicPage`).
        (Pane::Doc(path), Place::Today | Place::Inbox) => rsx! { DocPane { key: "{path}", path } },
        (Pane::Thread(key, title), Place::Today | Place::Inbox) => rsx! { TranscriptPane { key: "{key}", session_key: key, title } },
        (_, Place::Today) => rsx! { Today {} },
        (_, Place::Inbox) => rsx! { Inbox {} },
        (_, Place::Topic(id)) => match ui.ws.read().as_ref().and_then(|w| w.find(&id)).cloned() {
            Some(t) => rsx! { TopicPage { topic: t } },
            None => rsx! { div { class: "empty", "This topic is no longer in the space." } },
        },
    }
}

fn tabs(t: &TopicView) -> Vec<(Tab, &'static str, usize)> {
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
fn TopicPage(topic: TopicView) -> Element {
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
fn Overview(topic: TopicView) -> Element {
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
fn AgendaRow(agenda: Agenda, today: String, topic: Option<TopicTag>, #[props(default)] full: bool) -> Element {
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
fn ThreadItem(thread: ThreadRow, topic_title: Option<String>) -> Element {
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
fn Discussions(topic: TopicView) -> Element {
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
fn Questions(topic: TopicView) -> Element {
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

fn kind_tone(kind: &str) -> Tone {
    match kind {
        "PDF" => Tone::Alert,
        "IMG" => Tone::Info,
        _ => Tone::Accent,
    }
}

/// A document: a tile in the Docs tab, a row in the overview.
#[component]
fn DocEntry(doc: DocRow, tile: bool) -> Element {
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
fn ChecklistCard(list: Checklist, topic: String, index: usize, procedure: bool) -> Element {
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
fn toggle(ui: Ui, topic: &str, procedure: bool, index: usize, item: usize, path: Option<String>) {
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

/// An item of Today's shopping list.
#[derive(Clone, Debug, PartialEq)]
struct ShoppingItem {
    text: String,
    done: bool,
    topic: TopicTag,
    list: usize,
    item: usize,
    path: Option<String>,
}

/// What Today shows, derived from the workspace.
#[derive(Clone, Debug, PartialEq)]
struct TodayData {
    key_dates: Vec<KeyDate>,
    agenda: Vec<(Agenda, TopicTag)>,
    reminders_unavailable: bool,
    /// Unread threads when there are some, else the most recent ones.
    threads: Vec<(ThreadRow, String)>,
    unread: bool,
    shopping: Vec<ShoppingItem>,
    label: String,
    today: String,
    topics: usize,
}

fn today_data(w: &Workspace) -> TodayData {
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
        label: dates::long_date(&w.today),
        today: w.today.clone(),
        topics: w.topics.len(),
    }
}

#[component]
fn Today() -> Element {
    let ui = use_context::<Ui>();
    let data = use_memo(move || ui.ws.read().as_ref().map(today_data));
    let Some(d) = data() else { return rsx! {} };
    rsx! {
        div { class: "crumb", "{d.label}" }
        div { class: "head", div { class: "grow",
            h1 { "Today" }
            div { class: "meta",
                if !d.reminders_unavailable { "{d.agenda.len()} reminders in the next three weeks · " }
                "{d.threads.len()} recent discussions · across {d.topics} topics"
            }
        } }
        div { class: "ov spaced",
            div { class: "ov-col l",
                if !d.key_dates.is_empty() {
                    Card {
                        CardTitle { text: "Key dates", icon: Icon::Pin, action: "Open Planning →",
                            onaction: move |_| ui.open(Pane::Doc(model::PLANNING_PAGE.into())) }
                        Rows {
                            for entry in &d.key_dates {
                                div { class: "ui-row", MarkdownView { text: entry.markdown.clone(), compact: true } }
                            }
                        }
                    }
                }
                Card {
                    CardTitle { text: "Coming up", icon: Icon::Calendar }
                    if d.reminders_unavailable { div { class: "empty small", "Reminders are unavailable." } }
                    else if d.agenda.is_empty() { div { class: "empty small", "Nothing due in the next three weeks." } }
                    Rows {
                        for (a, t) in d.agenda.iter() {
                            AgendaRow { today: d.today.clone(), agenda: a.clone(), topic: Some(t.clone()) }
                        }
                    }
                }
                if !d.shopping.is_empty() {
                    Card { class: "proc",
                        CardTitle { text: "Shopping", icon: Icon::List, note: "rolled up from topic lists" }
                        for s in d.shopping.iter().cloned() {
                            CheckItem { text: s.text.clone(), checked: s.done, tag: s.topic.title.clone(),
                                onclick: move |_| toggle(ui, &s.topic.id, false, s.list, s.item, s.path.clone()) }
                        }
                    }
                }
            }
            div { class: "ov-col r",
                Card {
                    CardTitle { text: if d.unread { "New since yesterday" } else { "Recent discussions" }, icon: Icon::Chat }
                    Rows { for (th, title) in d.threads.iter() { ThreadItem { key: "{th.key}", thread: th.clone(), topic_title: Some(title.clone()) } } }
                }
            }
        }
    }
}

#[component]
fn Inbox() -> Element {
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
        div { class: "crumb", " " }
        div { class: "head", div { class: "grow",
            h1 { "{w.inbox_title}" }
            div { class: "meta", "{w.inbox_note}" }
        } }
        div { class: "stack",
            for item in w.inbox.iter() {
                Card { key: "{item.key}",
                    Row { title: item.text.clone(), trailing: item.when.clone(),
                        leading: rsx! { ChannelGlyph { channel: channel(item.source) } },
                        sub: rsx! { SubItem { text: format!("{} · {}", item.source.name(), item.place) } },
                        if let Some(to) = filed.get(&item.key) {
                            div { class: "filed", if to == "none" { "Dismissed — not a topic." } else { "Filed to {w.find(to).map(|t| t.title.clone()).unwrap_or_default()} ✓" } }
                        } else if !item.suggestions.is_empty() {
                            div { class: "sug",
                                for (k, (id, score)) in item.suggestions.iter().enumerate() {
                                    if let Some(t) = w.find(id) {
                                        Button { label: format!("File to {}", t.title), text: format!("{} · {score}", t.title), icon: t.icon,
                                            variant: if k == 0 { Variant::Primary } else { Variant::Default }, onclick: file(&item.key, id) }
                                    }
                                }
                                Button { label: "Not a topic", variant: Variant::Ghost, onclick: file(&item.key, "none") }
                            }
                        } else {
                            div { class: "sug",
                                Button { label: "Read",
                                    onclick: { let (key, place) = (item.key.clone(), item.place.clone()); move |_| ui.open(Pane::Thread(key.clone(), place.clone())) } }
                            }
                        }
                    }
                }
            }
            if w.inbox.is_empty() { div { class: "empty", "Every chat is filed under a topic." } }
        }
    }
}

// ── Panes ───────────────────────────────────────────────────────────────────

/// The notebook on a page of the space. A component, so it keeps its state
/// while the pane around it re-renders.
/// Where a pane's back button returns to.
fn back_label(ui: &Ui) -> &'static str {
    match ((ui.place)(), (ui.tab)()) {
        (Place::Today, _) => "Today",
        (Place::Inbox, _) => "Inbox",
        (Place::Topic(_), Tab::Overview) => "Overview",
        (Place::Topic(_), Tab::Discussions) => "Discussions",
        (Place::Topic(_), Tab::Lists) => "Lists",
        (Place::Topic(_), Tab::Procedures) => "Procedures",
        (Place::Topic(_), _) => "Docs",
    }
}

#[component]
fn PageEditor(text: String, options: NotebookOptions) -> Element {
    margin_editor::notebook_view(&text, options)
}

/// A page in the editor, below the topic's header. It opens at once from the
/// pages read with the workspace (or the demo's), then checks the server's
/// version; a newer one replaces it before you type.
#[component]
fn DocPane(path: String) -> Element {
    let ui = use_context::<Ui>();
    let mut opened = use_signal({
        let path = path.clone();
        move || match ui.live() {
            None => Some(demo::page(&path).map(|text| Page { path: path.clone(), text: text.into(), etag: None }).ok_or_else(|| "This demo document has no content.".to_string())),
            Some(_) => ui.pages.peek().get(&path).cloned().map(Ok),
        }
    });
    {
        let path = path.clone();
        use_hook(move || {
            let Some(live) = ui.live() else { return };
            spawn(async move {
                let space = live.space.clone();
                let read = work(move || space.read(&path)).await;
                let mut ui = ui;
                match read {
                    Ok(page) => {
                        let changed = opened.peek().as_ref().is_none_or(|o| o.as_ref().map_or(true, |p| p.etag != page.etag));
                        ui.pages.write().insert(page.path.clone(), page.clone());
                        if changed {
                            opened.set(Some(Ok(page)));
                        }
                    }
                    // A copy from the load is still worth showing.
                    Err(e) if opened.peek().is_none() => opened.set(Some(Err(e))),
                    Err(_) => {}
                }
            });
        });
    }
    let back_to = back_label(&ui);
    let onback = use_callback(move |_| ui.open(Pane::Tab));
    // Under a topic, the header's SilverBullet button already opens this page.
    let web = ui.live().filter(|_| !matches!((ui.place)(), Place::Topic(_))).map(|l| l.space.page_url(&path));
    let open_web = use_callback({
        let web = web.clone();
        move |_| if let Some(url) = &web { open_url(url) }
    });
    let host = HostBar {
        back: back_to.into(),
        onback,
        crumb: model::basename(&path).into(),
        actions: web.map(|_| HostAction { label: "SilverBullet".into(), icon: Icon::Open, onclick: open_web }).into_iter().collect(),
    };
    match opened() {
        None => rsx! { div { class: "ui-panebar", Button { label: back_to, icon: Icon::Back, text: back_to, variant: Variant::Ghost, onclick: move |_| ui.open(Pane::Tab) } span { class: "dock-crumb", "{host.crumb}" } } },
        Some(Err(e)) => rsx! { Banner { text: e, tone: Tone::Alert } },
        Some(Ok(page)) => {
            let comments = match ui.live() {
                Some(live) if live.zc.paired() => Comments::Agent(CommentAgent { alias: live.endpoints.comment_agent.clone(), chat: Arc::new(live.zc.clone()) }),
                Some(live) => Comments::Agent(CommentAgent::unavailable(&live.endpoints.comment_agent, "Osom is not paired with ZeroClaw: run `osom pair <code>`.")),
                None => Comments::Empty,
            };
            let source = ui.live().map(|live| PageSource { path: page.path.clone(), space: live.space.clone(), etag: page.etag.clone() });
            let key = format!("{}@{}", page.path, page.etag.clone().unwrap_or_default());
            let options = NotebookOptions { comments, page: source, host: Some(host), ..Default::default() };
            rsx! { div { class: "editor-host", PageEditor { key: "{key}", text: page.text.clone(), options } } }
        }
    }
}

/// A stored conversation: the transcript read with the workspace, or read
/// now when it was not. Keyed by session.
#[component]
fn TranscriptPane(session_key: String, title: String) -> Element {
    let ui = use_context::<Ui>();
    let known = ui.turns.read().get(&session_key).cloned();
    let mut fetched = use_signal(|| None::<Result<Vec<ChatLine>, String>>);
    {
        let key = session_key.clone();
        let missing = known.is_none();
        use_hook(move || {
            let Some(live) = ui.live().filter(|l| missing && l.zc.paired()) else {
                return;
            };
            spawn(async move {
                let result = work(move || live.zc.messages(&key).map(|m| model::transcript(&m))).await;
                fetched.set(Some(result));
            });
        });
    }
    let turns = match known {
        Some(t) => Some(Ok(t)),
        None if ui.live().is_none() => Some(Ok(Vec::new())),
        None => fetched(),
    };
    rsx! {
        div { class: "ui-panebar",
            Button { label: back_label(&ui), icon: Icon::Back, text: back_label(&ui), variant: Variant::Ghost, onclick: move |_| ui.open(Pane::Tab) }
            span { class: "dock-crumb", "{title}" }
            span { class: "dock-spacer" }
            span { class: "dock-note", "{session_key}" }
        }
        match turns {
            None => rsx! { div { class: "loading", "Loading the conversation…" } },
            Some(Err(e)) => rsx! { Banner { text: e, tone: Tone::Alert } },
            Some(Ok(turns)) => rsx! {
                div { class: "transcript",
                    if turns.is_empty() { div { class: "empty", "No messages." } }
                    for (i, t) in turns.into_iter().enumerate() {
                        Message { key: "{i}", author: if t.from_user { "you" } else { "agent" }, body: t.text, ai: !t.from_user,
                            tools: t.tools, at: (!t.at.is_empty()).then_some(t.at), style: MessageStyle::Card }
                    }
                }
            },
        }
    }
}

// ── Chat ────────────────────────────────────────────────────────────────────

/// Send a message in the current place's chat; the reply streams into
/// `Ui::pending` and joins the lines when it is done.
fn send(ui: Ui, text: String) {
    let text = text.trim().to_string();
    let (mut chat, mut pending) = (ui.chat, ui.pending);
    if text.is_empty() || pending.peek().is_some() {
        return;
    }
    let Some(backend) = ui.backend.peek().clone() else {
        return;
    };
    let session = ui.session();
    let (name, context) = ui.topic_brief().unwrap_or_else(|| ("General".into(), String::new()));
    let (reply, decisions) = std::sync::mpsc::channel::<bool>();
    {
        let mut c = chat.write();
        c.lines.push(ChatLine {
            from_user: true,
            text: text.clone(),
            ..Default::default()
        });
        c.reply = Some(reply);
        c.error = None;
    }
    pending.set(Some(ChatLine::default()));
    let ask = Ask {
        agent: backend.alias(),
        session: session.clone(),
        name: format!("Margin · {name}"),
        content: text,
        // The agent learns what a fresh topic chat is about before its first message.
        preamble: (!context.is_empty()).then(|| model::preamble(&context)),
    };
    let mut events = turn(backend.agent(), ask, move || {
        decisions.recv_timeout(Duration::from_secs(300)).unwrap_or(false)
    });
    spawn(async move {
        while let Some(events) = batch(&mut events).await {
            for e in events {
                match e {
                    ChatEvent::Chunk(s) => {
                        if let Some(p) = pending.write().as_mut() {
                            p.text.push_str(&s);
                        }
                    }
                    ChatEvent::Tool(name) => {
                        if let Some(p) = pending.write().as_mut() {
                            p.tools.push(name);
                        }
                    }
                    ChatEvent::Approval { tool, summary } => chat.write().approval = Some((tool, summary)),
                    ChatEvent::Done(full) => {
                        let streamed = pending.take().unwrap_or_default();
                        let text = if full.trim().is_empty() { streamed.text } else { full };
                        if !text.trim().is_empty() && chat.peek().session == session {
                            chat.write().lines.push(ChatLine {
                                text,
                                tools: streamed.tools,
                                ..Default::default()
                            });
                        }
                    }
                    ChatEvent::Error(e) => {
                        pending.set(None);
                        chat.write().error = Some(e);
                    }
                    _ => {}
                }
            }
        }
        pending.set(None);
        let mut c = chat.write();
        c.approval = None;
        c.reply = None;
        if c.session == session {
            let lines = c.lines.clone();
            drop(c);
            let mut turns = ui.turns;
            turns.write().insert(model::session_key(&session), lines);
        }
    });
}

/// The reply while it streams in: plain text until it is done.
#[component]
fn PendingReply() -> Element {
    let ui = use_context::<Ui>();
    let pending = ui.pending.read();
    let Some(p) = pending.as_ref() else {
        return rsx! {};
    };
    rsx! { Message { author: "agent", body: p.text.clone(), ai: true, pending: true, tools: p.tools.clone() } }
}

#[component]
fn ChatPanel() -> Element {
    let ui = use_context::<Ui>();
    let brief = use_memo(move || ui.topic_brief());
    let session = use_memo(move || ui.session());
    let (name, context) = brief().unwrap_or_else(|| ("General".into(), "General · all topics".into()));
    let agent = ui.ws.read().as_ref().map(|w| w.agent.clone()).unwrap_or_default();
    let paired = ui.backend.read().as_ref().is_some_and(Backend::paired);
    let busy = ui.pending.read().is_some();
    let actions: &[&str] = if brief.read().is_some() {
        &["Summarize this week", "Add reminder", "What’s next?"]
    } else {
        &["Plan my week", "What’s overdue?"]
    };
    let decide = move |yes: bool| {
        move |_| {
            let mut chat = ui.chat;
            if let Some(reply) = chat.peek().reply.as_ref() {
                let _ = reply.send(yes);
            }
            chat.write().approval = None;
        }
    };
    let chat = ui.chat.read();
    rsx! {
        section { class: "chat",
            div { class: "chead",
                div { class: "t", {ui::icon(Icon::Chat)} "Chat · {name}"
                    span { class: "live", class: if busy { "on" }, b {} if busy { "streaming" } else { "idle" } }
                }
                div { class: "s", "session {session} · agent {agent} · /ws/chat" }
            }
            div { class: "msgs",
                div { class: "ctx", {ui::icon(Icon::Spark)} "Context — {context}" }
                if chat.loading { div { class: "loading", "Loading the conversation…" } }
                for (i, l) in chat.lines.iter().enumerate() {
                    Message { key: "{i}", author: "agent", body: l.text.clone(), ai: !l.from_user, tools: l.tools.clone() }
                }
                PendingReply {}
                if let Some((tool, summary)) = chat.approval.as_ref() {
                    div { class: "approval",
                        Banner { text: if summary.is_empty() { format!("The agent wants to run {tool}.") } else { format!("The agent wants to run {tool}: {summary}") }, tone: Tone::Warn }
                        div { class: "row-btns",
                            Button { label: "Approve", variant: Variant::Primary, onclick: decide(true) }
                            Button { label: "Deny", onclick: decide(false) }
                        }
                    }
                }
                if let Some(e) = chat.error.clone() { Banner { text: e, tone: Tone::Alert } }
            }
            if paired {
                div { class: "qa",
                    for q in actions.iter().copied() { Chip { label: q, onclick: move |_| send(ui, q.to_string()) } }
                }
                div { class: "comp",
                    Composer { placeholder: format!("Ask about {name}…"), multiline: true, onsend: move |text: String| send(ui, text) }
                }
            } else if ui.live().is_some() {
                Pairing {}
            }
        }
    }
}

#[component]
fn Pairing() -> Element {
    let ui = use_context::<Ui>();
    let mut code = use_signal(String::new);
    let pair = use_callback(move |()| {
        let Some(l) = ui.live() else { return };
        let c = code.peek().trim().to_string();
        if c.is_empty() {
            return;
        }
        let mut chat = ui.chat;
        chat.write().error = Some("Pairing…".into());
        spawn(async move {
            let endpoints = l.endpoints.clone();
            match work(move || hub::pair(&endpoints, &c)).await {
                Ok(zc) => {
                    let mut backend = ui.backend;
                    if let Some(Backend::Live(l)) = backend.write().as_mut() {
                        l.zc = zc;
                    }
                    chat.set(ChatState::default());
                    code.set(String::new());
                    ui.refresh.call(());
                }
                Err(e) => chat.write().error = Some(e),
            }
        });
    });
    rsx! {
        div { class: "pair",
            p { "Pair this Mac with ZeroClaw to read the agent’s conversations and chat. On the server run " code { "zeroclaw gateway get-paircode --new" } "." }
            div { class: "pair-field", TextField { value: code, placeholder: "Pairing code", onenter: pair } }
            Button { label: "Pair", variant: Variant::Primary, onclick: move |_| pair(()) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Paint the window for each view into `target/topics-shots`.
    /// `cargo test -p osom topics_shots -- --ignored --nocapture`
    /// Set `MARGIN_TOPICS_LIVE=1` to shoot the live space instead of the demo.
    #[test]
    #[ignore = "writes screenshots; live mode needs the tailnet"]
    fn topics_shots() {
        use blitz_dom::Document;
        use blitz_traits::shell::Viewport;
        use dioxus_native::DioxusDocument;
        let live = std::env::var("MARGIN_TOPICS_LIVE").is_ok_and(|v| v == "1");
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).expect("crates/<name> sits two levels below the workspace").join("target/topics-shots");
        std::fs::create_dir_all(&dir).unwrap();
        let views: &[(&str, &str, &str, &str)] = if live {
            &[
                ("live-today", "today", "", ""),
                ("live-car", "car", "", ""),
                ("live-wedding-questions", "wedding", "questions", ""),
                ("live-unfiled", "inbox", "", ""),
                ("live-doc", "car", "", "car/maintenance.md"),
            ]
        } else {
            &[
                ("demo-house", "house", "", ""),
                ("demo-today", "today", "", ""),
                ("demo-inbox", "inbox", "", ""),
                ("demo-mazda-calendar", "mazda", "calendar", ""),
                ("demo-nikki-lists", "nikki", "lists", ""),
                ("demo-hot-water-code", "house/hot-water", "code", ""),
                ("demo-wedding-discussions", "wedding", "discussions", ""),
                ("demo-house-docs", "house", "docs", ""),
                ("demo-doc", "house", "docs", "topics/house/house-systems.md"),
            ]
        };
        for (name, open, tab, doc) in views {
            let start = Start::new(!live, Some(open), Some(tab), Some(doc));
            let (w, h) = (1320u32, 860u32);
            let config = dioxus_native::DocumentConfig {
                font_ctx: Some(margin_ui::typefaces::notebook_fonts()),
                viewport: Some(Viewport { window_size: (w, h), ..Default::default() }),
                ..Default::default()
            };
            let mut doc = DioxusDocument::new(VirtualDom::new_with_props(Shell, ShellProps { start }), config);
            doc.initial_build();
            let settle = Duration::from_millis(if live { 4000 } else { 300 });
            let started = std::time::Instant::now();
            while started.elapsed() < settle {
                doc.poll(None);
                doc.inner.borrow_mut().resolve(0.0);
                std::thread::sleep(Duration::from_millis(40));
            }
            let pixels = anyrender::render_to_buffer::<anyrender_vello_cpu::VelloCpuImageRenderer, _>(
                |scene| blitz_paint::paint_scene(scene, &mut doc.inner.borrow_mut(), 1.0, w, h, 0, 0),
                w,
                h,
            );
            let path = dir.join(format!("{name}.png"));
            let mut enc = png::Encoder::new(std::fs::File::create(&path).unwrap(), w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(&pixels).unwrap();
            println!("{}", path.display());
        }
    }

    #[test]
    fn start_options_choose_where_the_window_opens() {
        let s = Start::new(true, None, None, None);
        assert_eq!((s.place, s.tab, s.doc), (Place::Topic("house".into()), Tab::Overview, None));
        let s = Start::new(false, Some("inbox"), Some("questions"), Some("car/log.md"));
        assert_eq!((s.place, s.tab, s.doc.as_deref()), (Place::Inbox, Tab::Questions, Some("car/log.md")));
    }

    #[test]
    fn returning_to_today_requests_a_refresh_once_per_navigation() {
        use dioxus_native::DioxusDocument;

        let start = Start::new(true, Some("today"), None, None);
        let dom = VirtualDom::new_with_props(Shell, ShellProps { start });
        let mut doc = DioxusDocument::new(dom, Default::default());
        doc.initial_build();
        doc.vdom.in_scope(ScopeId::APP, || {
            let mut ui = consume_context::<Ui>();
            let mut requests = Signal::new(0);
            ui.refresh = Callback::new(move |()| *requests.write() += 1);

            ui.go(Place::Today);
            ui.go(Place::Inbox);
            assert_eq!(*requests.peek(), 0);
            ui.go(Place::Today);
            ui.go(Place::Today);
            assert_eq!(*requests.peek(), 1);
            ui.open(Pane::Doc(model::PLANNING_PAGE.into()));
            ui.open(Pane::Tab);
            assert_eq!(*requests.peek(), 2);
        });
    }

    #[test]
    fn today_shows_planning_dates_even_without_a_deadline_reminder() {
        let md = "## Key dates\n- 2031-04-07 Old\n- 2031-04-08 Appointment\n- 2031-04-14 Visit\n- 2031-04-28 Concert\n- 2031-04-29 Later";
        let w = Workspace {
            today: "2031-04-08".into(),
            key_dates: model::key_dates(md),
            ..Default::default()
        };
        let today = today_data(&w);
        assert!(today.agenda.is_empty());
        assert_eq!(today.key_dates.iter().map(|d| d.markdown.as_str()).collect::<Vec<_>>(),
            ["2031-04-08 Appointment", "2031-04-14 Visit", "2031-04-28 Concert"]);
        assert_eq!(upcoming_key_dates(&w, 7).count(), 2);
    }

    #[test]
    fn demo_workspace_has_every_designed_topic() {
        let w = crate::topics::demo::workspace();
        let ids: Vec<_> = w.all().map(|t| t.id.clone()).collect();
        for id in ["mazda", "wedding", "house", "house/hot-water", "house/gutters", "modular", "people", "nikki"] {
            assert!(ids.contains(&id.to_string()), "missing {id}");
        }
        assert_eq!(w.inbox.len(), 3);
        let soon: Vec<_> = upcoming(&w, 7).into_iter().map(|(a, _)| a.title).collect();
        assert_eq!(soon.len(), 4, "plumber, Black Friday, flowers, cylinder check, once each: {soon:?}");
    }
}
