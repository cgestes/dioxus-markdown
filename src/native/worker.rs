//! Blocking work off the UI thread: network calls, and agent turns whose
//! events stream back to the UI.
use futures_channel::mpsc::{UnboundedReceiver, unbounded};
use futures_util::StreamExt;
use margin_editor_core::hub::{AgentChat, Ask, ChatEvent};
use std::sync::Arc;

/// Run blocking work on a thread and await it from the UI.
pub(super) fn work<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> impl Future<Output = T> {
    let (tx, rx) = futures_channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    async move { rx.await.expect("worker thread") }
}

/// Start one agent turn; its events arrive on the receiver. Dropping the
/// receiver stops the turn. A stand-in agent that answers at once runs here,
/// so its whole answer is waiting before this returns.
pub(super) fn turn(
    agent: Arc<dyn AgentChat>,
    ask: Ask,
    mut decide: impl FnMut() -> bool + Send + 'static,
) -> UnboundedReceiver<ChatEvent> {
    let (tx, rx) = unbounded();
    let inline = agent.inline();
    let mut run = move || agent.chat(&ask, &mut |e| tx.unbounded_send(e).is_ok(), &mut decide);
    if inline {
        run();
    } else {
        std::thread::spawn(run);
    }
    rx
}

/// The events that have arrived: waits for one, then takes the rest without
/// waiting, so a burst of chunks makes one update. `None` once the turn is over.
pub(super) async fn batch(rx: &mut UnboundedReceiver<ChatEvent>) -> Option<Vec<ChatEvent>> {
    let first = rx.next().await?;
    let mut events = vec![first];
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    Some(events)
}

/// A finished turn, for callers that wait for the whole answer.
#[derive(Debug, Default)]
pub(super) struct Answer {
    pub text: String,
    pub tools: Vec<String>,
    pub error: Option<String>,
}

/// Run one turn on this thread and collect its answer.
pub(super) fn answer(agent: &dyn AgentChat, ask: &Ask) -> Answer {
    let mut out = Answer::default();
    agent.chat(
        ask,
        &mut |e| {
            match e {
                ChatEvent::Done(full) => out.text = full,
                ChatEvent::Tool(t) => out.tools.push(t),
                ChatEvent::Error(err) => out.error = Some(err),
                _ => {}
            }
            true
        },
        &mut || false,
    );
    out
}
