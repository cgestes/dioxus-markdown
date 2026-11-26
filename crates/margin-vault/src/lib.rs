//! Encrypted personal notes and immutable attachments. SQLite only receives ciphertext.
pub mod files;
pub mod keys;
pub mod record;
pub mod relay;
mod stream;
pub mod sync;

use anyhow::{Context, Result, anyhow, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use keys::Identity;
use loro::{ExportMode, LoroDoc};
use margin_note::note::Note;
use record::{DOCUMENT, Packet};
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

pub fn b64(data: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(data)
}
pub fn unb64(data: &str) -> Result<Vec<u8>> {
    Ok(URL_SAFE_NO_PAD.decode(data)?)
}
pub fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn default_dir() -> Result<PathBuf> {
    let name = if cfg!(debug_assertions) {
        "personal-dev"
    } else {
        "personal"
    };
    Ok(directories::ProjectDirs::from("org", "Margin", "Margin")
        .context("missing app data directory")?
        .data_local_dir()
        .join(name))
}

pub(crate) fn database(path: &Path) -> Result<Connection> {
    let c = Connection::open(path)?;
    c.busy_timeout(Duration::from_secs(10))?;
    c.pragma_update(None, "journal_mode", "WAL")?;
    c.pragma_update(None, "synchronous", "FULL")?;
    Ok(c)
}

#[derive(Clone)]
pub struct Vault {
    pub id: Arc<Identity>,
    db: Arc<Mutex<Connection>>,
    pub(crate) wake: Arc<tokio::sync::Notify>,
}

pub(crate) struct Stored {
    pub seq: i64,
    pub room: String,
    pub part: i64,
    pub data: Vec<u8>,
}

impl Vault {
    pub fn create(dir: &Path, id: Identity, pass: Option<&Path>) -> Result<Self> {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(dir)?;
        ensure!(
            !dir.join("vault.sqlite3").exists(),
            "vault data already exists"
        );
        keys::save_identity(dir, &id, pass, true)?;
        Self::with_identity(dir, id)
    }

    pub fn open(dir: &Path, pass: Option<&Path>) -> Result<Self> {
        Self::with_identity(dir, keys::load_identity(dir, pass)?)
    }

    fn with_identity(dir: &Path, id: Identity) -> Result<Self> {
        let c = database(&dir.join("vault.sqlite3"))?;
        c.execute_batch("CREATE TABLE IF NOT EXISTS docs(room TEXT PRIMARY KEY, shared INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS records(seq INTEGER PRIMARY KEY AUTOINCREMENT, room TEXT NOT NULL, part INTEGER NOT NULL,
                digest BLOB UNIQUE NOT NULL, data BLOB NOT NULL, shared INTEGER NOT NULL, pending INTEGER NOT NULL);
            CREATE INDEX IF NOT EXISTS by_room ON records(room,part,seq);
            CREATE UNIQUE INDEX IF NOT EXISTS file_parts ON records(room,part) WHERE part>=0;
            CREATE INDEX IF NOT EXISTS outbox ON records(seq) WHERE pending=1;
            CREATE TABLE IF NOT EXISTS settings(k TEXT PRIMARY KEY,v TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS discussions(room TEXT PRIMARY KEY,iv BLOB NOT NULL, data BLOB NOT NULL);")?;
        let v = Self {
            id: Arc::new(id),
            db: Arc::new(Mutex::new(c)),
            wake: Default::default(),
        };
        v.conn()?
            .execute("INSERT OR IGNORE INTO docs VALUES(?,0)", [v.inbox()])?;
        Ok(v)
    }

    pub(crate) fn conn(&self) -> Result<MutexGuard<'_, Connection>> {
        self.db
            .lock()
            .map_err(|_| anyhow!("vault database lock failed"))
    }

    pub fn inbox(&self) -> String {
        hex::encode(&keys::derive(&self.id.local, b"device inbox")[..16])
    }

    pub fn field_notes_room(&self) -> String {
        hex::encode(&keys::derive(&self.id.local, b"desktop field notes")[..16])
    }

    /// Resume the desktop selection, or initialize a pristine device with its first note.
    pub fn open_desktop_note(
        &self,
        requested: Option<&str>,
        starter: impl FnOnce() -> Result<(LoroDoc, Vec<u8>)>,
    ) -> Result<String> {
        let mut c = self.conn()?;
        let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let room = if let Some(room) = requested {
            room.to_owned()
        } else {
            let selected: Option<String> = tx.query_row(
                "SELECT room FROM docs JOIN settings ON docs.room=settings.v WHERE settings.k='desktop-note'",
                [], |r| r.get(0),
            ).optional()?;
            if let Some(room) = selected {
                tx.commit()?;
                return Ok(room);
            }
            // Older vaults always have an inbox, even before their first edit.
            let existing: Option<String> = tx
                .query_row(
                    "SELECT room FROM docs WHERE room<>?1
                    OR EXISTS(SELECT 1 FROM records WHERE room=?1)
                    OR EXISTS(SELECT 1 FROM discussions WHERE room=?1)
                 ORDER BY rowid DESC LIMIT 1",
                    [self.inbox()],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(room) = existing {
                room
            } else {
                let room = self.field_notes_room();
                let (doc, discussion) = starter()?;
                let bytes =
                    self.encrypted_delta(&room, false, &doc.export(ExportMode::all_updates())?)?;
                tx.execute("INSERT INTO docs VALUES(?,0)", [&room])?;
                Self::insert(&tx, &room, DOCUMENT, &bytes, false, false)?;
                self.write_private_value(&tx, &room, &discussion)?;
                room
            }
        };
        let found = tx.execute(
            "INSERT OR REPLACE INTO settings(k,v) SELECT 'desktop-note',room FROM docs WHERE room=?",
            [&room],
        )?;
        ensure!(found == 1, "unknown note");
        tx.commit()?;
        Ok(room)
    }

    fn private_bytes(&self, c: &Connection, room: &str) -> Result<Option<Vec<u8>>> {
        let row: Option<(Vec<u8>, Vec<u8>)> = c
            .query_row(
                "SELECT iv,data FROM discussions WHERE room=?",
                [room],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        row.map(|(iv, data)| {
            let key = keys::derive(&self.id.local, b"local discussions");
            keys::open(
                &key,
                &iv.try_into()
                    .map_err(|_| anyhow!("invalid discussion nonce"))?,
                room.as_bytes(),
                &data,
            )
        })
        .transpose()
    }

    pub fn private_value(&self, room: &str) -> Result<Option<Vec<u8>>> {
        self.private_bytes(&*self.conn()?, room)
    }

    pub fn set_private_value(&self, room: &str, previous: &[u8], value: &[u8]) -> Result<()> {
        let mut c = self.conn()?;
        let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let stored = self.private_bytes(&tx, room)?;
        let stored = stored.as_deref().unwrap_or(b"[]");
        if stored == value {
            return Ok(());
        }
        ensure!(
            stored == previous,
            "discussion changed in another window; reopen before editing"
        );
        self.write_private_value(&tx, room, value)?;
        tx.commit()?;
        Ok(())
    }

    fn write_private_value(&self, c: &Connection, room: &str, value: &[u8]) -> Result<()> {
        let iv = keys::random();
        let data = keys::seal(
            &keys::derive(&self.id.local, b"local discussions"),
            &iv,
            room.as_bytes(),
            value,
        )?;
        c.execute(
            "INSERT OR REPLACE INTO discussions VALUES(?,?,?)",
            params![room, iv.as_slice(), data],
        )?;
        Ok(())
    }

    pub fn shared(&self, room: &str) -> Result<bool> {
        Ok(self
            .conn()?
            .query_row("SELECT shared FROM docs WHERE room=?", [room], |r| r.get(0))
            .context("unknown note")?)
    }

    pub fn rooms(&self) -> Result<Vec<(String, bool)>> {
        Ok(self
            .conn()?
            .prepare("SELECT room,shared FROM docs ORDER BY rowid")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn current_epoch(&self) -> u32 {
        *self
            .id
            .paired
            .keys
            .last_key_value()
            .expect("validated vault")
            .0
    }

    fn doc_key(&self, room: &str, epoch: u32, shared: bool) -> Result<[u8; 32]> {
        let secret = if shared {
            self.id
                .paired
                .keys
                .get(&epoch)
                .context("missing vault key version")?
        } else {
            ensure!(epoch == 0, "invalid local key version");
            &self.id.local
        };
        Ok(keys::derive(secret, format!("document/{room}").as_bytes()))
    }

    fn decode(&self, room: &str, shared: bool, record: &[u8]) -> Result<Vec<u8>> {
        record::decrypt(&self.doc_key(room, record::epoch(record)?, shared)?, record)
    }

    pub fn read_since(&self, room: &str, seq: i64) -> Result<(i64, Vec<Vec<u8>>)> {
        let shared = self.shared(room)?;
        let c = self.conn()?;
        let mut query = c.prepare(
            "SELECT seq,data FROM records WHERE room=? AND part=-1 AND seq>? ORDER BY seq",
        )?;
        let mut rows = query.query(params![room, seq])?;
        let mut last = seq;
        let mut plain = Vec::new();
        while let Some(row) = rows.next()? {
            last = row.get(0)?;
            plain.push(self.decode(room, shared, &row.get::<_, Vec<u8>>(1)?)?);
        }
        Ok((last, plain))
    }

    pub fn load(&self, room: &str) -> Result<(LoroDoc, i64)> {
        let (seq, records) = self.read_since(room, 0)?;
        let doc = LoroDoc::new();
        doc.import_batch(&records)?;
        Ok((doc, seq))
    }

    fn encrypted_delta(&self, room: &str, shared: bool, bytes: &[u8]) -> Result<Vec<u8>> {
        let epoch = if shared { self.current_epoch() } else { 0 };
        record::delta(&self.doc_key(room, epoch, shared)?, epoch, bytes)
    }

    pub fn append_update(&self, room: &str, bytes: &[u8]) -> Result<()> {
        let shared = self.shared(room)?;
        let data = self.encrypted_delta(room, shared, bytes)?;
        Self::insert(&*self.conn()?, room, DOCUMENT, &data, shared, shared)?;
        self.wake.notify_one();
        Ok(())
    }

    pub(crate) fn insert(
        c: &Connection,
        room: &str,
        part: i64,
        data: &[u8],
        shared: bool,
        pending: bool,
    ) -> Result<()> {
        let digest = Packet::digest(room, part, data);
        c.execute("INSERT INTO records(room,part,digest,data,shared,pending) VALUES(?,?,?,?,?,?) ON CONFLICT(digest) DO NOTHING",
            params![room,part,digest.as_slice(),data,shared,pending])?;
        Ok(())
    }

    pub fn new_note(&self, markdown: &str, shared: bool) -> Result<String> {
        let room = hex::encode(keys::random::<16>());
        // `Note` lays out the document; the marker is the vault's own.
        let note = Note::new(markdown);
        let d = note.document();
        d.get_map("margin").insert("format", 1)?;
        let data = self.encrypted_delta(&room, shared, &d.export(ExportMode::all_updates())?)?;
        let mut c = self.conn()?;
        let tx = c.transaction()?;
        tx.execute("INSERT INTO docs VALUES(?,?)", params![room, shared])?;
        Self::insert(&tx, &room, DOCUMENT, &data, shared, shared)?;
        tx.commit()?;
        self.wake.notify_one();
        Ok(room)
    }

    pub fn publish(&self, room: &str) -> Result<String> {
        let note = Note::from_doc(self.load(room)?.0);
        self.new_note(&note.markdown(), true)
    }

    pub fn checkpoint(&self, room: &str) -> Result<bool> {
        let shared = self.shared(room)?;
        let (doc, seq) = self.load(room)?;
        if doc.export_pending_updates()?.is_some() {
            return Ok(false);
        }
        let epoch = if shared { self.current_epoch() } else { 0 };
        let data = record::checkpoint(&self.doc_key(room, epoch, shared)?, epoch, &doc)?;
        let mut c = self.conn()?;
        let tx = c.transaction()?;
        Self::insert(&tx, room, DOCUMENT, &data, shared, shared)?;
        // This snapshot was reopened and contains every operation imported through seq.
        // Unacknowledged deltas still own their place in the outbound stream.
        tx.execute(
            "DELETE FROM records WHERE room=? AND part=-1 AND pending=0 AND seq<=?",
            params![room, seq],
        )?;
        tx.commit()?;
        self.wake.notify_one();
        Ok(true)
    }

    fn compact_received(
        c: &Connection,
        room: &str,
        checkpoint: &[u8],
        vv: &record::Coverage,
    ) -> Result<()> {
        let digest = Packet::digest(room, DOCUMENT, checkpoint);
        let mut query = c.prepare(
            "SELECT seq,data FROM records WHERE room=? AND part=-1 AND pending=0 AND digest<>?",
        )?;
        let mut rows = query.query(params![room, digest.as_slice()])?;
        let mut covered = Vec::new();
        while let Some(row) = rows.next()? {
            let bytes: Vec<u8> = row.get(1)?;
            let includes = match record::parse(&bytes)?.header {
                loro_protocol::EloHeader::Delta(h) => {
                    vv.get(std::str::from_utf8(&h.peer_id)?)
                        .copied()
                        .unwrap_or(0)
                        >= h.end
                }
                loro_protocol::EloHeader::Snapshot(h) => record::from_header(h.vv)?
                    .iter()
                    .all(|(peer, end)| vv.get(peer).copied().unwrap_or(0) >= *end),
            };
            if includes {
                covered.push(row.get::<_, i64>(0)?);
            }
        }
        drop(rows);
        let mut delete = c.prepare("DELETE FROM records WHERE seq=?")?;
        for seq in covered {
            delete.execute([seq])?;
        }
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn()?
            .query_row("SELECT v FROM settings WHERE k=?", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_relay(&self, url: &str) -> Result<()> {
        sync::validate_url(url)?;
        self.conn()?
            .execute("INSERT OR REPLACE INTO settings VALUES('relay',?)", [url])?;
        Ok(())
    }

    pub(crate) fn cursor(&self) -> Result<i64> {
        Ok(self
            .setting("cursor")?
            .unwrap_or_else(|| "0".into())
            .parse()?)
    }

    pub(crate) fn pending(&self) -> Result<Option<Stored>> {
        Ok(self.conn()?.query_row("SELECT seq,room,part,data FROM records WHERE pending=1 AND shared=1 ORDER BY seq LIMIT 1",
            [], |r| Ok(Stored {seq:r.get(0)?,room:r.get(1)?,part:r.get(2)?,data:r.get(3)?})).optional()?)
    }

    pub(crate) fn ack(&self, seq: i64) -> Result<()> {
        self.conn()?
            .execute("UPDATE records SET pending=0 WHERE seq=?", [seq])?;
        Ok(())
    }

    pub(crate) fn outbound(&self, row: &Stored) -> Result<Packet> {
        if row.part == DOCUMENT && record::epoch(&row.data)? != self.current_epoch() {
            let clear = self.decode(&row.room, true, &row.data)?;
            let epoch = self.current_epoch();
            let key = self.doc_key(&row.room, epoch, true)?;
            let data = match record::parse(&row.data)?.header {
                loro_protocol::EloHeader::Delta(_) => record::delta(&key, epoch, &clear)?,
                loro_protocol::EloHeader::Snapshot(_) => {
                    record::checkpoint(&key, epoch, &LoroDoc::from_snapshot(&clear)?)?
                }
            };
            return Ok(Packet::new(&self.id, &row.room, row.part, &data));
        }
        Ok(Packet::new(&self.id, &row.room, row.part, &row.data))
    }

    pub(crate) fn receive(&self, seq: i64, packet: &Packet) -> Result<()> {
        let raw = packet.verify(&self.id.paired.vault)?;
        let mut coverage = None;
        if packet.part == DOCUMENT {
            self.decode(&packet.room, true, &raw)?;
            if let loro_protocol::EloHeader::Snapshot(h) = record::parse(&raw)?.header {
                coverage = Some(record::from_header(h.vv)?);
            }
        }
        let mut c = self.conn()?;
        let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let current: i64 = tx.query_row(
            "SELECT coalesce((SELECT CAST(v AS INTEGER) FROM settings WHERE k='cursor'),0)",
            [],
            |row| row.get(0),
        )?;
        if current >= seq {
            return Ok(());
        }
        if packet.part == DOCUMENT {
            tx.execute("INSERT OR IGNORE INTO docs VALUES(?,1)", [&packet.room])?;
            ensure!(
                tx.query_row(
                    "SELECT shared FROM docs WHERE room=?",
                    [&packet.room],
                    |r| r.get::<_, bool>(0)
                )?,
                "local room collision"
            );
        }
        Self::insert(&tx, &packet.room, packet.part, &raw, true, false)?;
        if let Some(vv) = coverage {
            Self::compact_received(&tx, &packet.room, &raw, &vv)?;
        }
        tx.execute("INSERT INTO settings VALUES('cursor',?) ON CONFLICT(k) DO UPDATE SET v=excluded.v WHERE CAST(v AS INTEGER)<CAST(excluded.v AS INTEGER)",[seq.to_string()])?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
