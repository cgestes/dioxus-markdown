//! Transport-independent on-demand Loro history sync, pre-root journaling,
//! and sender-side lagged-epoch CRDT rebase for Margin [`Note`]s.
//!
//! All methods are synchronous and lock-scoped: network/relay I/O always happens
//! outside document locks.

use crate::note::Note;
use loro::cursor::{CannotFindRelativePosition, Cursor};
use loro::{
    ExportMode, Frontiers, IdSpan, LoroDoc, LoroEncodeError, LoroError, LoroResult, PeerID,
    VersionVector,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::ops::Range;
use std::path::Path;

/// What history the application must fetch outside document locks before retrying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncDemand {
    None,
    /// Post-cutoff causal gaps (half-open `[start, end)` per peer).
    MissingSpans(Vec<IdSpan>),
    /// Shallow boundary rejected a pre-root update or cursor needs pre-cutoff tombstones.
    NeedsPreRootHistory { shallow_since_frontiers: Frontiers },
}

/// Explicit terminal outcome when remote history cannot be supplied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveUnavailable {
    MissingArchive,
    IrreversiblyRedacted { reason: String },
}

/// Serializable state that preserves both Loro's pending buffer and rejected pre-root
/// updates across process restarts mid-fetch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedSyncState {
    pub epoch: u64,
    pub peer_id: PeerID,
    pub snapshot: Vec<u8>,
    pub loro_pending: Option<Vec<u8>>,
    pub pre_root_journal: Vec<Vec<u8>>,
    pub needs_checkpoint: bool,
}

/// Result of rebasing a stale offline [`Note`] onto a newer lagged shallow epoch.
pub struct RebasedEpoch {
    /// Post-cutoff Loro update ready to push to the new epoch.
    pub rebased_update: Vec<u8>,
    /// Comment anchor pairs re-anchored onto the new epoch's op IDs (in caller order).
    pub reanchored: Vec<Option<(Cursor, Cursor)>>,
}

fn fnv1a_fingerprint(bytes: &[u8]) -> (usize, u64, u64) {
    let mut h1: u64 = 0xcbf2_9ce4_8422_2325;
    let mut h2: u64 = 0x8422_2325_cbf2_9ce4;
    for &b in bytes {
        h1 = (h1 ^ b as u64).wrapping_mul(0x0100_0000_01b3);
        h2 = (h2 ^ b as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
    (bytes.len(), h1, h2)
}

/// Compute half-open missing spans `[have, dep.counter + 1)` from an update blob's
/// external `start_frontiers` against `vv`.
pub fn missing_spans_from_meta(bytes: &[u8], vv: &VersionVector) -> LoroResult<Vec<IdSpan>> {
    let meta = LoroDoc::decode_import_blob_meta(bytes, true)?;
    let mut need: BTreeMap<PeerID, i32> = BTreeMap::new();
    for dep in meta.start_frontiers.iter() {
        let end = dep.counter.saturating_add(1);
        if end > vv.get(&dep.peer).copied().unwrap_or(0) {
            need.entry(dep.peer)
                .and_modify(|cur| *cur = (*cur).max(end))
                .or_insert(end);
        }
    }
    Ok(need
        .into_iter()
        .map(|(peer, end)| IdSpan::new(peer, vv.get(&peer).copied().unwrap_or(0), end))
        .collect())
}

/// Transport-independent sync state wrapping a Margin [`Note`].
pub struct NoteSync {
    note: Note,
    epoch: u64,
    pre_root_journal: Vec<Vec<u8>>,
    seen: HashSet<(usize, u64, u64)>,
    need_end: BTreeMap<PeerID, i32>,
    needs_checkpoint: bool,
    last_unavailable: Option<ArchiveUnavailable>,
}

impl NoteSync {
    pub fn new(note: Note, epoch: u64) -> Self {
        Self {
            note,
            epoch,
            pre_root_journal: Vec::new(),
            seen: HashSet::new(),
            need_end: BTreeMap::new(),
            needs_checkpoint: false,
            last_unavailable: None,
        }
    }

    pub fn note(&self) -> &Note {
        &self.note
    }

    pub fn note_mut(&mut self) -> &mut Note {
        &mut self.note
    }

    pub fn into_note(self) -> Note {
        self.note
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn pre_root_journal(&self) -> &[Vec<u8>] {
        &self.pre_root_journal
    }

    pub fn last_unavailable(&self) -> Option<&ArchiveUnavailable> {
        self.last_unavailable.as_ref()
    }

    fn record_deps(&mut self, bytes: &[u8]) -> LoroResult<()> {
        let vv = self.note.document().oplog_vv();
        for span in missing_spans_from_meta(bytes, &vv)? {
            self.need_end
                .entry(span.peer)
                .and_modify(|cur| *cur = (*cur).max(span.counter.end))
                .or_insert(span.counter.end);
        }
        Ok(())
    }

    /// Import an incoming update/snapshot without blocking on I/O.
    /// Parks post-root gaps in Loro and journals rejected pre-root updates locally.
    pub fn ingest(&mut self, bytes: &[u8]) -> LoroResult<SyncDemand> {
        if !self.seen.insert(fnv1a_fingerprint(bytes)) {
            return Ok(self.next_demand());
        }
        match self.note.document().import(bytes) {
            Ok(status) => {
                if status.pending.is_some() {
                    self.record_deps(bytes)?;
                }
                if let Some(pending) = self.note.document().export_pending_updates().ok().flatten() {
                    self.record_deps(&pending)?;
                }
                Ok(self.next_demand())
            }
            Err(LoroError::ImportUpdatesThatDependsOnOutdatedVersion)
                if self.note.document().is_shallow() =>
            {
                self.pre_root_journal.push(bytes.to_vec());
                self.needs_checkpoint = true;
                Ok(self.next_demand())
            }
            Err(err) => Err(err),
        }
    }

    /// Resolve a Margin comment anchor pair, returning a history demand if its target
    /// was deleted before the shallow cutoff (`CannotFindRelativePosition::HistoryCleared`).
    pub fn resolve_anchor_or_demand(
        &mut self,
        (a, b): &(Cursor, Cursor),
    ) -> Result<Option<Range<usize>>, SyncDemand> {
        let doc = self.note.document();
        for c in [a, b] {
            if matches!(
                doc.get_cursor_pos(c),
                Err(CannotFindRelativePosition::HistoryCleared)
            ) && doc.is_shallow()
            {
                self.needs_checkpoint = true;
                return Err(self.next_demand());
            }
        }
        Ok(self.note.resolve_anchor(&(a.clone(), b.clone())))
    }

    /// Export logical spans if this replica holds them (returns `Ok(None)` if trimmed).
    pub fn export_spans(&self, spans: &[IdSpan]) -> Result<Option<Vec<u8>>, LoroEncodeError> {
        let doc = self.note.document();
        let vv = doc.oplog_vv();
        let shallow = doc.is_shallow().then(|| doc.shallow_since_vv());
        let mut req = Vec::new();
        for &span in spans {
            if let Some(sv) = &shallow
                && sv.get(&span.peer).copied().unwrap_or(0) > span.counter.start
            {
                return Ok(None);
            }
            let have = vv.get(&span.peer).copied().unwrap_or(0);
            let end = have.min(span.counter.end);
            if end > span.counter.start {
                req.push(IdSpan::new(span.peer, span.counter.start, end));
            }
        }
        if req.is_empty() {
            return Ok(None);
        }
        doc.export(ExportMode::updates_in_range(req)).map(Some)
    }

    pub fn next_demand(&mut self) -> SyncDemand {
        let doc = self.note.document();
        let vv = doc.oplog_vv();
        self.need_end
            .retain(|peer, end| vv.get(peer).copied().unwrap_or(0) < *end);
        if self.needs_checkpoint && doc.is_shallow() {
            return SyncDemand::NeedsPreRootHistory {
                shallow_since_frontiers: doc.shallow_since_frontiers(),
            };
        }
        if !self.need_end.is_empty() {
            let spans = self
                .need_end
                .iter()
                .map(|(&peer, &end)| IdSpan::new(peer, vv.get(&peer).copied().unwrap_or(0), end))
                .collect();
            return SyncDemand::MissingSpans(spans);
        }
        SyncDemand::None
    }

    /// Record an explicit missing-archive or irreversible-redaction outcome without
    /// altering local offline edits or discarding journals.
    pub fn mark_unavailable(&mut self, reason: ArchiveUnavailable) {
        self.last_unavailable = Some(reason);
    }

    /// Impl A: rebuild a fresh full replica from a retained full checkpoint + local suffix
    /// + Loro pending updates + application pre-root journal.
    pub fn rebuild_from_checkpoint(&mut self, full_checkpoint: &[u8]) -> LoroResult<SyncDemand> {
        let old_doc = self.note.document();
        let fresh = LoroDoc::new();
        fresh.set_peer_id(old_doc.peer_id())?;
        fresh.import(full_checkpoint)?;

        let suffix = old_doc
            .export(ExportMode::updates(&fresh.oplog_vv()))
            .map_err(LoroError::from)?;
        let pending = old_doc
            .export_pending_updates()
            .map_err(LoroError::from)?;

        fresh.import(&suffix)?;
        if let Some(p) = pending {
            fresh.import(&p)?;
        }

        let mut remaining = Vec::new();
        for upd in std::mem::take(&mut self.pre_root_journal) {
            let st = fresh.import(&upd)?;
            if st.pending.is_some() {
                remaining.push(upd);
            }
        }

        self.note = Note::from_doc(fresh);
        self.pre_root_journal = remaining;
        self.needs_checkpoint = false;
        self.need_end.clear();
        self.last_unavailable = None;
        if let Some(p) = self.note.document().export_pending_updates().ok().flatten() {
            self.record_deps(&p)?;
        }
        Ok(self.next_demand())
    }

    /// Sender-side CRDT-assisted rebase across lagged shallow epochs (`E_old -> E_new`).
    ///
    /// Requires only the archived delta `[F_old .. F_new)` and `E_new`'s snapshot + tail —
    /// zero genesis history `[0 .. F_old)`. Mints a fresh `PeerID` on `E_new` so op IDs
    /// never collide across rebases, and re-anchors caller comment cursors onto `E_new`.
    pub fn rebase_onto_epoch(
        &mut self,
        base_epoch_snap: &[u8],
        delta_to_new: &[u8],
        new_epoch_snap: &[u8],
        new_epoch_tail: &[&[u8]],
        new_epoch: u64,
        anchors: &[&(Cursor, Cursor)],
    ) -> LoroResult<RebasedEpoch> {
        // 1. Compute E_new head frontiers inside E_old's causal space.
        let head_doc = LoroDoc::new();
        head_doc.import(base_epoch_snap)?;
        head_doc.import(delta_to_new)?;
        for u in new_epoch_tail {
            head_doc.import(u)?;
        }
        let head_frontiers = head_doc.oplog_frontiers();

        // 2. Exact CRDT merge inside our own E_old doc (no genesis needed!).
        let doc = self.note.document();
        doc.import(delta_to_new)?;
        for u in new_epoch_tail {
            doc.import(u)?;
        }
        let resolved_ranges: Vec<Option<Range<usize>>> = anchors
            .iter()
            .map(|a| self.note.resolve_anchor(a))
            .collect();
        let diff = doc.diff(&head_frontiers, &doc.oplog_frontiers())?;

        // 3. Apply diff onto fresh E_new doc with a fresh PeerID and re-anchor comments.
        let next_doc = LoroDoc::new();
        let next_note = Note::from_doc(next_doc);
        next_note.document().import(new_epoch_snap)?;
        for u in new_epoch_tail {
            next_note.document().import(u)?;
        }
        let before = next_note.document().oplog_vv();
        next_note.document().apply_diff(diff)?;
        next_note.document().commit();

        let reanchored = resolved_ranges
            .into_iter()
            .map(|r| r.and_then(|range| next_note.anchor_range(range)))
            .collect();
        let rebased_update = next_note
            .document()
            .export(ExportMode::updates(&before))
            .map_err(LoroError::from)?;

        self.note = next_note;
        self.epoch = new_epoch;
        self.pre_root_journal.clear();
        self.needs_checkpoint = false;
        self.need_end.clear();
        self.last_unavailable = None;
        Ok(RebasedEpoch {
            rebased_update,
            reanchored,
        })
    }

    pub fn persist(&self) -> Result<PersistedSyncState, LoroEncodeError> {
        let doc = self.note.document();
        Ok(PersistedSyncState {
            epoch: self.epoch,
            peer_id: doc.peer_id(),
            snapshot: doc.export(ExportMode::Snapshot)?,
            loro_pending: doc.export_pending_updates()?,
            pre_root_journal: self.pre_root_journal.clone(),
            needs_checkpoint: self.needs_checkpoint,
        })
    }

    pub fn restore(saved: &PersistedSyncState) -> LoroResult<Self> {
        let doc = LoroDoc::new();
        doc.set_peer_id(saved.peer_id)?;
        doc.import(&saved.snapshot)?;
        let mut sync = Self::new(Note::from_doc(doc), saved.epoch);
        if let Some(p) = &saved.loro_pending {
            sync.ingest(p)?;
        }
        for u in &saved.pre_root_journal {
            sync.ingest(u)?;
        }
        sync.needs_checkpoint |= saved.needs_checkpoint && sync.note.document().is_shallow();
        Ok(sync)
    }

    /// Atomically persist sync state (snapshot + pending + pre-root journal) to disk.
    pub fn save_to_file(&self, path: &Path) -> std::io::Result<()> {
        let state = self
            .persist()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let bytes = serde_json::to_vec(&state).map_err(std::io::Error::other)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(tmp, path)
    }

    /// Restore a persisted [`NoteSync`] from disk.
    pub fn load_from_file(path: &Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        let state: PersistedSyncState =
            serde_json::from_slice(&bytes).map_err(std::io::Error::other)?;
        Self::restore(&state).map_err(|e| std::io::Error::other(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use margin_editor_core::checkbox::{Input, Mode};

    #[test]
    fn note_sync_persists_pending_and_rebases_comment_anchors_across_epochs() {
        let a = Note::new("- [ ] Ship sync\nShared body\n");
        let vv_f1 = a.document().oplog_vv();
        let e1_snap = a
            .document()
            .export(ExportMode::shallow_snapshot(&a.document().oplog_frontiers()))
            .unwrap();

        // Stale client B boots from E1 and edits offline with a checkbox toggle + comment anchor
        let b_doc = LoroDoc::new();
        b_doc.import(&e1_snap).unwrap();
        let mut b_sync = NoteSync::new(Note::from_doc(b_doc), 1);
        let box_at = b_sync.note().markdown().find("[ ]").unwrap() + 1;
        b_sync
            .note()
            .edit_marker(box_at..box_at, Mode::Structured, Input::Type('x'))
            .unwrap();
        let shared_at = b_sync.note().markdown().find("Shared body").unwrap();
        b_sync
            .note()
            .replace(shared_at..shared_at + 11, "Shared body (B)")
            .unwrap();
        let b_pos = b_sync.note().markdown().find("(B)").unwrap();
        let b_anchor = b_sync.note().anchor_range(b_pos..b_pos + 3).unwrap();

        // Online peer advances to lagged cutoff F2, then into lag window
        let end = a.markdown().len();
        a.replace(end..end, "Pre-F2 online\n").unwrap();
        let f2 = a.document().oplog_frontiers();
        let end2 = a.markdown().len();
        a.replace(end2..end2, "Lag window edit\n").unwrap();
        let delta_1_to_2 = a.document().export(ExportMode::updates(&vv_f1)).unwrap();
        let e2_snap = a.document().export(ExportMode::shallow_snapshot(&f2)).unwrap();

        // Rebase B from E1 -> E2 and verify healthy shallow S imports with zero history fetches
        let rebased = b_sync
            .rebase_onto_epoch(&e1_snap, &delta_1_to_2, &e2_snap, &[], 2, &[&b_anchor])
            .unwrap();
        assert_eq!(b_sync.epoch(), 2);

        let s_doc = LoroDoc::new();
        s_doc.import(&e2_snap).unwrap();
        let mut s_sync = NoteSync::new(Note::from_doc(s_doc), 2);
        assert_eq!(s_sync.ingest(&rebased.rebased_update).unwrap(), SyncDemand::None);
        assert!(s_sync.note().document().is_shallow());
        assert_eq!(s_sync.note().markdown(), b_sync.note().markdown());
        assert!(s_sync.note().markdown().contains("- [x] Ship sync"));
        assert!(s_sync.note().markdown().contains("Shared body (B)"));

        let new_anchor = rebased.reanchored[0].as_ref().unwrap();
        let resolved = s_sync.note().resolve_anchor(new_anchor).unwrap();
        assert_eq!(&s_sync.note().markdown()[resolved], "(B)");

        // Round-trip atomic disk persistence
        let tmp = std::env::temp_dir().join(format!("margin-sync-test-{}.json", std::process::id()));
        s_sync.save_to_file(&tmp).unwrap();
        let loaded = NoteSync::load_from_file(&tmp).unwrap();
        let _ = std::fs::remove_file(&tmp);
        assert_eq!(loaded.epoch(), 2);
        assert_eq!(loaded.note().markdown(), s_sync.note().markdown());
    }
}
