//! What the Topics window shows, whatever it came from: the live space
//! (SilverBullet + ZeroClaw) or the demo data in `topics_demo`.
pub use margin_editor_core::topics::{Question as QuestionRow, Source, Turn as ChatLine};

pub use super::ui::{Hue, Icon};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Agenda {
    pub title: String,
    pub repeats: bool,
    /// Where the ping goes: `Telegram › House`.
    pub deliver_to: String,
    /// Schedule or state: `cron 0 9 14 12 *`, `pending · next ping 2026-11-26`.
    pub detail: String,
    /// `[house]`, or a reminder id.
    pub tag: String,
    /// ISO date used for ordering and for Today.
    pub sort: String,
    pub notes: String,
    /// Shown on Today: false on a topic that only repeats another's entry.
    pub primary: bool,
}

/// A conversation: a Telegram group, a Slack thread, a Margin chat.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThreadRow {
    /// Unique: the ZeroClaw session key, or a demo id.
    pub key: String,
    pub source: Source,
    pub title: String,
    pub snippet: String,
    /// `Home › Car`, `#house`, `Margin chat`.
    pub place: String,
    /// `2 h`, `Yesterday`, `12 Nov`.
    pub when: String,
    /// ISO time for ordering.
    pub sort: String,
    pub messages: u64,
    pub unread: bool,
    /// Turns shown inline when the row is expanded; a row without them opens
    /// its transcript instead.
    pub turns: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocRow {
    /// `PDF`, `MD`, `IMG`.
    pub kind: String,
    pub name: String,
    pub meta: String,
    /// Space path when the document opens in the editor.
    pub path: Option<String>,
}

/// A procedure or a list: Markdown tasks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Checklist {
    pub title: String,
    pub meta: String,
    pub items: Vec<(String, bool)>,
    /// Groceries and shopping lists roll up into Today.
    pub shopping: bool,
    /// The page that holds it; a list without one is edited in place.
    pub path: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snippet {
    pub file: String,
    pub code: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TopicView {
    /// `house`, `house/hot-water`, `people/alex-sample`.
    pub id: String,
    pub title: String,
    /// Shown above the title: `Osom wife`, `chauffe-eau`.
    pub subtitle: String,
    pub icon: Icon,
    pub hue: Hue,
    /// Sidebar hint: `WOF · 14 Dec`.
    pub next: String,
    pub unread: bool,
    /// Agent-maintained summary; hidden when empty.
    pub brief: String,
    pub brief_by: String,
    /// The topic's own page (README or person page), rendered read-only.
    pub overview: Option<String>,
    pub overview_path: Option<String>,
    pub agenda: Vec<Agenda>,
    pub threads: Vec<ThreadRow>,
    pub docs: Vec<DocRow>,
    pub procedures: Vec<Checklist>,
    pub lists: Vec<Checklist>,
    pub questions: Vec<QuestionRow>,
    /// Facts the agent remembers (`memory · topic:<id>`).
    pub facts: Vec<(String, String)>,
    pub code: Vec<Snippet>,
    /// Earlier chat turns in this topic's agent session.
    pub chat: Vec<ChatLine>,
    /// What the context chip says: `Mazda · brief, 4 facts, 2 procedures`.
    pub chat_context: String,
    /// Folder in the space: `topics/house`, `procedures/car`.
    pub vault_path: String,
    /// The topic in SilverBullet.
    pub web_url: Option<String>,
    pub children: Vec<TopicView>,
}

impl TopicView {
    fn find_mut(&mut self, id: &str) -> Option<&mut TopicView> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter_mut().find_map(|c| c.find_mut(id))
    }

    /// This topic, then its sub-topics depth first.
    pub fn all(&self) -> impl Iterator<Item = &TopicView> {
        let mut stack = vec![self];
        std::iter::from_fn(move || {
            let t = stack.pop()?;
            stack.extend(t.children.iter().rev());
            Some(t)
        })
    }

    /// Sources that have discussions here, in order of first appearance.
    pub fn sources(&self) -> Vec<Source> {
        let mut out = Vec::new();
        for t in &self.threads {
            if !out.contains(&t.source) {
                out.push(t.source);
            }
        }
        out
    }
}

/// A turn the hub could not file with confidence.
#[derive(Clone, Debug, PartialEq)]
pub struct InboxItem {
    pub key: String,
    pub source: Source,
    pub place: String,
    pub text: String,
    pub when: String,
    /// (topic id, confidence such as `0.74`); the first is the best guess.
    pub suggestions: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Workspace {
    pub topics: Vec<TopicView>,
    pub inbox: Vec<InboxItem>,
    /// `Inbox` for turns a classifier would file, `Unfiled` for chats no
    /// route files.
    pub inbox_title: String,
    pub inbox_note: String,
    /// Gateway host and agent for the sidebar footer.
    pub host: String,
    pub agent: String,
    /// ISO date of today, for the agenda.
    pub today: String,
    /// Turns of the general (non-topic) chat.
    pub general_chat: Vec<ChatLine>,
}

impl Workspace {
    pub fn find(&self, id: &str) -> Option<&TopicView> {
        self.all().find(|t| t.id == id)
    }

    pub fn find_mut(&mut self, id: &str) -> Option<&mut TopicView> {
        self.topics.iter_mut().find_map(|t| t.find_mut(id))
    }

    /// Every topic, children after their parent.
    pub fn all(&self) -> impl Iterator<Item = &TopicView> {
        self.topics.iter().flat_map(TopicView::all)
    }
}
