//! Runnable Rust reproducer, transport-independent sync adapter (Impl A),
//! separated encrypted archive index, sender-side lagged-epoch rebase, and benchmark.
//!
//! Run: `cargo test --test loro_archive -- --nocapture`

use loro::cursor::{CannotFindRelativePosition, Cursor, PosType, Side};
use loro::{
    ExpandType, ExportMode, Frontiers, IdSpan, LoroDoc, LoroError, PeerID, StyleConfig,
    StyleConfigMap, VersionVector, ID,
};
use margin_editor_core::note::Note;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

// ---------------------------------------------------------------------------
// 1. Opaque Relay (zero-knowledge) & Client-Side Encrypted Archive
// ---------------------------------------------------------------------------

#[derive(Default)]
struct OpaqueRelay {
    objects: HashMap<(String, String), Vec<u8>>,
    epochs: HashMap<String, (u64, String)>, // doc_id -> (current_epoch, manifest_oid)
    requests: usize,
    bytes: usize,
}

impl OpaqueRelay {
    fn put(&mut self, doc_id: &str, oid: &str, ct: Vec<u8>) {
        self.objects.insert((doc_id.into(), oid.into()), ct);
    }

    fn get(&mut self, doc_id: &str, oid: &str) -> Option<Vec<u8>> {
        self.requests += 1;
        let data = self.objects.get(&(doc_id.into(), oid.into()))?.clone();
        self.bytes += data.len();
        Some(data)
    }

    fn delete(&mut self, doc_id: &str, oid: &str) {
        self.objects.remove(&(doc_id.into(), oid.into()));
    }

    fn reset_stats(&mut self) {
        self.requests = 0;
        self.bytes = 0;
    }

    fn init_epoch(&mut self, doc_id: &str, epoch: u64, manifest_oid: &str) {
        self.epochs
            .insert(doc_id.into(), (epoch, manifest_oid.into()));
    }

    fn cas_epoch(&mut self, doc_id: &str, expected: u64, next: u64, manifest_oid: &str) -> bool {
        let Some((cur, oid)) = self.epochs.get_mut(doc_id) else {
            return false;
        };
        if *cur != expected {
            return false;
        }
        *cur = next;
        *oid = manifest_oid.into();
        true
    }

    fn push_for_epoch(&mut self, doc_id: &str, epoch: u64, oid: &str, ct: Vec<u8>) -> Result<(), u64> {
        let cur = self.epochs.get(doc_id).map(|(e, _)| *e).unwrap_or(0);
        if epoch < cur {
            return Err(cur);
        }
        self.put(doc_id, oid, ct);
        Ok(())
    }
}

// Self-contained authenticated envelope (zero extra Cargo deps; swap in XChaCha20-Poly1305 in prod).
fn mix64(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn mac(key: &[u8; 32], nonce: u64, aad: &[u8], data: &[u8]) -> [u8; 16] {
    let mut s = [
        u64::from_le_bytes(key[0..8].try_into().unwrap()) ^ nonce,
        u64::from_le_bytes(key[8..16].try_into().unwrap()) ^ (aad.len() as u64),
        u64::from_le_bytes(key[16..24].try_into().unwrap()) ^ (data.len() as u64),
        u64::from_le_bytes(key[24..32].try_into().unwrap()) ^ 0x9e37_79b9_7f4a_7c15,
    ];
    for &b in aad.iter().chain(data) {
        s[0] = mix64(s[0] ^ b as u64);
        s[1] = mix64(s[1].wrapping_add(s[0]));
        s[2] = mix64(s[2] ^ s[1]);
        s[3] = mix64(s[3].wrapping_add(s[2]));
    }
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&mix64(s[0] ^ s[2]).to_le_bytes());
    out[8..].copy_from_slice(&mix64(s[1] ^ s[3]).to_le_bytes());
    out
}

fn xor_stream(key: &[u8; 32], nonce: u64, data: &[u8]) -> Vec<u8> {
    let k0 = u64::from_le_bytes(key[0..8].try_into().unwrap());
    let k1 = u64::from_le_bytes(key[8..16].try_into().unwrap());
    data.iter()
        .enumerate()
        .map(|(i, &b)| b ^ mix64(k0 ^ nonce.wrapping_add(i as u64) ^ k1.rotate_left((i % 61) as u32)) as u8)
        .collect()
}

fn seal(key: &[u8; 32], nonce: u64, aad: &[u8], plain: &[u8]) -> Vec<u8> {
    let enc = xor_stream(key, nonce, plain);
    let mut ct = Vec::with_capacity(24 + enc.len());
    ct.extend_from_slice(&nonce.to_le_bytes());
    ct.extend_from_slice(&mac(key, nonce, aad, &enc));
    ct.extend_from_slice(&enc);
    ct
}

fn open(key: &[u8; 32], aad: &[u8], ct: &[u8]) -> Result<Vec<u8>, &'static str> {
    if ct.len() < 24 {
        return Err("truncated");
    }
    let nonce = u64::from_le_bytes(ct[..8].try_into().unwrap());
    if &ct[8..24] != mac(key, nonce, aad, &ct[24..]) {
        return Err("auth tag mismatch");
    }
    Ok(xor_stream(key, nonce, &ct[24..]))
}

fn oid_of(ct: &[u8]) -> String {
    mac(&[0x5a; 32], 0, b"oid", ct)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SpanEntry {
    peer: PeerID,
    start: i32,
    end: i32,
    oid: String,
    aad: String,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct Manifest {
    redacted: Option<String>,
    spans: Vec<SpanEntry>,
    checkpoints: Vec<(String, String)>, // (oid, aad)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ArchiveErr {
    Missing,
    Redacted(String),
}

struct EncryptedArchive {
    doc_id: String,
    key: [u8; 32],
    manifest: Manifest,
    nonce: u64,
}

impl EncryptedArchive {
    fn new(doc_id: &str, key: [u8; 32]) -> Self {
        Self {
            doc_id: doc_id.into(),
            key,
            manifest: Manifest::default(),
            nonce: 1,
        }
    }

    fn put_sealed(&mut self, relay: &mut OpaqueRelay, aad: &str, plain: &[u8]) -> String {
        let n = self.nonce;
        self.nonce += 1;
        let ct = seal(&self.key, n, aad.as_bytes(), plain);
        let oid = oid_of(&ct);
        relay.put(&self.doc_id, &oid, ct);
        oid
    }

    fn save_manifest(&mut self, relay: &mut OpaqueRelay) {
        let n = self.nonce;
        self.nonce += 1;
        let ct = seal(
            &self.key,
            n,
            format!("{}:manifest", self.doc_id).as_bytes(),
            &serde_json::to_vec(&self.manifest).unwrap(),
        );
        relay.put(&self.doc_id, "manifest-v1", ct);
    }

    fn publish_span(&mut self, relay: &mut OpaqueRelay, peer: PeerID, start: i32, end: i32, bytes: &[u8]) {
        let aad = format!("{}:span:{peer}:{start}:{end}", self.doc_id);
        let oid = self.put_sealed(relay, &aad, bytes);
        self.manifest.spans.push(SpanEntry { peer, start, end, oid, aad });
        self.save_manifest(relay);
    }

    fn publish_from_doc(&mut self, relay: &mut OpaqueRelay, doc: &LoroDoc, chunk: i32) {
        for (&peer, &end) in doc.oplog_vv().iter() {
            let mut cur = 0;
            while cur < end {
                let next = (cur + chunk).min(end);
                let blob = doc
                    .export(ExportMode::updates_in_range(vec![IdSpan::new(peer, cur, next)]))
                    .unwrap();
                self.publish_span(relay, peer, cur, next, &blob);
                cur = next;
            }
        }
        let aad = format!("{}:ckpt:{}", self.doc_id, self.manifest.checkpoints.len());
        let oid = self.put_sealed(relay, &aad, &doc.export(ExportMode::Snapshot).unwrap());
        self.manifest.checkpoints.push((oid, aad));
        self.save_manifest(relay);
    }

    fn redact(&mut self, relay: &mut OpaqueRelay, reason: &str) {
        for (oid, _) in self.manifest.checkpoints.drain(..) {
            relay.delete(&self.doc_id, &oid);
        }
        for s in self.manifest.spans.drain(..) {
            relay.delete(&self.doc_id, &s.oid);
        }
        self.manifest.redacted = Some(reason.into());
        self.save_manifest(relay);
    }

    fn load_manifest(&self, relay: &mut OpaqueRelay) -> Result<Manifest, ArchiveErr> {
        let ct = relay.get(&self.doc_id, "manifest-v1").ok_or(ArchiveErr::Missing)?;
        let plain = open(&self.key, format!("{}:manifest", self.doc_id).as_bytes(), &ct).unwrap();
        let m: Manifest = serde_json::from_slice(&plain).unwrap();
        if let Some(reason) = m.redacted {
            return Err(ArchiveErr::Redacted(reason));
        }
        Ok(m)
    }

    fn fetch_span(&self, relay: &mut OpaqueRelay, span: IdSpan) -> Result<Vec<Vec<u8>>, ArchiveErr> {
        let hits: Vec<_> = self
            .load_manifest(relay)?
            .spans
            .into_iter()
            .filter(|e| e.peer == span.peer && e.end > span.counter.start && e.start < span.counter.end)
            .collect();
        if hits.is_empty() {
            return Err(ArchiveErr::Missing);
        }
        hits.into_iter()
            .map(|e| {
                let ct = relay.get(&self.doc_id, &e.oid).ok_or(ArchiveErr::Missing)?;
                Ok(open(&self.key, e.aad.as_bytes(), &ct).unwrap())
            })
            .collect()
    }

    fn fetch_checkpoint(&self, relay: &mut OpaqueRelay) -> Result<Vec<u8>, ArchiveErr> {
        let (oid, aad) = self
            .load_manifest(relay)?
            .checkpoints
            .pop()
            .ok_or(ArchiveErr::Missing)?;
        let ct = relay.get(&self.doc_id, &oid).ok_or(ArchiveErr::Missing)?;
        Ok(open(&self.key, aad.as_bytes(), &ct).unwrap())
    }
}

// ---------------------------------------------------------------------------
// 2. Transport-Independent On-Demand History Adapter (Impl A: Ships Today)
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
enum Demand {
    None,
    MissingSpans(Vec<IdSpan>),
    NeedsCheckpoint(Frontiers),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    Ready,
    RebuiltFull,
    Unavailable(ArchiveErr),
}

struct Persisted {
    peer_id: PeerID,
    snapshot: Vec<u8>,
    pending: Option<Vec<u8>>,
    pre_root_journal: Vec<Vec<u8>>,
    needs_checkpoint: bool,
}

fn configure_styles(doc: &LoroDoc) {
    let mut styles = StyleConfigMap::new();
    styles.insert("markdown.checked".into(), StyleConfig { expand: ExpandType::None });
    styles.insert("bold".into(), StyleConfig { expand: ExpandType::After });
    doc.config_text_style(styles);
}

fn make_doc(peer: Option<PeerID>) -> LoroDoc {
    let doc = LoroDoc::new();
    configure_styles(&doc);
    if let Some(p) = peer {
        doc.set_peer_id(p).unwrap();
    }
    doc
}

fn missing_spans_from_meta(bytes: &[u8], vv: &VersionVector) -> Vec<IdSpan> {
    let meta = LoroDoc::decode_import_blob_meta(bytes, true).unwrap();
    let mut need: BTreeMap<PeerID, i32> = BTreeMap::new();
    for dep in meta.start_frontiers.iter() {
        let end = dep.counter + 1;
        if end > vv.get(&dep.peer).copied().unwrap_or(0) {
            need.entry(dep.peer).and_modify(|e| *e = (*e).max(end)).or_insert(end);
        }
    }
    need.into_iter()
        .map(|(p, end)| IdSpan::new(p, vv.get(&p).copied().unwrap_or(0), end))
        .collect()
}

struct HistorySession {
    doc: LoroDoc,
    pre_root_journal: Vec<Vec<u8>>,
    seen: HashSet<[u8; 16]>,
    need_end: BTreeMap<PeerID, i32>,
    needs_checkpoint: bool,
    outcome: Outcome,
}

impl HistorySession {
    fn new(doc: LoroDoc) -> Self {
        Self {
            doc,
            pre_root_journal: Vec::new(),
            seen: HashSet::new(),
            need_end: BTreeMap::new(),
            needs_checkpoint: false,
            outcome: Outcome::Ready,
        }
    }

    fn record_deps(&mut self, bytes: &[u8]) {
        for s in missing_spans_from_meta(bytes, &self.doc.oplog_vv()) {
            self.need_end
                .entry(s.peer)
                .and_modify(|e| *e = (*e).max(s.counter.end))
                .or_insert(s.counter.end);
        }
    }

    fn ingest(&mut self, bytes: &[u8]) -> Demand {
        let h = mac(&[0x11; 32], 0, b"upd", bytes);
        if !self.seen.insert(h) {
            return self.next_demand();
        }
        match self.doc.import(bytes) {
            Ok(st) => {
                if st.pending.is_some() {
                    self.record_deps(bytes);
                }
                if let Some(p) = self.doc.export_pending_updates().unwrap() {
                    self.record_deps(&p);
                }
            }
            Err(LoroError::ImportUpdatesThatDependsOnOutdatedVersion) if self.doc.is_shallow() => {
                self.pre_root_journal.push(bytes.to_vec());
                self.needs_checkpoint = true;
            }
            Err(e) => panic!("unexpected import error: {e}"),
        }
        self.next_demand()
    }

    fn resolve_cursor(&mut self, c: &Cursor) -> Result<usize, Demand> {
        match self.doc.get_cursor_pos(c) {
            Ok(p) => Ok(p.current.pos),
            Err(CannotFindRelativePosition::HistoryCleared) if self.doc.is_shallow() => {
                self.needs_checkpoint = true;
                Err(self.next_demand())
            }
            Err(e) => panic!("cursor error: {e}"),
        }
    }

    fn next_demand(&mut self) -> Demand {
        let vv = self.doc.oplog_vv();
        self.need_end.retain(|p, end| vv.get(p).copied().unwrap_or(0) < *end);
        if self.needs_checkpoint && self.doc.is_shallow() {
            return Demand::NeedsCheckpoint(self.doc.shallow_since_frontiers());
        }
        if !self.need_end.is_empty() {
            return Demand::MissingSpans(
                self.need_end
                    .iter()
                    .map(|(&p, &end)| IdSpan::new(p, vv.get(&p).copied().unwrap_or(0), end))
                    .collect(),
            );
        }
        Demand::None
    }

    fn rebuild_from_checkpoint(&mut self, ckpt: &[u8]) {
        let fresh = make_doc(Some(self.doc.peer_id()));
        fresh.import(ckpt).unwrap();
        fresh.import(&self.doc.export(ExportMode::updates(&fresh.oplog_vv())).unwrap()).unwrap();
        if let Some(p) = self.doc.export_pending_updates().unwrap() {
            fresh.import(&p).unwrap();
        }
        self.pre_root_journal
            .retain(|u| fresh.import(u).unwrap().pending.is_some());
        self.doc = fresh;
        self.needs_checkpoint = false;
        self.need_end.clear();
        if let Some(p) = self.doc.export_pending_updates().unwrap() {
            self.record_deps(&p);
        }
        self.outcome = Outcome::RebuiltFull;
    }

    fn pump(&mut self, archive: &EncryptedArchive, relay: &mut OpaqueRelay) -> Outcome {
        for _ in 0..16 {
            match self.next_demand() {
                Demand::None => return self.outcome.clone(),
                Demand::NeedsCheckpoint(_) => match archive.fetch_checkpoint(relay) {
                    Ok(ckpt) => self.rebuild_from_checkpoint(&ckpt),
                    Err(e) => {
                        self.outcome = Outcome::Unavailable(e);
                        return self.outcome.clone();
                    }
                },
                Demand::MissingSpans(spans) => {
                    for span in spans {
                        match archive.fetch_span(relay, span) {
                            Ok(blobs) => blobs.iter().for_each(|b| {
                                self.ingest(b);
                            }),
                            Err(e) => {
                                self.outcome = Outcome::Unavailable(e);
                                return self.outcome.clone();
                            }
                        }
                    }
                    self.outcome = Outcome::Ready;
                }
            }
        }
        panic!("exceeded max fetch rounds");
    }

    fn persist(&self) -> Persisted {
        Persisted {
            peer_id: self.doc.peer_id(),
            snapshot: self.doc.export(ExportMode::Snapshot).unwrap(),
            pending: self.doc.export_pending_updates().unwrap(),
            pre_root_journal: self.pre_root_journal.clone(),
            needs_checkpoint: self.needs_checkpoint,
        }
    }

    fn restore(p: Persisted) -> Self {
        let doc = make_doc(Some(p.peer_id));
        doc.import(&p.snapshot).unwrap();
        let mut s = Self::new(doc);
        if let Some(pending) = p.pending {
            s.ingest(&pending);
        }
        for u in p.pre_root_journal {
            s.ingest(&u);
        }
        s.needs_checkpoint |= p.needs_checkpoint && s.doc.is_shallow();
        s
    }
}

fn change_summary(doc: &LoroDoc) -> Vec<(PeerID, i32, usize, u32, Vec<ID>)> {
    let mut peers: Vec<_> = doc.oplog_vv().iter().map(|(&p, &e)| (p, e)).collect();
    peers.sort_by_key(|(p, _)| *p);
    let mut out = Vec::new();
    for (peer, end) in peers {
        let mut ctr = 0;
        while ctr < end {
            let m = doc.get_change(ID::new(peer, ctr)).unwrap();
            let mut deps: Vec<ID> = m.deps.iter().collect();
            deps.sort_by_key(|d| (d.peer, d.counter));
            out.push((peer, m.id.counter, m.len, m.lamport, deps));
            ctr = m.id.counter + m.len as i32;
        }
    }
    out
}

fn assert_converged(actual: &LoroDoc, reference: &LoroDoc, cursors: &[(&str, &Cursor)]) {
    assert_eq!(actual.oplog_vv(), reference.oplog_vv());
    assert_eq!(actual.state_vv(), reference.state_vv());
    assert_eq!(actual.oplog_frontiers(), reference.oplog_frontiers());
    assert_eq!(change_summary(actual), change_summary(reference));
    assert_eq!(actual.get_deep_value(), reference.get_deep_value());
    assert_eq!(
        actual.get_text("markdown").to_delta(),
        reference.get_text("markdown").to_delta()
    );
    for (name, c) in cursors {
        assert_eq!(
            actual.get_cursor_pos(c).unwrap().current,
            reference.get_cursor_pos(c).unwrap().current,
            "cursor {name}"
        );
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn baseline_1_out_of_order_discovers_missing_span_and_releases_pending() {
    let a = make_doc(Some(1));
    a.get_text("t").insert(0, "abc").unwrap();
    a.commit();

    let b = make_doc(Some(2));
    b.import(&a.export(ExportMode::all_updates()).unwrap()).unwrap();
    let before_b = b.oplog_vv();
    b.get_text("t").insert(3, "!").unwrap();
    b.commit();
    let b_upd = b.export(ExportMode::updates(&before_b)).unwrap();

    let r = make_doc(Some(3));
    assert!(r.import(&b_upd).unwrap().pending.is_some());
    let missing = missing_spans_from_meta(&b_upd, &r.oplog_vv());
    assert_eq!(missing, vec![IdSpan::new(1, 0, 3)]);

    let fetched = a.export(ExportMode::updates_in_range(missing)).unwrap();
    assert!(r.import(&fetched).unwrap().pending.is_none());
    assert_eq!(r.get_text("t").to_string(), "abc!");
}

#[test]
fn baseline_2_shallow_rejects_pre_cutoff_fork_and_fresh_replica_converges() {
    let a = make_doc(Some(1));
    a.get_text("t").insert(0, "abc").unwrap();
    a.commit();
    let full_at_abc = a.export(ExportMode::Snapshot).unwrap();

    let b = make_doc(Some(2));
    b.import(&full_at_abc).unwrap();
    let before_b = b.oplog_vv();
    b.get_text("t").insert(1, "!").unwrap();
    b.commit();
    let b_upd = b.export(ExportMode::updates(&before_b)).unwrap();

    a.get_text("t").insert(3, " DEF").unwrap();
    a.commit();
    let shallow = make_doc(Some(3));
    shallow
        .import(&a.export(ExportMode::shallow_snapshot(&a.oplog_frontiers())).unwrap())
        .unwrap();
    assert!(matches!(
        shallow.import(&b_upd),
        Err(LoroError::ImportUpdatesThatDependsOnOutdatedVersion)
    ));

    // Importing earlier full snapshot into SAME shallow doc leaves boundary intact
    shallow.import(&full_at_abc).unwrap();
    assert!(shallow.is_shallow());
    assert!(matches!(
        shallow.import(&b_upd),
        Err(LoroError::ImportUpdatesThatDependsOnOutdatedVersion)
    ));

    // Shallow alone trimmed [3..6), so exporting from shallow without A's suffix parks [6..7)
    let partial = make_doc(Some(3));
    partial.import(&full_at_abc).unwrap();
    assert!(
        partial
            .import(&shallow.export(ExportMode::updates(&partial.oplog_vv())).unwrap())
            .unwrap()
            .pending
            .is_some()
    );

    // Fresh replica from full_at_abc + A's suffix [3..7) + B's update converges with full history
    let fresh = make_doc(Some(3));
    fresh.import(&full_at_abc).unwrap();
    fresh.import(&a.export(ExportMode::updates(&fresh.oplog_vv())).unwrap()).unwrap();
    fresh.import(&b_upd).unwrap();
    assert!(!fresh.is_shallow());
    assert_eq!(fresh.get_text("t").to_string(), "a!bc DEF");
}

#[test]
fn full_convergence_forks_deletes_movable_list_marks_and_margin_comment_anchors() {
    let mut relay = OpaqueRelay::default();
    let mut archive = EncryptedArchive::new("margin-note-1", [0x42; 32]);

    let a = make_doc(Some(1));
    let ta = a.get_text("markdown");
    ta.insert(0, "- [ ] Alpha\nOLD_GONE\nKeep Me\nDelAfter\n").unwrap();
    ta.mark(2..5, "bold", true).unwrap();
    for (i, v) in ["intro", "body", "outro"].into_iter().enumerate() {
        a.get_movable_list("blocks").insert(i, v).unwrap();
    }
    a.get_map("meta").insert("title", "Draft 1").unwrap();
    a.commit();

    let note_a = Note::from_doc(a.fork());
    let md = note_a.markdown();
    let keep = md.find("Keep Me").unwrap();
    let (anc_alive_a, anc_alive_b) = note_a.anchor_range(keep..keep + 7).unwrap();
    let anc_del_before = ta
        .get_cursor(ta.convert_pos(md.find("OLD_GONE").unwrap() + 2, PosType::Bytes, PosType::Unicode).unwrap(), Side::Middle)
        .unwrap();
    let anc_del_after = ta
        .get_cursor(ta.convert_pos(md.find("DelAfter").unwrap() + 2, PosType::Bytes, PosType::Unicode).unwrap(), Side::Middle)
        .unwrap();

    let b = a.fork();
    configure_styles(&b);
    b.set_peer_id(2).unwrap();
    let before_b = b.oplog_vv();

    let old = ta.to_string().find("OLD_GONE\n").unwrap();
    ta.delete_utf8(old, "OLD_GONE\n".len()).unwrap();
    ta.insert_utf8(ta.len_utf8(), "Post-cutoff\n").unwrap();
    a.get_movable_list("blocks").mov(2, 0).unwrap();
    a.get_map("meta").insert("phase", "cutoff").unwrap();
    a.commit();
    archive.publish_from_doc(&mut relay, &a, 128);
    let shallow_snap = a.export(ExportMode::shallow_snapshot(&a.oplog_frontiers())).unwrap();

    let c = a.fork();
    c.set_peer_id(3).unwrap();
    let before_c = c.oplog_vv();
    let tc = c.get_text("markdown");
    tc.insert_utf8(tc.len_utf8(), "From C\n").unwrap();
    c.get_map("meta").insert("reviewer", "C").unwrap();
    c.commit();
    let c_upd = c.export(ExportMode::updates(&before_c)).unwrap();

    let tb = b.get_text("markdown");
    let box_at = tb.to_string().find("[ ]").unwrap() + 1;
    tb.mark_utf8(box_at..box_at + 1, "markdown.checked", true).unwrap();
    let alpha = tb.to_string().find("Alpha").unwrap();
    tb.delete_utf8(alpha, 5).unwrap();
    tb.insert_utf8(alpha, "Alpha-B").unwrap();
    b.get_movable_list("blocks").set(1, "body-B").unwrap();
    b.get_movable_list("blocks").mov(0, 2).unwrap();
    b.get_map("meta").insert("title", "Draft 2").unwrap();
    b.commit();
    let b_upd = b.export(ExportMode::updates(&before_b)).unwrap();

    let s_doc = make_doc(Some(4));
    s_doc.import(&shallow_snap).unwrap();
    assert!(s_doc.get_cursor_pos(&anc_alive_a).is_ok());

    let ts = s_doc.get_text("markdown");
    let del_pos = ts.to_string().find("DelAfter").unwrap();
    ts.delete_utf8(del_pos, "DelAfter".len()).unwrap();
    ts.insert_utf8(ts.len_utf8(), "Local S\n").unwrap();
    s_doc.get_movable_list("blocks").insert(0, "local-header").unwrap();
    s_doc.commit();
    let s_upd = s_doc.export(ExportMode::updates(&a.oplog_vv())).unwrap();
    assert!(s_doc.get_cursor_pos(&anc_del_after).is_ok());

    let mut session = HistorySession::new(s_doc);
    assert!(matches!(session.resolve_cursor(&anc_del_before), Err(Demand::NeedsCheckpoint(_))));
    session.needs_checkpoint = false;

    relay.reset_stats();
    assert_eq!(session.ingest(&c_upd), Demand::None);
    assert_eq!(relay.requests, 0);
    assert!(matches!(session.ingest(&b_upd), Demand::NeedsCheckpoint(_)));
    assert_eq!(session.pump(&archive, &mut relay), Outcome::RebuiltFull);

    let reference = a.fork();
    configure_styles(&reference);
    for u in [&b_upd, &c_upd, &s_upd] {
        reference.import(u).unwrap();
    }
    assert_converged(
        &session.doc,
        &reference,
        &[("alive_a", &anc_alive_a), ("alive_b", &anc_alive_b), ("del_before", &anc_del_before), ("del_after", &anc_del_after)],
    );
    let note_s = Note::from_doc(session.doc.fork());
    let note_r = Note::from_doc(reference.fork());
    assert_eq!(note_s.markdown(), note_r.markdown());
    assert_eq!(
        note_s.resolve_anchor(&(anc_alive_a.clone(), anc_alive_b.clone())),
        note_r.resolve_anchor(&(anc_alive_a, anc_alive_b))
    );
}

#[test]
fn out_of_order_duplicates_missing_archive_redaction_and_restart_mid_fetch() {
    for redact in [false, true] {
        let mut relay = OpaqueRelay::default();
        let mut archive = EncryptedArchive::new("err-doc", [0x77; 32]);
        let a = make_doc(Some(1));
        a.get_text("markdown").insert(0, "secret\n").unwrap();
        a.commit();
        let b = a.fork();
        b.set_peer_id(2).unwrap();
        let before_b = b.oplog_vv();
        b.get_text("markdown").insert(0, "B ").unwrap();
        b.commit();
        let b_upd = b.export(ExportMode::updates(&before_b)).unwrap();

        a.get_text("markdown").delete(0, 7).unwrap();
        a.get_text("markdown").insert(0, "clean\n").unwrap();
        a.commit();
        if redact {
            archive.publish_from_doc(&mut relay, &a, 64);
            archive.redact(&mut relay, "erasure-policy");
        }

        let s = make_doc(Some(3));
        s.import(&a.export(ExportMode::shallow_snapshot(&a.oplog_frontiers())).unwrap()).unwrap();
        s.get_text("markdown").insert(6, "offline\n").unwrap();
        s.commit();

        let mut session = HistorySession::new(s);
        assert!(matches!(session.ingest(&b_upd), Demand::NeedsCheckpoint(_)));
        let expected = if redact {
            Outcome::Unavailable(ArchiveErr::Redacted("erasure-policy".into()))
        } else {
            Outcome::Unavailable(ArchiveErr::Missing)
        };
        assert_eq!(session.pump(&archive, &mut relay), expected);
        assert_eq!(session.doc.get_text("markdown").to_string(), "clean\noffline\n");
        assert_eq!(session.pre_root_journal.len(), 1);
    }

    // Restart mid-fetch + duplicate delivery
    let mut relay = OpaqueRelay::default();
    let mut archive = EncryptedArchive::new("restart-rust", [0x99; 32]);
    let a = make_doc(Some(1));
    a.get_text("markdown").insert(0, "base\n").unwrap();
    a.commit();
    let b = a.fork();
    b.set_peer_id(2).unwrap();
    let before_b = b.oplog_vv();
    b.get_text("markdown").insert(0, "B\n").unwrap();
    b.commit();
    let b_upd = b.export(ExportMode::updates(&before_b)).unwrap();

    a.get_text("markdown").insert(5, "cutoff\n").unwrap();
    a.commit();
    archive.publish_from_doc(&mut relay, &a, 32);
    let shallow_snap = a.export(ExportMode::shallow_snapshot(&a.oplog_frontiers())).unwrap();

    let c = a.fork();
    c.set_peer_id(3).unwrap();
    let v0 = c.oplog_vv();
    c.get_text("markdown").insert(12, "C1\n").unwrap();
    c.commit();
    let v1 = c.oplog_vv();
    let c1_upd = c.export(ExportMode::updates(&v0)).unwrap();
    c.get_text("markdown").insert(15, "C2\n").unwrap();
    c.commit();
    let c2_upd = c.export(ExportMode::updates(&v1)).unwrap();
    archive.publish_span(&mut relay, 3, 0, c.oplog_vv().get(&3).copied().unwrap() - 3, &c1_upd);

    let s = make_doc(Some(4));
    s.import(&shallow_snap).unwrap();
    s.get_text("markdown").insert(12, "S\n").unwrap();
    s.commit();
    let s_upd = s.export(ExportMode::updates(&a.oplog_vv())).unwrap();

    let mut s1 = HistorySession::new(s);
    for u in [&c2_upd, &c2_upd, &b_upd, &b_upd] {
        s1.ingest(u);
    }
    assert!(s1.doc.export_pending_updates().unwrap().is_some());
    assert_eq!(s1.pre_root_journal.len(), 1);

    let mut s2 = HistorySession::restore(s1.persist());
    assert_eq!(s2.pump(&archive, &mut relay), Outcome::Ready);

    let reference = a.fork();
    configure_styles(&reference);
    for u in [&b_upd, &c1_upd, &c2_upd, &s_upd] {
        reference.import(u).unwrap();
    }
    assert_converged(&s2.doc, &reference, &[]);
}

#[test]
fn sender_side_crdt_rebase_across_lagged_epochs_zero_genesis_and_zero_receiver_fetches() {
    let mut relay = OpaqueRelay::default();
    let doc_id = "margin-epoch-rust";
    let key = [0x55; 32];
    let mut nonce = 1u64;
    let mut put = |relay: &mut OpaqueRelay, aad: &[u8], bytes: &[u8]| -> String {
        let ct = seal(&key, nonce, aad, bytes);
        nonce += 1;
        let oid = oid_of(&ct);
        relay.put(doc_id, &oid, ct);
        oid
    };

    // Genesis [0..F1) — never uploaded to relay!
    let author = make_doc(Some(1));
    let ta = author.get_text("markdown");
    for i in 0..40 {
        ta.insert_utf8(ta.len_utf8(), &format!("genesis-{i}\n")).unwrap();
        author.commit();
    }
    ta.delete_utf8(0, ta.len_utf8()).unwrap();
    ta.insert_utf8(0, "- [ ] Ship epochs\nShared line\nFooter\n").unwrap();
    ta.mark_utf8(2..5, "bold", true).unwrap();
    author.get_movable_list("blocks").insert(0, "p1").unwrap();
    author.get_movable_list("blocks").insert(1, "p2").unwrap();
    author.get_map("meta").insert("status", "e1").unwrap();
    author.commit();

    // Epoch 1 cut at F1
    let vv_f1 = author.oplog_vv();
    let e1_snap = author.export(ExportMode::shallow_snapshot(&author.oplog_frontiers())).unwrap();
    let e1_oid = put(&mut relay, b"e1-snap", &e1_snap);
    relay.init_epoch(doc_id, 1, &e1_oid);

    let b_doc = make_doc(Some(2));
    b_doc.import(&open(&key, b"e1-snap", &relay.get(doc_id, &e1_oid).unwrap()).unwrap()).unwrap();

    // Online peers advance to lagged cutoff F2, then continue in lag window [F2..head]
    ta.insert_utf8(ta.len_utf8(), "Online pre-F2\n").unwrap();
    author.get_movable_list("blocks").mov(1, 0).unwrap();
    author.commit();
    let f2 = author.oplog_frontiers();

    ta.insert_utf8(ta.len_utf8(), "Inside lag window\n").unwrap();
    author.get_map("meta").insert("status", "e2-live").unwrap();
    author.commit();

    let d_doc = author.fork();
    d_doc.set_peer_id(3).unwrap();
    let before_d = d_doc.oplog_vv();
    let td = d_doc.get_text("markdown");
    td.insert_utf8(td.len_utf8(), "From D\n").unwrap();
    d_doc.commit();
    let d_upd = d_doc.export(ExportMode::updates(&before_d)).unwrap();

    // Compact E1 -> E2 at lagged F2 (archive ONLY [F1..F2), NO genesis!)
    let delta_oid = put(&mut relay, b"delta-1-2", &author.export(ExportMode::updates(&vv_f1)).unwrap());
    let e2_oid = put(&mut relay, b"e2-snap", &author.export(ExportMode::shallow_snapshot(&f2)).unwrap());
    assert!(relay.cas_epoch(doc_id, 1, 2, &e2_oid));
    assert!(!relay.cas_epoch(doc_id, 1, 2, &e2_oid));

    let s_doc = make_doc(Some(4));
    s_doc.import(&open(&key, b"e2-snap", &relay.get(doc_id, &e2_oid).unwrap()).unwrap()).unwrap();
    relay.reset_stats();
    assert!(relay.push_for_epoch(doc_id, 2, "upd-d", seal(&key, 900, b"upd-d", &d_upd)).is_ok());
    s_doc.import(&d_upd).unwrap();
    assert_eq!(relay.requests, 0);

    // Stale B edits offline on E1 + captures Margin comment anchor
    let tb = b_doc.get_text("markdown");
    let box_at = tb.to_string().find("[ ]").unwrap() + 1;
    tb.mark_utf8(box_at..box_at + 1, "markdown.checked", true).unwrap();
    let shared = tb.to_string().find("Shared line").unwrap();
    tb.delete_utf8(shared, 11).unwrap();
    tb.insert_utf8(shared, "Shared line (B)").unwrap();
    b_doc.get_movable_list("blocks").set(0, "p1-B").unwrap();
    b_doc.get_map("meta").insert("authorB", "offline").unwrap();
    b_doc.commit();
    let note_b = Note::from_doc(b_doc.fork());
    let pos_b = note_b.markdown().find("(B)").unwrap();
    let b_anchor = note_b.anchor_range(pos_b..pos_b + 3).unwrap();
    let b_stale_upd = b_doc.export(ExportMode::updates(&vv_f1)).unwrap();
    assert_eq!(relay.push_for_epoch(doc_id, 1, "stale", seal(&key, 901, b"stale", &b_stale_upd)), Err(2));

    // Sender-side CRDT-assisted rebase on B (pulls ONLY delta-1-2 + e2-snap; fresh PeerID!)
    let delta = open(&key, b"delta-1-2", &relay.get(doc_id, &delta_oid).unwrap()).unwrap();
    let head_e1 = make_doc(None);
    head_e1.import(&e1_snap).unwrap();
    head_e1.import(&delta).unwrap();
    head_e1.import(&d_upd).unwrap();
    let e2_head = head_e1.oplog_frontiers();

    b_doc.import(&delta).unwrap();
    b_doc.import(&d_upd).unwrap();
    let merged_range = Note::from_doc(b_doc.fork()).resolve_anchor(&b_anchor).unwrap();
    let diff = b_doc.diff(&e2_head, &b_doc.oplog_frontiers()).unwrap();

    let b_on_e2 = make_doc(None); // fresh PeerID prevents op-ID reuse across rebases!
    b_on_e2.import(&open(&key, b"e2-snap", &relay.get(doc_id, &e2_oid).unwrap()).unwrap()).unwrap();
    b_on_e2.import(&d_upd).unwrap();
    let before_rebase = b_on_e2.oplog_vv();
    b_on_e2.apply_diff(diff).unwrap();
    b_on_e2.commit();
    let b_reanchored = Note::from_doc(b_on_e2.fork()).anchor_range(merged_range).unwrap();
    let rebased_upd = b_on_e2.export(ExportMode::updates(&before_rebase)).unwrap();
    assert!(relay.push_for_epoch(doc_id, 2, "b-reb", seal(&key, 902, b"b-reb", &rebased_upd)).is_ok());

    // S edits concurrently on E2, imports B's rebased push with 0 fetches, and stays shallow
    let before_s = s_doc.oplog_vv();
    let ts = s_doc.get_text("markdown");
    ts.insert_utf8(ts.len_utf8(), "Concurrent S\n").unwrap();
    s_doc.commit();
    let s_upd = s_doc.export(ExportMode::updates(&before_s)).unwrap();

    relay.reset_stats();
    assert!(s_doc.import(&rebased_upd).unwrap().pending.is_none());
    assert_eq!(relay.requests, 0);
    assert!(s_doc.is_shallow());
    b_on_e2.import(&s_upd).unwrap();
    assert_eq!(b_on_e2.get_deep_value(), s_doc.get_deep_value());

    let full_ref = author.fork();
    configure_styles(&full_ref);
    for u in [&d_upd, &b_stale_upd, &s_upd] {
        full_ref.import(u).unwrap();
    }
    assert_eq!(s_doc.get_deep_value(), full_ref.get_deep_value());
    assert_eq!(s_doc.get_text("markdown").to_delta(), full_ref.get_text("markdown").to_delta());
    assert_eq!(
        Note::from_doc(s_doc.fork()).resolve_anchor(&b_reanchored),
        Note::from_doc(full_ref.fork()).resolve_anchor(&b_anchor)
    );

    // Redacted-delta fallback: delta purged -> B recovers via fresh-PeerID edit on E2
    relay.delete(doc_id, &delta_oid);
    assert!(relay.get(doc_id, &delta_oid).is_none());
    let b_fb = make_doc(None);
    b_fb.import(&open(&key, b"e2-snap", &relay.get(doc_id, &e2_oid).unwrap()).unwrap()).unwrap();
    let before_fb = b_fb.oplog_vv();
    let tfb = b_fb.get_text("markdown");
    tfb.insert_utf8(tfb.len_utf8(), "Recovered offline B\n").unwrap();
    b_fb.commit();
    let fb_upd = b_fb.export(ExportMode::updates(&before_fb)).unwrap();
    assert!(relay.push_for_epoch(doc_id, 2, "b-fb", seal(&key, 903, b"b-fb", &fb_upd)).is_ok());
    s_doc.import(&fb_upd).unwrap();
    assert!(s_doc.get_text("markdown").to_string().contains("Recovered offline B"));
}

#[test]
fn benchmark_rust_full_vs_shallow_vs_impl_a() {
    let mut relay = OpaqueRelay::default();
    let mut archive = EncryptedArchive::new("bench-rust", [0xab; 32]);

    let author = make_doc(Some(1));
    let text = author.get_text("markdown");
    let list = author.get_movable_list("blocks");
    for i in 0..300 {
        text.insert_utf8(text.len_utf8(), &format!("Rev {i}: {}\n", "lorem ipsum dolor sit amet ".repeat(4)))
            .unwrap();
        if i > 20 && i % 3 == 0 {
            text.delete(0, 60).unwrap();
        }
        if i < 40 {
            list.insert(i, format!("block-{i}")).unwrap();
        } else {
            list.set(i % 40, format!("block-{}-r{i}", i % 40)).unwrap();
        }
        author.commit();
    }

    let b = author.fork();
    b.set_peer_id(2).unwrap();
    let before_b = b.oplog_vv();
    b.get_text("markdown").insert(0, "[B pre-cutoff] ").unwrap();
    b.commit();
    let b_upd = b.export(ExportMode::updates(&before_b)).unwrap();

    for i in 300..310 {
        text.insert_utf8(text.len_utf8(), &format!("Tail {i}\n")).unwrap();
        author.commit();
    }

    let full_snap = author.export(ExportMode::Snapshot).unwrap();
    let shallow_snap = author.export(ExportMode::shallow_snapshot(&author.oplog_frontiers())).unwrap();
    archive.publish_from_doc(&mut relay, &author, 256);

    let measure = |peer, snap: &[u8]| {
        let t = Instant::now();
        let d = make_doc(Some(peer));
        d.import(snap).unwrap();
        d.get_text("markdown").insert(0, "edit ").unwrap();
        d.commit();
        (d, t.elapsed().as_micros())
    };
    let (_, full_us) = measure(10, &full_snap);
    let (shallow_doc, shallow_us) = measure(11, &shallow_snap);

    relay.reset_stats();
    let mut session = HistorySession::new(shallow_doc);
    let t_reb = Instant::now();
    session.ingest(&b_upd);
    assert_eq!(session.pump(&archive, &mut relay), Outcome::RebuiltFull);

    println!(
        "\n[Rust Benchmark — 310 revisions]\n\
         - Full checkpoint:   init={} B, fetch=0 B, reqs=1, TTFE={} us\n\
         - Shallow (happy):   init={} B, fetch=0 B, reqs=1, TTFE={} us\n\
         - Impl A (rebuild):  init={} B, fetch={} B, reqs={}, TTFE={} us, rebuild={} us",
        full_snap.len(), full_us,
        shallow_snap.len(), shallow_us,
        shallow_snap.len(), relay.bytes, 1 + relay.requests, shallow_us, t_reb.elapsed().as_micros(),
    );
    assert!(shallow_snap.len() < full_snap.len());
}
