//! A document or a stored conversation, opened in its own pane.
use super::*;
use margin_editor::{Note, NoteStore, PageSource, Store};

/// Where a pane's back button returns to.
pub(super) fn back_label(ui: &Ui) -> &'static str {
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

/// A page of the space, as the editor's store: it saves when asked (the
/// editor's Save bar), through the space.
struct SpacePage(PageSource);

impl NoteStore for SpacePage {
    /// The Save bar says where the page stands; the status bar says nothing.
    fn use_persistence(&self, _note: Signal<Note>) -> Signal<String> {
        use_signal(String::new)
    }

    fn page(&self) -> Option<PageSource> {
        Some(self.0.clone())
    }
}

/// The notebook on a page of the space (or a demo page, kept nowhere). A
/// component, so it keeps its state while the pane around it re-renders;
/// the pane gives it a new key for each version of the page.
#[component]
pub(super) fn PageEditor(text: String, source: Option<PageSource>, options: NotebookOptions) -> Element {
    let store = use_hook(move || source.map(|page| Store::new(SpacePage(page))));
    margin_editor::notebook_view(&text, NotebookOptions { store, ..options })
}

/// A page of the space as it opens: at once from the pages read with the
/// workspace (or the demo's), then the server's version; a newer one
/// replaces it before you type. `None` until there is something to show.
fn use_page(path: String) -> Signal<Option<Result<Page, String>>> {
    let ui = use_context::<Ui>();
    let mut opened = use_signal({
        let path = path.clone();
        move || match ui.live() {
            None => Some(demo::page(&path).map(|text| Page { path: path.clone(), text: text.into(), etag: None }).ok_or_else(|| "This demo document has no content.".to_string())),
            Some(_) => ui.pages.peek().get(&path).cloned().map(Ok),
        }
    });
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
    opened
}

/// The editor on an opened page, in Rich view: it saves through the space
/// (unless `read_only`), and the comment agent answers its comments.
fn page_editor(ui: &Ui, page: Page, bar: HostBar, read_only: bool) -> Element {
    let comments = match ui.live() {
        Some(live) if live.zc.paired() => Comments::Agent(CommentAgent { alias: live.endpoints.comment_agent.clone(), chat: Arc::new(live.zc.clone()) }),
        Some(live) => Comments::Agent(CommentAgent::unavailable(&live.endpoints.comment_agent, "Osom is not paired with ZeroClaw: run `osom pair <code>`.")),
        None => Comments::Empty,
    };
    let source = ui.live().map(|live| PageSource { path: page.path.clone(), files: Arc::new(live.space.clone()), etag: page.etag.clone() });
    let key = format!("{}@{}", page.path, page.etag.clone().unwrap_or_default());
    let options = NotebookOptions { view: View::Rich, comments, host: Host { bar: Some(bar), ..Default::default() }, read_only, ..Default::default() };
    rsx! { PageEditor { key: "{key}", text: page.text, source, options } }
}

/// A page in the editor, below the topic's header.
#[component]
pub(super) fn DocPane(path: String) -> Element {
    let ui = use_context::<Ui>();
    let opened = use_page(path.clone());
    let back_to = back_label(&ui);
    let onback = use_callback(move |_| ui.open(Pane::Tab));
    // Under a topic, the header's SilverBullet button already opens this page.
    let web = ui.live().filter(|_| !matches!((ui.place)(), Place::Topic(_))).map(|l| l.space.page_url(&path));
    let open_web = use_callback({
        let web = web.clone();
        move |_| if let Some(url) = &web { open_url(url) }
    });
    let host = HostBar {
        back: Some(HostAction { label: back_to.into(), icon: Icon::Back, onclick: onback }),
        crumb: model::basename(&path).into(),
        actions: web.map(|_| HostAction { label: "SilverBullet".into(), icon: Icon::Open, onclick: open_web }).into_iter().collect(),
    };
    match opened() {
        None => rsx! { div { class: PANEBAR, Button { label: back_to, icon: Icon::Back, text: back_to, variant: Variant::Ghost, onclick: move |_| ui.open(Pane::Tab) } span { class: PANEBAR_CRUMB, "{host.crumb}" } } },
        Some(Err(e)) => rsx! { Banner { text: e, tone: Tone::Alert } },
        Some(Ok(page)) => rsx! {
            div { class: "min-h-[400px] bg-page [&_.notebook]:min-h-0 [&_#comments-pane]:top-0 [&_#comments-pane]:max-h-[calc(100vh-120px)]",
                {page_editor(&ui, page, host, false)}
            }
        },
    }
}

/// The topic's own page on its Overview: read-only, but you can select a
/// passage and comment on it there; Edit opens it in the editor. Its bar is
/// the card's header. The card is too narrow to hold the comments beside
/// the page, so they open over the window's right side.
#[component]
pub(super) fn OverviewPage(path: String) -> Element {
    let ui = use_context::<Ui>();
    let opened = use_page(path.clone());
    let edit = use_callback({
        let path = path.clone();
        move |_| ui.open(Pane::Doc(path.clone()))
    });
    let bar = HostBar { back: None, crumb: path, actions: vec![HostAction { label: "Edit".into(), icon: Icon::Pencil, onclick: edit }] };
    match opened() {
        None => rsx! { div { class: "py-6 text-faint", "Opening the page…" } },
        Some(Err(e)) => rsx! { Banner { text: e, tone: Tone::Alert } },
        Some(Ok(page)) => rsx! {
            div { class: "[&_.notebook]:min-h-0 [&_#mode-bar]:bg-surface [&_#mode-bar]:-mt-1 [&_.paper]:mt-3 [&_.paper]:pb-2 [&_footer]:hidden [&_#comments-pane]:fixed [&_#comments-pane]:top-0 [&_#comments-pane]:right-0 [&_#comments-pane]:h-screen [&_#comments-pane]:z-[16] [&_#comments-pane]:max-h-none [&_#comments-pane]:shadow-side",
                {page_editor(&ui, page, bar, true)}
            }
        },
    }
}

/// A stored conversation: the transcript read with the workspace, or read
/// now when it was not. Keyed by session.
#[component]
pub(super) fn TranscriptPane(session_key: String, title: String) -> Element {
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
        div { class: PANEBAR,
            Button { label: back_label(&ui), icon: Icon::Back, text: back_label(&ui), variant: Variant::Ghost, onclick: move |_| ui.open(Pane::Tab) }
            span { class: PANEBAR_CRUMB, "{title}" }
            span { class: "flex-auto" }
            span { class: "text-faint type-mono-ui", "{session_key}" }
        }
        match turns {
            None => rsx! { div { class: "py-6 text-faint", "Loading the conversation…" } },
            Some(Err(e)) => rsx! { Banner { text: e, tone: Tone::Alert } },
            Some(Ok(turns)) => rsx! {
                div { class: "flex flex-col gap-3.5 max-w-[760px] pt-3.5",
                    if turns.is_empty() { div { class: EMPTY, "No messages." } }
                    for (i, t) in turns.into_iter().enumerate() {
                        Message { key: "{i}", author: if t.from_user { "you" } else { "agent" }, body: t.text.clone(), ai: !t.from_user,
                            tools: t.tools, at: (!t.at.is_empty()).then_some(t.at), style: MessageStyle::Card,
                            rich: rsx! { MarkdownView { text: t.text } } }
                    }
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use margin_core::comments as file;
    use margin_core::files::{Versioned, WriteError};
    use margin_editor::testing::{comment_on, settle};
    use margin_editor::{CommentAgent, PageFiles};
    use osom_hub::{Endpoints, Space};
    use std::sync::Mutex;

    /// The space, keeping what the notebook reads and writes through it: the
    /// app's view of the notebook.
    #[derive(Clone)]
    struct Watched {
        space: Space,
        /// Each comment file read (`Ok` when it worked) and written, in order.
        reads: Arc<Mutex<Vec<Result<(), String>>>>,
        writes: Arc<Mutex<Vec<String>>>,
    }

    impl PageFiles for Watched {
        fn read(&self, path: &str) -> Result<Option<Versioned>, String> {
            let read = PageFiles::read(&self.space, path);
            if path.ends_with(".json") {
                self.reads.lock().unwrap().push(read.as_ref().map(|_| ()).map_err(Clone::clone));
            }
            read
        }

        fn write(&self, path: &str, text: &str, etag: Option<&str>) -> Result<Option<String>, WriteError> {
            let written = PageFiles::write(&self.space, path, text, etag);
            if written.is_ok() && path.ends_with(".json") {
                self.writes.lock().unwrap().push(text.to_owned());
            }
            written
        }

        fn web_url(&self, path: &str) -> String {
            PageFiles::web_url(&self.space, path)
        }
    }

    impl Watched {
        /// The last comment file written.
        fn saved(&self) -> Option<file::Sidecar> {
            self.writes.lock().unwrap().last().and_then(|text| file::Sidecar::parse(text).ok())
        }
    }

    /// The notebook asks the real comment agent on a real page of the space,
    /// streams the reply into the thread and saves the comment file next to
    /// the page; both are removed afterwards. It drives the page as a reader
    /// does, and watches only what the app sees: the files written. The test
    /// binary is not the app the Keychain trusts, so pass the token to avoid
    /// a Keychain prompt:
    /// `ZEROCLAW_TOKEN=… cargo test -p osom live_comment -- --ignored --nocapture`
    #[test]
    #[ignore = "talks to the live space and agent over the tailnet"]
    fn live_comment_streams_a_reply_and_saves_the_thread() {
        use dioxus_native::DioxusDocument;
        let e = Endpoints::load().unwrap();
        let page = "car/maintenance.md";
        let space = Space::new(&e.space);
        let read = space.read(page).unwrap();
        let watched = Watched { space: space.clone(), reads: Default::default(), writes: Default::default() };
        let source = PageSource { path: page.into(), files: Arc::new(watched.clone()), etag: read.etag };
        let options = NotebookOptions {
            comments: Comments::Agent(CommentAgent { alias: e.comment_agent.clone(), chat: Arc::new(e.zeroclaw()) }),
            ..Default::default()
        };
        let mut doc = DioxusDocument::new(
            VirtualDom::new_with_props(PageEditor, PageEditorProps { text: read.text, source: Some(source), options }),
            dioxus_native::DocumentConfig { font_ctx: Some(margin_ui::typefaces::fonts()), ..Default::default() },
        );
        doc.initial_build();
        // The page has no comment file yet: loading finds it absent.
        settle(&mut doc, Duration::from_secs(20), |_| !watched.reads.lock().unwrap().is_empty());
        assert_eq!(watched.reads.lock().unwrap().first(), Some(&Ok(())), "the comment file was read");
        let quote = "Front-left proximity sensor failing";
        comment_on(&mut doc, quote, "Shorten this to a few words.");
        let answered = |s: &file::Sidecar| s.threads.first().is_some_and(|t| t.messages.len() == 2);
        settle(&mut doc, Duration::from_secs(240), |_| watched.saved().is_some_and(|s| answered(&s)));
        let saved = watched.saved().expect("comment file written");
        let on_server = space.read(&file::sidecar_path(page));
        // Clean up before asserting.
        let _ = space.delete(&file::sidecar_path(page));
        if let Some(thread) = saved.threads.first() {
            let _ = e.zeroclaw().delete_session(&file::request::session_id(&thread.id));
        }
        assert_eq!(saved.threads.len(), 1);
        assert_eq!(saved.threads[0].anchor.exact, quote);
        let reply = saved.threads[0].messages.last().unwrap();
        println!("reply: {}\nproposal: {:?}", reply.body, reply.proposal.as_ref().map(|p| &p.new));
        assert!(reply.ai && !reply.body.is_empty(), "the agent answered: {}", reply.body);
        assert!(!reply.body.contains(file::request::FENCE), "the edit block never shows");
        let on_server = file::Sidecar::parse(&on_server.expect("comment file on the server").text).unwrap();
        assert_eq!(on_server.threads[0].messages.len(), 2, "your comment and the answer");
    }
}
