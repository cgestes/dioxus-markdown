//! The agent's conversations: sessions and where they came from, the threads
//! of a topic, and a transcript's turns.
use crate::routes::{Route, Source};
use crate::space::Topic;
use serde::Deserialize;

/// One entry of ZeroClaw's `GET /api/sessions`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Session {
    pub session_id: String,
    #[serde(default)]
    pub session_key: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub last_activity: String,
    #[serde(default)]
    pub message_count: u64,
    #[serde(default)]
    pub name: Option<String>,
}

/// Where a session's messages come from, read from its key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Margin's chat about one topic (`topic-<slug>`).
    Topic(String),
    /// Margin's chat outside any topic.
    General,
    /// A comment thread on a page (`margin-comment-<thread>`).
    Comment(String),
    /// `telegram_<alias>_<chat>_<user>`.
    Telegram { chat: String },
    /// `slack_<alias>_<channel>_<thread ts>_<user>`; each thread is a session.
    Slack { channel: String, thread: String },
    /// A ZeroClaw dashboard chat.
    Gateway,
    Other,
}

impl Origin {
    pub fn source(&self) -> Source {
        match self {
            Self::Telegram { .. } => Source::Telegram,
            Self::Slack { .. } => Source::Slack,
            Self::Topic(_) | Self::General | Self::Comment(_) | Self::Gateway => Source::Chat,
            Self::Other => Source::Other,
        }
    }

    /// One of Margin's own chats.
    pub fn margin(&self) -> bool {
        matches!(self, Self::Topic(_) | Self::General | Self::Comment(_))
    }
}

/// The session Margin uses outside any topic.
pub const GENERAL_CHAT: &str = "margin-general";

/// The ZeroClaw session Margin uses to chat about one topic.
pub fn chat_session(topic_id: &str) -> String {
    format!("topic-{}", topic_id.replace('/', "-"))
}

/// The stored key of a session Margin opened through the gateway.
pub fn session_key(session_id: &str) -> String {
    format!("gw_{session_id}")
}

/// Parse a session key; the inverse of `chat_session` and the channel keys.
pub fn origin(key: &str) -> Origin {
    let bare = key.strip_prefix("gw_").unwrap_or(key);
    if let Some(slug) = bare.strip_prefix("topic-") {
        return Origin::Topic(slug.into());
    }
    if bare == GENERAL_CHAT {
        return Origin::General;
    }
    if let Some(thread) = bare.strip_prefix(margin_core::comments::request::SESSION_PREFIX) {
        return Origin::Comment(thread.into());
    }
    let mut parts = key.split('_');
    match parts.next() {
        Some("telegram") => Origin::Telegram {
            chat: parts.find(|p| p.parse::<i64>().is_ok()).unwrap_or_default().into(),
        },
        Some("slack") => {
            let slack_id = |p: &&str| {
                p.len() >= 9
                    && p.starts_with(['C', 'D', 'G'])
                    && p.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            };
            let channel = parts.find(slack_id).unwrap_or_default().to_string();
            let thread = parts
                .take_while(|p| p.chars().all(|c| c.is_ascii_digit() || c == '.'))
                .collect::<Vec<_>>()
                .join(".");
            Origin::Slack { channel, thread }
        }
        Some("gw") => Origin::Gateway,
        _ => Origin::Other,
    }
}

impl Session {
    pub fn key(&self) -> &str {
        if self.session_key.is_empty() {
            &self.session_id
        } else {
            &self.session_key
        }
    }

    pub fn origin(&self) -> Origin {
        origin(self.key())
    }
}

/// One turn of `GET /api/sessions/{key}/messages`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Message {
    pub role: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// A conversation filed under a topic.
#[derive(Clone, Debug, PartialEq)]
pub struct Thread {
    pub session: Session,
    pub origin: Origin,
    pub label: String,
}

impl Thread {
    fn new(session: &Session, origin: Origin, label: String) -> Self {
        Self {
            session: session.clone(),
            origin,
            label,
        }
    }
}

fn by_activity(threads: &mut [Thread]) {
    threads.sort_by(|a, b| b.session.last_activity.cmp(&a.session.last_activity));
}

/// Sessions whose chat is routed to this topic, plus Margin's own topic chat,
/// most recent first.
pub fn threads(topic: &Topic, routes: &[Route], sessions: &[Session]) -> Vec<Thread> {
    let own = Origin::Topic(chat_session(&topic.id)["topic-".len()..].into());
    let routes: Vec<&Route> = routes.iter().filter(|r| r.target == topic.id).collect();
    let mut out: Vec<Thread> = sessions
        .iter()
        .filter_map(|s| {
            let origin = s.origin();
            if origin == own {
                return Some(Thread::new(s, origin, "Margin chat".into()));
            }
            let route = routes.iter().find(|r| r.matches(s.key()))?;
            Some(Thread::new(s, origin, route.label.clone()))
        })
        .collect();
    by_activity(&mut out);
    out
}

/// Chats no route files under a topic: the Telegram DM, dashboard chats,
/// channels missing from the README map. Margin's own chats are never
/// unfiled. Most recent first.
pub fn unfiled(routes: &[Route], sessions: &[Session]) -> Vec<Thread> {
    let mut out: Vec<Thread> = sessions
        .iter()
        .filter(|s| s.message_count > 0 && !routes.iter().any(|r| r.matches(s.key())))
        .filter_map(|s| {
            let origin = s.origin();
            let label = match &origin {
                _ if origin.margin() => return None,
                Origin::Telegram { chat } if chat.starts_with('-') => "Telegram group".into(),
                Origin::Telegram { .. } => "Telegram DM".into(),
                Origin::Slack { channel, .. } if channel.starts_with('D') => "Slack DM".into(),
                Origin::Slack { .. } => "Slack".into(),
                Origin::Gateway => s.name.clone().unwrap_or_else(|| "Dashboard chat".into()),
                _ => s.name.clone().unwrap_or_else(|| s.key().into()),
            };
            Some(Thread::new(s, origin, label))
        })
        .collect();
    by_activity(&mut out);
    out
}

/// What Margin tells the agent about a fresh topic chat, before the first
/// message. The chat panel shows the same text.
pub fn chat_context(title: &str, path: &str, reminders: usize, open_questions: usize) -> String {
    format!("{title} · space path {path} · {reminders} reminders · {open_questions} open questions")
}

const PREAMBLE: &str = "[Margin · ";

/// The context line as sent ahead of a fresh session's first message.
pub fn preamble(context: &str) -> String {
    format!("{PREAMBLE}{context}]")
}

/// A turn a person would read: text from the user or the agent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Turn {
    pub from_user: bool,
    pub text: String,
    /// `2026-11-24 14:56` when known.
    pub at: String,
    /// Tools the agent ran since the previous visible turn.
    pub tools: Vec<String>,
}

/// User and agent text from a stored session. Tool results, tool-calling
/// steps, Margin's context preamble and silent `NO_REPLY` answers are
/// dropped; the tools an agent used are attached to its next visible answer.
pub fn transcript(messages: &[Message]) -> Vec<Turn> {
    let mut out = Vec::new();
    let mut tools = Vec::new();
    for m in messages {
        let at = m
            .created_at
            .as_deref()
            .map(|t| t.get(..16).unwrap_or(t).replace('T', " "))
            .unwrap_or_default();
        match m.role.as_str() {
            "user" => {
                let (stamp, text) = split_stamp(&m.content);
                out.push(Turn {
                    from_user: true,
                    text: strip_preamble(text).trim().into(),
                    at: stamp.unwrap_or(at),
                    tools: Vec::new(),
                });
            }
            "assistant" => {
                let trimmed = m.content.trim();
                let text = match trimmed
                    .starts_with('{')
                    .then(|| serde_json::from_str::<serde_json::Value>(trimmed).ok())
                    .flatten()
                {
                    Some(v) if v.is_object() => {
                        for call in v["tool_calls"].as_array().into_iter().flatten() {
                            let name = call["name"].as_str().or_else(|| call["function"]["name"].as_str());
                            tools.extend(name.map(str::to_string));
                        }
                        v["content"].as_str().unwrap_or_default().trim().to_string()
                    }
                    _ => trimmed.to_string(),
                };
                if text.is_empty() || text == "NO_REPLY" {
                    continue;
                }
                out.push(Turn {
                    from_user: false,
                    text,
                    at,
                    tools: std::mem::take(&mut tools),
                });
            }
            _ => {}
        }
    }
    out
}

/// Margin's context ahead of a message: the topic line or a comment's page.
fn strip_preamble(text: &str) -> &str {
    if let Some(rest) = margin_core::comments::request::strip_preamble(text) {
        return rest;
    }
    text.strip_prefix(PREAMBLE)
        .and_then(|rest| rest.split_once(']'))
        .map_or(text, |(_, after)| after)
}

/// `[2026-11-24 14:56:19 +01:00] Done` → (`2026-11-24 14:56`, `Done`).
fn split_stamp(s: &str) -> (Option<String>, &str) {
    if let Some(rest) = s.strip_prefix('[')
        && let Some(end) = rest.find("] ")
        && rest.as_bytes().first().is_some_and(u8::is_ascii_digit)
    {
        let stamp = &rest[..end];
        return (Some(stamp.get(..16).unwrap_or(stamp).to_string()), &rest[end + 2..]);
    }
    (None, s)
}
