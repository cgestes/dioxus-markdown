//! Topics in a SilverBullet space that a ZeroClaw agent maintains.
//!
//! The space is the source of truth. A top-level folder is a topic (`car/`,
//! `wedding/`) and its sub-folders are sub-topics. Pages in `people/`, and any
//! page a channel routes to directly, are topics of their own
//! (`people/nikki-sample`). The space README maps Slack and Telegram chats to
//! folders, `reminders.md` holds every deadline, and `<folder>/questions.md`
//! holds open questions. This module only parses; `hub` does the I/O.
pub use margin_editor_core::heading::{h1, page_title, split_heading};
use margin_editor_core::{
    dates,
    preview::{self, Block},
    table,
};
use pulldown_cmark::{Event, Options, Parser, Tag};
use serde::Deserialize;
use std::collections::HashMap;

/// One entry of SilverBullet's `GET /.fs` listing.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SpaceFile {
    pub name: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Folder,
    Page,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Topic {
    /// Space path without extension: `car`, `people/nikki-sample`.
    pub id: String,
    pub title: String,
    /// The rest of the first heading: `Alex & Sam (2028)`.
    pub subtitle: String,
    pub kind: Kind,
    /// Files directly inside a folder topic (space paths with extension).
    /// A page topic lists only its own page.
    pub files: Vec<String>,
    pub children: Vec<Topic>,
}

/// Roots that belong to SilverBullet itself, not to the agent's topics.
/// Margin's own files (`_margin/comments/…`) are not a topic either.
const HIDDEN: [&str; 3] = ["Library", "Repositories", margin_editor_core::comments::ROOT];
/// Folders whose pages are each a topic (one file per person). Their README
/// relates each page to its owner (`Relationship…` and `File` columns).
pub const REGISTRIES: [&str; 1] = ["people"];

impl Topic {
    fn folder(id: &str) -> Self {
        Self {
            id: id.into(),
            title: title_from_slug(basename(id)),
            subtitle: String::new(),
            kind: Kind::Folder,
            files: Vec::new(),
            children: Vec::new(),
        }
    }

    fn page(path: &str) -> Self {
        let id = path.strip_suffix(".md").unwrap_or(path);
        Self {
            id: id.into(),
            title: title_from_slug(basename(id)),
            subtitle: String::new(),
            kind: Kind::Page,
            files: vec![path.into()],
            children: Vec::new(),
        }
    }

    /// The page shown as the topic's overview: the README, else the folder's
    /// main page (not its log, question bank or templates).
    pub fn home(&self) -> Option<&str> {
        match self.kind {
            Kind::Page => self.files.first().map(String::as_str),
            Kind::Folder => {
                let readme = |f: &&String| basename(f) == "README.md";
                let main = |f: &&String| {
                    let name = basename(f);
                    f.ends_with(".md")
                        && !name.starts_with('_')
                        && !matches!(name, "log.md" | "questions.md")
                };
                let mut files = self.files.iter();
                files
                    .clone()
                    .find(readme)
                    .or_else(|| files.find(main))
                    .map(String::as_str)
            }
        }
    }

    /// Files listed under Docs: everything but the overview and templates.
    pub fn docs(&self) -> impl Iterator<Item = &str> {
        let home = self.home();
        self.files
            .iter()
            .map(String::as_str)
            .filter(move |f| Some(*f) != home && !basename(f).starts_with('_'))
    }

    /// A file of this folder by basename, such as `questions.md`.
    pub fn file(&self, name: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|f| f.strip_prefix(&self.id).and_then(|r| r.strip_prefix('/')) == Some(name))
            .map(String::as_str)
    }

    pub fn find(&self, id: &str) -> Option<&Topic> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(id))
    }

    fn find_mut(&mut self, id: &str) -> Option<&mut Topic> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter_mut().find_map(|c| c.find_mut(id))
    }

    /// Short slug used for reminder ids: `car`, `nikki` for `people/nikki-sample`.
    fn stem(&self) -> &str {
        let last = basename(&self.id);
        match self.kind {
            Kind::Folder => last,
            Kind::Page => last.split('-').next().unwrap_or(last),
        }
    }

    /// Apply the page's first heading: `# Weddings — Alex & Sam` gives the
    /// title `Weddings` and the subtitle `Alex & Sam`.
    pub fn retitle(&mut self, markdown: &str) {
        if let Some(heading) = h1(markdown) {
            let (title, rest) = split_heading(heading);
            if !title.is_empty() {
                self.title = title.into();
                self.subtitle = rest.into();
            }
        }
    }
}

pub fn find<'a>(topics: &'a [Topic], id: &str) -> Option<&'a Topic> {
    topics.iter().find_map(|t| t.find(id))
}

pub fn find_mut<'a>(topics: &'a mut [Topic], id: &str) -> Option<&'a mut Topic> {
    topics.iter_mut().find_map(|t| t.find_mut(id))
}

/// Every topic, each followed by its sub-topics.
pub fn all(topics: &[Topic]) -> Vec<&Topic> {
    let mut out = Vec::new();
    let mut stack: Vec<&Topic> = topics.iter().rev().collect();
    while let Some(t) = stack.pop() {
        out.push(t);
        stack.extend(t.children.iter().rev());
    }
    out
}

/// Retitle every topic from its overview page, when `text` has it.
pub fn retitle_all<'a>(topics: &mut [Topic], text: &impl Fn(&str) -> Option<&'a str>) {
    for t in topics {
        if let Some(md) = t.home().and_then(text) {
            t.retitle(md);
        }
        retitle_all(&mut t.children, text);
    }
}

/// The last segment of a space path.
pub fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// `nikki-sample` → `Nikki Sample`.
pub fn title_from_slug(slug: &str) -> String {
    slug.split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Build the topic tree from the space listing.
pub fn topics(files: &[SpaceFile]) -> Vec<Topic> {
    let mut roots: Vec<Topic> = Vec::new();
    for file in files {
        let parts: Vec<&str> = file.name.split('/').collect();
        if parts.len() < 2 || HIDDEN.contains(&parts[0]) || parts.iter().any(|p| p.starts_with('.')) {
            continue;
        }
        let folder = folder_at(&mut roots, &parts[..parts.len() - 1], "");
        let name = basename(&file.name);
        let is_person = REGISTRIES.contains(&folder.id.as_str())
            && name.ends_with(".md")
            && !name.starts_with('_')
            && name != "README.md";
        if is_person {
            folder.children.push(Topic::page(&file.name));
        } else {
            folder.files.push(file.name.clone());
        }
    }
    sort(&mut roots);
    roots
}

fn folder_at<'a>(list: &'a mut Vec<Topic>, path: &[&str], prefix: &str) -> &'a mut Topic {
    let id = if prefix.is_empty() {
        path[0].to_string()
    } else {
        format!("{prefix}/{}", path[0])
    };
    let i = match list.iter().position(|t| t.id == id) {
        Some(i) => i,
        None => {
            list.push(Topic::folder(&id));
            list.len() - 1
        }
    };
    if path.len() == 1 {
        &mut list[i]
    } else {
        folder_at(&mut list[i].children, &path[1..], &id)
    }
}

fn sort(list: &mut [Topic]) {
    list.sort_by(|a, b| a.id.cmp(&b.id));
    for t in list {
        t.files.sort();
        sort(&mut t.children);
    }
}

/// A route target that is a page becomes a topic of its own.
pub fn promote_routed_pages(topics: &mut [Topic], routes: &[Route]) {
    for route in routes {
        let Some((folder, _)) = route.target.rsplit_once('/') else {
            continue;
        };
        let page = format!("{}.md", route.target);
        if let Some(parent) = find_mut(topics, folder)
            && let Some(i) = parent.files.iter().position(|f| *f == page)
        {
            let file = parent.files.remove(i);
            parent.children.push(Topic::page(&file));
            parent.children.sort_by(|a, b| a.id.cmp(&b.id));
        }
    }
}

/// A Markdown table with the heading it sits under.
#[derive(Clone, Debug, PartialEq)]
pub struct MdTable {
    pub heading: Option<String>,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl MdTable {
    pub fn column(&self, name: &str) -> Option<usize> {
        self.headers.iter().position(|h| h.eq_ignore_ascii_case(name))
    }

    /// The first column whose header starts with `prefix`, ignoring case.
    pub fn column_starting(&self, prefix: &str) -> Option<usize> {
        self.headers
            .iter()
            .position(|h| h.to_lowercase().starts_with(&prefix.to_lowercase()))
    }

    fn get(&self, row: &[String], name: &str) -> String {
        self.column(name)
            .and_then(|c| row.get(c))
            .map(|s| plain(s))
            .unwrap_or_default()
    }
}

pub fn md_tables(md: &str) -> Vec<MdTable> {
    table::tables(md)
        .into_iter()
        .map(|t| {
            let mut rows = t.texts(md).into_iter();
            MdTable {
                heading: table::heading_above(md, t.range.start).map(String::from),
                headers: rows.next().unwrap_or_default(),
                rows: rows.collect(),
            }
        })
        .collect()
}

/// Whether a table cell says nothing: empty, a dash, or `_TBD_`.
pub fn blank(s: &str) -> bool {
    let t = s.trim().trim_matches('_');
    t.is_empty() || t.chars().all(|c| !c.is_alphanumeric()) || t.eq_ignore_ascii_case("tbd")
}

/// One line of inline Markdown, read by pulldown-cmark.
#[derive(Default)]
struct Inline {
    text: String,
    code: Vec<String>,
    links: Vec<String>,
}

fn inline(s: &str) -> Inline {
    let mut out = Inline::default();
    // A leading letter keeps the line a paragraph, so `- x` or `1. x` stay literal.
    let source = format!("x{s}");
    for event in Parser::new_ext(&source, Options::ENABLE_STRIKETHROUGH) {
        match event {
            Event::Text(t) => out.text.push_str(&t),
            Event::Code(c) => {
                out.text.push_str(&c);
                out.code.push(c.to_string());
            }
            Event::Start(Tag::Link { dest_url, .. }) => out.links.push(dest_url.to_string()),
            Event::SoftBreak | Event::HardBreak => out.text.push(' '),
            _ => {}
        }
    }
    out.text = out.text.strip_prefix('x').unwrap_or(&out.text).trim().to_string();
    out
}

/// Inline Markdown to display text: links keep their label, emphasis and
/// code punctuation go away.
pub fn plain(s: &str) -> String {
    inline(s).text
}

/// Task items of a page, as the notebook reads them: (text, checked).
pub fn tasks(md: &str) -> Vec<(String, bool)> {
    preview::document(md)
        .into_iter()
        .filter_map(|block| match block {
            Block::Text { passage } if passage.kind == "item" => {
                let (first, rest) = passage.runs.split_first()?;
                (first.style == "box").then(|| {
                    let text: String = rest.iter().map(|r| r.text.as_str()).collect();
                    (text.trim().to_string(), first.text.starts_with('☑'))
                })
            }
            _ => None,
        })
        .collect()
}

/// A registry README's relations: page path → relationship, from its
/// `Relationship…` and `File` columns.
pub fn relations(readme: &str, folder: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for t in md_tables(readme) {
        let (Some(file), Some(rel)) = (t.column("File"), t.column_starting("Relationship")) else {
            continue;
        };
        for row in &t.rows {
            let (Some(cell), Some(r)) = (row.get(file), row.get(rel)) else {
                continue;
            };
            if let Some(target) = inline(cell).links.into_iter().next() {
                out.insert(format!("{folder}/{target}"), plain(r));
            }
        }
    }
    out
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Source {
    Telegram,
    Slack,
    Chat,
    #[default]
    Other,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Self::Telegram => "Telegram",
            Self::Slack => "Slack",
            Self::Chat => "Chat",
            Self::Other => "Channel",
        }
    }
}

/// One row of the space README's "Channel → folder" table.
#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    pub label: String,
    pub channel: String,
    /// Folder or page id: `car`, `people/nikki-sample`.
    pub target: String,
}

impl Route {
    /// Whether a session key belongs to this route's chat. Keys are
    /// `_`-separated, so the chat id must be a whole part of the key.
    pub fn matches(&self, key: &str) -> bool {
        key.split('_').any(|part| part == self.channel)
    }
}

pub fn routes(readme: &str) -> Vec<Route> {
    let mut out = Vec::new();
    for t in md_tables(readme) {
        let (Some(id), Some(folder)) = (t.column("Channel ID"), t.column("Folder")) else {
            continue;
        };
        for row in &t.rows {
            let channel = row.get(id).map(|s| plain(s)).unwrap_or_default();
            if blank(&channel) {
                continue;
            }
            let cell = inline(row.get(folder).map(String::as_str).unwrap_or_default());
            let target = cell
                .code
                .into_iter()
                .next()
                .unwrap_or_else(|| cell.text.split_whitespace().next().unwrap_or_default().into());
            let target = target.trim_end_matches('/').trim_end_matches(".md").to_string();
            if target.is_empty() {
                continue;
            }
            out.push(Route {
                label: plain(row.first().map(String::as_str).unwrap_or_default()),
                channel,
                target,
            });
        }
    }
    out
}

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
    if let Some(thread) = bare.strip_prefix(margin_editor_core::comments::SESSION_PREFIX) {
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
    if let Some(rest) = margin_editor_core::comments::strip_preamble(text) {
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

#[cfg(test)]
mod tests {
    use super::*;

    const README: &str = "# Procedures\n\n## Channel → folder map\n\n| Slack channel | Channel ID   | Folder |\n|---------------|--------------|--------|\n| DM with luc   | D0DM0000001  | `dm/`  |\n| #car          | C0CAR000001  | `car/` |\n| Telegram car group | -1001000001 | `car/` |\n| Telegram Nikki group | -1001000002 | `people/nikki-sample.md` — everything about Nikki. |\n| Telegram Nikki group (wedding topics) | -1001000002 | `wedding/` — its own project |\n| _(all channels)_ | —         | `people/` — global people registry |\n";

    const REMINDERS: &str = "# Reminders\n\n## Active items\n| ID | What | Recurrence | Deadline | Start | Status | Next ping | Last ping | Deliver to | Source file | Notes |\n|----|------|------------|----------|-------|--------|-----------|-----------|------------|-------------|-------|\n| car-ct-2027 | Book the **CT** | every 2 years | 2027-06-25 | 2027-03-25 | pending | 2027-03-25 | — | Telegram car group | `car/maintenance.md` | Last CT 2025 |\n| nikki-nz-wedding-confirm | Borland: check payment | one-off | 2026-12-01 | 2026-11-26 | pending | 2026-11-26 | — | Telegram DM | `wedding/questions.md` | [Borland thread](https://example.com) |\n| car-sensor | Fix the parking sensor | — | 2027-01-10 | — | pending | — | — | Telegram DM | `people/dana-anghel.md` | |\n\n## Done / cancelled\n| ID | What | Closed on | Outcome |\n|----|------|-----------|---------|\n";

    fn files(names: &[&str]) -> Vec<SpaceFile> {
        names
            .iter()
            .map(|n| SpaceFile {
                name: (*n).into(),
                ..Default::default()
            })
            .collect()
    }

    fn space() -> Vec<Topic> {
        let mut t = topics(&files(&[
            "README.md",
            "reminders.md",
            "Library/Std/Thing.md",
            "_margin/comments/car/log.md.json",
            "Repositories/Std.md",
            "car/log.md",
            "car/maintenance.md",
            "car/invoice.pdf",
            "people/README.md",
            "people/_template.md",
            "people/nikki-sample.md",
            "people/dana-anghel.md",
            "wedding/README.md",
            "wedding/questions.md",
            "house/hot-water/notes.md",
        ]));
        promote_routed_pages(&mut t, &routes(README));
        t
    }

    #[test]
    fn folders_become_topics_and_people_become_page_topics() {
        let t = space();
        let ids: Vec<_> = t.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["car", "house", "people", "wedding"]);
        let people = find(&t, "people").unwrap();
        assert_eq!(people.home(), Some("people/README.md"));
        let kids: Vec<_> = people.children.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(kids, ["Dana Anghel", "Nikki Sample"]);
        let nikki = find(&t, "people/nikki-sample").unwrap();
        assert_eq!(nikki.kind, Kind::Page);
        assert_eq!(nikki.home(), Some("people/nikki-sample.md"));
        assert_eq!(people.docs().count(), 0, "templates and README are not docs");
        let car = find(&t, "car").unwrap();
        assert_eq!(car.home(), Some("car/maintenance.md"), "no README: the main page");
        assert_eq!(car.docs().collect::<Vec<_>>(), ["car/invoice.pdf", "car/log.md"]);
        assert_eq!(find(&t, "wedding").unwrap().file("questions.md"), Some("wedding/questions.md"));
        assert!(find(&t, "house/hot-water").is_some(), "sub-folders are sub-topics");
        let order: Vec<_> = all(&t).iter().map(|t| t.id.as_str()).collect();
        assert_eq!(&order[..4], ["car", "house", "house/hot-water", "people"]);
    }

    #[test]
    fn titles_come_from_the_first_heading() {
        let mut t = Topic::folder("wedding");
        assert_eq!(t.title, "Wedding");
        t.retitle("# Weddings — Alex & Sam (2028)\n\nText");
        assert_eq!((t.title.as_str(), t.subtitle.as_str()), ("Weddings", "Alex & Sam (2028)"));
        t.retitle("# DM: general");
        assert_eq!((t.title.as_str(), t.subtitle.as_str()), ("DM", "general"));
        assert_eq!(split_heading("Car (Mazda 3)"), ("Car", "(Mazda 3)"));
    }

    #[test]
    fn readme_routes_chats_to_folders_and_pages() {
        let r = routes(README);
        assert_eq!(r.len(), 5, "the all-channels row has no id");
        assert_eq!(r[1].target, "car");
        assert_eq!(r[3].target, "people/nikki-sample");
        assert_eq!(r[4].target, "wedding");
        assert_eq!(r[0].label, "DM with luc");
        assert!(r[2].matches("telegram_telegram_-1001000001_cgestes"));
        assert!(!r[2].matches("telegram_telegram_-10010000010_cgestes"), "whole parts only");
    }

    #[test]
    fn reminders_belong_to_the_most_specific_topic() {
        let all = reminders(REMINDERS);
        assert_eq!(all.len(), 3, "the done table has no Deadline column");
        assert_eq!(all[0].what, "Book the CT");
        assert_eq!(all[0].source, "car/maintenance.md");
        assert_eq!(all[1].notes, "Borland thread");
        assert!(all[0].repeats() && !all[1].repeats() && !all[2].repeats());
        let t = space();
        let of = |id: &str| -> Vec<String> {
            let topic = find(&t, id).unwrap();
            all.iter().filter(|r| r.concerns(topic)).map(|r| r.id.clone()).collect()
        };
        assert_eq!(of("car"), ["car-ct-2027", "car-sensor"]);
        assert_eq!(of("wedding"), ["nikki-nz-wedding-confirm"]);
        assert_eq!(of("people/nikki-sample"), ["nikki-nz-wedding-confirm"]);
        let owners: Vec<_> = all.iter().map(|r| r.owner(&t)).collect();
        assert_eq!(owners, [Some("car"), Some("wedding"), Some("people/dana-anghel")]);
        assert_eq!(all[1].due(), "2026-11-26");
    }

    #[test]
    fn planning_key_dates_keep_the_whole_entry_and_only_read_the_named_section() {
        let md = "# Planning\n\n- 2031-04-01 outside the section\n\n## Key dates ⭐\n\n\
                  A note written on 2031-04-02.\n\n\
                  - **Tue 2031-04-08: important**. Meet at 10:30.\n  Bring the forms.\n\
                  - 2031-04-05 — [Workshop](details.md), heads-up 2031-04-04.\n\
                  - 2031-02-30 invalid date\n\n\
                  ```md\n- 2031-04-06 example only\n```\n\n\
                  ## Day brief\n\n- 2031-04-07 outside the section\n";
        let entries = key_dates(md);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].day, dates::parse_date("2031-04-05").unwrap());
        assert_eq!(entries[1].day, dates::parse_date("2031-04-08").unwrap());
        assert_eq!(entries[1].markdown, "**Tue 2031-04-08: important**. Meet at 10:30.\n  Bring the forms.");
    }

    #[test]
    fn headings_in_quotes_and_lists_do_not_change_the_planning_section() {
        let md = "\
> ## Key dates

- 2031-04-01 Outside

- ## Key dates

- 2031-04-02 Also outside

## Key dates

- 2031-04-08 Appointment
  - Bring the forms

> ## Example
> - 2031-04-09 Quoted example

- ## Checklist

- 2031-04-10 Visit";
        let entries = key_dates(md);
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.markdown.as_str())
                .collect::<Vec<_>>(),
            [
                "2031-04-08 Appointment\n  - Bring the forms",
                "2031-04-10 Visit"
            ]
        );
    }

    #[test]
    fn questions_split_open_and_answered() {
        let md = "# Wedding — open questions\n\n## Open\n| ID | Question | Why it matters | Ask when… | Ask by |\n|----|----------|----------------|-----------|--------|\n| Q1 | Which wedding is **legal**? | Paperwork | mairie | 2027-03 |\n\n## Answered\n| ID | Question | Answer | Date | Source |\n|----|----------|--------|------|--------|\n| Q0 | Year? | **2028** | 2026-11-24 | TG |\n";
        let q = questions(md);
        assert_eq!(q.len(), 2);
        assert!(q[0].open && !q[1].open);
        assert_eq!(q[0].question, "Which wedding is legal?");
        assert_eq!(q[0].ask_by, "2027-03");
        assert_eq!(q[1].answer, "2028");
    }

    #[test]
    fn session_keys_parse_to_their_origin() {
        assert_eq!(
            origin("telegram_telegram_-1001000001_cgestes"),
            Origin::Telegram { chat: "-1001000001".into() }
        );
        assert_eq!(
            origin("slack_main_C0CAR000001_1795508185_314769_U0CKVEP9VQW"),
            Origin::Slack { channel: "C0CAR000001".into(), thread: "1795508185.314769".into() }
        );
        assert_eq!(origin("gw_topic-people-nikki-sample"), Origin::Topic("people-nikki-sample".into()));
        assert_eq!(origin("gw_margin-general"), Origin::General);
        assert_eq!(origin("gw_08aac21c"), Origin::Gateway);
        assert_eq!(origin("rpc_280e"), Origin::Other);
        assert_eq!(chat_session("people/nikki-sample"), "topic-people-nikki-sample");
    }

    #[test]
    fn sessions_are_filed_by_routed_chat_id() {
        let session = |key: &str, at: &str| Session {
            session_id: key.into(),
            session_key: key.into(),
            last_activity: at.into(),
            message_count: 3,
            ..Default::default()
        };
        let sessions = [
            session("telegram_telegram_-1001000001_cgestes", "2026-11-24T10:00:00Z"),
            session("slack_main_C0CAR000001_1732400000_1_U0", "2026-11-24T12:00:00Z"),
            session("telegram_telegram_-1001000002_cgestes", "2026-11-23T09:00:00Z"),
            session("gw_topic-car", "2026-11-20T09:00:00Z"),
        ];
        let t = space();
        let r = routes(README);
        let car = threads(find(&t, "car").unwrap(), &r, &sessions);
        let labels: Vec<_> = car.iter().map(|t| (t.label.as_str(), t.origin.source())).collect();
        assert_eq!(
            labels,
            [
                ("#car", Source::Slack),
                ("Telegram car group", Source::Telegram),
                ("Margin chat", Source::Chat)
            ]
        );
        assert_eq!(threads(find(&t, "people/nikki-sample").unwrap(), &r, &sessions).len(), 1);
        assert_eq!(threads(find(&t, "wedding").unwrap(), &r, &sessions).len(), 1);
    }

    #[test]
    fn unrouted_chats_are_unfiled_but_margins_own_are_not() {
        let s = |key: &str, n: u64| Session {
            session_id: key.into(),
            session_key: key.into(),
            message_count: n,
            ..Default::default()
        };
        let u = unfiled(
            &routes(README),
            &[
                s("telegram_telegram_1000000001_cgestes", 69),
                s("telegram_telegram_-1001000001_cgestes", 50),
                s("gw_08aac21c", 35),
                s("gw_topic-car", 4),
                s("gw_margin-general", 2),
                s("rpc_280e", 0),
            ],
        );
        let labels: Vec<_> = u.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(labels, ["Telegram DM", "Dashboard chat"]);
    }

    #[test]
    fn transcripts_keep_what_people_said() {
        let m = |role: &str, content: &str| Message {
            role: role.into(),
            content: content.into(),
            created_at: Some("2026-11-24T13:56:20.1+00:00".into()),
        };
        let first = format!("{}\n\nIs the CT booked?", preamble(&chat_context("Car", "car/", 1, 0)));
        let t = transcript(&[
            m("user", "[2026-11-24 14:56:19 +01:00] Done for gmail"),
            m("assistant", r#"{"content":null,"tool_calls":[{"id":"1","name":"file_edit","arguments":"{}"}]}"#),
            m("tool", r#"{"content":"Edited","tool_call_id":"1"}"#),
            m("assistant", "Corrected **everywhere**."),
            m("assistant", "NO_REPLY"),
            m("user", &first),
        ]);
        assert_eq!(t.len(), 3);
        assert!(t[0].from_user);
        assert_eq!(t[0].text, "Done for gmail");
        assert_eq!(t[0].at, "2026-11-24 14:56");
        assert_eq!(t[1].text, "Corrected **everywhere**.");
        assert_eq!(t[1].tools, ["file_edit"]);
        assert_eq!(t[1].at, "2026-11-24 13:56");
        assert_eq!(t[2].text, "Is the CT booked?", "the context preamble is hidden");
    }

    #[test]
    fn registries_relate_pages_to_people() {
        let readme = "## Index\n| Person | Relationship to me | Circle | File |\n|---|---|---|---|\n| Nikki Sample | Wife | Family | [nikki-sample.md](nikki-sample.md) |\n| Dr A | Doctor (pregnancy) | Professional | [a.md](a.md) |\n";
        let r = relations(readme, "people");
        assert_eq!(r.get("people/nikki-sample.md").map(String::as_str), Some("Wife"));
        assert_eq!(r.get("people/a.md").map(String::as_str), Some("Doctor (pregnancy)"));
    }

    #[test]
    fn task_items_are_read_like_the_notebook() {
        let md = "# Groceries ideas\n- [ ] Mango\n- [x] **Basil**\n1. [ ] Bread\n- plain\n\n```\n- [ ] not a task\n```\n";
        assert_eq!(
            tasks(md),
            [("Mango".to_string(), false), ("Basil".to_string(), true), ("Bread".to_string(), false)]
        );
    }

    #[test]
    fn plain_text_drops_inline_markup() {
        assert_eq!(plain("**2027-04** or `x` [a](b) c"), "2027-04 or x a c");
        assert_eq!(plain("[ ] not a link"), "[ ] not a link");
        assert_eq!(plain("- not a list"), "- not a list");
        assert!(blank("_TBD_") && blank("—") && !blank("2027-03"));
    }
}
