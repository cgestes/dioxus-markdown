// Loopback research archive. The serving side takes only encoded records and their
// public range headers. Checkpoint references and the document key belong to the device.
import { createHash, randomUUID } from "node:crypto";
import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { decodeImportBlobMeta, LoroDoc } from "loro-crdt";
import { decryptEloRecord, EloRecordKind, parseEloRecordHeader } from "loro-protocol";

export const hash = (data) => createHash("sha256").update(data).digest("hex");
const peerName = (bytes) => new TextDecoder().decode(bytes);

export async function serveArchive(objects, profile = "loopback") {
  if (!["loopback", "constrained"].includes(profile)) throw new Error("Unknown link profile");
  const ranges = new Map();
  for (const data of objects.values()) {
    const { kind, header } = parseEloRecordHeader(data);
    if (kind !== EloRecordKind.DeltaSpan) continue;
    const peer = peerName(header.peerId);
    if (!ranges.has(peer)) ranges.set(peer, []);
    ranges.get(peer).push({ start: header.start, end: header.end, data });
  }
  const server = createServer(async (req, res) => {
    const url = new URL(req.url, "http://127.0.0.1");
    let body;
    const match = /^\/objects\/([a-f0-9]{64})$/.exec(url.pathname);
    if (match) body = objects.get(match[1]);
    if (url.pathname === "/range") {
      const peer = url.searchParams.get("peer"),
        start = Number(url.searchParams.get("start"));
      const end = Number(url.searchParams.get("end"));
      if (Number.isSafeInteger(start) && start >= 0 && Number.isSafeInteger(end) && end > start) {
        body = ranges.get(peer)?.find((r) => r.start <= start && start < r.end)?.data;
      }
    }
    // Emulate one 40 ms round trip plus serialization at 10 decimal Mbit/s.
    // The adapter consumes whole authenticated records before import.
    if (profile === "constrained") await delay(40 + ((body?.length || 0) * 8) / 10000);
    res.writeHead(body ? 200 : 404, { "content-type": "application/octet-stream" });
    res.end(body);
  });
  await new Promise((resolve, reject) => server.once("error", reject).listen(0, "127.0.0.1", resolve));
  return {
    url: "http://127.0.0.1:" + server.address().port,
    close: () =>
      new Promise((resolve) => {
        server.close(resolve);
        server.closeAllConnections();
      }),
  };
}

export async function documentKey(raw) {
  return crypto.subtle.importKey("raw", raw, { name: "AES-GCM" }, false, ["encrypt", "decrypt"]);
}

export async function openDelta(record, key, requested) {
  const opened = await decryptEloRecord(record, () => key);
  if (opened.kind !== EloRecordKind.DeltaSpan) throw new Error("Expected a delta record");
  const { peerId, start, end } = opened.header,
    peer = peerName(peerId);
  const meta = decodeImportBlobMeta(opened.plaintext, true);
  const last = meta.partialEndVersionVector.toJSON();
  if (
    meta.mode !== "update" ||
    last.size !== 1 ||
    last.get(peer) !== end ||
    (meta.partialStartVersionVector.toJSON().get(peer) || 0) !== start ||
    end <= start
  ) {
    throw new Error("Delta header disagrees with its contents");
  }
  if (requested && (peer !== requested.peer || start > requested.start || end <= requested.start)) {
    throw new Error("Archive returned a different range");
  }
  return opened.plaintext;
}

export async function openCheckpoint(record, key, ref) {
  if (hash(record) !== ref.id) throw new Error("Checkpoint digest mismatch");
  const opened = await decryptEloRecord(record, () => key);
  if (opened.kind !== EloRecordKind.Snapshot) throw new Error("Expected a checkpoint record");
  const doc = LoroDoc.fromSnapshot(opened.plaintext);
  try {
    const vv = doc.oplogVersion().toJSON(),
      advertised = opened.header.vv;
    if (
      doc.isShallow() !== ref.shallow ||
      vv.size !== advertised.length ||
      new Set(advertised.map((v) => peerName(v.peerId))).size !== vv.size ||
      !advertised.every(({ peerId, counter }) => vv.get(peerName(peerId)) === counter)
    ) {
      throw new Error("Checkpoint coverage mismatch");
    }
  } finally {
    doc.free();
  }
  return opened.plaintext;
}

export async function archiveReader(url, key, cacheDir) {
  if (cacheDir) await mkdir(cacheDir, { recursive: true });
  const stats = { bytes: 0, requests: 0, hits: 0, missing: 0 };
  async function read(path, cacheName, validate) {
    const file = cacheDir && join(cacheDir, cacheName);
    if (file) {
      let cached;
      try {
        cached = await readFile(file);
      } catch (error) {
        if (error.code !== "ENOENT") throw error;
      }
      if (cached) {
        stats.hits++;
        return validate(cached);
      }
    }
    stats.requests++;
    const res = await fetch(url + path, { signal: AbortSignal.timeout(10000) });
    if (res.status === 404) {
      stats.missing++;
      return;
    }
    if (!res.ok) throw new Error("Archive HTTP " + res.status);
    const body = new Uint8Array(await res.arrayBuffer());
    stats.bytes += body.length;
    const raw = await validate(body);
    if (file) {
      const temporary = file + "." + randomUUID();
      await writeFile(temporary, body, { mode: 0o600 });
      await rename(temporary, file);
    }
    return raw;
  }
  return {
    stats,
    checkpoint: (ref) => {
      if (!/^[a-f0-9]{64}$/.test(ref.id) || typeof ref.shallow !== "boolean")
        throw new Error("Invalid checkpoint reference");
      return read("/objects/" + ref.id, ref.id, (bytes) => openCheckpoint(bytes, key, ref));
    },
    range: (peer, start, end) => {
      if (
        !/^\d+$/.test(peer) ||
        !Number.isSafeInteger(start) ||
        start < 0 ||
        !Number.isSafeInteger(end) ||
        end <= start
      ) {
        throw new Error("Invalid range");
      }
      return read("/range?" + new URLSearchParams({ peer, start, end }), peer + "-" + start, (bytes) =>
        openDelta(bytes, key, { peer, start }),
      );
    },
  };
}
