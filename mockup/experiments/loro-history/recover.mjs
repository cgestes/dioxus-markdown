// Uses public Loro APIs only. The caller authenticates archive reads and journals incoming
// envelopes before calling this function. The live document stays editable during I/O.
import { Cursor, decodeImportBlobMeta, LoroDoc } from "loro-crdt";

export class HistoryUnavailable extends Error {}

function include(vector, peer, end) {
  if (end > (vector.get(peer) || 0)) vector.set(peer, end);
}

function ingest(doc, raw, goal) {
  if (!raw) return;
  const meta = decodeImportBlobMeta(raw, true);
  for (const [peer, end] of meta.partialEndVersionVector.toJSON()) include(goal, peer, end);
  try {
    doc.import(raw);
  } catch (error) {
    // The JS binding to 1.18.0 exposes this particular Rust error as a string.
    if (doc.isShallow() && String(error).includes("not included in the shallow history")) {
      throw new HistoryUnavailable("Edit depends on history before this checkpoint");
    }
    throw error;
  }
}

export function resolveAnchor(doc, bytes) {
  const cursor = Cursor.decode(bytes);
  try {
    const position = doc.getCursorPos(cursor);
    position?.update?.free();
    return position && { offset: position.offset, side: position.side };
  } finally {
    cursor.free();
  }
}

async function fill(doc, goal, archive) {
  const tried = new Set();
  for (;;) {
    const known = doc.oplogVersion().toJSON();
    const pending = doc.exportPendingUpdates();
    if (!pending && [...goal].every(([p, end]) => (known.get(p) || 0) >= end)) return;
    const needed = new Map();
    if (pending) {
      // Re-read after each import. A multi-writer blob can contain dependencies of
      // its own, and a disjoint export from one writer can hide an interior hole.
      const meta = decodeImportBlobMeta(pending, true);
      for (const { peer, counter } of meta.startFrontiers) include(needed, peer, counter + 1);
      for (const [peer, start] of meta.partialStartVersionVector.toJSON()) include(needed, peer, start);
    }
    for (const [peer, end] of goal) include(needed, peer, end);
    let received = false;
    for (const [peer, end] of needed) {
      const start = known.get(peer) || 0;
      const id = peer + "/" + start;
      if (start >= end || tried.has(id)) continue;
      tried.add(id);
      const raw = await archive.range(peer, start, end);
      if (!raw) continue;
      ingest(doc, raw, goal);
      received = true;
      break;
    }
    if (!received) throw new HistoryUnavailable("Required operation range is unavailable");
  }
}

export async function recoverHistory({
  source,
  updates,
  anchors = [],
  archive,
  checkpoints = [],
  useCurrent = true,
}) {
  // Loro snapshots omit pending and rejected changes. Capture pending separately.
  // The caller retains the original authenticated input even if import rejected it.
  const start = source.oplogVersion();
  const pending = source.exportPendingUpdates();
  const root = useCurrent && source.export({ mode: "snapshot" });
  const options = useCurrent ? [null, ...checkpoints] : checkpoints;
  const reasons = [];
  for (const ref of options) {
    const snapshot = ref ? await archive.checkpoint(ref) : root;
    if (!snapshot) {
      reasons.push("Checkpoint unavailable");
      continue;
    }
    const candidate = LoroDoc.fromSnapshot(snapshot);
    let keep = false;
    try {
      const goal = new Map(start.toJSON());
      if (ref) ingest(candidate, source.export({ mode: "update", from: candidate.oplogVersion() }), goal);
      ingest(candidate, pending, goal);
      for (const raw of updates) ingest(candidate, raw, goal);
      for (;;) {
        await fill(candidate, goal, archive);
        // Incorporate edits made in the live replica while an archive read awaited I/O.
        // The final coverage/anchor check and return run without yielding.
        const local = source.oplogVersion().toJSON();
        const applied = candidate.oplogVersion().toJSON();
        if ([...local].some(([p, end]) => (applied.get(p) || 0) < end)) {
          for (const [p, end] of local) include(goal, p, end);
          ingest(candidate, source.export({ mode: "update", from: candidate.oplogVersion() }), goal);
          continue;
        }
        const inflight = source.exportPendingUpdates();
        if (inflight) {
          ingest(candidate, inflight, goal);
          if (candidate.exportPendingUpdates()) continue;
        }
        if (anchors.some((anchor) => !resolveAnchor(candidate, anchor))) {
          throw new HistoryUnavailable("Checkpoint cannot resolve a required cursor");
        }
        keep = true;
        return { doc: candidate, from: ref?.label ?? "current", reasons };
      }
    } catch (error) {
      if (!(error instanceof HistoryUnavailable)) throw error;
      reasons.push(error.message);
    } finally {
      if (!keep) candidate.free();
    }
  }
  throw new HistoryUnavailable(reasons.join(". ") || "No checkpoint available");
}
