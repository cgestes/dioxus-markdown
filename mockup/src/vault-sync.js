// Margin control frames carry standard %ELO records and signed immutable file chunks.
import { decodeImportBlobMeta, LoroDoc } from "loro-crdt/base64";
import {
  encryptDeltaSpan,
  encryptSnapshot,
  decryptEloRecord,
  EloRecordKind,
  MessageType,
  UpdateStatusCode,
} from "loro-protocol";
import {
  b64,
  bytes,
  decodeFrame,
  documentReader,
  sendPacket,
} from "./vault-stream.js";
import { request, complete } from "./private-storage.js";

const utf8 = new TextEncoder(),
  text = new TextDecoder();
const hex = (b) =>
  Array.from(b, (n) => n.toString(16).padStart(2, "0")).join("");
const unhex = (s) =>
  Uint8Array.from(s.match(/../g) || [], (x) => parseInt(x, 16));
export const sharedRoom = (s) => /^[0-9a-f]{32}$/.test(s);
const concat = (...parts) => {
  const a = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let off = 0;
  for (const p of parts) {
    a.set(p, off);
    off += p.length;
  }
  return a;
};
const sha = async (data) =>
  new Uint8Array(await crypto.subtle.digest("SHA-256", data));
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const upgradeRelay = "Update the relay to enable encrypted streaming";
function check(ok, msg) {
  if (!ok) throw new Error(msg);
}
function relayURL(s) {
  const u = new URL(s);
  check(
    !u.username &&
      !u.password &&
      !u.search &&
      !u.hash &&
      (u.protocol === "wss:" ||
        (u.protocol === "ws:" &&
          (u.hostname === "127.0.0.1" || u.hostname === "[::1]"))),
    "Use a secure relay URL.",
  );
  return u.href;
}
async function verify(key, sig, data) {
  check(/^[0-9a-f]{64}$/.test(key), "Invalid signing key");
  const publicKey = await crypto.subtle.importKey(
    "raw",
    unhex(key),
    "Ed25519",
    false,
    ["verify"],
  );
  check(
    await crypto.subtle.verify("Ed25519", publicKey, bytes(sig), data),
    "Invalid signature",
  );
}
function certify(vault, g) {
  return verify(
    vault,
    g.signature,
    utf8.encode(
      "Margin device v1/" +
        vault +
        "/" +
        g.device +
        "/" +
        g.token_hash +
        "/" +
        g.epoch,
    ),
  );
}
async function digest(room, part, data) {
  const index = new Uint8Array(8);
  new DataView(index.buffer).setBigInt64(0, BigInt(part));
  return sha(concat(utf8.encode("Margin record v1\0" + room), index, data));
}
async function credentials(kit) {
  const p = kit.secret;
  await certify(p.vault, p.grant);
  check(
    hex(await sha(Uint8Array.from(p.token))) === p.grant.token_hash,
    "Invalid relay credential",
  );
  const seed = concat(
    unhex("302e020100300506032b657004220420"),
    Uint8Array.from(p.device_seed),
  );
  const signer = await crypto.subtle.importKey(
    "pkcs8",
    seed,
    "Ed25519",
    false,
    ["sign"],
  );
  const challenge = crypto.getRandomValues(new Uint8Array(32));
  await verify(
    p.grant.device,
    b64(new Uint8Array(await crypto.subtle.sign("Ed25519", signer, challenge))),
    challenge,
  );
  const url = relayURL(kit.relay),
    epoch = p.grant.epoch;
  check(p.keys[epoch]?.length === 32, "Missing vault key");
  async function key(room, e) {
    check(p.keys[e]?.length === 32, "Missing vault key version");
    const master = await crypto.subtle.importKey(
      "raw",
      Uint8Array.from(p.keys[e]),
      "HKDF",
      false,
      ["deriveKey"],
    );
    return crypto.subtle.deriveKey(
      {
        name: "HKDF",
        hash: "SHA-256",
        salt: utf8.encode("Margin personal vault v1"),
        info: utf8.encode("document/" + room),
      },
      master,
      { name: "AES-GCM", length: 256 },
      false,
      ["encrypt", "decrypt"],
    );
  }
  async function packet(room, part, data) {
    const signature = await crypto.subtle.sign(
      "Ed25519",
      signer,
      await digest(room, part, data),
    );
    return {
      room,
      part,
      data: b64(data),
      grant: p.grant,
      signature: b64(new Uint8Array(signature)),
    };
  }
  async function delta(room, raw) {
    const meta = decodeImportBlobMeta(raw, true);
    const entries = [...meta.partialEndVersionVector.toJSON()];
    check(entries.length === 1, "Expected a local edit");
    const [[peer, end]] = entries,
      start = meta.partialStartVersionVector.toJSON().get(peer) || 0;
    const { record } = await encryptDeltaSpan(
      raw,
      { peerId: utf8.encode(peer), start, end, keyId: String(epoch) },
      await key(room, epoch),
    );
    return packet(room, -1, record);
  }
  async function checkpoint(room, doc) {
    if (doc.exportPendingUpdates()) return;
    const raw = doc.export({ mode: "snapshot" });
    const vv = [...doc.version().toJSON()].map(([id, counter]) => ({
      peerId: utf8.encode(id),
      counter,
    }));
    const { record } = await encryptSnapshot(
      raw,
      { vv, keyId: String(epoch) },
      await key(room, epoch),
    );
    const opened = await decryptEloRecord(record, (e) => key(room, e));
    const restored = LoroDoc.fromSnapshot(opened.plaintext);
    try {
      check(
        !restored.isShallow() &&
          restored.version().compare(doc.version()) === 0,
        "Checkpoint verification failed",
      );
    } finally {
      restored.free();
    }
    return packet(room, -1, record);
  }
  async function unpack(pkt) {
    check(
      sharedRoom(pkt.room) &&
        Number.isSafeInteger(pkt.part) &&
        pkt.part >= -1 &&
        pkt.data.length <= 23 * 1024 * 1024,
      "Invalid record",
    );
    const data = bytes(pkt.data);
    await certify(p.vault, pkt.grant);
    await verify(
      pkt.grant.device,
      pkt.signature,
      await digest(pkt.room, pkt.part, data),
    );
    if (pkt.part !== -1) return { data };
    const out = await decryptEloRecord(data, (e) => key(pkt.room, e)),
      h = out.header;
    const meta = decodeImportBlobMeta(out.plaintext, true);
    const end = meta.partialEndVersionVector.toJSON();
    if (out.kind === EloRecordKind.Snapshot) {
      const doc = LoroDoc.fromSnapshot(out.plaintext);
      try {
        const vv = doc.version().toJSON();
        check(
          !doc.isShallow() &&
            vv.size === h.vv.length &&
            h.vv.every((v) => vv.get(text.decode(v.peerId)) === v.counter),
          "Invalid checkpoint coverage",
        );
      } finally {
        doc.free();
      }
    } else {
      const peer = text.decode(h.peerId);
      check(
        end.size === 1 &&
          end.get(peer) === h.end &&
          (meta.partialStartVersionVector.toJSON().get(peer) || 0) === h.start,
        "Invalid edit coverage",
      );
    }
    return {
      raw: out.plaintext,
      snapshot: out.kind === EloRecordKind.Snapshot,
    };
  }
  return { url, p, delta, checkpoint, unpack, packet };
}

async function connection(url, receive, idle, wake) {
  const ws = new WebSocket(url);
  ws.binaryType = "arraybuffer";
  let waiting, incoming, fragmentTimer, failure;
  let applying = Promise.resolve();
  const disconnect = (error) => {
    if (failure) return;
    failure = error;
    clearTimeout(fragmentTimer);
    waiting?.reject(error);
    ws.close();
    wake();
  };
  const ensureOpen = () => {
    if (failure) throw failure;
  };
  ws.onclose = ws.onerror = () => disconnect(new Error("Relay disconnected"));

  function expectFragment() {
    clearTimeout(fragmentTimer);
    fragmentTimer = setTimeout(
      () => disconnect(new Error("Incomplete Loro update")),
      10000,
    );
  }
  async function handle(data) {
    ensureOpen();
    if (data === "ping") {
      ws.send("pong");
      return;
    }
    if (data === "pong") return;
    if (incoming) {
      check(typeof data !== "string", "Incomplete encrypted document");
      const record = incoming.read(decodeFrame(data));
      if (record) {
        clearTimeout(fragmentTimer);
        const { seq, packet } = incoming;
        incoming = undefined;
        await receive(seq, { ...packet, data: b64(record) });
      } else expectFragment();
      return;
    }
    const binary = typeof data !== "string";
    const reply = binary ? decodeFrame(data) : JSON.parse(data);
    check(
      binary || typeof reply.type === "string",
      "Invalid relay control frame",
    );
    if (!binary && reply.type === "event") {
      check(
        Number.isSafeInteger(reply.seq) && reply.seq > 0,
        "Invalid relay cursor",
      );
      if (reply.packet.part === -1) {
        check(
          !reply.packet.data && sharedRoom(reply.packet.room),
          "Invalid streamed document header",
        );
        incoming = { ...reply, read: documentReader(reply.packet.room) };
        expectFragment();
      } else await receive(reply.seq, reply.packet);
    } else if (!binary && reply.type === "window") {
      ws.send(JSON.stringify({ type: "continue" }));
    } else if (!binary && reply.type === "idle") {
      idle(reply.checkpoint);
    } else {
      check(
        waiting && waiting.accepts(reply),
        "Unexpected relay acknowledgement",
      );
      waiting.resolve(reply);
    }
  }
  ws.onmessage = ({ data }) => {
    // IndexedDB, signatures and decryption are asynchronous. Finish each event
    // before processing the next cursor, upload ACK or flow-control window.
    applying = applying.then(() => handle(data)).catch(disconnect);
  };
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      disconnect(new Error("Relay timed out"));
      reject(failure);
    }, 12000);
    ws.onopen = () => {
      clearTimeout(timer);
      resolve();
    };
    ws.addEventListener(
      "close",
      () => {
        clearTimeout(timer);
        reject(new Error("Relay offline"));
      },
      { once: true },
    );
  });
  async function requestReply(send, accepts) {
    ensureOpen();
    check(!waiting, "Upload already pending");
    let timer;
    try {
      return await new Promise((resolve, reject) => {
        waiting = { resolve, reject, accepts };
        send();
        let buffered = ws.bufferedAmount;
        timer = setInterval(() => {
          if (ws.bufferedAmount < buffered) buffered = ws.bufferedAmount;
          else disconnect(new Error("Relay timed out"));
        }, 30000);
      });
    } finally {
      clearInterval(timer);
      waiting = undefined;
    }
  }
  return {
    ensureOpen,
    close: () => disconnect(new Error("Relay closed")),
    login: (hello) =>
      requestReply(
        () => ws.send(JSON.stringify(hello)),
        (r) => r.type === "ready",
      ),
    subscribe: (after) => ws.send(JSON.stringify({ type: "subscribe", after })),
    push: (packet) => {
      let batch;
      return requestReply(
        () => {
          batch = sendPacket(ws, packet);
        },
        (r) =>
          batch === null
            ? r.type === "ack"
            : r.type === MessageType.Ack &&
              r.roomId === packet.room &&
              r.refId === batch &&
              r.status === UpdateStatusCode.Ok,
      );
    },
  };
}

export async function browserReplica(disk, onDocument, status) {
  const db = disk.db,
    cipher = disk.cipher;
  const setting = (k) =>
    request(db.transaction("settings").objectStore("settings").get(k));
  let auth,
    notify,
    dirty = false,
    stopped = false;
  const wake = () => {
    dirty = true;
    notify?.();
  };
  const metaKey = "paired device";
  const stored = await setting(metaKey);
  if (stored)
    auth = await credentials(
      JSON.parse(text.decode(await cipher.open(metaKey, stored))),
    );
  async function receive(seq, pkt) {
    const unpacked = await auth.unpack(pkt);
    const row = unpacked.raw && (await disk.sealedRow(pkt.room, unpacked.raw));
    const tx = db.transaction(["settings", "sealed", "blobs"], "readwrite"),
      done = complete(tx);
    const settings = tx.objectStore("settings"),
      read = settings.get("relay-cursor");
    read.onsuccess = () => {
      if ((read.result || 0) >= seq) return;
      if (row) tx.objectStore("sealed").add(row);
      else tx.objectStore("blobs").put(unpacked.data, [pkt.room, pkt.part]);
      settings.put(seq, "relay-cursor");
    };
    await done;
    if (row) onDocument(pkt.room, unpacked.raw);
  }
  async function snap(room) {
    check(sharedRoom(room), "Checkpoints require a shared document");
    const records = await request(
      db.transaction("sealed").objectStore("sealed").index("note").getAll(room),
    );
    if (!records.length) return;
    const doc = new LoroDoc();
    try {
      doc.importBatch(
        await Promise.all(
          records.map(async (r) => (await disk.readRow(r)).bytes),
        ),
      );
      return await auth.checkpoint(room, doc);
    } finally {
      doc.free();
    }
  }
  let active;
  async function run() {
    while (!stopped) {
      let sock, checkpoint;
      try {
        sock = await connection(
          auth.url,
          receive,
          (requested) => {
            checkpoint = requested;
            status("Connected");
            wake();
          },
          wake,
        );
        active = sock;
        if (stopped) return;
        const ready = await sock.login({
          type: "hello",
          streaming: true,
          vault: auth.p.vault,
          grant: auth.p.grant,
          token: b64(Uint8Array.from(auth.p.token)),
        });
        check(ready.streaming, upgradeRelay);
        sock.subscribe((await setting("relay-cursor")) || 0);
        while (!stopped) {
          dirty = false;
          sock.ensureOpen();
          const head = await request(
            db.transaction("outbox").objectStore("outbox").openCursor(),
          );
          if (head) {
            let pkt = head.value;
            const job = pkt.upload ? pkt : null;
            if (job) {
              const data = await request(
                db
                  .transaction("blobs")
                  .objectStore("blobs")
                  .get([job.upload, job.next]),
              );
              check(data, "Missing local file chunk");
              pkt = await auth.packet(job.upload, job.next, data);
            } else if (
              pkt.grant.device !== auth.p.grant.device ||
              pkt.grant.epoch !== auth.p.grant.epoch
            ) {
              const opened = await auth.unpack(pkt);
              if (pkt.part >= 0)
                pkt = await auth.packet(pkt.room, pkt.part, opened.data);
              else if (opened.snapshot) pkt = await snap(pkt.room);
              else pkt = await auth.delta(pkt.room, opened.raw);
            }
            await sock.push(pkt);
            const tx = db.transaction("outbox", "readwrite"),
              done = complete(tx);
            const store = tx.objectStore("outbox");
            if (job && job.next < job.last) {
              const get = store.get(head.key);
              get.onsuccess = () => {
                if (get.result?.next === job.next)
                  store.put({ ...job, next: job.next + 1 }, head.key);
              };
            } else store.delete(head.key);
            await done;
            continue;
          }
          if (checkpoint) {
            const room = checkpoint;
            checkpoint = undefined;
            const pkt = await snap(room);
            if (pkt) await sock.push(pkt);
            continue;
          }
          if (!dirty)
            await new Promise((resolve) => {
              // Another tab can append to the same local outbox.
              const timer = setTimeout(resolve, 3000);
              notify = () => {
                clearTimeout(timer);
                resolve();
              };
            });
          notify = undefined;
        }
      } catch (error) {
        if (!stopped)
          status(
            error.message === upgradeRelay
              ? upgradeRelay
              : "Offline · encrypted edits queued",
          );
      } finally {
        sock?.close();
        active = undefined;
      }
      if (!stopped) await sleep(1500);
    }
  }
  if (auth) void run();
  window.addEventListener(
    "pagehide",
    () => {
      stopped = true;
      active?.close();
      wake();
    },
    { once: true },
  );
  return {
    paired: () => Boolean(auth),
    wake,
    delta: (room, raw) => {
      check(auth, "Pair a device before syncing");
      return auth.delta(room, raw);
    },
    async pair(code) {
      const prefix = "margin-pair-1.";
      check(
        code.trim().startsWith(prefix),
        "Choose a Margin device invitation",
      );
      const kit = JSON.parse(
        text.decode(bytes(code.trim().slice(prefix.length))),
      );
      const next = await credentials(kit);
      if (auth)
        check(
          auth.p.vault === next.p.vault &&
            next.p.grant.epoch >= auth.p.grant.epoch,
          "Choose an invitation for this vault",
        );
      const sealed = await cipher.seal(
        metaKey,
        utf8.encode(JSON.stringify(kit)),
      );
      const tx = db.transaction("settings", "readwrite"),
        done = complete(tx);
      tx.objectStore("settings").put(sealed, metaKey);
      await done;
      location.reload();
    },
  };
}
