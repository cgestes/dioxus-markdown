import { createHash, randomBytes } from "node:crypto";
import { decodeImportBlobMeta, LoroDoc } from "loro-crdt";
import { encryptDeltaSpan, encryptSnapshot } from "loro-protocol";
import { documentKey, hash } from "./archive.mjs";
import { resolveAnchor } from "./recover.mjs";

const utf8 = new TextEncoder();
const encode = (data) => Buffer.from(data).toString("base64");
export const decode = (value) => Buffer.from(value, "base64");

function historyText(size) {
  const chunks = [];
  for (let i = 0; i * 43 < size; i++) chunks.push(createHash("sha256").update(String(i)).digest("base64url"));
  return chunks.join("").slice(0, size);
}
function fork(doc, peer) {
  const branch = doc.fork();
  branch.setPeerId(peer);
  branch.configTextStyle({ bold: { expand: "none" } });
  return branch;
}
function change(doc, edit) {
  const before = doc.oplogVersion();
  edit(doc.getText("markdown"), doc.getMovableList("tasks"));
  doc.commit({ timestamp: 0 });
  return doc.export({ mode: "update", from: before });
}
function anchor(text, index) {
  const cursor = text.getCursor(index, 0);
  try {
    return encode(cursor.encode());
  } finally {
    cursor.free();
  }
}
function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((k) => [k, canonical(value[k])]),
    );
  }
  return value;
}
export const fingerprint = (value) => hash(JSON.stringify(canonical(value)));
export function inspect(doc, anchors) {
  const text = doc.getText("markdown");
  const changes = ["41", "91"].map((peer) => {
    const change = doc.getChangeAt({ peer, counter: 0 });
    return { peer: change.peer, counter: change.counter, length: change.length, deps: change.deps };
  });
  return {
    state: fingerprint(doc.toJSON()),
    markdown: text.toString(),
    richText: text.toDelta(),
    tasks: doc.getMovableList("tasks").toJSON(),
    vector: Object.fromEntries(doc.oplogVersion().toJSON()),
    frontiers: doc.oplogFrontiers().sort((a, b) => a.peer.localeCompare(b.peer)),
    anchors: anchors.map((bytes) => resolveAnchor(doc, decode(bytes))),
    changes,
  };
}

export async function makeFixture({
  size = 512 * 1024,
  visible = false,
  beforeRoot = false,
  beforeEarlier = false,
  oldAnchor = false,
} = {}) {
  const keyBytes = randomBytes(32),
    key = await documentKey(keyBytes);
  const objects = new Map(),
    spanIds = [];
  const a = new LoroDoc();
  a.setPeerId("11");
  a.configTextStyle({ bold: { expand: "none" } });
  const text = a.getText("markdown");
  text.insert(0, "Field diary 📓 · old draft · Meet at the east bridge after lunch.");
  const tasks = a.getMovableList("tasks");
  for (const task of ["north", "east", "south"]) tasks.push(task);
  a.commit({ timestamp: 0 });
  const oldestBranch = fork(a, "19");
  const history = a.getText("past-work");
  history.insert(0, historyText(size));
  a.commit({ timestamp: 0 });
  if (!visible && size) {
    history.delete(0, size);
    a.commit({ timestamp: 0 });
  }

  async function seal(raw) {
    const meta = decodeImportBlobMeta(raw, true);
    const writers = meta.partialEndVersionVector.toJSON();
    if (writers.size !== 1) throw new Error("Each archived delta span must contain exactly one writer");
    const [[peer, end]] = writers;
    const start = meta.partialStartVersionVector.toJSON().get(peer) || 0;
    return (await encryptDeltaSpan(raw, { peerId: utf8.encode(peer), start, end, keyId: "1" }, key)).record;
  }
  async function retain(raw) {
    const record = await seal(raw),
      id = hash(record);
    objects.set(id, record);
    spanIds.push(id);
  }
  async function checkpoint(doc, label, shallow) {
    const raw = doc.export(
      shallow ? { mode: "shallow-snapshot", frontiers: doc.oplogFrontiers() } : { mode: "snapshot" },
    );
    const vv = [...doc.oplogVersion().toJSON()].map(([peer, counter]) => ({
      peerId: utf8.encode(peer),
      counter,
    }));
    const { record } = await encryptSnapshot(raw, { vv, keyId: "1" }, key);
    const id = hash(record);
    objects.set(id, record);
    return { label, shallow, id };
  }

  const anchors = [
    anchor(text, text.toString().indexOf("east")),
    anchor(text, text.toString().indexOf("bridge") + 5),
  ];
  const historical = anchor(text, text.toString().indexOf("draft") + 2);
  if (oldAnchor) anchors.push(historical);
  const earlier = await checkpoint(a, "earlier shallow", true);
  const earlyBranch = fork(a, "20");
  await retain(change(a, (t) => t.delete(t.toString().indexOf("old draft"), "old draft · ".length)));
  await retain(change(a, (t) => t.insert(0, "Tuesday · ")));
  const current = await checkpoint(a, "current", true);
  const initialFull = await checkpoint(a, "initial full", false);
  const branchBase = beforeEarlier ? oldestBranch : beforeRoot ? earlyBranch : a;
  const left = fork(branchBase, "21"),
    right = fork(branchBase, "22");
  const p = change(left, (t) => t.insert(t.length, " Bring a pen."));
  const q = change(right, (t, list) => {
    list.move(2, 0);
    const bridge = t.toString().indexOf("bridge");
    t.mark({ start: bridge, end: bridge + 6 }, "bold", true);
    t.delete(t.toString().indexOf("east"), 1);
  });
  await retain(p);
  await retain(q);
  const join = fork(left, "31");
  join.import(q);
  const merged = change(join, (t) => t.insert(t.toString().indexOf("bridge") + 6, " ⚑"));
  await retain(merged);
  const offline = fork(join, "41");
  const incoming = change(offline, (t) => t.insert(t.length, "\nMeet by the water."));
  const device = fork(a, "91");
  const local = change(device, (t) => t.insert(0, "Device copy · "));

  a.importBatch([p, q, merged]);
  const full = await checkpoint(a, "full", false);
  a.importBatch([local, incoming]);
  const expected = inspect(a, anchors);
  for (const doc of [a, oldestBranch, earlyBranch, left, right, join, offline, device]) doc.free();
  return {
    objects,
    spanIds,
    expected,
    device: {
      key: encode(keyBytes),
      earlier,
      current,
      initialFull,
      full,
      anchors,
      local: encode(await seal(local)),
      incoming: encode(await seal(incoming)),
    },
  };
}
