use super::{
    Vault, b64, keys,
    record::{DOCUMENT, FILE_CHUNK},
    unb64,
};
use anyhow::{Context, Result, anyhow, ensure};
use crypto_secretstream::{Header, Key, PullStream, PushStream, Tag};
use loro::{ExportMode, LoroValue, ValueOrContainer};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

#[derive(Serialize, Deserialize)]
struct Manifest {
    name: String,
    key: String,
    parts: i64,
    size: u64,
}

impl Vault {
    pub fn attach(&self, room: &str, name: &str, mut input: impl Read) -> Result<String> {
        let shared = self.shared(room)?;
        let file = hex::encode(keys::random::<16>());
        let raw_key: [u8; 32] = keys::random();
        let key = Key::from(raw_key);
        let (header, mut stream) = PushStream::init(rand::rngs::OsRng, &key);
        let (doc, _) = self.load(room)?;
        let before = doc.oplog_vv();
        let mut c = self.conn()?;
        let tx = c.transaction()?;
        Self::insert(&tx, &file, 0, header.as_ref(), shared, shared)?;
        let mut part = 0i64;
        let mut size = 0;
        loop {
            let mut chunk = Vec::with_capacity(FILE_CHUNK + 17);
            input
                .by_ref()
                .take(FILE_CHUNK as u64)
                .read_to_end(&mut chunk)?;
            size += chunk.len() as u64;
            let end = chunk.is_empty();
            stream
                .push(
                    &mut chunk,
                    file.as_bytes(),
                    if end { Tag::Final } else { Tag::Message },
                )
                .map_err(|_| anyhow!("file encryption failed"))?;
            part += 1;
            Self::insert(&tx, &file, part, &chunk, shared, shared)?;
            if end {
                break;
            }
        }
        let manifest = Manifest {
            name: name.into(),
            key: b64(&raw_key),
            parts: part,
            size,
        };
        doc.get_map("attachments")
            .insert(&file, serde_json::to_string(&manifest)?)?;
        let data =
            self.encrypted_delta(room, shared, &doc.export(ExportMode::updates(&before))?)?;
        Self::insert(&tx, room, DOCUMENT, &data, shared, shared)?;
        tx.commit()?;
        self.wake.notify_one();
        Ok(file)
    }

    pub fn attachments(&self, room: &str) -> Result<Vec<(String, String)>> {
        let map = self.load(room)?.0.get_map("attachments");
        let mut entries = Vec::new();
        map.for_each(|id, v| {
            if let ValueOrContainer::Value(LoroValue::String(s)) = v {
                entries.push((id.to_string(), serde_json::from_str::<Manifest>(&s)));
            }
        });
        entries
            .into_iter()
            .map(|(id, m)| Ok((id, m?.name)))
            .collect()
    }

    pub fn extract(&self, room: &str, file: &str, output: &Path) -> Result<()> {
        let map = self.load(room)?.0.get_map("attachments");
        let Some(ValueOrContainer::Value(LoroValue::String(s))) = map.get(file) else {
            anyhow::bail!("unknown attachment")
        };
        let manifest: Manifest = serde_json::from_str(&s)?;
        let raw: [u8; 32] = unb64(&manifest.key)?
            .try_into()
            .map_err(|_| anyhow!("invalid file key"))?;
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        let c = self.conn()?;
        let mut stmt =
            c.prepare("SELECT part,data FROM records WHERE room=? AND part>=0 ORDER BY part")?;
        let mut rows = stmt.query(params![file])?;
        let first = rows.next()?.context("file has not finished syncing")?;
        ensure!(first.get::<_, i64>(0)? == 0, "missing file header");
        let hdr: [u8; 24] = first
            .get::<_, Vec<u8>>(1)?
            .try_into()
            .map_err(|_| anyhow!("invalid file header"))?;
        let mut stream = PullStream::init(Header::from(hdr), &Key::from(raw));
        let mut part = 0;
        let mut size = 0;
        while let Some(row) = rows.next()? {
            part += 1;
            ensure!(
                row.get::<_, i64>(0)? == part && part <= manifest.parts,
                "file chunk missing"
            );
            let mut bytes: Vec<u8> = row.get(1)?;
            let tag = stream
                .pull(&mut bytes, file.as_bytes())
                .map_err(|_| anyhow!("file authentication failed"))?;
            ensure!(
                tag == if part == manifest.parts {
                    Tag::Final
                } else {
                    Tag::Message
                },
                "invalid file end marker"
            );
            temp.write_all(&bytes)?;
            size += bytes.len() as u64;
        }
        ensure!(
            part == manifest.parts && part > 0 && size == manifest.size,
            "incomplete file"
        );
        temp.as_file().sync_all()?;
        temp.persist_noclobber(output)?;
        Ok(())
    }

    pub fn attach_path(&self, room: &str, path: &Path) -> Result<String> {
        self.attach(
            room,
            path.file_name()
                .and_then(|n| n.to_str())
                .context("invalid file name")?,
            fs::File::open(path)?,
        )
    }
}
