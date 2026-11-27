//! Talking to an agent, whoever it is: what a turn asks, what the agent
//! reports while it answers, and a stand-in that answers locally. The
//! editor's comment threads and Osom's chat use this interface; osom-hub
//! implements it for ZeroClaw.
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
pub type ChatFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;
#[cfg(target_arch = "wasm32")]
pub type ChatFuture<T> = Pin<Box<dyn Future<Output = T> + 'static>>;

/// Something the agent did during a chat turn.
#[derive(Clone, Debug, PartialEq)]
pub enum ChatEvent {
    Chunk(String),
    Tool(String),
    /// The agent asks before running a tool; the chat's `decide` answers.
    Approval { tool: String, summary: String },
    /// The whole reply; the last event of a turn that worked.
    Done(String),
    /// Why the turn failed; the last event of a turn that did not.
    Error(String),
    /// Nothing happened for a while; lets the listener stop a quiet turn.
    Tick,
}

/// One message to the agent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ask {
    pub agent: String,
    pub session: String,
    /// Shown for the session in the agent's dashboard.
    pub name: String,
    pub content: String,
    /// Sent ahead of `content` when the session is new.
    pub preamble: Option<String>,
}

/// Something that answers a chat turn: ZeroClaw, or a stand-in.
///
/// `chat` reports the reply through `on` and always ends with exactly one
/// `Done` (carrying the whole reply) or `Error`, unless `on` returned `false`
/// to stop the turn early (the listener is gone). `decide` answers tool
/// approval prompts asynchronously.
pub trait AgentChat: Send + Sync {
    fn chat(
        self: Arc<Self>,
        ask: Ask,
        on: Box<dyn FnMut(ChatEvent) -> bool + Send>,
        decide: Box<dyn FnMut() -> ChatFuture<bool> + Send>,
    ) -> ChatFuture<()>;

    /// Answers at once, before the returned future is even polled: callers
    /// can read the whole answer immediately.
    fn inline(&self) -> bool {
        false
    }
}

/// A stand-in agent that answers every turn with the text `reply` builds,
/// word by word. `pause` spaces the words out (the demo); without it the
/// answer is immediate (tests, the sample note).
pub struct Canned {
    pub reply: Box<dyn Fn(&Ask) -> String + Send + Sync>,
    pub pause: Option<Duration>,
}

impl AgentChat for Canned {
    fn chat(
        self: Arc<Self>,
        ask: Ask,
        mut on: Box<dyn FnMut(ChatEvent) -> bool + Send>,
        _decide: Box<dyn FnMut() -> ChatFuture<bool> + Send>,
    ) -> ChatFuture<()> {
        let reply = (self.reply)(&ask);
        if self.pause.is_none() || cfg!(target_arch = "wasm32") {
            for word in reply.split_inclusive(' ') {
                if !on(ChatEvent::Chunk(word.into())) {
                    return Box::pin(async {});
                }
            }
            on(ChatEvent::Done(reply));
            return Box::pin(async {});
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            use std::sync::Mutex;
            use std::task::{Context, Poll, Waker};

            struct ThreadDone(Arc<Mutex<(bool, Option<Waker>)>>);
            impl Future for ThreadDone {
                type Output = ();
                fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
                    let mut state = self.0.lock().unwrap();
                    if state.0 {
                        Poll::Ready(())
                    } else {
                        state.1 = Some(cx.waker().clone());
                        Poll::Pending
                    }
                }
            }

            let pause = self.pause.unwrap_or_default();
            let done = Arc::new(Mutex::new((false, None::<Waker>)));
            let signal = done.clone();
            std::thread::spawn(move || {
                let mut keep = true;
                for word in reply.split_inclusive(' ') {
                    std::thread::sleep(pause);
                    if !on(ChatEvent::Chunk(word.into())) {
                        keep = false;
                        break;
                    }
                }
                if keep {
                    on(ChatEvent::Done(reply));
                }
                let mut state = signal.lock().unwrap();
                state.0 = true;
                if let Some(waker) = state.1.take() {
                    waker.wake();
                }
            });
            Box::pin(ThreadDone(done))
        }
        #[cfg(target_arch = "wasm32")]
        Box::pin(async {})
    }

    fn inline(&self) -> bool {
        self.pause.is_none()
    }
}
