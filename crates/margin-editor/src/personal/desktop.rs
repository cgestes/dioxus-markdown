//! The personal notebook on the encrypted vault. Editor signals are updated on
//! the Dioxus thread.
use crate::{CSS, notebook_view};
use anyhow::{Result, ensure};
use crate::prelude::*;
use futures_util::StreamExt;
use loro::LoroDoc;
use margin_note::note::Note;
use margin_vault::{
    self as vault, Vault,
    keys::{Identity, Recovery},
};
use std::{
    collections::VecDeque,
    rc::Rc,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

/// What the status bar says when a note is kept nowhere (the in-memory demo).
pub(crate) const UNSAVED: &str = "Not saved: edits last until you close the window";

pub(crate) static DEVICE: OnceLock<(Vault, String)> = OnceLock::new();

fn initialize() -> Result<()> {
    let args = crate::cli::current();
    let dir = match &args.vault {
        Some(path) => path.clone(),
        None => vault::default_dir()?,
    };
    let pass = args.passphrase_file.as_deref();
    let v = if dir.join("device.json").exists() {
        Vault::open(&dir, pass)?
    } else {
        Vault::create(&dir, Identity::from_recovery(&Recovery::new()), pass)?
    };
    let room = open_note(&v, args.note.as_deref())?;
    DEVICE
        .set((v, room))
        .map_err(|_| anyhow::anyhow!("vault already initialized"))
}

pub(crate) fn open_note(vault: &Vault, requested: Option<&str>) -> Result<String> {
    vault.open_desktop_note(requested, || {
        let note = Note::new(crate::SAMPLE);
        let threads = crate::comments::seed_sample_comments(&note);
        Ok((note.document().clone(), serde_json::to_vec(&threads)?))
    })
}

#[derive(Clone)]
pub(crate) struct Menu(pub Signal<bool>);

#[derive(Clone)]
pub(super) struct RelayStatus(Arc<Mutex<String>>);

#[derive(Clone)]
pub(crate) struct Session {
    pub vault: Vault,
    pub room: String,
    pub doc: LoroDoc,
    pub seq: i64,
    pub threads: Vec<crate::comments::CommentThread>,
}

impl Session {
    /// The note this session opened.
    pub(crate) fn note(&self) -> Note {
        Note::from_doc(self.doc.clone())
    }

    /// The note's comment threads, as the vault keeps them.
    pub(crate) fn threads(&self) -> Vec<crate::comments::CommentThread> {
        self.threads.clone()
    }

    /// Save the comment threads, given what was saved last.
    pub(crate) fn save_discussion(&self, previous: &[u8], current: &[u8]) -> Result<()> {
        save_discussion(&self.vault, &self.room, previous, current)
    }

    /// Keep a diagnostics report as an encrypted inbox attachment; returns its id.
    pub(crate) fn attach_report(&self, name: &str, bytes: &[u8]) -> Result<String> {
        self.vault.attach(&self.vault.inbox(), name, bytes)
    }
}

#[component]
pub(crate) fn Desktop() -> Element {
    let mut demo = use_signal(|| false);
    let mut opening = use_resource(|| async {
        if DEVICE.get().is_some() {
            return Ok(());
        }
        vault_task(initialize).await.map_err(|e| e.to_string())
    });
    if demo() {
        return rsx! { crate::SampleEditor { inspect: false } };
    }
    let error = match opening() {
        Some(Ok(())) => return rsx! { Home {} },
        Some(Err(error)) => Some(error),
        None => None,
    };
    rsx! {
        style { "{CSS}" }
        main { class: "vault-startup",
            if let Some(error) = error {
                h1 { "Couldn’t open your notebook" }
                p { role: "alert", "{error}" }
                div { class: "vault-actions",
                    button { id: "vault-retry", aria_label: "Try again", onclick: move |_| {
                        if opening.finished() {
                            opening.clear();
                            opening.restart();
                        }
                    }, "Try again" }
                    button { id: "vault-demo", aria_label: "Open Field notes demo", onclick: move |_| demo.set(true), "Open Field notes demo" }
                }
                p { "Demo edits last until you close the window." }
            } else {
                p { role: "status", "Opening your notebook…" }
            }
        }
    }
}

#[component]
fn Home() -> Element {
    let (v, initial) = DEVICE.get().expect("vault was initialized");
    let mut room = use_signal(|| initial.clone());
    let mut menu = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut revision = use_signal(|| 0u64);
    use_context_provider(|| Menu(menu));
    let relay_status = use_context_provider(|| {
        RelayStatus(Arc::new(Mutex::new("No relay configured".to_owned())))
    });
    use_hook(move || {
        let (send, rx) = tokio::sync::watch::channel(false);
        if let Ok(Some(url)) = v.setting("relay") {
            if let Ok(mut state) = relay_status.0.lock() {
                *state = "Connecting".to_owned();
            }
            let v = v.clone();
            std::thread::spawn(move || {
                if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    rt.block_on(vault::sync::follow(v, url, rx, |label| {
                        if let Ok(mut state) = relay_status.0.lock() {
                            *state = label.to_owned();
                        }
                    }));
                }
            });
        }
        Rc::new(send)
    });
    let selected = room();
    let mut pick = move |result: Result<String>| match result.and_then(|id| open_note(v, Some(&id)))
    {
        Ok(id) => {
            room.set(id);
            menu.set(false);
            error.set(String::new());
        }
        Err(e) => error.set(e.to_string()),
    };
    let _ = revision();
    let attachments = if menu() {
        v.attachments(&selected)
    } else {
        Ok(Vec::new())
    };
    let mut finish_file = move |result: Result<()>| match result {
        Ok(()) => {
            error.set(String::new());
            revision += 1;
        }
        Err(e) => error.set(e.to_string()),
    };
    let notes = if menu() {
        v.rooms().and_then(|rooms| {
            rooms
                .into_iter()
                .map(|(id, shared)| {
                    let md = Note::from_doc(v.load(&id)?.0).markdown();
                    let title = md
                        .lines()
                        .find(|l| !l.trim().is_empty())
                        .unwrap_or("Untitled")
                        .trim_start_matches('#')
                        .trim()
                        .to_owned();
                    Ok((id, title, shared))
                })
                .collect::<Result<Vec<_>>>()
        })
    } else {
        Ok(Vec::new())
    };
    rsx! {
        if menu() {
            div { class: "vault-menu", role: "dialog", aria_label: "Personal notes",
                match notes {
                    Ok(ref rows) => rsx! {
                        for (id, title, shared) in rows {
                            button {
                                key: "{id}", id: "vault-room-{id}",
                                onclick: { let id = id.clone(); move |_| pick(Ok(id.clone())) },
                                "{title}"
                                small { if *shared { "Paired devices" } else { "Device only" } }
                            }
                        }
                    },
                    Err(ref error) => rsx! { p { role: "alert", "{error}" } }
                }
                button { id: "vault-new", onclick: move |_| pick(v.new_note("", false)), "New private note" }
                button { id: "vault-publish", onclick: move |_| pick(v.publish(&room.peek())), "Sync a copy of current text" }
                if FILE_DIALOGS { button {
                    onclick: move |_| {
                        let Some(path) = pick_file() else { return };
                        let room = room();
                        spawn(async move {
                            let result = vault_task(move || v.attach_path(&room, &path).map(|_| ())).await;
                            finish_file(result);
                        });
                    },
                    "Attach a file…"
                } }
                match attachments {
                    Ok(ref rows) if FILE_DIALOGS => rsx! {
                        for (file, name) in rows {
                            button {
                                key: "{file}",
                                onclick: {
                                    let file = file.clone();
                                    let name = name.clone();
                                    move |_| {
                                        let suggested = std::path::Path::new(&name).file_name()
                                            .and_then(|n| n.to_str()).unwrap_or("attachment");
                                        let Some(path) = save_file(suggested) else { return };
                                        let room = room();
                                        let file = file.clone();
                                        spawn(async move {
                                            finish_file(vault_task(move || v.extract(&room, &file, &path)).await);
                                        });
                                    }
                                },
                                "Export {name}…"
                            }
                        }
                    },
                    Ok(_) => rsx! {},
                    Err(ref error) => rsx! { p { role: "alert", "{error}" } }
                }
                if !error.read().is_empty() { p { role: "alert", "{error}" } }
            }
        }
        for selected in std::iter::once(selected) {
            PrivateEditor { key: "{selected}", room: selected }
        }
    }
}

async fn vault_task(work: impl FnOnce() -> Result<()> + Send + 'static) -> Result<()> {
    let (send, receive) = futures_channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = send.send(work());
    });
    receive.await?
}

#[component]
fn PrivateEditor(room: String) -> Element {
    let boot = use_hook(|| -> Result<Session, String> {
        let vault = DEVICE.get().unwrap().0.clone();
        let (doc, seq) = vault.load(&room).map_err(|e| e.to_string())?;
        let threads = vault
            .private_value(&room)
            .and_then(|bytes| match bytes {
                // Threads saved in an older shape are dropped rather than
                // keeping the note from opening.
                Some(b) => Ok(serde_json::from_slice(&b).unwrap_or_default()),
                None => Ok(Vec::new()),
            })
            .map_err(|e| e.to_string())?;
        Ok(Session {
            vault,
            room,
            doc,
            seq,
            threads,
        })
    });
    let session = match boot {
        Ok(session) => session,
        Err(error) => return rsx! {p{role:"alert","Could not open private note. {error}"}},
    };
    let show_discussion = session.room == session.vault.field_notes_room()
        && session.threads.iter().any(|thread| !thread.file.resolved);
    use_context_provider(|| session);
    // The vault's own threads come from the session; `Sample` only opens the
    // discussion pane on them.
    notebook_view(
        "",
        crate::NotebookOptions {
            comments: if show_discussion { crate::Comments::Sample } else { crate::Comments::Empty },
            ..Default::default()
        },
    )
}

type Pending = Arc<Mutex<VecDeque<Vec<u8>>>>;

fn flush(v: &Vault, room: &str, queue: &Pending) -> Result<()> {
    let mut q = queue
        .lock()
        .map_err(|_| anyhow::anyhow!("edit queue unavailable"))?;
    while let Some(bytes) = q.front() {
        v.append_update(room, bytes)?;
        q.pop_front();
    }
    Ok(())
}

pub(crate) fn use_persistence(session: Option<Session>, mut note: Signal<Note>) -> Signal<String> {
    let mut status = use_signal(String::new);
    let relay = use_hook(try_consume_context::<RelayStatus>);
    use_hook(move || {
        let Some(s) = session else { return None };
        let (send, mut rx) = futures_channel::mpsc::unbounded();
        let pending = Pending::default();
        let writer = s.vault.clone();
        let room = s.room.clone();
        let q = pending.clone();
        let tx = send.clone();
        let subscription = s.doc.subscribe_local_update(Box::new(move |bytes| {
            if let Ok(mut queue) = q.lock() {
                queue.push_back(bytes.clone());
            }
            if let Err(e) = flush(&writer, &room, &q) {
                let _ = tx.unbounded_send(Err(format!("Unsaved edit · {e}")));
            }
            true
        }));
        let mut seq = s.seq;
        let shared = s.vault.shared(&s.room).unwrap_or(false);
        std::thread::spawn(move || {
            let mut last = String::new();
            while !send.is_closed() {
                let result = (|| -> Result<_> {
                    flush(&s.vault, &s.room, &pending)?;
                    let (next, updates) = s.vault.read_since(&s.room, seq)?;
                    Ok((next, updates))
                })();
                match result {
                    Ok((next, updates)) => {
                        let scope = if shared {
                            relay
                                .as_ref()
                                .and_then(|r| r.0.lock().ok().map(|s| s.clone()))
                                .unwrap_or_else(|| "Saved on device".into())
                        } else {
                            "Device only".to_owned()
                        };
                        if last != scope || !updates.is_empty() {
                            if send
                                .unbounded_send(Ok((scope.to_owned(), updates)))
                                .is_err()
                            {
                                break;
                            }
                            last = scope;
                        }
                        seq = next;
                    }
                    Err(e) => {
                        let msg = format!("Vault error · {e}");
                        if last != msg {
                            let _ = send.unbounded_send(Err(msg.clone()));
                            last = msg;
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(180));
            }
        });
        spawn(async move {
            while let Some(event) = rx.next().await {
                match event {
                    Ok((label, updates)) => {
                        if !updates.is_empty() {
                            let result = note.write().document().import_batch(&updates);
                            if let Err(e) = result {
                                status.set(format!("Could not merge · {e}"));
                                continue;
                            }
                        }
                        if *status.peek() != label {
                            status.set(label)
                        }
                    }
                    Err(msg) => status.set(msg),
                }
            }
        });
        Some(Rc::new(subscription))
    });
    status
}

pub fn save_discussion(v: &Vault, room: &str, previous: &[u8], current: &[u8]) -> Result<()> {
    ensure!(!current.is_empty(), "empty discussion record");
    v.set_private_value(room, previous, current)
}

/// iOS has no native file dialog backend yet, so attachments can't be added or exported there.
const FILE_DIALOGS: bool = cfg!(not(target_os = "ios"));

#[cfg(not(target_os = "ios"))]
fn pick_file() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new().pick_file()
}

#[cfg(not(target_os = "ios"))]
fn save_file(suggested: &str) -> Option<std::path::PathBuf> {
    rfd::FileDialog::new().set_file_name(suggested).save_file()
}

#[cfg(target_os = "ios")]
fn pick_file() -> Option<std::path::PathBuf> {
    None
}

#[cfg(target_os = "ios")]
fn save_file(_suggested: &str) -> Option<std::path::PathBuf> {
    None
}
