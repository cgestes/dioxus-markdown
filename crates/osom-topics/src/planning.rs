//! What is due and what is open: reminders, key dates and questions.
use crate::page::{blank, md_tables, plain};
use crate::space::{Kind, Topic, all};
use margin_editor_core::dates;
use pulldown_cmark::{Event, Parser, Tag};

/// One row of `reminders.md` "Active items".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reminder {
    pub id: String,
    pub what: String,
    pub recurrence: String,
    pub deadline: String,
    pub status: String,
    pub next_ping: String,
    pub deliver_to: String,
    pub source: String,
    pub notes: String,
}

impl Reminder {
    /// Whether its source file lies in the topic.
    fn filed_in(&self, topic: &Topic) -> bool {
        match topic.kind {
            Kind::Folder => self
                .source
                .strip_prefix(&topic.id)
                .is_some_and(|rest| rest.starts_with('/')),
            Kind::Page => self.source.strip_suffix(".md") == Some(&topic.id),
        }
    }

    /// Whether its id starts with the topic's stem (`car-…`, `nikki-…`).
    fn named_for(&self, topic: &Topic) -> bool {
        self.id.split('-').next() == Some(topic.stem())
    }

    pub fn concerns(&self, topic: &Topic) -> bool {
        self.filed_in(topic) || self.named_for(topic)
    }

    /// The topic it belongs to: the most specific one holding its source
    /// file, else the first one its id is named for.
    pub fn owner<'a>(&self, topics: &'a [Topic]) -> Option<&'a str> {
        let every = all(topics);
        every
            .iter()
            .filter(|t| self.filed_in(t))
            .max_by_key(|t| t.id.len())
            .or_else(|| every.iter().find(|t| self.named_for(t)))
            .map(|t| t.id.as_str())
    }

    pub fn open(&self) -> bool {
        !matches!(
            self.status.to_ascii_lowercase().as_str(),
            "done" | "cancelled" | "canceled"
        )
    }

    pub fn repeats(&self) -> bool {
        !blank(&self.recurrence) && !self.recurrence.to_ascii_lowercase().starts_with("one")
    }

    /// The date that orders the agenda: the next ping, else the deadline.
    pub fn due(&self) -> &str {
        if self.next_ping.starts_with(|c: char| c.is_ascii_digit()) {
            &self.next_ping
        } else {
            &self.deadline
        }
    }
}

pub fn reminders(md: &str) -> Vec<Reminder> {
    let Some(t) = md_tables(md)
        .into_iter()
        .find(|t| t.column("ID").is_some() && t.column("Deadline").is_some())
    else {
        return Vec::new();
    };
    t.rows
        .iter()
        .map(|r| Reminder {
            id: t.get(r, "ID"),
            what: t.get(r, "What"),
            recurrence: t.get(r, "Recurrence"),
            deadline: t.get(r, "Deadline"),
            status: t.get(r, "Status"),
            next_ping: t.get(r, "Next ping"),
            deliver_to: t.get(r, "Deliver to"),
            source: t.get(r, "Source file"),
            notes: t.get(r, "Notes"),
        })
        .filter(|r| !r.id.is_empty())
        .collect()
}

/// The same planning page the agent reads for its day brief.
pub const PLANNING_PAGE: &str = "planning/README.md";

#[derive(Clone, Debug, PartialEq)]
pub struct KeyDate {
    pub day: i64,
    pub markdown: String,
}

impl KeyDate {
    fn from_list_item(item: &str) -> Option<Self> {
        let (_, body) = item.trim().split_once(char::is_whitespace)?;
        let markdown = body.trim();
        let day = plain(markdown)
            .split(|c: char| !c.is_ascii_digit() && c != '-')
            .filter(|token| token.len() == 10)
            .find_map(|token| {
                let day = dates::parse_date(token)?;
                // parse_date normalizes impossible dates such as February 30.
                (dates::iso_date(day * 86400) == token).then_some(day)
            })?;
        Some(Self {
            day,
            markdown: markdown.to_owned(),
        })
    }
}

/// Dated list entries under the planning page's “Key dates” heading.
/// Keep the whole entry, including emphasis and the times the agent wrote.
pub fn key_dates(md: &str) -> Vec<KeyDate> {
    let mut section_level = None;
    let mut skip_until = 0;
    let mut entries = Vec::new();
    for (event, range) in Parser::new(md).into_offset_iter() {
        if range.start < skip_until {
            continue;
        }
        match event {
            Event::Start(Tag::BlockQuote(_)) => skip_until = range.end,
            Event::Start(Tag::Heading { level, .. })
                if section_level.is_none_or(|parent| level <= parent) =>
            {
                let title = plain(md[range].trim().trim_matches('#').trim());
                section_level = title
                    .trim_matches(|c: char| !c.is_alphanumeric())
                    .eq_ignore_ascii_case("key dates")
                    .then_some(level);
            }
            Event::Start(Tag::Item) => {
                // A list item owns its nested bullets, headings and wrapped lines.
                skip_until = range.end;
                if section_level.is_some()
                    && let Some(entry) = KeyDate::from_list_item(&md[range])
                {
                    entries.push(entry);
                }
            }
            _ => {}
        }
    }
    entries.sort_by_key(|entry| entry.day);
    entries
}

/// One row of a `questions.md` bank.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Question {
    pub id: String,
    pub question: String,
    pub why: String,
    pub ask_when: String,
    pub ask_by: String,
    pub answer: String,
    pub open: bool,
}

pub fn questions(md: &str) -> Vec<Question> {
    let mut out = Vec::new();
    for t in md_tables(md) {
        if t.column("Question").is_none() {
            continue;
        }
        let open = !t
            .heading
            .as_deref()
            .is_some_and(|h| h.to_ascii_lowercase().starts_with("answered"));
        out.extend(t.rows.iter().map(|r| Question {
            id: t.get(r, "ID"),
            question: t.get(r, "Question"),
            why: t.get(r, "Why it matters"),
            ask_when: t.get(r, "Ask when…"),
            ask_by: t.get(r, "Ask by"),
            answer: t.get(r, "Answer"),
            open,
        }));
    }
    out
}
