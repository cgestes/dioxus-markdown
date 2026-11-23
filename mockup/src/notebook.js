import { EphemeralStore, LoroDoc } from "loro-crdt/base64";
import { examples } from "./notes.mjs";
import { checkedStyle } from "./loro-editor.js";

const directory = "@files";
const request = req => new Promise((resolve, reject) => {
  req.onsuccess = () => resolve(req.result); req.onerror = () => reject(req.error);
});
function seeded(id) {
  const base = new LoroDoc();
  base.setPeerId("141889");
  if (id === directory) {
    const map = base.getMap("files");
    for (const n of examples) map.set(n.id, n.name);
  } else {
    const sample = examples.find(n => n.id === id);
    if (sample) base.getText("markdown").insert(0, sample.markdown);
  }
  base.commit({ timestamp: 0 });
  const doc = base.fork();
  base.free();
  doc.configTextStyle({ [checkedStyle]: { expand: "none" } });
  return doc;
}

async function previousNotebook(name) {
  // The previous study stored Yjs updates. Migrate a saved notebook once,
  // reading its old database without altering it.
  const open = indexedDB.open("margin-md.1/" + name);
  open.onupgradeneeded = () => open.transaction.abort();
  let db;
  try { db = await request(open); } catch { return []; }
  let updates;
  try { updates = await request(db.transaction("updates").objectStore("updates").getAll()); }
  finally { db.close(); }
  if (!updates.length) return [];
  const Y = await import("yjs");
  const old = new Y.Doc();
  Y.applyUpdate(old, Uint8Array.from(atob(MARGIN_LEGACY_SEED), c => c.charCodeAt(0)));
  Y.applyUpdate(old, Y.mergeUpdates(updates));
  const meta = seeded(directory), map = meta.getMap("files"), rows = [];
  for (const [id, name] of old.getMap("files")) {
    map.set(id, name);
    const doc = seeded(id);
    doc.getText("markdown").update(old.getText(id).toString());
    rows.push({ id, bytes: doc.export({ mode: "snapshot" }) });
    doc.free();
  }
  rows.push({ id: directory, bytes: meta.export({ mode: "snapshot" }) });
  meta.free(); old.destroy();
  return rows;
}

async function storage(name, report) {
  const open = indexedDB.open("margin-loro.1/" + name, 1);
  open.onupgradeneeded = () => {
    open.result.createObjectStore("updates", { autoIncrement: true }).createIndex("note", "id");
    open.result.createObjectStore("settings");
  };
  const db = await request(open);
  if (!await request(db.transaction("settings").objectStore("settings").get("initialized"))) {
    const legacy = await previousNotebook(name);
    await new Promise((resolve, reject) => {
      const tx = db.transaction(["updates", "settings"], "readwrite");
      const settings = tx.objectStore("settings"), check = settings.get("initialized");
      check.onsuccess = () => {
        if (check.result) return;
        for (const row of legacy) tx.objectStore("updates").add(row);
        settings.put(true, "initialized");
      };
      tx.oncomplete = resolve; tx.onabort = () => reject(tx.error);
    });
  }
  const rows = await request(db.transaction("updates").objectStore("updates").getAll());
  let pending = 0, failed = false;
  const counts = new Map();
  const count = id => counts.set(id, (counts.get(id) || 0) + 1).get(id);
  for (const { id } of rows) count(id);
  function save(id, bytes) {
    if (failed) return;
    try {
      const tx = db.transaction("updates", "readwrite"), store = tx.objectStore("updates");
      store.add({ id, bytes });
      if (count(id) > 150) {
        counts.set(id, 0);
        const index = store.index("note"), keys = index.getAllKeys(id), all = index.getAll(id);
        all.onsuccess = () => {
          const merged = seeded(id);
          const status = merged.importBatch(all.result.map(r => r.bytes));
          if (!status.pending?.size) {
            for (const key of keys.result) store.delete(key);
            store.add({ id, bytes: merged.export({ mode: "snapshot" }) });
          }
          merged.free();
        };
      }
      pending++; report("Saving…");
      tx.oncomplete = () => { if (!--pending && !failed) report("Saved in this browser"); };
      tx.onabort = () => { failed = true; report("Session only · use Export .md"); };
    } catch { failed = true; report("Session only · use Export .md"); }
  }
  report("Saved in this browser");
  return { rows, save };
}

export async function openNotebook(report) {
  const params = new URLSearchParams(location.search);
  const name = location.pathname + "/" + (params.get("room") || "fieldwork");
  let disk;
  try { disk = await storage(name, report); }
  catch { report("Session only · use Export .md"); }
  let channel;
  try { channel = new BroadcastChannel("margin-loro.1/" + name); } catch { /* File preview */ }
  const docs = new Map();
  function getDoc(id) {
    if (docs.has(id)) return docs.get(id);
    const doc = seeded(id);
    docs.set(id, doc);
    doc.subscribeLocalUpdates(bytes => {
      disk?.save(id, bytes);
      channel?.postMessage({ kind: "update", id, update: bytes });
    });
    return doc;
  }
  for (const { id, bytes } of disk?.rows ?? []) getDoc(id).import(bytes);
  const meta = getDoc(directory), map = meta.getMap("files"), files = new Map(map.entries());
  const clientID = meta.peerIdStr;
  const listeners = new Set();
  map.subscribe(() => queueMicrotask(() => {
    files.clear(); for (const [id, value] of map.entries()) files.set(id, value);
    for (const listener of listeners) listener();
  }));
  const presence = new EphemeralStore(30000);
  const observers = new Set();
  presence.subscribe(() => { for (const fn of observers) fn(); });
  presence.subscribeLocalUpdates(bytes => channel?.postMessage({ kind: "presence", bytes }));
  let local = { user: params.get("editor") === "b"
    ? { name: "Editor B", color: "#47706b", colorLight: "#b6d4ca77" }
    : { name: "Editor A", color: "#a74729", colorLight: "#dfb89977" } };
  const publish = () => presence.set(clientID, local);
  const awareness = {
    getStates: () => new Map(Object.entries(presence.getAllStates())),
    getLocalState: () => local,
    setLocalStateField(key, value) { local = { ...local, [key]: value }; publish(); },
    on(_kind, fn) { observers.add(fn); }, off(_kind, fn) { observers.delete(fn); }
  };
  if (channel) {
    function sendSnapshots(to) {
      for (const [id, doc] of docs) channel.postMessage({ kind: "snapshot", id, update: doc.export({ mode: "snapshot" }), to });
    }
    channel.onmessage = ({ data }) => {
      if (data.to && data.to !== clientID) return;
      if (data.kind === "hello") {
        sendSnapshots(data.from);
        channel.postMessage({ kind: "reply", to: data.from, from: clientID });
        publish();
      } else if (data.kind === "reply") sendSnapshots(data.from);
      else if (data.kind === "presence") presence.apply(data.bytes);
      else if (data.kind === "update" || data.kind === "snapshot") {
        const doc = getDoc(data.id);
        const initial = doc.opCount();
        const imported = doc.import(data.update);
        if (initial !== doc.opCount() || imported.pending?.size) disk?.save(data.id, data.update);
      }
    };
    const hello = () => channel.postMessage({ kind: "hello", from: clientID });
    hello(); publish();
    let heartbeat = setInterval(publish, 10000);
    window.addEventListener("pagehide", () => { clearInterval(heartbeat); presence.delete(clientID); });
    window.addEventListener("pageshow", event => { if (event.persisted) { hello(); publish(); heartbeat = setInterval(publish, 10000); } });
  } else publish();
  return {
    clientID, files, awareness, canShare: Boolean(channel), getDoc,
    onFiles(fn) { listeners.add(fn); },
    create(id, name) {
      const doc = getDoc(id); doc.getText("markdown").insert(0, "# " + name + "\n\n"); doc.commit();
      map.set(id, name); meta.commit(); files.set(id, name);
    }
  };
}
