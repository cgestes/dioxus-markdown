import { storage, encryptedChannel } from "./private-storage.js";
import { EphemeralStore, LoroDoc } from "loro-crdt/base64";
import { examples } from "./notes.mjs";
import { checkedStyle, markdownText } from "./loro-editor.js";
import { SyncSession } from "./sync-archive.js";
import { stageFile, exportFile } from "./vault-files.js";
import { browserReplica, sharedRoom } from "./vault-sync.js";

const directory = "@files";
function seeded(id, privateMode = false) {
  const base = new LoroDoc();
  base.setPeerId("141889");
  if (id === directory) {
    const map = base.getMap("files");
    if (privateMode) map.set("inbox", "Private note");
    else for (const n of examples) map.set(n.id, n.name);
  } else {
    const sample = !privateMode && examples.find(n => n.id === id);
    if (sample) base.getText("markdown").insert(0, sample.markdown);
  }
  base.commit({ timestamp: 0 });
  const doc = base.fork();
  base.free();
  doc.configTextStyle({ [checkedStyle]: { expand: "none" } });
  return doc;
}

async function previousNotebook(updates) {
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

export async function openNotebook(report) {
  const params = new URLSearchParams(location.search);
  const name = location.pathname + "/" + (params.get("room") || "fieldwork");
  const privateMode = params.has("private");
  const seed = id => seeded(id, privateMode);
  let disk;
  try { disk = await storage(name, report, privateMode, previousNotebook, seed); }
  catch (error) {
    if (privateMode) throw error;
    report("Session only · storage unavailable");
  }
  let channel;
  if (disk && typeof BroadcastChannel !== "undefined") {
    const scope = privateMode ? "margin-private/" : "margin-sample/";
    try { channel = encryptedChannel(scope + name, disk.cipher, () => report("Could not authenticate tab update")); }
    catch { /* Storage can still be used in this tab. */ }
  }
  const sessions = new Map();
  let replica;
  const uploads = new Map();
  function attachLocalUpdates(id, doc) {
    doc.subscribeLocalUpdates(bytes => {
      const outbound = privateMode && sharedRoom(id);
      const job = uploads.get(id);
      uploads.delete(id);
      const saved = disk?.save(id, bytes, outbound ? raw => replica.delta(id, raw) : null, job);
      if (outbound) saved.then(replica.wake);
      channel?.postMessage({ kind: "update", id, update: bytes });
    });
  }
  function getSession(id) {
    if (sessions.has(id)) return sessions.get(id);
    const session = new SyncSession(seed(id));
    sessions.set(id, session);
    attachLocalUpdates(id, session.doc);
    return session;
  }
  const getDoc = id => getSession(id).doc;
  for (const { id, bytes } of disk?.rows ?? []) getSession(id).ingest(bytes);
  const clientID = getDoc(directory).peerIdStr, files = new Map();
  const fileMap = () => getDoc(directory).getMap("files");
  function requestDemand(id, demand) {
    if (demand.kind === "missing-spans") {
      channel?.postMessage({ kind: "need-spans", id, spans: demand.spans, from: clientID });
    } else if (demand.kind === "needs-checkpoint") {
      channel?.postMessage({ kind: "need-checkpoint", id, from: clientID });
    }
  }
  const listeners = new Set();
  function fileList() {
    files.clear();
    for (const [id, value] of fileMap().entries()) files.set(id, value);
    if (privateMode) for (const [id, {doc}] of sessions) {
      if (!sharedRoom(id)) continue;
      const title = markdownText(doc.getText("markdown")).split("\n", 1)[0].replace(/^#+\s*/, "").trim();
      files.set(id, title || "Untitled");
    }
    for (const listener of listeners) listener();
  }
  fileMap().subscribe(() => queueMicrotask(fileList));
  fileList();
  if (privateMode) {
    replica = await browserReplica(disk, (id, raw) => {
      requestDemand(id, getSession(id).ingest(raw).demand);
      fileList();
      channel?.postMessage({ kind: "remote", id, update: raw });
    }, message => { document.getElementById("relay-status").textContent = message; });
  }
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
      for (const [id, {doc}] of sessions) channel.postMessage({ kind: "snapshot", id, update: doc.export({ mode: "snapshot" }), to });
    }
    channel.onmessage = ({ data }) => {
      if (data.to && data.to !== clientID) return;
      if (data.kind === "hello") {
        sendSnapshots(data.from);
        channel.postMessage({ kind: "reply", to: data.from, from: clientID });
        publish();
      } else if (data.kind === "reply") sendSnapshots(data.from);
      else if (data.kind === "presence") presence.apply(data.bytes);
      else if (data.kind === "need-spans") {
        const bytes = sessions.get(data.id)?.exportSpans(data.spans);
        if (bytes) channel.postMessage({ kind: "update", id: data.id, update: bytes, to: data.from });
      } else if (data.kind === "need-checkpoint") {
        const session = sessions.get(data.id);
        if (session && !session.doc.isShallow()) channel.postMessage({
          kind: "checkpoint", id: data.id, update: session.doc.export({ mode: "snapshot" }), to: data.from
        });
      } else if (data.kind === "checkpoint") {
        const session = sessions.get(data.id);
        if (session?.nextDemand().kind !== "needs-checkpoint") return;
        const demand = session.rebuildFromCheckpoint(data.update);
        attachLocalUpdates(data.id, session.doc);
        if (data.id === directory) fileMap().subscribe(() => queueMicrotask(fileList));
        disk?.save(data.id, session.doc.export({ mode: "snapshot" }));
        fileList();
        requestDemand(data.id, demand);
      }
      else if (data.kind === "remote" && privateMode && sharedRoom(data.id)) {
        requestDemand(data.id, getSession(data.id).ingest(data.update).demand);
        fileList();
      } else if (data.kind === "update" || data.kind === "snapshot") {
        const { changed, demand } = getSession(data.id).ingest(data.update);
        if (changed) {
          disk?.save(data.id, data.update);
          if (privateMode && sharedRoom(data.id)) fileList();
        }
        requestDemand(data.id, demand);
      }
    };
    const hello = () => {
      channel.postMessage({ kind: "hello", from: clientID });
      for (const [id, session] of sessions) requestDemand(id, session.nextDemand());
    };
    hello(); publish();
    let heartbeat = setInterval(publish, 10000);
    window.addEventListener("pagehide", () => { clearInterval(heartbeat); presence.delete(clientID); });
    window.addEventListener("pageshow", event => { if (event.persisted) { hello(); publish(); heartbeat = setInterval(publish, 10000); } });
  } else publish();
  return {
    privateMode, replica, clientID, files, awareness, flush: () => disk?.flush(),
    async attach(room, file) {
      const { id, manifest } = await stageFile(disk.db, file);
      const doc = getDoc(room);
      doc.commit();
      if (sharedRoom(room)) uploads.set(room, { upload: id, next: 0, last: manifest.parts });
      doc.getMap("attachments").set(id, JSON.stringify(manifest));
      doc.commit();
      await disk.flush();
    },
    attachments(room) {
      return [...getDoc(room).getMap("attachments").entries()].map(([id, raw]) => [id, JSON.parse(raw)]);
    },
    extract: (id, manifest, handle) => exportFile(disk.db, id, manifest, handle),
    async publish(markdown) {
      if (!replica?.paired()) throw new Error("Pair this notebook with a vault first.");
      const id = crypto.randomUUID().replaceAll("-", ""), doc = getDoc(id);
      doc.getMap("margin").set("format", 1);
      doc.getText("markdown").insert(0, markdown);
      doc.commit();
      await disk.flush();
      fileList();
      return id;
    },
    canShare: Boolean(channel), getDoc, getSession,
    onFiles(fn) { listeners.add(fn); },
    create(id, name) {
      const doc = getDoc(id); doc.getText("markdown").insert(0, "# " + name + "\n\n"); doc.commit();
      fileMap().set(id, name); getDoc(directory).commit(); files.set(id, name);
    }
  };
}
