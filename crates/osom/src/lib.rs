//! Osom: the agent's topics, a window of their own over the editor. The
//! Topics window shows the agent's space as a notebook.
//!
//! `osom` reads the space and the agent's conversations (ZeroClaw) into a
//! `view::Workspace`, and the views render it from `ui` components. The window,
//! its state and the sidebar are here; `today` has Today and the Inbox, `topic`
//! a topic's page, `pages` a document or transcript in its own pane, and `chat`
//! the agent's chat and pairing.
pub mod cli;
pub mod commands;
mod live;
mod view;
mod chat;
mod pages;
mod today;
mod topic;
use margin_editor::CommentAgent;
use crate::live::{Live, load};
use crate::view::*;
use chat::*;
use pages::*;
use today::*;
use topic::*;
use margin_ui as ui;
use margin_ui::{
    self, Badge, Banner, Button, Card, CardTitle, CheckItem, Chip, Composer, DateBox, Message,
    MessageStyle, NavItem, Row, Rows, ChannelGlyph, SubItem, TabItem, Tabs, TextField, Tile, TileSize, Tone, Variant,
};
use margin_runtime::worker::{batch, turn, work};
use margin_editor::{Comments, Host, HostAction, HostBar, MarkdownView, NotebookOptions, View};
use margin_ui::prelude::*;
use margin_core::dates;
use osom_hub::{self as hub, Ask, ChatEvent, Endpoints, Page};
use osom_topics as model;
use osom_topics::calendar;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

// ── Shared layout classes ───────────────────────────────────────────────────
const CRUMB: &str = "min-h-4 mb-1 text-faint text-[11.5px]";
const PAGE_H1: &str = "m-0 text-ink font-display text-[34px]/[1.1] font-normal";
const PAGE_META: &str = "mt-[3px] text-faint text-[12px]";
const EMPTY: &str = "py-7 px-4 border border-dashed border-line-strong rounded-lg text-faint text-center";
const EMPTY_SMALL: &str = "p-3 border border-dashed border-line-strong rounded-lg text-faint text-center";
const TWO_COL: &str = "flex flex-wrap items-start gap-3.5 [&>.ui-card]:flex-none [&>.ui-card]:w-[calc(50%-7px)] max-[1080px]:[&>.ui-card]:w-full";
const PANEBAR: &str = "sticky top-0 z-[6] flex items-center justify-start gap-2 min-h-10 pb-2 bg-page border-0 border-b border-solid border-line";
const PANEBAR_CRUMB: &str = "min-w-0 overflow-hidden text-muted type-mono-ui whitespace-nowrap";

// ── Start ───────────────────────────────────────────────────────────────────

/// Where the window opens:
/// `osom [--open <topic|today|inbox>] [--tab <name>] [--doc <path>]`.
#[derive(Clone, Debug, PartialEq)]
pub struct Start {
    place: Place,
    tab: Tab,
    doc: Option<String>,
}

impl Start {
    /// The window's start, from the parsed `osom` command line.
    pub fn from_cli(args: &crate::cli::TopicsArgs) -> Self {
        Self::build(args.open.as_deref(), args.tab.unwrap_or(Tab::Overview), args.doc.as_deref())
    }

    fn build(open: Option<&str>, tab: Tab, doc: Option<&str>) -> Self {
        let place = match open.filter(|s| !s.is_empty()) {
            Some("today") => Place::Today,
            Some("inbox") => Place::Inbox,
            Some(id) => Place::Topic(id.into()),
            None => Place::Today,
        };
        Self {
            place,
            tab,
            doc: doc.filter(|d| !d.is_empty()).map(String::from),
        }
    }

    #[cfg(test)]
    fn new(open: Option<&str>, tab: Option<&str>, doc: Option<&str>) -> Self {
        use clap::ValueEnum;
        let tab = tab
            .and_then(|s| Tab::from_str(s, true).ok())
            .unwrap_or(Tab::Overview);
        Self::build(open, tab, doc)
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
    live: Signal<Option<Live>>,
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
        self.live.read().clone()
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
    let boot = use_hook(|| Endpoints::load().map(Live::new));
    let (request, refreshes) = use_hook(|| watch::channel(()));
    let ui = Ui {
        ws: use_signal(|| None),
        error: use_signal(|| boot.clone().err()),
        live: use_signal(|| boot.clone().ok()),
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
        let known = ui.turns.read().get(&key).cloned();
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
        div { class: "flex flex-row w-full h-screen overflow-hidden font-sans text-[13px]/[1.45] font-normal text-ink bg-page",
            Sidebar {}
            main { class: "flex-auto min-w-0 h-full pt-4 px-[26px] pb-8 overflow-y-auto [&>.ui-banner]:mb-3",
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

/// How many characters of a hint's mono type fit beside a topic's tile in
/// the rail. A nested topic has 2 fewer and an unread dot takes 3.
const HINT_CHARS: usize = 23;

/// `hint` in at most `n` characters, ending in "…" when it is cut. A
/// trailing `· date` stays whole; the text before it gives way.
fn fit(hint: &str, n: usize) -> String {
    if hint.chars().count() <= n {
        return hint.into();
    }
    let (text, tail) = match hint.rsplit_once(" · ") {
        Some((text, when)) if when.chars().count() < n / 2 => (text, format!(" · {when}")),
        _ => (hint, String::new()),
    };
    let cut: String = text.chars().take(n.saturating_sub(tail.chars().count() + 1)).collect();
    let cut = cut.trim_end_matches(|c: char| c.is_whitespace() || ",;:(-—–+&".contains(c));
    format!("{cut}…{tail}")
}

fn date_box(iso: &str, today: &str) -> Element {
    let (top, day) = calendar::chip(iso, today).unwrap_or_default();
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
        let room = if child { HINT_CHARS - 2 } else { HINT_CHARS } - if t.unread { 3 } else { 0 };
        let hint = fit(if child { &t.subtitle } else { &t.next }, room);
        let (icon, hue) = (t.icon, t.hue);
        rsx! {
            NavItem { key: "{t.id}", label: t.title.clone(), hint, dot: t.unread, selected: on, indent: child,
                leading: rsx! { Tile { icon, hue } },
                onclick: move |_| ui.go(Place::Topic(id.clone())) }
        }
    };
    let footer = match ui.live() {
        Some(l) => {
            let paired = l.zc.paired();
            let zc_dot = if paired { "bg-ok" } else { "bg-alert" };
            rsx! {
                span { class: "inline-block w-[7px] h-[7px] mr-1.5 rounded-sm bg-ok" } strong { "SilverBullet" } br {}
                {host(&l.endpoints.space)} br {}
                span { class: "inline-block w-[7px] h-[7px] mr-1.5 rounded-sm {zc_dot}" } strong { "ZeroClaw" } " · agent " strong { "{l.endpoints.agent}" } br {}
                if paired { {format!("{} · paired", host(&l.endpoints.zeroclaw))} } else { "not paired" }
            }
        }
        None => rsx! {},
    };
    rsx! {
        aside { class: "flex flex-col gap-0.5 flex-[0_0_220px] w-[220px] h-full pt-4 px-2.5 pb-3 overflow-hidden bg-rail border-0 border-r border-solid border-line",
            div { class: "flex items-center gap-2 w-full h-8 mb-2 px-2.5 border border-solid border-line rounded-md bg-surface text-faint leading-[22px] [&_input]:h-[22px]",
                {ui::icon(Icon::Search)}
                TextField { value: query, placeholder: "Search topics" }
            }
            NavItem { label: "Today", selected: place == Place::Today, count: (today_count(), Tone::Accent),
                leading: ui::icon(Icon::Calendar), onclick: move |_| ui.go(Place::Today) }
            NavItem { label: ws.as_ref().map(|w| w.inbox_title.clone()).unwrap_or_default(), selected: place == Place::Inbox,
                count: (inbox_left, Tone::Alert), leading: ui::icon(Icon::Inbox), onclick: move |_| ui.go(Place::Inbox) }
            div { class: "mt-3.5 mx-2 mb-1 text-faint type-label uppercase", "Topics" }
            // Only the topics scroll: the search, Today and the footer stay put.
            div { class: "flex flex-col gap-0.5 flex-auto min-h-0 overflow-y-auto",
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
                    div { class: "py-6 text-faint", "Loading the space…" }
                }
            }
            div { class: "flex-none mt-1 pt-2.5 px-2 pb-0.5 border-0 border-t border-solid border-line text-faint font-mono text-[10.5px]/[1.6] font-normal [&_strong]:text-ink [&_strong]:font-semibold",
                {footer}
                Button { label: "Refresh", variant: Variant::Ghost, class: "mt-1", onclick: ui.refresh }
            }
        }
    }
}

// ── Center ──────────────────────────────────────────────────────────────────

#[component]
fn Center() -> Element {
    let ui = use_context::<Ui>();
    if ui.ws.read().is_none() {
        return rsx! { div { class: EMPTY, if ui.error.read().is_some() { "Nothing to show yet." } else { "Loading the space…" } } };
    }
    match ((ui.pane)(), (ui.place)()) {
        // Under a topic, they open below its header (see `TopicPage`).
        (Pane::Doc(path), Place::Today | Place::Inbox) => rsx! { DocPane { key: "{path}", path } },
        (Pane::Thread(key, title), Place::Today | Place::Inbox) => rsx! { TranscriptPane { key: "{key}", session_key: key, title } },
        (_, Place::Today) => rsx! { Today {} },
        (_, Place::Inbox) => rsx! { Inbox {} },
        (_, Place::Topic(id)) => match ui.ws.read().as_ref().and_then(|w| w.find(&id)).cloned() {
            Some(t) => rsx! { TopicPage { topic: t } },
            None => rsx! { div { class: EMPTY, "This topic is no longer in the space." } },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cut_hint_keeps_its_date_and_ends_in_an_ellipsis() {
        assert_eq!(fit("WOF · 14 Dec", 23), "WOF · 14 Dec");
        assert_eq!(fit("Upload the SNS CDI form · 27 Nov", 23), "Upload the SN… · 27 Nov");
        assert_eq!(fit("Antalya trip: book the bungalow · 29 Dec", 23), "Antalya trip… · 29 Dec");
        assert_eq!(fit("Ex-girlfriend; main co-owner of the Mazda", 21), "Ex-girlfriend; main…");
        assert_eq!(fit("Sebag-Peyrelevade-Kergozou", 10), "Sebag-Pey…");
    }

    /// Paint the live window for each view into `target/topics-shots`.
    /// `cargo test -p osom topics_shots -- --ignored --nocapture`
    #[test]
    #[ignore = "writes screenshots; live mode needs the tailnet"]
    fn topics_shots() {
        use blitz_dom::Document;
        use blitz_traits::shell::Viewport;
        use dioxus_native::DioxusDocument;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).expect("crates/<name> sits two levels below the workspace").join("target/topics-shots");
        std::fs::create_dir_all(&dir).unwrap();
        let views: &[(&str, &str, &str, &str)] = &[
            ("live-today", "today", "", ""),
            ("live-car", "car", "", ""),
            ("live-wedding-questions", "wedding", "questions", ""),
            ("live-unfiled", "inbox", "", ""),
            ("live-doc", "car", "", "car/maintenance.md"),
        ];
        for (name, open, tab, doc) in views {
            let start = Start::new(Some(open), Some(tab), Some(doc));
            let (w, h) = (1320u32, 860u32);
            let config = dioxus_native::DocumentConfig {
                font_ctx: Some(margin_ui::typefaces::fonts()),
                viewport: Some(Viewport { window_size: (w, h), ..Default::default() }),
                ..Default::default()
            };
            let mut doc = DioxusDocument::new(VirtualDom::new_with_props(Shell, ShellProps { start }), config);
            doc.initial_build();
            let settle = Duration::from_millis(4000);
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
        let s = Start::new(None, None, None);
        assert_eq!((s.place, s.tab, s.doc), (Place::Today, Tab::Overview, None));
        let s = Start::new(Some("inbox"), Some("questions"), Some("car/log.md"));
        assert_eq!((s.place, s.tab, s.doc.as_deref()), (Place::Inbox, Tab::Questions, Some("car/log.md")));
    }

    #[test]
    fn returning_to_today_requests_a_refresh_once_per_navigation() {
        use dioxus_native::DioxusDocument;

        let start = Start::new(Some("today"), None, None);
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
    fn workspace_flattens_nested_topics_and_deduplicates_primary_agenda() {
        let w = Workspace {
            today: "2026-11-26".into(),
            topics: vec![
                TopicView {
                    id: "house".into(),
                    title: "House".into(),
                    agenda: vec![Agenda { title: "Plumber".into(), sort: "2026-11-28".into(), primary: true, ..Default::default() }],
                    children: vec![TopicView {
                        id: "house/hot-water".into(),
                        title: "Hot water".into(),
                        agenda: vec![
                            Agenda { title: "Plumber".into(), sort: "2026-11-28".into(), primary: false, ..Default::default() },
                            Agenda { title: "Cylinder check".into(), sort: "2026-11-30".into(), primary: true, ..Default::default() },
                        ],
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                TopicView {
                    id: "car".into(),
                    title: "Car".into(),
                    agenda: vec![Agenda { title: "WOF".into(), sort: "2026-12-20".into(), primary: true, ..Default::default() }],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let ids: Vec<_> = w.all().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["house", "house/hot-water", "car"]);
        let soon: Vec<_> = upcoming(&w, 7).into_iter().map(|(a, _)| a.title).collect();
        assert_eq!(soon, ["Plumber", "Cylinder check"]);
    }
}
