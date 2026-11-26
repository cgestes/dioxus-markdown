//! Talking to an agent, whoever it is: what a turn asks, what the agent
//! reports while it answers, and a stand-in that answers locally. The
//! editor's comment threads and Osom's chat use this interface; margin-hub
//! implements it for ZeroClaw.
use std::time::Duration;

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
/// approval prompts; it may block until someone chooses.
pub trait AgentChat: Send + Sync {
    fn chat(&self, ask: &Ask, on: &mut dyn FnMut(ChatEvent) -> bool, decide: &mut dyn FnMut() -> bool);

    /// Answers at once, without a network round trip: callers may run it
    /// on their own thread.
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
    fn chat(&self, ask: &Ask, on: &mut dyn FnMut(ChatEvent) -> bool, _decide: &mut dyn FnMut() -> bool) {
        let reply = (self.reply)(ask);
        for word in reply.split_inclusive(' ') {
            // A browser tab has one thread: the stand-in answers without the pauses.
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(pause) = self.pause {
                std::thread::sleep(pause);
            }
            if !on(ChatEvent::Chunk(word.into())) {
                return;
            }
        }
        on(ChatEvent::Done(reply));
    }

    fn inline(&self) -> bool {
        self.pause.is_none()
    }
}
