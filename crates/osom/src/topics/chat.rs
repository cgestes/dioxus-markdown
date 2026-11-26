//! The chat with the agent, and pairing Osom with ZeroClaw.
use super::*;

/// Send a message in the current place's chat; the reply streams into
/// `Ui::pending` and joins the lines when it is done.
pub(super) fn send(ui: Ui, text: String) {
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
pub(super) fn PendingReply() -> Element {
    let ui = use_context::<Ui>();
    let pending = ui.pending.read();
    let Some(p) = pending.as_ref() else {
        return rsx! {};
    };
    rsx! { Message { author: "agent", body: p.text.clone(), ai: true, pending: true, tools: p.tools.clone() } }
}

#[component]
pub(super) fn ChatPanel() -> Element {
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
pub(super) fn Pairing() -> Element {
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
