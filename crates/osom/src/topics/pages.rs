//! A document or a stored conversation, opened in its own pane.
use super::*;

/// The notebook on a page of the space. A component, so it keeps its state
/// while the pane around it re-renders.
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

#[component]
pub(super) fn PageEditor(text: String, options: NotebookOptions) -> Element {
    margin_editor::notebook_view(&text, options)
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
