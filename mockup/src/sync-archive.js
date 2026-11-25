import { Cursor, LoroDoc, decodeImportBlobMeta } from "loro-crdt/base64";
import { checkedStyle, locate, remember } from "./loro-editor.js";

/** Fast, browser-safe 64-bit FNV-1a hex fingerprint for update deduplication. */
function bytesKey(bytes) {
  let h1 = 0x811c9dc5;
  let h2 = 0x01000193;
  for (let i = 0; i < bytes.length; i++) {
    h1 = Math.imul(h1 ^ bytes[i], 0x01000193) >>> 0;
    h2 = Math.imul(h2 ^ bytes[i], 0x85ebca6b) >>> 0;
  }
  return `${bytes.length}:${h1.toString(16)}:${h2.toString(16)}`;
}

export function configureDoc(doc) {
  doc.configTextStyle({ [checkedStyle]: { expand: "none" }, bold: { expand: "after" } });
  return doc;
}

/**
 * Compute half-open logical spans `[have, dep.counter + 1)` missing from `vvMap`
 * for an update blob's external `startFrontiers`.
 */
export function missingSpansFromMeta(meta, vvMap) {
  const needEnd = new Map();
  for (const dep of meta.startFrontiers) {
    const peer = String(dep.peer);
    const end = dep.counter + 1;
    if (end > (vvMap.get(peer) ?? 0)) {
      needEnd.set(peer, Math.max(needEnd.get(peer) ?? 0, end));
    }
  }
  return [...needEnd].map(([peer, end]) => ({ peer, start: vvMap.get(peer) ?? 0, end }));
}

export function isShallowOutdatedError(err) {
  return /shallow history|outdated/i.test(String(err?.message ?? err ?? ""));
}

/**
 * Transport-independent on-demand history session for a single LoroDoc.
 * All methods are synchronous and never perform I/O while holding document state.
 */
export class SyncSession {
  constructor(doc, { epoch = 0, configure = configureDoc } = {}) {
    this.doc = configure(doc);
    this.epoch = epoch;
    this.configure = configure;
    this.preRootJournal = [];
    this.seen = new Set();
    this.needEnd = new Map(); // peer -> required end counter
    this.needsCheckpoint = false;
  }

  _recordDeps(bytes) {
    const vv = this.doc.oplogVersion().toJSON();
    for (const { peer, end } of missingSpansFromMeta(decodeImportBlobMeta(bytes, true), vv)) {
      this.needEnd.set(peer, Math.max(this.needEnd.get(peer) ?? 0, end));
    }
  }

  /** Import an update or snapshot, recording missing spans or journaling pre-root rejections. */
  ingest(bytes) {
    const key = bytesKey(bytes);
    if (this.seen.has(key)) return { changed: false, demand: this.nextDemand() };
    const beforeOps = this.doc.opCount();
    try {
      const status = this.doc.import(bytes);
      this.seen.add(key);
      if (status.pending?.size) this._recordDeps(bytes);
      const pending = this.doc.exportPendingUpdates();
      if (pending) this._recordDeps(pending);
      return {
        changed: beforeOps !== this.doc.opCount() || Boolean(status.pending?.size),
        demand: this.nextDemand(),
      };
    } catch (err) {
      if (this.doc.isShallow() && isShallowOutdatedError(err)) {
        this.seen.add(key);
        this.preRootJournal.push(new Uint8Array(bytes));
        this.needsCheckpoint = true;
        return { changed: true, demand: this.nextDemand() };
      }
      throw err;
    }
  }

  /** Resolve a cursor; detects pre-cutoff deleted targets (`HistoryCleared`) on shallow docs. */
  resolveCursor(cursorBytes) {
    const cursor = Cursor.decode(cursorBytes);
    try {
      const res = this.doc.getCursorPos(cursor);
      if (res !== undefined) {
        res.update?.free();
        return { status: "resolved", offset: res.offset, side: res.side };
      }
      const id = cursor.pos();
      if (this.doc.isShallow() && id) {
        const shallowEnd = this.doc.shallowSinceVV().toJSON().get(String(id.peer)) ?? 0;
        if (shallowEnd > id.counter) {
          this.needsCheckpoint = true;
          return { status: "needs-history", demand: this.nextDemand() };
        }
      }
      return { status: "not-found" };
    } finally {
      cursor.free();
    }
  }

  /** Export logical spans `{ peer, start, end }[]` if this doc holds them (returns null if trimmed). */
  exportSpans(spans) {
    const vv = this.doc.oplogVersion().toJSON();
    const shallow = this.doc.isShallow() ? this.doc.shallowSinceVV().toJSON() : null;
    const req = [];
    for (const s of spans) {
      const peer = String(s.peer);
      if (shallow && (shallow.get(peer) ?? 0) > s.start) return null;
      const have = vv.get(peer) ?? 0;
      const end = Math.min(have, s.end);
      if (end > s.start) req.push({ id: { peer, counter: s.start }, len: end - s.start });
    }
    if (!req.length) return null;
    return this.doc.export({ mode: "updates-in-range", spans: req });
  }

  nextDemand() {
    const vv = this.doc.oplogVersion().toJSON();
    for (const [peer, end] of this.needEnd) {
      if ((vv.get(peer) ?? 0) >= end) this.needEnd.delete(peer);
    }
    if (this.needsCheckpoint && this.doc.isShallow()) {
      return { kind: "needs-checkpoint", shallowSinceFrontiers: this.doc.shallowSinceFrontiers() };
    }
    if (this.needEnd.size) {
      return {
        kind: "missing-spans",
        spans: [...this.needEnd].map(([peer, end]) => ({ peer, start: vv.get(peer) ?? 0, end })),
      };
    }
    return { kind: "none" };
  }

  /** Impl A: rebuild a fresh full replica from a retained checkpoint + local suffix + journals. */
  rebuildFromCheckpoint(checkpointBytes) {
    const fresh = this.configure(new LoroDoc());
    fresh.setPeerId(this.doc.peerId);
    fresh.import(checkpointBytes);
    fresh.import(this.doc.export({ mode: "update", from: fresh.oplogVersion() }));
    const pending = this.doc.exportPendingUpdates();
    if (pending) fresh.import(pending);
    this.preRootJournal = this.preRootJournal.filter((u) => fresh.import(u).pending?.size);
    const old = this.doc;
    this.doc = fresh;
    old.free();
    this.needsCheckpoint = false;
    this.needEnd.clear();
    const stillPending = this.doc.exportPendingUpdates();
    if (stillPending) this._recordDeps(stillPending);
    return this.nextDemand();
  }

  /**
   * Sender-side CRDT-assisted rebase across epochs (`E_old -> E_new`) with zero genesis history.
   * Mints a fresh PeerID on `E_new` to guarantee unique op IDs and re-anchors offline cursors.
   */
  rebaseOntoEpoch({ baseEpochSnap, deltaToNew, newEpochSnap, newEpochTail = [], newEpoch, cursors = [] }) {
    const headRef = this.configure(new LoroDoc());
    headRef.import(baseEpochSnap);
    headRef.import(deltaToNew);
    for (const u of newEpochTail) headRef.import(u);
    const headFrontiers = headRef.oplogFrontiers();
    headRef.free();

    this.doc.import(deltaToNew);
    for (const u of newEpochTail) this.doc.import(u);
    const offsets = cursors.map((c) => locate(this.doc, c));
    const diff = this.doc.diff(headFrontiers, this.doc.oplogFrontiers(), false);

    // Fresh PeerID avoids op-ID reuse across rebases.
    const nextDoc = this.configure(new LoroDoc());
    nextDoc.import(newEpochSnap);
    for (const u of newEpochTail) nextDoc.import(u);
    const before = nextDoc.oplogVersion();
    nextDoc.applyDiff(diff);
    nextDoc.commit();

    const text = nextDoc.getText("markdown");
    const reanchored = offsets.map((off) => (off == null ? null : remember(text, off)));
    const rebasedUpdate = nextDoc.export({ mode: "update", from: before });

    const old = this.doc;
    this.doc = nextDoc;
    this.epoch = newEpoch;
    this.preRootJournal = [];
    this.needsCheckpoint = false;
    this.needEnd.clear();
    old.free();
    return { rebasedUpdate, reanchored };
  }

  persist() {
    return {
      epoch: this.epoch,
      peerId: this.doc.peerId,
      snapshot: this.doc.export({ mode: "snapshot" }),
      pending: this.doc.exportPendingUpdates() ?? null,
      preRootJournal: this.preRootJournal.map((b) => new Uint8Array(b)),
      needsCheckpoint: this.needsCheckpoint,
    };
  }

  static restore(saved, opts = {}) {
    const doc = (opts.configure ?? configureDoc)(new LoroDoc());
    doc.setPeerId(saved.peerId);
    doc.import(saved.snapshot);
    const session = new SyncSession(doc, { epoch: saved.epoch ?? 0, configure: opts.configure });
    if (saved.pending) session.ingest(saved.pending);
    for (const u of saved.preRootJournal ?? []) session.ingest(u);
    session.needsCheckpoint ||= Boolean(saved.needsCheckpoint) && doc.isShallow();
    return session;
  }
}
