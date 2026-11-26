//! The Topics window: the agent's space as a notebook.
//!
//! `osom` reads the live space (SilverBullet, over the tailnet) and the
//! agent's conversations (ZeroClaw); `osom --demo` shows the same window
//! with sample data and a stand-in agent. Both build a `view::Workspace`
//! (`live`, `demo`), and the views render it from `ui` components, so the
//! two modes share one interface. The window, its state and the sidebar
//! are here; `today` has Today and the Inbox, `topic` a topic's page,
//! `pages` a document or transcript in its own pane, and `chat` the
//! agent's chat and pairing.
pub mod commands;
mod demo;
mod live;
mod view;
mod chat;
mod pages;
mod today;
mod topic;
use margin_editor::CommentAgent;
use margin_editor::support::page::PageSource;
use crate::topics::live::{Backend, Live, load};
use crate::topics::view::*;
use chat::*;
use pages::*;
use today::*;
use topic::*;
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
