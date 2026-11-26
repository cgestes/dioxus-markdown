//! Loro %ELO records. The encoded header is the AES-GCM associated data.
use super::{b64, is_hex, keys, unb64};
use anyhow::{Context, Result, anyhow, bail, ensure};
use loro::{ExportMode, LoroDoc, VersionVector};
use loro_protocol::{
    BytesReader, BytesWriter, EloHeader, ParsedEloRecord, parse_elo_record_header,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const DOCUMENT: i64 = -1;
pub const MAX_RECORD: usize = 16 * 1024 * 1024;
pub const FILE_CHUNK: usize = 128 * 1024;

pub type Coverage = BTreeMap<String, u64>;

pub fn coverage(v: &VersionVector) -> Coverage {
    v.iter()
        .map(|(p, end)| (p.to_string(), *end as u64))
        .collect()
}

pub fn parse(data: &[u8]) -> Result<ParsedEloRecord<'_>> {
    ensure!(
        !data.is_empty() && data.len() <= MAX_RECORD,
        "invalid encrypted record size"
    );
    // Bound the upstream parser's allocation before handing it a snapshot.
    if data[0] == 1 {
        let count = BytesReader::new(&data[1..])
            .read_uleb128()
            .map_err(|e| anyhow!(e))?;
        ensure!(
            count <= 4096 && count <= data.len() as u64,
            "too many snapshot writers"
        );
    }
    let parsed = parse_elo_record_header(data).map_err(|e| anyhow!(e))?;
    ensure!(parsed.ct.len() >= 16, "missing authentication tag");
    Ok(parsed)
}

fn encrypt(key: &[u8; 32], epoch: u32, mut header: BytesWriter, plain: &[u8]) -> Result<Vec<u8>> {
    let iv = keys::random();
    header.push_var_string(&epoch.to_string());
    header.push_var_bytes(&iv);
    let aad = header.finalize();
    let ct = keys::seal(key, &iv, &aad, plain)?;
    let mut record = BytesWriter::new();
    record.push_bytes(&aad);
    record.push_var_bytes(&ct);
    let bytes = record.finalize();
    ensure!(bytes.len() <= MAX_RECORD, "document record exceeds 16 MiB");
    Ok(bytes)
}

pub fn delta(key: &[u8; 32], epoch: u32, bytes: &[u8]) -> Result<Vec<u8>> {
    let meta = LoroDoc::decode_import_blob_meta(bytes, true)?;
    ensure!(
        meta.partial_end_vv.len() == 1,
        "expected a single local writer"
    );
    let (&peer, &end) = meta.partial_end_vv.iter().next().context("empty update")?;
    let start = meta
        .partial_start_vv
        .get(&peer)
        .copied()
        .unwrap_or_default();
    let mut header = BytesWriter::new();
    header.push_byte(0);
    header.push_var_string(&peer.to_string());
    header.push_uleb128(start.try_into()?);
    header.push_uleb128(end.try_into()?);
    encrypt(key, epoch, header, bytes)
}

pub fn checkpoint(key: &[u8; 32], epoch: u32, doc: &LoroDoc) -> Result<Vec<u8>> {
    let raw = doc.export(ExportMode::Snapshot)?;
    let mut header = BytesWriter::new();
    header.push_byte(1);
    let vv = coverage(&doc.oplog_vv());
    header.push_uleb128(vv.len() as u64);
    for (peer, end) in vv {
        header.push_var_string(&peer);
        header.push_uleb128(end);
    }
    let record = encrypt(key, epoch, header, &raw)?;
    let restored = LoroDoc::from_snapshot(&decrypt(key, &record)?)?;
    ensure!(
        !restored.is_shallow()
            && restored.oplog_vv() == doc.oplog_vv()
            && restored.get_deep_value() == doc.get_deep_value(),
        "checkpoint verification failed"
    );
    Ok(record)
}

pub fn epoch(record: &[u8]) -> Result<u32> {
    let p = parse(record)?;
    Ok(match p.header {
        EloHeader::Delta(h) => h.key_id,
        EloHeader::Snapshot(h) => h.key_id,
    }
    .parse()?)
}

pub fn decrypt(key: &[u8; 32], record: &[u8]) -> Result<Vec<u8>> {
    let p = parse(record)?;
    let iv = match &p.header {
        EloHeader::Delta(h) => &h.iv,
        EloHeader::Snapshot(h) => &h.iv,
    };
    let clear = keys::open(key, iv, p.aad, p.ct)?;
    let meta = LoroDoc::decode_import_blob_meta(&clear, true)?;
    match p.header {
        EloHeader::Delta(h) => {
            let peer = std::str::from_utf8(&h.peer_id)?.parse::<u64>()?;
            ensure!(
                meta.partial_end_vv.len() == 1
                    && meta.partial_end_vv.get(&peer).copied().map(i64::from)
                        == Some(i64::try_from(h.end)?)
                    && i64::from(
                        meta.partial_start_vv
                            .get(&peer)
                            .copied()
                            .unwrap_or_default()
                    ) == i64::try_from(h.start)?,
                "delta coverage mismatch"
            );
        }
        EloHeader::Snapshot(h) => {
            let doc = LoroDoc::from_snapshot(&clear)?;
            ensure!(
                !doc.is_shallow() && coverage(&doc.oplog_vv()) == from_header(h.vv)?,
                "snapshot coverage mismatch"
            );
        }
    }
    Ok(clear)
}

pub fn from_header(vv: Vec<(Vec<u8>, u64)>) -> Result<Coverage> {
    let mut result = Coverage::new();
    for (peer, count) in vv {
        let peer = String::from_utf8(peer)?;
        ensure!(
            peer.parse::<u64>()?.to_string() == peer
                && count <= i32::MAX as u64
                && result.insert(peer, count).is_none(),
            "invalid snapshot coverage"
        );
    }
    Ok(result)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Packet {
    pub room: String,
    pub part: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub data: String,
    pub grant: keys::Grant,
    pub signature: String,
}

impl Packet {
    pub fn digest(room: &str, part: i64, data: &[u8]) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(b"Margin record v1\0");
        hash.update(room.as_bytes());
        hash.update(part.to_be_bytes());
        hash.update(data);
        hash.finalize().into()
    }

    pub fn new(id: &keys::Identity, room: &str, part: i64, bytes: &[u8]) -> Self {
        let digest = Self::digest(room, part, bytes);
        Self {
            room: room.into(),
            part,
            data: b64(bytes),
            grant: id.paired.grant.clone(),
            signature: keys::sign(&id.paired.device_seed, &digest),
        }
    }

    pub fn verify(&self, vault: &str) -> Result<Vec<u8>> {
        ensure!(is_hex(&self.room, 32), "invalid room");
        self.grant.verify(vault)?;
        let data = unb64(&self.data)?;
        match self.part {
            DOCUMENT => {
                parse(&data)?;
            }
            0 => ensure!(data.len() == 24, "invalid file header"),
            1..=i64::MAX => ensure!(
                (17..=FILE_CHUNK + 17).contains(&data.len()),
                "invalid file chunk"
            ),
            _ => bail!("invalid record type"),
        }
        keys::verify(
            &self.grant.device,
            &Self::digest(&self.room, self.part, &data),
            &self.signature,
        )?;
        Ok(data)
    }
}
