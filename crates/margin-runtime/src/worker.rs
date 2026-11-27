//! Agent turns whose events stream back to the UI. Identical async turn
//! and tool-approval flow on both native and wasm32.
use futures_channel::mpsc::{UnboundedReceiver, unbounded};
use futures_channel::oneshot;
use futures_util::StreamExt;
use margin_core::agent::{AgentChat, Ask, ChatEvent, ChatFuture};
use std::sync::{Arc, Mutex};

/// Answers a tool-approval prompt for an in-flight agent turn on both native and wasm32.
#[derive(Clone, Debug, Default)]
pub struct ApprovalReply {
    pending: Arc<Mutex<Option<oneshot::Sender<bool>>>>,
}

impl ApprovalReply {
    pub fn send(&self, approve: bool) {
        if let Some(tx) = self.pending.lock().unwrap().take() {
            let _ = tx.send(approve);
        }
    }
}

/// Start one agent turn whose tool approvals can be answered asynchronously via
/// the returned `ApprovalReply`.
pub fn turn_with_approval(agent: Arc<dyn AgentChat>, ask: Ask) -> (UnboundedReceiver<ChatEvent>, ApprovalReply) {
    let (tx, rx) = unbounded();
    let reply = ApprovalReply::default();
    let slot = reply.pending.clone();
    let inline = agent.inline();
    let on = Box::new(move |e: ChatEvent| tx.unbounded_send(e).is_ok());
    let decide = Box::new(move || -> ChatFuture<bool> {
        let (dtx, drx) = oneshot::channel();
        *slot.lock().unwrap() = Some(dtx);
        Box::pin(async move { drx.await.unwrap_or(false) })
    });
    let fut = agent.chat(ask, on, decide);
    if !inline {
        margin_ui::prelude::spawn(fut);
    }
    (rx, reply)
}

/// Start one agent turn; its events arrive on the receiver. Dropping the
/// receiver stops the turn. A stand-in agent that answers at once runs here,
/// so its whole answer is waiting before this returns.
pub fn turn(
    agent: Arc<dyn AgentChat>,
    ask: Ask,
    mut decide: impl FnMut() -> bool + Send + 'static,
) -> UnboundedReceiver<ChatEvent> {
    let (tx, rx) = unbounded();
    let inline = agent.inline();
    let on = Box::new(move |e: ChatEvent| tx.unbounded_send(e).is_ok());
    let decide = Box::new(move || -> ChatFuture<bool> {
        let yes = decide();
        Box::pin(async move { yes })
    });
    let fut = agent.chat(ask, on, decide);
    if !inline {
        margin_ui::prelude::spawn(fut);
    }
    rx
}

/// The events that have arrived: waits for one, then takes the rest without
/// waiting, so a burst of chunks makes one update. `None` once the turn is over.
pub async fn batch(rx: &mut UnboundedReceiver<ChatEvent>) -> Option<Vec<ChatEvent>> {
    let first = rx.next().await?;
    let mut events = vec![first];
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    Some(events)
}

/// A finished turn, for callers that wait for the whole answer.
#[derive(Debug, Default)]
pub struct Answer {
    pub text: String,
    pub tools: Vec<String>,
    pub error: Option<String>,
}

/// Run one turn asynchronously and collect its answer.
pub async fn answer(agent: Arc<dyn AgentChat>, ask: Ask) -> Answer {
    let out = Arc::new(Mutex::new(Answer::default()));
    let collector = out.clone();
    let on = Box::new(move |e: ChatEvent| {
        let mut a = collector.lock().unwrap();
        match e {
            ChatEvent::Done(full) => a.text = full,
            ChatEvent::Tool(t) => a.tools.push(t),
            ChatEvent::Error(err) => a.error = Some(err),
            _ => {}
        }
        true
    });
    let decide = Box::new(|| -> ChatFuture<bool> { Box::pin(async { false }) });
    agent.chat(ask, on, decide).await;
    Arc::try_unwrap(out)
        .map(|m| m.into_inner().unwrap())
        .unwrap_or_else(|a| {
            let g = a.lock().unwrap();
            Answer {
                text: g.text.clone(),
                tools: g.tools.clone(),
                error: g.error.clone(),
            }
        })
}
