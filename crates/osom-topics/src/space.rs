//! The space's files as topics: a folder, a page in a registry, or a page a
//! channel routes to.
use crate::routes::Route;
use margin_editor_core::heading::{h1, split_heading};
use serde::Deserialize;

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
    pub(crate) fn folder(id: &str) -> Self {
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
    pub(crate) fn stem(&self) -> &str {
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
