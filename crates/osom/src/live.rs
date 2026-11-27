//! Where the Topics window's data comes from: the live space and agent,
//! built into a `view::Workspace`.
use crate::view::*;
use futures_util::stream::{self, StreamExt};
use margin_core::{dates, heading};
use osom_hub::{AgentChat, Endpoints, Page, Space, ZeroClaw};
use osom_topics::{self as model, Kind, Origin, Reminder, Session, Topic, calendar};
use std::collections::HashMap;
use std::sync::Arc;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

/// The space and the agent, once configured.
#[derive(Clone)]
pub(super) struct Live {
    pub endpoints: Endpoints,
    pub space: Space,
    pub zc: ZeroClaw,
}

impl Live {
    pub fn new(endpoints: Endpoints) -> Self {
        Self {
            space: Space::with_web_base(&endpoints.space, &endpoints.space_web),
            zc: endpoints.zeroclaw(),
            endpoints,
        }
    }

    /// Who answers the topic chat.
    pub fn agent(&self) -> Arc<dyn AgentChat> {
        Arc::new(self.zc.clone())
    }

    /// The topic chat's agent alias.
    pub fn alias(&self) -> String {
        self.endpoints.agent.clone()
    }

    /// Whether this device has paired with ZeroClaw.
    pub fn paired(&self) -> bool {
        self.zc.paired()
    }
}

/// Concurrent requests the loader keeps in flight at once (matches `osom-hub`'s pool).
const WORKERS: usize = 8;
const REMINDERS_PAGE: &str = "reminders.md";

/// Everything read from the space.
#[derive(Default)]
struct SpaceData {
    topics: Vec<Topic>,
    routes: Vec<model::Route>,
    reminders: Vec<Reminder>,
    /// Listed agenda pages still unreadable after a retry.
    agenda_errors: Vec<(&'static str, String)>,
    /// Registry pages → relationship (`people/alex-sample.md` → `Partner`).
    relations: HashMap<String, String>,
    pages: HashMap<String, Page>,
    sizes: HashMap<String, u64>,
}

/// Read pages concurrently, skipping those that fail.
async fn read_all(space: &Space, paths: Vec<String>) -> HashMap<String, Page> {
    stream::iter(paths)
        .map(|p| async move { space.read(&p).await.ok() })
        .buffer_unordered(WORKERS)
        .filter_map(|p| async move { p.map(|page| (page.path.clone(), page)) })
        .collect()
        .await
}

async fn load_space(space: &Space) -> Result<SpaceData, String> {
    // The listing, the routes and the reminders at once, then every page.
    let (files, mut pages) = futures_util::future::join(
        space.list(),
        read_all(space, vec!["README.md".into(), REMINDERS_PAGE.into()]),
    )
    .await;
    let files = files?;
    let readme = pages.remove("README.md").map(|p| p.text).unwrap_or_default();
    let mut topics = model::topics(&files);
    let routes = model::routes(&readme);
    model::promote_routed_pages(&mut topics, &routes);
    let wanted: Vec<String> = model::all(&topics)
        .into_iter()
        .flat_map(|t| t.files.iter().cloned())
        .filter(|f| f.ends_with(".md"))
        .collect();
    pages.extend(read_all(space, wanted).await);
    let mut agenda_errors = Vec::new();
    for path in [REMINDERS_PAGE, model::PLANNING_PAGE] {
        if pages.contains_key(path) || !files.iter().any(|f| f.name == path) {
            continue;
        }
        match space.read_optional(path).await {
            Ok(Some(page)) => {
                pages.insert(page.path.clone(), page);
            }
            Ok(None) => {} // Deleted between the listing and the read.
            Err(error) => agenda_errors.push((path, error)),
        }
    }
    let reminders = pages
        .remove(REMINDERS_PAGE)
        .map(|p| model::reminders(&p.text))
        .unwrap_or_default();
    model::retitle_all(&mut topics, &|p| pages.get(p).map(|page| page.text.as_str()));
    let relations = model::REGISTRIES
        .iter()
        .filter_map(|folder| Some(model::relations(&pages.get(&format!("{folder}/README.md"))?.text, folder)))
        .flatten()
        .collect();
    Ok(SpaceData {
        topics,
        routes,
        reminders,
        agenda_errors,
        relations,
        pages,
        sizes: files.into_iter().map(|f| (f.name, f.size)).collect(),
    })
}

/// The conversations ZeroClaw keeps, with what was said in each.
#[derive(Default)]
struct Chats {
    sessions: Vec<Session>,
    turns: HashMap<String, Vec<ChatLine>>,
}

async fn load_chats(zc: &ZeroClaw) -> Result<Chats, String> {
    let sessions = zc.sessions().await?;
    let keys: Vec<String> = sessions
        .iter()
        .filter(|x| x.message_count > 0)
        .map(|s| s.key().to_string())
        .collect();
    let turns = stream::iter(keys)
        .map(|key| async move { zc.messages(&key).await.ok().map(|m| (key, model::transcript(&m))) })
        .buffer_unordered(WORKERS)
        .filter_map(|item| async move { item })
        .collect()
        .await;
    Ok(Chats { sessions, turns })
}

/// A finished load: the workspace, the transcripts behind its threads, and
/// what could not be read.
pub(super) struct Loaded {
    pub ws: Workspace,
    /// Every page read at load, by path: opening one needs no round trip.
    pub pages: HashMap<String, Page>,
    pub turns: HashMap<String, Vec<ChatLine>>,
    pub warning: Option<String>,
}

/// Read the space and the agent's conversations concurrently, and build the
/// workspace (one shared async loader for both native and wasm32).
pub(super) async fn load(live: &Live) -> Result<Loaded, String> {
    let chats_fut = async {
        if live.zc.paired() {
            Some(load_chats(&live.zc).await)
        } else {
            None
        }
    };
    let (space, chats) = futures_util::future::join(load_space(&live.space), chats_fut).await;
    let data = space.map_err(|e| format!("SilverBullet: {e}"))?;
    let (chats, warning) = match chats {
        Some(Ok(c)) => (c, None),
        Some(Err(e)) => (Chats::default(), Some(format!("ZeroClaw: {e}"))),
        None => (Chats::default(), None),
    };
    let ws = live_workspace(&data, &chats, live, dates::now());
    let warnings: Vec<_> = data
        .agenda_errors
        .into_iter()
        .map(|(path, error)| format!("Could not read {path} ({error})"))
        .chain(warning)
        .collect();
    Ok(Loaded {
        ws,
        pages: data.pages,
        turns: chats.turns,
        warning: (!warnings.is_empty()).then(|| warnings.join("\n")),
    })
}

fn icon_for(t: &Topic, relation: &str) -> (Icon, Hue) {
    let rel = relation.to_ascii_lowercase();
    if t.kind == Kind::Page {
        return if ["wife", "husband", "partner"].iter().any(|r| rel.contains(r)) {
            (Icon::Heart, Hue::Plum)
        } else {
            (Icon::Person, Hue::Pine)
        };
    }
    match model::basename(&t.id) {
        "car" | "mazda" => (Icon::Car, Hue::Slate),
        "wedding" | "weddings" => (Icon::Rings, Hue::Rose),
        "house" | "home" => (Icon::Home, Hue::Clay),
        "modular" => (Icon::Sliders, Hue::Ochre),
        "people" => (Icon::Users, Hue::Pine),
        "pregnancy" | "baby" => (Icon::Sprout, Hue::Plum),
        "dm" => (Icon::Chat, Hue::Slate),
        _ => (Icon::Folder, Hue::Ochre),
    }
}

fn kind_of(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default().to_ascii_lowercase().as_str() {
        "md" => "MD",
        "pdf" => "PDF",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" => "IMG",
        _ => "FILE",
    }
}

/// What every topic of one load shares.
struct Cx<'a> {
    data: &'a SpaceData,
    chats: &'a Chats,
    space: &'a Space,
    /// The topic each reminder belongs to, by index.
    owners: Vec<Option<&'a str>>,
    now: i64,
}

fn live_topic(t: &Topic, cx: &Cx) -> TopicView {
    let d = cx.data;
    let relation = d.relations.get(&format!("{}.md", t.id)).cloned().unwrap_or_default();
    let (icon, hue) = icon_for(t, &relation);
    let home = t.home().and_then(|h| d.pages.get(h));
    let mut reminders: Vec<(usize, &Reminder)> = d
        .reminders
        .iter()
        .enumerate()
        .filter(|(_, r)| r.open() && r.concerns(t))
        .collect();
    reminders.sort_by(|a, b| a.1.due().cmp(b.1.due()));
    let next = reminders
        .first()
        .map(|(_, r)| format!("{} · {}", r.what, calendar::day_month(r.due())))
        .unwrap_or_default();
    let agenda: Vec<Agenda> = reminders
        .iter()
        .map(|&(i, r)| Agenda {
            title: r.what.clone(),
            repeats: r.repeats(),
            deliver_to: r.deliver_to.clone(),
            detail: format!("{} · deadline {}", r.status, r.deadline),
            tag: r.id.clone(),
            sort: r.due().into(),
            notes: r.notes.clone(),
            primary: cx.owners[i] == Some(t.id.as_str()),
        })
        .collect();
    let threads = model::threads(t, &d.routes, &cx.chats.sessions)
        .iter()
        .map(|th| thread_row(th, cx.now, cx.chats.turns.get(th.session.key())))
        .collect();
    let questions = t
        .file("questions.md")
        .and_then(|p| d.pages.get(p))
        .map(|p| model::questions(&p.text))
        .unwrap_or_default();
    let docs = t
        .docs()
        .map(|f| {
            let size = kb(d.sizes.get(f).copied().unwrap_or(0));
            DocRow {
                kind: kind_of(f).into(),
                name: model::basename(f).into(),
                meta: match d.pages.get(f).and_then(|p| heading::page_title(&p.text)) {
                    Some(h) => format!("{h} · {size}"),
                    None => size,
                },
                path: f.ends_with(".md").then(|| f.into()),
            }
        })
        .collect();
    let lists = t
        .home()
        .into_iter()
        .chain(t.docs())
        .filter_map(|f| d.pages.get(f))
        .filter_map(|page| {
            let items = model::tasks(&page.text);
            let title = heading::page_title(&page.text).unwrap_or(&page.path).to_string();
            let lower = title.to_lowercase();
            (!items.is_empty()).then(|| Checklist {
                shopping: lower.contains("grocer") || lower.contains("shopping"),
                meta: page.path.clone(),
                path: Some(page.path.clone()),
                title,
                items,
            })
        })
        .collect();
    let vault_path = match t.kind {
        Kind::Folder => format!("{}/", t.id),
        Kind::Page => format!("{}.md", t.id),
    };
    TopicView {
        id: t.id.clone(),
        title: t.title.clone(),
        subtitle: if relation.is_empty() { t.subtitle.clone() } else { relation },
        icon,
        hue,
        next,
        overview_path: home.map(|p| p.path.clone()),
        chat_context: model::chat_context(
            &t.title,
            &vault_path,
            agenda.len(),
            questions.iter().filter(|q| q.open).count(),
        ),
        agenda,
        threads,
        docs,
        lists,
        questions,
        web_url: Some(cx.space.page_url(t.home().unwrap_or(&t.id))),
        vault_path,
        children: t.children.iter().map(|c| live_topic(c, cx)).collect(),
        ..Default::default()
    }
}

/// The first line of some Markdown as plain text, at most `n` characters.
fn first_line(md: &str, n: usize) -> String {
    let line = md
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default()
        .trim_start_matches(['#', '-', '*', '>', ' ']);
    let text = model::plain(line);
    if text.chars().count() > n {
        format!("{}…", text.chars().take(n).collect::<String>().trim_end())
    } else {
        text
    }
}

fn thread_row(th: &model::Thread, now: i64, turns: Option<&Vec<ChatLine>>) -> ThreadRow {
    let s = &th.session;
    // Each Slack thread is its own session: name it after its first message.
    let opening = turns
        .and_then(|t| t.iter().find(|x| x.from_user))
        .map(|x| first_line(&x.text, 72))
        .filter(|o| !o.is_empty() && matches!(th.origin, Origin::Slack { .. }));
    let snippet = match turns.and_then(|t| t.last()) {
        Some(last) => format!(
            "{}{}",
            if last.from_user { "you: " } else { "agent: " },
            first_line(&last.text, 160)
        ),
        None => s.name.clone().unwrap_or_else(|| {
            format!("{} messages since {}", s.message_count, s.created_at.get(..10).unwrap_or_default())
        }),
    };
    ThreadRow {
        key: s.key().into(),
        source: th.origin.source(),
        title: opening.unwrap_or_else(|| th.label.clone()),
        snippet,
        place: th.label.clone(),
        when: dates::parse_time(&s.last_activity)
            .map(|t| calendar::ago(now, t))
            .unwrap_or_default(),
        sort: s.last_activity.clone(),
        messages: s.message_count,
        ..Default::default()
    }
}

fn live_workspace(d: &SpaceData, chats: &Chats, live: &Live, now: i64) -> Workspace {
    let cx = Cx {
        data: d,
        chats,
        space: &live.space,
        owners: d.reminders.iter().map(|r| r.owner(&d.topics)).collect(),
        now,
    };
    let inbox = model::unfiled(&d.routes, &chats.sessions)
        .iter()
        .map(|th| {
            let row = thread_row(th, now, chats.turns.get(th.session.key()));
            InboxItem {
                key: row.key,
                source: row.source,
                place: th.label.clone(),
                text: row.snippet,
                when: row.when,
                suggestions: Vec::new(),
            }
        })
        .collect();
    Workspace {
        topics: d.topics.iter().map(|t| live_topic(t, &cx)).collect(),
        key_dates: d.pages.get(model::PLANNING_PAGE).map(|p| model::key_dates(&p.text)).unwrap_or_default(),
        reminders_unavailable: d.agenda_errors.iter().any(|(path, _)| *path == REMINDERS_PAGE),
        inbox,
        inbox_title: "Unfiled".into(),
        inbox_note: "Chats no README route files under a topic. Add their chat id to the channel map to file them.".into(),
        host: live.endpoints.account().into(),
        agent: live.endpoints.agent.clone(),
        today: dates::iso_date(now),
        general_chat: Vec::new(),
    }
}

fn kb(size: u64) -> String {
    if size >= 1024 * 1024 {
        format!("{:.1} MB", size as f64 / 1048576.0)
    } else {
        format!("{} KB", size.div_ceil(1024).max(1))
    }
}
