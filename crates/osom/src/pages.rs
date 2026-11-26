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

/// A page in the editor, below the topic's header. It opens at once from the
/// pages read with the workspace (or the demo's), then checks the server's
/// version; a newer one replaces it before you type.
#[component]
pub(super) fn DocPane(path: String) -> Element {
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
        None => rsx! { div { class: PANEBAR, Button { label: back_to, icon: Icon::Back, text: back_to, variant: Variant::Ghost, onclick: move |_| ui.open(Pane::Tab) } span { class: PANEBAR_CRUMB, "{host.crumb}" } } },
        Some(Err(e)) => rsx! { Banner { text: e, tone: Tone::Alert } },
        Some(Ok(page)) => {
            let comments = match ui.live() {
                Some(live) if live.zc.paired() => Comments::Agent(CommentAgent { alias: live.endpoints.comment_agent.clone(), chat: Arc::new(live.zc.clone()) }),
                Some(live) => Comments::Agent(CommentAgent::unavailable(&live.endpoints.comment_agent, "Osom is not paired with ZeroClaw: run `osom pair <code>`.")),
                None => Comments::Empty,
            };
            let source = ui.live().map(|live| PageSource { path: page.path.clone(), files: Arc::new(live.space.clone()), etag: page.etag.clone() });
            let key = format!("{}@{}", page.path, page.etag.clone().unwrap_or_default());
            let options = NotebookOptions { view: View::Rich, comments, host: Host { bar: Some(host), ..Default::default() }, ..Default::default() };
            rsx! { div { class: "min-h-[400px] bg-page [&_.notebook]:min-h-0 [&_#comments-pane]:top-0 [&_#comments-pane]:max-h-[calc(100vh-120px)]", PageEditor { key: "{key}", text: page.text.clone(), source, options } } }
        }
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
    use margin_editor::CommentAgent;
    use margin_editor::testing::{ask_about, comment_file, threads};
    use margin_editor_core::comments as file;
    use osom_hub::{Endpoints, Space};

    /// The notebook asks the real comment agent on a real page of the space,
    /// streams the reply into the thread and saves the comment file next to
    /// the page; both are removed afterwards. The test binary is not the app
    /// the Keychain trusts, so pass the token to avoid a Keychain prompt:
    /// `ZEROCLAW_TOKEN=… cargo test -p osom live_comment -- --ignored --nocapture`
    #[test]
    #[ignore = "talks to the live space and agent over the tailnet"]
    fn live_comment_streams_a_reply_and_saves_the_thread() {
        use dioxus_native::DioxusDocument;
        let e = Endpoints::load().unwrap();
        let page = "car/maintenance.md";
        let space = Space::new(&e.space);
        let read = space.read(page).unwrap();
        let text = read.text.clone();
        let source = PageSource { path: page.into(), files: Arc::new(space.clone()), etag: read.etag };
        let options = NotebookOptions {
            comments: Comments::Agent(CommentAgent { alias: e.comment_agent.clone(), chat: Arc::new(e.zeroclaw()) }),
            ..Default::default()
        };
        // The page editor is the document's root, as the test driver expects.
        let mut doc = DioxusDocument::new(
            VirtualDom::new_with_props(PageEditor, PageEditorProps { text: text.clone(), source: Some(source), options }),
            dioxus_native::DocumentConfig { font_ctx: Some(margin_ui::typefaces::fonts()), ..Default::default() },
        );
        doc.initial_build();
        let settle = |doc: &mut DioxusDocument, secs: u64, done: &dyn Fn(&DioxusDocument) -> bool| {
            let started = std::time::Instant::now();
            while !done(doc) && started.elapsed() < Duration::from_secs(secs) {
                blitz_dom::Document::poll(doc, None);
                std::thread::sleep(Duration::from_millis(50));
            }
        };
        // The page has no comment file yet: loading finds it absent.
        settle(&mut doc, 20, &|d| {
            let state = comment_file(d);
            state.loaded || state.error.is_some()
        });
        assert_eq!(comment_file(&doc).error, None);
        let quote = "Front-left proximity sensor failing";
        let at = text.find(quote).unwrap();
        ask_about(&doc, at..at + quote.len(), quote, "Shorten this to a few words.");
        settle(&mut doc, 240, &|d| threads(d).first().is_some_and(|t| t.pending.is_none()) && !comment_file(d).saving);
        let thread = threads(&doc).first().cloned().expect("thread");
        let reply = thread.file.messages.last().unwrap().clone();
        println!("reply: {}\nproposal: {:?}", reply.body, reply.proposal.as_ref().map(|p| &p.new));
        let saved = space.read(&file::sidecar_path(page));
        // Clean up before asserting.
        let _ = space.delete(&file::sidecar_path(page));
        let _ = e.zeroclaw().delete_session(&file::request::session_id(&thread.file.id));
        assert!(!thread.failed.contains(&reply.id) && !reply.body.is_empty(), "the agent answered: {}", reply.body);
        assert!(!reply.body.contains(file::request::FENCE), "the edit block never shows");
        let saved = file::Sidecar::parse(&saved.expect("comment file written").text).unwrap();
        assert_eq!(saved.threads.len(), 1);
        assert_eq!(saved.threads[0].anchor.exact, quote);
        assert_eq!(saved.threads[0].messages.len(), 2, "your comment and the answer");
    }
}
