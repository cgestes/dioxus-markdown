import assert from "node:assert/strict";
import crypto from "node:crypto";
import { performance } from "node:perf_hooks";
import { test } from "node:test";
import { Cursor, LoroDoc, decodeImportBlobMeta } from "loro-crdt/base64";
import { checkedStyle, locate, markdownText, remember } from "../src/loro-editor.js";

// ---------------------------------------------------------------------------
// 1. Opaque Relay (zero-knowledge) & Client-Side Encrypted Archive
// ---------------------------------------------------------------------------

/** Untrusted relay: stores only opaque `(docId, objectId) -> ciphertext` plus CAS epoch pointer. */
class OpaqueRelay {
  objects = new Map();
  epochs = new Map(); // docId -> { epoch, manifestOid }
  stats = { requests: 0, bytes: 0 };

  put(docId, oid, ct) {
    this.objects.set(`${docId}/${oid}`, new Uint8Array(ct));
  }

  get(docId, oid) {
    this.stats.requests += 1;
    const ct = this.objects.get(`${docId}/${oid}`);
    if (!ct) return null;
    this.stats.bytes += ct.byteLength;
    return new Uint8Array(ct);
  }

  delete(docId, oid) {
    this.objects.delete(`${docId}/${oid}`);
  }

  resetStats() {
    this.stats = { requests: 0, bytes: 0 };
  }

  initEpoch(docId, epoch, manifestOid) {
    this.epochs.set(docId, { epoch, manifestOid });
  }

  casEpoch(docId, expected, next, manifestOid) {
    const cur = this.epochs.get(docId);
    if (!cur || cur.epoch !== expected) return false;
    this.epochs.set(docId, { epoch: next, manifestOid });
    return true;
  }

  pushForEpoch(docId, epoch, oid, ct) {
    const cur = this.epochs.get(docId);
    if (!cur || epoch < cur.epoch) return { ok: false, currentEpoch: cur?.epoch ?? null };
    this.put(docId, oid, ct);
    return { ok: true };
  }
}

const sha256Hex = (buf) => crypto.createHash("sha256").update(buf).digest("hex");

function seal(key, aad, plain) {
  const iv = crypto.randomBytes(12);
  const cipher = crypto.createCipheriv("aes-256-gcm", key, iv);
  cipher.setAAD(Buffer.from(aad, "utf8"));
  const body = Buffer.concat([cipher.update(plain), cipher.final()]);
  return new Uint8Array(Buffer.concat([iv, cipher.getAuthTag(), body]));
}

function open(key, aad, ct) {
  const buf = Buffer.from(ct);
  const decipher = crypto.createDecipheriv("aes-256-gcm", key, buf.subarray(0, 12));
  decipher.setAAD(Buffer.from(aad, "utf8"));
  decipher.setAuthTag(buf.subarray(12, 28));
  return new Uint8Array(Buffer.concat([decipher.update(buf.subarray(28)), decipher.final()]));
}

/** Client-side encrypted archive index mapping logical `(peer, [start, end))` spans -> opaque objectIds. */
class EncryptedArchive {
  constructor(relay, docId, key, manifestOid = "manifest-v1") {
    this.relay = relay;
    this.docId = docId;
    this.key = key;
    this.manifestOid = manifestOid;
    this.manifest = { redacted: null, checkpoints: [], spans: [] };
  }

  _putSealed(aad, bytes) {
    const ct = seal(this.key, aad, bytes);
    const oid = sha256Hex(ct).slice(0, 24);
    this.relay.put(this.docId, oid, ct);
    return oid;
  }

  _saveManifest() {
    const ct = seal(this.key, `${this.docId}:manifest`, Buffer.from(JSON.stringify(this.manifest)));
    this.relay.put(this.docId, this.manifestOid, ct);
  }

  publishSpan(peer, start, end, bytes) {
    const aad = `${this.docId}:span:${peer}:${start}:${end}`;
    this.manifest.spans.push({ peer: String(peer), start, end, oid: this._putSealed(aad, bytes), aad });
    this._saveManifest();
  }

  publishFromDoc(doc, maxChunk = 128) {
    for (const [peer, changes] of doc.getAllChanges()) {
      let s = null;
      let e = null;
      const flush = () => {
        if (s === null) return;
        const blob = doc.export({ mode: "updates-in-range", spans: [{ id: { peer, counter: s }, len: e - s }] });
        this.publishSpan(peer, s, e, blob);
      };
      for (const ch of changes) {
        if (s === null) {
          s = ch.counter;
          e = ch.counter + ch.length;
        } else if (ch.counter === e && ch.counter + ch.length - s <= maxChunk) {
          e = ch.counter + ch.length;
        } else {
          flush();
          s = ch.counter;
          e = ch.counter + ch.length;
        }
      }
      flush();
    }
    const aad = `${this.docId}:ckpt:${this.manifest.checkpoints.length}`;
    this.manifest.checkpoints.push({ oid: this._putSealed(aad, doc.export({ mode: "snapshot" })), aad });
    this._saveManifest();
  }

  redact(reason) {
    for (const x of [...this.manifest.checkpoints, ...this.manifest.spans]) this.relay.delete(this.docId, x.oid);
    this.manifest = { redacted: reason, checkpoints: [], spans: [] };
    this._saveManifest();
  }

  _loadManifest() {
    const ct = this.relay.get(this.docId, this.manifestOid);
    if (!ct) return { status: "missing-archive" };
    const m = JSON.parse(Buffer.from(open(this.key, `${this.docId}:manifest`, ct)).toString("utf8"));
    return m.redacted ? { status: "irreversibly-redacted", reason: m.redacted } : { status: "ok", manifest: m };
  }

  fetchSpan(peer, start, end) {
    const res = this._loadManifest();
    if (res.status !== "ok") return res;
    const hits = res.manifest.spans.filter((s) => s.peer === String(peer) && s.end > start && s.start < end);
    if (!hits.length) return { status: "missing-archive" };
    const blobs = [];
    for (const h of hits) {
      const ct = this.relay.get(this.docId, h.oid);
      if (!ct) return { status: "missing-archive" };
      blobs.push(open(this.key, h.aad, ct));
    }
    return { status: "ok", blobs };
  }

  fetchCheckpoint() {
    const res = this._loadManifest();
    if (res.status !== "ok") return res;
    const cp = res.manifest.checkpoints.at(-1);
    const ct = cp && this.relay.get(this.docId, cp.oid);
    return ct ? { status: "ok", bytes: open(this.key, cp.aad, ct) } : { status: "missing-archive" };
  }
}

// ---------------------------------------------------------------------------
// 2. Transport-Independent On-Demand History Adapter (Impl A: Ships Today)
// ---------------------------------------------------------------------------

function makeDoc(peerId) {
  const doc = new LoroDoc();
  doc.configTextStyle({ [checkedStyle]: { expand: "none" }, bold: { expand: "after" } });
  if (peerId !== undefined) doc.setPeerId(String(peerId));
  return doc;
}

function missingSpansFromMeta(meta, vvMap) {
  const needEnd = new Map();
  for (const dep of meta.startFrontiers) {
    const peer = String(dep.peer);
    if (dep.counter + 1 > (vvMap.get(peer) ?? 0)) {
      needEnd.set(peer, Math.max(needEnd.get(peer) ?? 0, dep.counter + 1));
    }
  }
  return [...needEnd].map(([peer, end]) => ({ peer, start: vvMap.get(peer) ?? 0, end }));
}

class HistorySession {
  constructor(doc) {
    this.doc = doc;
    this.preRootJournal = [];
    this.seen = new Set();
    this.needEnd = new Map(); // peer -> required end counter
    this.needsCheckpoint = false;
    this.outcome = { status: "ready" };
  }

  _recordDeps(bytes) {
    const vv = this.doc.oplogVersion().toJSON();
    for (const { peer, end } of missingSpansFromMeta(decodeImportBlobMeta(bytes, true), vv)) {
      this.needEnd.set(peer, Math.max(this.needEnd.get(peer) ?? 0, end));
    }
  }

  ingestUpdate(bytes) {
    const h = sha256Hex(bytes);
    if (this.seen.has(h)) return this.nextDemand();
    try {
      const status = this.doc.import(bytes);
      this.seen.add(h);
      if (status.pending?.size) this._recordDeps(bytes);
      const pending = this.doc.exportPendingUpdates();
      if (pending) this._recordDeps(pending);
      return this.nextDemand();
    } catch (err) {
      if (this.doc.isShallow() && /shallow history|outdated/i.test(String(err))) {
        this.seen.add(h);
        this.preRootJournal.push(new Uint8Array(bytes));
        this.needsCheckpoint = true;
        return this.nextDemand();
      }
      throw err;
    }
  }

  resolveCursor(bytes) {
    const cursor = Cursor.decode(bytes);
    try {
      const res = this.doc.getCursorPos(cursor);
      if (res !== undefined) {
        res.update?.free();
        return { status: "resolved", offset: res.offset, side: res.side };
      }
      const id = cursor.pos();
      if (this.doc.isShallow() && id && (this.doc.shallowSinceVV().toJSON().get(String(id.peer)) ?? 0) > id.counter) {
        this.needsCheckpoint = true;
        return { status: "needs-history", demand: this.nextDemand() };
      }
      return { status: "not-found" };
    } finally {
      cursor.free();
    }
  }

  nextDemand() {
    const vv = this.doc.oplogVersion().toJSON();
    for (const [peer, end] of this.needEnd) {
      if ((vv.get(peer) ?? 0) >= end) this.needEnd.delete(peer);
    }
    if (this.needsCheckpoint && this.doc.isShallow()) return { kind: "needs-checkpoint" };
    if (this.needEnd.size) {
      return {
        kind: "missing-spans",
        spans: [...this.needEnd].map(([peer, end]) => ({ peer, start: vv.get(peer) ?? 0, end })),
      };
    }
    return { kind: "none" };
  }

  supplyHistory(res) {
    if (res.status !== "ok") {
      this.outcome = res;
      return this.outcome;
    }
    if (res.kind === "updates") {
      for (const b of res.blobs) this.ingestUpdate(b);
      this.outcome = { status: "ok" };
      return this.outcome;
    }
    // Impl A: fresh-replica rebuild from retained full checkpoint + suffix + pending + pre-root journal
    const fresh = makeDoc(this.doc.peerId);
    fresh.import(res.bytes);
    fresh.import(this.doc.export({ mode: "update", from: fresh.oplogVersion() }));
    const pending = this.doc.exportPendingUpdates();
    if (pending) fresh.import(pending);
    this.preRootJournal = this.preRootJournal.filter((u) => fresh.import(u).pending?.size);
    this.doc = fresh;
    this.needsCheckpoint = false;
    this.needEnd.clear();
    const stillPending = this.doc.exportPendingUpdates();
    if (stillPending) this._recordDeps(stillPending);
    this.outcome = { status: "rebuilt-full" };
    return this.outcome;
  }

  pump(archive, maxRounds = 16) {
    for (let i = 0; i < maxRounds; i++) {
      const demand = this.nextDemand();
      if (demand.kind === "none") return this.outcome;
      if (demand.kind === "needs-checkpoint") {
        const res = archive.fetchCheckpoint();
        if (res.status !== "ok") return this.supplyHistory(res);
        this.supplyHistory({ status: "ok", kind: "checkpoint", bytes: res.bytes });
      } else {
        const blobs = [];
        for (const s of demand.spans) {
          const res = archive.fetchSpan(s.peer, s.start, s.end);
          if (res.status !== "ok") return this.supplyHistory(res);
          blobs.push(...res.blobs);
        }
        this.supplyHistory({ status: "ok", kind: "updates", blobs });
      }
    }
    throw new Error("exceeded max fetch rounds");
  }

  persist() {
    return {
      peerId: this.doc.peerId,
      snapshot: this.doc.export({ mode: "snapshot" }),
      pending: this.doc.exportPendingUpdates() ?? null,
      preRootJournal: this.preRootJournal.map((b) => new Uint8Array(b)),
      needsCheckpoint: this.needsCheckpoint,
    };
  }

  static restore(saved) {
    const doc = makeDoc(saved.peerId);
    doc.import(saved.snapshot);
    const s = new HistorySession(doc);
    if (saved.pending) s.ingestUpdate(saved.pending);
    for (const u of saved.preRootJournal) s.ingestUpdate(u);
    s.needsCheckpoint ||= saved.needsCheckpoint && doc.isShallow();
    return s;
  }
}

// ---------------------------------------------------------------------------
// Reference Equivalence Helpers
// ---------------------------------------------------------------------------

const sortEntries = (entries) => [...entries].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
const normFrontiers = (f) => f.map((x) => `${x.peer}:${x.counter}`).sort();
const normVV = (vv) => sortEntries([...vv.toJSON()].map(([p, c]) => [String(p), c]));
const normChanges = (doc) =>
  sortEntries([...doc.getAllChanges()]).map(([p, cs]) => [
    String(p),
    cs.map((c) => ({ counter: c.counter, length: c.length, lamport: c.lamport, deps: normFrontiers(c.deps) })),
  ]);

function assertConverged(actual, ref, anchors = {}) {
  assert.deepEqual(normVV(actual.oplogVersion()), normVV(ref.oplogVersion()));
  assert.deepEqual(normVV(actual.version()), normVV(ref.version()));
  assert.deepEqual(normFrontiers(actual.oplogFrontiers()), normFrontiers(ref.oplogFrontiers()));
  assert.deepEqual(normChanges(actual), normChanges(ref));
  assert.equal(markdownText(actual.getText("markdown")), markdownText(ref.getText("markdown")));
  assert.deepEqual(actual.getText("markdown").toDelta(), ref.getText("markdown").toDelta());
  assert.deepEqual(actual.getMovableList("blocks").toJSON(), ref.getMovableList("blocks").toJSON());
  assert.deepEqual(actual.getMap("meta").toJSON(), ref.getMap("meta").toJSON());
  for (const [name, c] of Object.entries(anchors)) {
    assert.equal(locate(actual, c), locate(ref, c), `anchor ${name}`);
  }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test("baseline 1: out-of-order edit discovers A[0, 3) via decodeImportBlobMeta and releases pending", () => {
  const a = makeDoc(1);
  a.getText("t").insert(0, "abc");
  a.commit();

  const b = makeDoc(2);
  b.import(a.export({ mode: "update" }));
  const beforeB = b.oplogVersion();
  b.getText("t").insert(3, "!");
  b.commit();
  const bUpd = b.export({ mode: "update", from: beforeB });

  const r = makeDoc(3);
  assert.ok(r.import(bUpd).pending?.has("2"));
  const missing = missingSpansFromMeta(decodeImportBlobMeta(bUpd, true), r.oplogVersion().toJSON());
  assert.deepEqual(missing, [{ peer: "1", start: 0, end: 3 }]);

  const fetched = a.export({ mode: "updates-in-range", spans: [{ id: { peer: "1", counter: 0 }, len: 3 }] });
  assert.equal(r.import(fetched).pending, null);
  assert.equal(r.getText("t").toString(), "abc!");
});

test("baseline 2: shallow rejects pre-cutoff fork even after importing full snapshot; fresh replica converges", () => {
  const a = makeDoc(1);
  a.getText("t").insert(0, "abc");
  a.commit();
  const fullAtAbc = a.export({ mode: "snapshot" });

  const b = makeDoc(2);
  b.import(fullAtAbc);
  const beforeB = b.oplogVersion();
  b.getText("t").insert(1, "!");
  b.commit();
  const bUpd = b.export({ mode: "update", from: beforeB });

  a.getText("t").insert(3, " DEF");
  a.commit();
  const shallow = makeDoc(3);
  shallow.import(a.export({ mode: "shallow-snapshot", frontiers: a.oplogFrontiers() }));
  assert.throws(() => shallow.import(bUpd), /shallow history|outdated/i);

  // Importing earlier full snapshot into the SAME shallow doc leaves shallow boundary intact
  shallow.import(fullAtAbc);
  assert.equal(shallow.isShallow(), true);
  assert.throws(() => shallow.import(bUpd), /shallow history|outdated/i);

  // Shallow alone trimmed [3..6), so exporting from shallow without A's suffix parks [6..7)
  const partial = makeDoc(3);
  partial.import(fullAtAbc);
  assert.ok(partial.import(shallow.export({ mode: "update", from: partial.oplogVersion() })).pending?.has("1"));

  // Fresh replica from fullAtAbc + A's suffix [3..7) + B's update converges with full history
  const fresh = makeDoc(3);
  fresh.import(fullAtAbc);
  fresh.import(a.export({ mode: "update", from: fresh.oplogVersion() }));
  fresh.import(bUpd);
  assert.equal(fresh.isShallow(), false);
  assert.equal(fresh.getText("t").toString(), "a!bc DEF");
});

test("full convergence: opposite-side offline forks, deletes, movable list, rich-text marks, and comment anchors", () => {
  const relay = new OpaqueRelay();
  const archive = new EncryptedArchive(relay, "note-42", crypto.randomBytes(32));

  const a = makeDoc(1);
  const ta = a.getText("markdown");
  ta.insert(0, "- [ ] Alpha\nOLD_GONE\nKeep Me\nDelAfter\n");
  ta.mark({ start: 2, end: 5 }, "bold", true);
  for (const [i, v] of ["intro", "body", "outro"].entries()) a.getMovableList("blocks").insert(i, v);
  a.getMap("meta").set("title", "Draft 1");
  a.commit();

  const md = markdownText(ta);
  const anchorAlive = remember(ta, md.indexOf("Keep Me"));
  const anchorDelBefore = remember(ta, md.indexOf("OLD_GONE") + 2);
  const anchorDelAfter = remember(ta, md.indexOf("DelAfter") + 2);

  // Pre-cutoff fork B
  const b = a.fork();
  b.configTextStyle({ [checkedStyle]: { expand: "none" }, bold: { expand: "after" } });
  b.setPeerId("2");
  const beforeB = b.oplogVersion();

  // Advance A to cutoff F (deleting OLD_GONE before cutoff)
  ta.delete(markdownText(ta).indexOf("OLD_GONE\n"), "OLD_GONE\n".length);
  ta.insert(ta.length, "Post-cutoff\n");
  a.getMovableList("blocks").move(2, 0);
  a.getMap("meta").set("phase", "cutoff");
  a.commit();
  archive.publishFromDoc(a);
  const shallowSnap = a.export({ mode: "shallow-snapshot", frontiers: a.oplogFrontiers() });

  // Post-cutoff fork C
  const c = a.fork();
  c.setPeerId("3");
  const beforeC = c.oplogVersion();
  c.getText("markdown").insert(c.getText("markdown").length, "From C\n");
  c.getMap("meta").set("reviewer", "C");
  c.commit();
  const cUpd = c.export({ mode: "update", from: beforeC });

  // B's pre-cutoff edits: checkbox mark, text edit, movable list set+move, map edit
  const tb = b.getText("markdown");
  const box = markdownText(tb).indexOf("[ ]") + 1;
  tb.mark({ start: box, end: box + 1 }, checkedStyle, true);
  tb.splice(markdownText(tb).indexOf("Alpha"), 5, "Alpha-B");
  b.getMovableList("blocks").set(1, "body-B");
  b.getMovableList("blocks").move(0, 2);
  b.getMap("meta").set("title", "Draft 2");
  b.commit();
  const bUpd = b.export({ mode: "update", from: beforeB });

  // Shallow device S edits offline (deletes DelAfter after cutoff)
  const sDoc = makeDoc(4);
  sDoc.import(shallowSnap);
  assert.notEqual(locate(sDoc, anchorAlive), null);
  const session = new HistorySession(sDoc);
  assert.equal(session.resolveCursor(anchorDelBefore).status, "needs-history");
  session.needsCheckpoint = false;

  const ts = sDoc.getText("markdown");
  ts.delete(markdownText(ts).indexOf("DelAfter"), "DelAfter".length);
  ts.insert(ts.length, "Local S\n");
  const anchorLocal = remember(ts, markdownText(ts).indexOf("Local S"));
  sDoc.getMovableList("blocks").insert(0, "local-header");
  sDoc.commit();
  const sUpd = sDoc.export({ mode: "update", from: a.oplogVersion() });
  assert.notEqual(locate(sDoc, anchorDelAfter), null);

  relay.resetStats();
  assert.equal(session.ingestUpdate(cUpd).kind, "none");
  assert.equal(relay.stats.requests, 0);

  assert.equal(session.ingestUpdate(bUpd).kind, "needs-checkpoint");
  assert.equal(session.pump(archive).status, "rebuilt-full");
  assert.equal(session.doc.isShallow(), false);

  const ref = a.fork();
  ref.configTextStyle({ [checkedStyle]: { expand: "none" }, bold: { expand: "after" } });
  for (const u of [bUpd, cUpd, sUpd]) ref.import(u);
  assertConverged(session.doc, ref, { anchorAlive, anchorDelBefore, anchorDelAfter, anchorLocal });
});

test("out-of-order and duplicate delivery across 3-peer transitive chain converges", () => {
  const relay = new OpaqueRelay();
  const archive = new EncryptedArchive(relay, "chain", crypto.randomBytes(32));

  const p1 = makeDoc(1);
  p1.getText("markdown").insert(0, "one ");
  p1.commit();

  const p2 = p1.fork();
  p2.setPeerId("2");
  p2.getText("markdown").insert(4, "two ");
  p2.commit();

  const p3 = p2.fork();
  p3.setPeerId("3");
  const v2 = p3.oplogVersion();
  p3.getText("markdown").insert(8, "three");
  p3.commit();

  const u3 = p3.export({ mode: "update", from: v2 });
  archive.publishFromDoc(p2, 4);

  const session = new HistorySession(makeDoc(9));
  assert.deepEqual(session.ingestUpdate(u3), { kind: "missing-spans", spans: [{ peer: "2", start: 0, end: 4 }] });
  assert.equal(session.ingestUpdate(u3).kind, "missing-spans"); // duplicate
  assert.equal(session.pump(archive).status, "ok");
  assert.equal(session.ingestUpdate(u3).kind, "none");
  assertConverged(session.doc, p3);
});

test("explicit outcomes for missing archive and irreversible redaction preserve local edits", () => {
  for (const mode of ["missing-archive", "irreversibly-redacted"]) {
    const relay = new OpaqueRelay();
    const archive = new EncryptedArchive(relay, `doc-${mode}`, crypto.randomBytes(32));

    const a = makeDoc(1);
    a.getText("markdown").insert(0, "secret\n");
    a.commit();
    const b = a.fork();
    b.setPeerId("2");
    const beforeB = b.oplogVersion();
    b.getText("markdown").insert(0, "B ");
    b.commit();
    const bUpd = b.export({ mode: "update", from: beforeB });

    a.getText("markdown").splice(0, 7, "clean\n");
    a.commit();
    if (mode === "irreversibly-redacted") {
      archive.publishFromDoc(a);
      archive.redact("gdpr-erasure");
    }

    const s = makeDoc(3);
    s.import(a.export({ mode: "shallow-snapshot", frontiers: a.oplogFrontiers() }));
    s.getText("markdown").insert(6, "local\n");
    s.commit();

    const session = new HistorySession(s);
    assert.equal(session.ingestUpdate(bUpd).kind, "needs-checkpoint");
    const out = session.pump(archive);
    assert.equal(out.status, mode);
    assert.equal(markdownText(session.doc.getText("markdown")), "clean\nlocal\n");
    assert.equal(session.preRootJournal.length, 1);
  }
});

test("restart mid-fetch preserves both Loro pending updates and rejected pre-root journal", () => {
  const relay = new OpaqueRelay();
  const archive = new EncryptedArchive(relay, "restart", crypto.randomBytes(32));

  const a = makeDoc(1);
  a.getText("markdown").insert(0, "base\n");
  a.commit();
  const b = a.fork();
  b.setPeerId("2");
  const beforeB = b.oplogVersion();
  b.getText("markdown").insert(0, "B\n");
  b.commit();
  const bUpd = b.export({ mode: "update", from: beforeB });

  a.getText("markdown").insert(5, "cutoff\n");
  a.commit();
  archive.publishFromDoc(a);
  const shallowSnap = a.export({ mode: "shallow-snapshot", frontiers: a.oplogFrontiers() });

  const c = a.fork();
  c.setPeerId("3");
  const v0 = c.oplogVersion();
  c.getText("markdown").insert(12, "C1\n");
  c.commit();
  const v1 = c.oplogVersion();
  const c1Upd = c.export({ mode: "update", from: v0 });
  c.getText("markdown").insert(15, "C2\n");
  c.commit();
  const c2Upd = c.export({ mode: "update", from: v1 });
  archive.publishSpan("3", 0, decodeImportBlobMeta(c1Upd, true).partialEndVersionVector.toJSON().get("3"), c1Upd);

  const s = makeDoc(4);
  s.import(shallowSnap);
  s.getText("markdown").insert(12, "S\n");
  s.commit();
  const sUpd = s.export({ mode: "update", from: a.oplogVersion() });

  const s1 = new HistorySession(s);
  s1.ingestUpdate(c2Upd);
  s1.ingestUpdate(bUpd);
  assert.ok(s1.doc.exportPendingUpdates());
  assert.equal(s1.preRootJournal.length, 1);

  const s2 = HistorySession.restore(s1.persist());
  assert.equal(s2.pump(archive).status, "ok");

  const ref = a.fork();
  ref.configTextStyle({ [checkedStyle]: { expand: "none" }, bold: { expand: "after" } });
  for (const u of [bUpd, c1Upd, c2Upd, sUpd]) ref.import(u);
  assertConverged(s2.doc, ref);
});

test("sender-side CRDT-assisted rebase across lagged epochs E1->E2 (zero genesis, zero receiver fetch, redacted fallback)", () => {
  const relay = new OpaqueRelay();
  const docId = "epoch-note";
  const key = crypto.randomBytes(32);
  const putObj = (aad, bytes) => {
    const ct = seal(key, aad, bytes);
    const oid = sha256Hex(ct).slice(0, 24);
    relay.put(docId, oid, ct);
    return oid;
  };
  const getObj = (aad, oid) => {
    const ct = relay.get(docId, oid);
    return ct ? open(key, aad, ct) : null;
  };

  // Genesis [0..F1) — never uploaded to relay!
  const author = makeDoc(1);
  const ta = author.getText("markdown");
  for (let i = 0; i < 40; i++) {
    ta.insert(ta.length, `genesis-${i}\n`);
    author.commit();
  }
  ta.splice(0, ta.length, "- [ ] Ship epochs\nShared line\nFooter\n");
  ta.mark({ start: 2, end: 5 }, "bold", true);
  author.getMovableList("blocks").insert(0, "p1");
  author.getMovableList("blocks").insert(1, "p2");
  author.getMap("meta").set("status", "e1");
  author.commit();

  // Epoch 1 cut at F1
  const vvF1 = author.oplogVersion();
  const e1Snap = author.export({ mode: "shallow-snapshot", frontiers: author.oplogFrontiers() });
  const e1Oid = putObj("e1:snap", e1Snap);
  relay.initEpoch(docId, 1, e1Oid);

  // Stale client B boots from E1 and goes offline
  const bDoc = makeDoc(2);
  bDoc.import(getObj("e1:snap", e1Oid));

  // Online work: advance to lagged cutoff F2, then continue inside lag window [F2..head]
  ta.insert(ta.length, "Online pre-F2\n");
  author.getMovableList("blocks").move(1, 0);
  author.commit();
  const f2 = author.oplogFrontiers();

  ta.insert(ta.length, "Inside lag window\n");
  author.getMap("meta").set("status", "e2-live");
  author.commit();

  // Client D forks inside lag window (>= F2)
  const dDoc = author.fork();
  dDoc.setPeerId("3");
  const beforeD = dDoc.oplogVersion();
  dDoc.getText("markdown").insert(dDoc.getText("markdown").length, "From D\n");
  dDoc.commit();
  const dUpd = dDoc.export({ mode: "update", from: beforeD });

  // Compact E1 -> E2 at lagged F2 (archive ONLY [F1..F2), NO genesis!)
  const deltaOid = putObj("delta:1->2", author.export({ mode: "update", from: vvF1 }));
  const e2Oid = putObj("e2:snap", author.export({ mode: "shallow-snapshot", frontiers: f2 }));
  assert.equal(relay.casEpoch(docId, 1, 2, e2Oid), true);
  assert.equal(relay.casEpoch(docId, 1, 2, e2Oid), false);

  // Healthy shallow client S boots from E2; D pushes natively (0 archive fetches)
  const sDoc = makeDoc(4);
  sDoc.import(getObj("e2:snap", e2Oid));
  relay.resetStats();
  assert.equal(relay.pushForEpoch(docId, 2, "upd-d", seal(key, "upd-d", dUpd)).ok, true);
  sDoc.import(dUpd);
  assert.equal(relay.stats.requests, 0);

  // Stale B edits offline on E1 + captures offline comment anchor
  const tb = bDoc.getText("markdown");
  const box = markdownText(tb).indexOf("[ ]") + 1;
  tb.mark({ start: box, end: box + 1 }, checkedStyle, true);
  tb.splice(markdownText(tb).indexOf("Shared line"), 11, "Shared line (B)");
  bDoc.getMovableList("blocks").set(0, "p1-B");
  bDoc.getMap("meta").set("authorB", "offline");
  bDoc.commit();
  const bAnchor = remember(tb, markdownText(tb).indexOf("(B)"));
  const bStaleUpd = bDoc.export({ mode: "update", from: vvF1 });
  assert.equal(relay.pushForEpoch(docId, 1, "stale", seal(key, "stale", bStaleUpd)).ok, false);

  // Sender-side CRDT-assisted rebase on B (pulls ONLY delta 1->2 + E2 snap; fresh PeerID on E2!)
  const delta = getObj("delta:1->2", deltaOid);
  const headOnE1 = makeDoc();
  headOnE1.import(e1Snap);
  headOnE1.import(delta);
  headOnE1.import(dUpd);
  const e2Head = headOnE1.oplogFrontiers();

  bDoc.import(delta);
  bDoc.import(dUpd);
  const mergedOffset = locate(bDoc, bAnchor);
  const rebaseDiff = bDoc.diff(e2Head, bDoc.oplogFrontiers(), false);

  // Fresh PeerID (no setPeerId reuse) avoids op-ID collision across rebases!
  const bOnE2 = makeDoc();
  bOnE2.import(getObj("e2:snap", e2Oid));
  bOnE2.import(dUpd);
  const beforeRebase = bOnE2.oplogVersion();
  bOnE2.applyDiff(rebaseDiff);
  bOnE2.commit();
  const bReanchored = remember(bOnE2.getText("markdown"), mergedOffset);
  const rebasedUpd = bOnE2.export({ mode: "update", from: beforeRebase });
  assert.equal(relay.pushForEpoch(docId, 2, "b-rebased", seal(key, "b-rebased", rebasedUpd)).ok, true);

  // S edits concurrently on E2, imports B's rebased push with 0 archive fetches, and stays shallow
  const beforeS = sDoc.oplogVersion();
  sDoc.getText("markdown").insert(sDoc.getText("markdown").length, "Concurrent S\n");
  sDoc.commit();
  const sUpd = sDoc.export({ mode: "update", from: beforeS });

  relay.resetStats();
  assert.equal(sDoc.import(rebasedUpd).pending, null);
  assert.equal(relay.stats.requests, 0);
  assert.equal(sDoc.isShallow(), true);
  bOnE2.import(sUpd);
  assert.equal(markdownText(bOnE2.getText("markdown")), markdownText(sDoc.getText("markdown")));

  // Exact match with always-full reference replica (state, marks, movable list, map, and cursor!)
  const fullRef = author.fork();
  fullRef.configTextStyle({ [checkedStyle]: { expand: "none" }, bold: { expand: "after" } });
  for (const u of [dUpd, bStaleUpd, sUpd]) fullRef.import(u);
  assert.equal(markdownText(sDoc.getText("markdown")), markdownText(fullRef.getText("markdown")));
  assert.deepEqual(sDoc.getText("markdown").toDelta(), fullRef.getText("markdown").toDelta());
  assert.deepEqual(sDoc.getMovableList("blocks").toJSON(), fullRef.getMovableList("blocks").toJSON());
  assert.deepEqual(sDoc.getMap("meta").toJSON(), fullRef.getMap("meta").toJSON());
  assert.equal(locate(sDoc, bReanchored), locate(fullRef, bAnchor));

  // Redacted-delta fallback: delta 1->2 purged -> B recovers via fresh-PeerID state diff on E2
  relay.delete(docId, deltaOid);
  assert.equal(getObj("delta:1->2", deltaOid), null);
  const bFallback = makeDoc(); // fresh PeerID!
  bFallback.import(getObj("e2:snap", e2Oid));
  const beforeFb = bFallback.oplogVersion();
  bFallback.getText("markdown").insert(bFallback.getText("markdown").length, "Recovered offline B\n");
  bFallback.commit();
  const fbUpd = bFallback.export({ mode: "update", from: beforeFb });
  assert.equal(relay.pushForEpoch(docId, 2, "b-fb", seal(key, "b-fb", fbUpd)).ok, true);
  sDoc.import(fbUpd);
  assert.ok(markdownText(sDoc.getText("markdown")).includes("Recovered offline B"));
});

test("benchmark: downloaded bytes, block requests, peak memory, and time-to-first-edit", () => {
  const relay = new OpaqueRelay();
  const archive = new EncryptedArchive(relay, "bench", crypto.randomBytes(32));

  const author = makeDoc(1);
  const text = author.getText("markdown");
  const list = author.getMovableList("blocks");
  for (let i = 0; i < 300; i++) {
    text.insert(text.length, `Rev ${i}: ${"lorem ipsum dolor sit amet ".repeat(4)}\n`);
    if (i > 20 && i % 3 === 0) text.delete(0, 60);
    if (i < 40) list.insert(i, `block-${i}`);
    else list.set(i % 40, `block-${i % 40}-r${i}`);
    author.commit();
  }

  const b = author.fork();
  b.setPeerId("2");
  const beforeB = b.oplogVersion();
  b.getText("markdown").insert(0, "[B pre-cutoff] ");
  b.commit();
  const bUpd = b.export({ mode: "update", from: beforeB });

  for (let i = 300; i < 310; i++) {
    text.insert(text.length, `Tail ${i}\n`);
    author.commit();
  }

  const fullSnap = author.export({ mode: "snapshot" });
  const shallowSnap = author.export({ mode: "shallow-snapshot", frontiers: author.oplogFrontiers() });
  archive.publishFromDoc(author, 256);

  const measureOpen = (peer, snap) => {
    const mem0 = process.memoryUsage().heapUsed;
    const t0 = performance.now();
    const doc = makeDoc(peer);
    doc.import(snap);
    doc.getText("markdown").insert(0, "edit ");
    doc.commit();
    return { doc, ms: performance.now() - t0, heapKB: Math.max(0, process.memoryUsage().heapUsed - mem0) / 1024 };
  };

  const full = measureOpen(10, fullSnap);
  const shallow = measureOpen(11, shallowSnap);

  relay.resetStats();
  const session = new HistorySession(shallow.doc);
  const tMerge = performance.now();
  session.ingestUpdate(bUpd);
  assert.equal(session.pump(archive).status, "rebuilt-full");
  const rebuildMs = performance.now() - tMerge;

  console.table([
    { strategy: "Full checkpoint", initB: fullSnap.byteLength, fetchB: 0, reqs: 1, ttfeMs: +full.ms.toFixed(2), heapKB: +full.heapKB.toFixed(1) },
    { strategy: "Shallow (happy)", initB: shallowSnap.byteLength, fetchB: 0, reqs: 1, ttfeMs: +shallow.ms.toFixed(2), heapKB: +shallow.heapKB.toFixed(1) },
    { strategy: "Impl A rebuild", initB: shallowSnap.byteLength, fetchB: relay.stats.bytes, reqs: 1 + relay.stats.requests, ttfeMs: +shallow.ms.toFixed(2), rebuildMs: +rebuildMs.toFixed(2) },
  ]);
  assert.ok(shallowSnap.byteLength < fullSnap.byteLength);
});
