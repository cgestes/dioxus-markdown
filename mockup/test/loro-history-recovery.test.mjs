import assert from "node:assert/strict";
import { test } from "node:test";
import { execFile, spawn } from "node:child_process";
import { once } from "node:events";
import { createServer } from "node:http";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { LoroDoc, decodeImportBlobMeta } from "loro-crdt";

import { mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  archiveReader,
  documentKey,
  openCheckpoint,
  openDelta,
  serveArchive,
} from "../experiments/loro-history/archive.mjs";
import { decode, inspect, makeFixture } from "../experiments/loro-history/fixture.mjs";
import { HistoryUnavailable, recoverHistory } from "../experiments/loro-history/recover.mjs";

function disconnectedWriters() {
  const a = new LoroDoc();
  a.setPeerId("1");
  a.getText("markdown").insert(0, "abc");
  a.commit();
  const base = a.export({ mode: "snapshot" });
  const before = a.version();
  const b = a.fork();
  b.setPeerId("2");
  b.getText("markdown").insert(1, "!");
  const offline = b.export({ mode: "update", from: before });
  return { a, base, before, offline };
}

test("the edit names its external dependency and waits while the app fetches its range", () => {
  const { a, offline } = disconnectedWriters();
  const replica = new LoroDoc();
  const status = replica.import(offline);
  assert.deepEqual([...status.pending], [["2", { start: 0, end: 1 }]]);
  const dependencies = decodeImportBlobMeta(offline, true).startFrontiers;
  assert.deepEqual(dependencies, [{ peer: "1", counter: 2 }]);

  // A's counter 2 is inclusive. Version vectors give the exclusive next counter.
  const known = replica.version().toJSON();
  const spans = dependencies.map(({ peer, counter }) => {
    const start = known.get(peer) ?? 0;
    return { id: { peer, counter: start }, len: counter + 1 - start };
  });
  const recovered = new LoroDoc();
  recovered.import(replica.exportPendingUpdates());
  recovered.import(a.export({ mode: "updates-in-range", spans }));
  assert.equal(recovered.getText("markdown").toString(), "a!bc");
  assert.equal(recovered.exportPendingUpdates(), undefined);
});

test("rebuilding from a retained full checkpoint preserves a pre-cutoff offline edit", () => {
  const { a, base, before, offline } = disconnectedWriters();
  a.getText("markdown").insert(3, " DEF");
  const suffix = a.export({ mode: "update", from: before });
  const shallow = LoroDoc.fromSnapshot(a.export({ mode: "shallow-snapshot", frontiers: a.oplogFrontiers() }));
  assert.throws(() => shallow.import(offline), /shallow history/i);
  assert.equal(shallow.exportPendingUpdates(), undefined);
  shallow.import(base);
  assert.equal(shallow.isShallow(), true);
  assert.throws(() => shallow.import(offline), /shallow history/i);

  const restored = LoroDoc.fromSnapshot(base);
  restored.import(suffix);
  restored.import(offline);
  a.import(offline);
  assert.equal(restored.getText("markdown").toString(), "a!bc DEF");
  assert.equal(restored.version().compare(a.version()), 0);
  assert.equal(restored.isShallow(), false);
  assert.deepEqual(restored.getChangeAt({ peer: "2", counter: 0 }), a.getChangeAt({ peer: "2", counter: 0 }));
});

async function deviceSession(fixture, cacheDir) {
  const info = fixture.device,
    key = await documentKey(decode(info.key));
  const server = await serveArchive(fixture.objects);
  const reader = await archiveReader(server.url, key, cacheDir);
  const root = await openCheckpoint(fixture.objects.get(info.current.id), key, info.current);
  const source = LoroDoc.fromSnapshot(root);
  source.import(await openDelta(decode(info.local), key));
  const incoming = await openDelta(decode(info.incoming), key);
  return {
    source,
    key,
    server,
    reader,
    options: {
      source,
      updates: [incoming, incoming],
      anchors: info.anchors.map(decode),
      archive: reader,
      checkpoints: [info.earlier, info.full],
    },
  };
}

for (const [label, scenario, expectedRoot] of [
  ["multi-writer causal chain", {}, "current"],
  ["offline fork before the current root", { beforeRoot: true, oldAnchor: true }, "earlier shallow"],
  ["already-integrated state with a missing historical anchor", { oldAnchor: true }, "earlier shallow"],
  ["offline fork before both shallow roots", { beforeEarlier: true, oldAnchor: true }, "full"],
]) {
  test("encrypted range recovery preserves IDs, marks, moved items and anchors for " + label, async () => {
    const fixture = await makeFixture({ size: 4096, ...scenario });
    const { source, server, reader, options } = await deviceSession(fixture);
    try {
      const recovered = await recoverHistory(options);
      try {
        assert.equal(recovered.from, expectedRoot);
        assert.deepEqual(inspect(recovered.doc, fixture.device.anchors), fixture.expected);
        assert.equal(recovered.doc.exportPendingUpdates(), undefined);
        if (expectedRoot === "current") {
          assert.equal(reader.stats.requests, 3);
          assert.equal(reader.stats.missing, 0);
        }
      } finally {
        recovered.doc.free();
      }
    } finally {
      source.free();
      await server.close();
    }
  });
}

test("a killed client recovers its encrypted journal and reuses a verified download", async () => {
  const fixture = await makeFixture({ size: 2048 });
  const dir = await mkdtemp(join(tmpdir(), "margin-range-restart-"));
  const cache = join(dir, "cache");
  const relay = await serveArchive(fixture.objects);
  let reads = 0;
  // Hold the second request open. The first response was authenticated and
  // atomically cached before the client could issue this request.
  const gate = createServer(async (req, res) => {
    if (req.url.startsWith("/range?") && ++reads === 2) {
      gate.emit("secondRange");
      return;
    }
    try {
      const response = await fetch(relay.url + req.url);
      res.writeHead(response.status);
      res.end(Buffer.from(await response.arrayBuffer()));
    } catch {
      res.destroy();
    }
  });
  const run = fileURLToPath(new URL("../experiments/loro-history/run.mjs", import.meta.url));
  let child;
  try {
    await new Promise((resolve, reject) => gate.once("error", reject).listen(0, "127.0.0.1", resolve));
    await writeFile(join(dir, "journal.json"), JSON.stringify(fixture.device), { mode: 0o600 });
    await writeFile(join(dir, "current.elo"), fixture.objects.get(fixture.device.current.id));
    const reached = once(gate, "secondRange", { signal: AbortSignal.timeout(10000) });
    child = spawn(
      process.execPath,
      [run, "worker", dir, "http://127.0.0.1:" + gate.address().port, "ranges", cache],
      { stdio: "ignore" },
    );
    const stopped = once(child, "exit");
    assert.equal(
      await Promise.race([reached.then(() => "fetching"), stopped.then(() => "exited")]),
      "fetching",
    );
    child.kill("SIGKILL");
    const [, signal] = await stopped;
    assert.equal(signal, "SIGKILL");
    const [cached] = await readdir(cache);
    const bytes = await readFile(join(cache, cached));
    await openDelta(bytes, await documentKey(decode(fixture.device.key)));
    assert.ok(!bytes.includes(Buffer.from("Bring a pen")));
    const result = await promisify(execFile)(
      process.execPath,
      [run, "worker", dir, relay.url, "ranges", cache],
      { timeout: 15000 },
    );
    const recovered = JSON.parse(result.stdout);
    assert.deepEqual(recovered.result, fixture.expected);
    assert.equal(recovered.hits, 1);
    assert.equal(recovered.requests, 2);
  } finally {
    if (child && child.exitCode === null && child.signalCode === null) child.kill();
    await new Promise((resolve) => {
      gate.close(resolve);
      gate.closeAllConnections();
    });
    await relay.close();
    await rm(dir, { recursive: true, force: true });
  }
});

test("expired ranges use a full checkpoint, and a missing archive leaves the original usable", async () => {
  const fixture = await makeFixture({ size: 2048, beforeRoot: true, oldAnchor: true });
  for (const id of fixture.spanIds) fixture.objects.delete(id);
  const { source, server, options } = await deviceSession(fixture);
  try {
    const { doc, from } = await recoverHistory(options);
    try {
      assert.equal(from, "full");
      assert.deepEqual(inspect(doc, fixture.device.anchors), fixture.expected);
    } finally {
      doc.free();
    }
    const original = source.oplogVersion();
    const missing = { ...fixture.device.full, id: "0".repeat(64) };
    await assert.rejects(
      recoverHistory({ ...options, checkpoints: [fixture.device.earlier, missing] }),
      HistoryUnavailable,
    );
    assert.equal(source.oplogVersion().compare(original), 0);
    assert.ok(source.getText("markdown").toString().startsWith("Device copy"));
  } finally {
    source.free();
    await server.close();
  }
});

test("the device verifies ciphertext and its requested address before using a range", async () => {
  const fixture = await makeFixture({ size: 512 });
  const key = await documentKey(decode(fixture.device.key));
  const data = fixture.objects.get(fixture.spanIds[0]);
  await assert.rejects(openDelta(data, key, { peer: "48", start: 0 }), /different range/);
  const broken = data.slice();
  broken[broken.length - 1] ^= 1;
  await assert.rejects(openDelta(broken, key));
  const otherKey = await documentKey(new Uint8Array(32));
  await assert.rejects(openDelta(data, otherKey));
  const ref = fixture.device.earlier;
  await assert.rejects(
    openCheckpoint(fixture.objects.get(ref.id), key, { ...ref, shallow: false }),
    /coverage mismatch/,
  );
});

test("a local edit made during a range request is present in the recovered document", async () => {
  const fixture = await makeFixture({ size: 1024 });
  const { source, key, server, reader, options } = await deviceSession(fixture);
  try {
    let tail;
    const archive = {
      ...reader,
      async range(...args) {
        if (!tail) {
          const before = source.oplogVersion();
          source.getText("markdown").insert(0, "Keep typing · ");
          tail = source.export({ mode: "update", from: before });
        }
        return reader.range(...args);
      },
    };
    const { doc } = await recoverHistory({ ...options, archive });
    const oracle = LoroDoc.fromSnapshot(
      await openCheckpoint(fixture.objects.get(fixture.device.full.id), key, fixture.device.full),
    );
    try {
      oracle.importBatch([await openDelta(decode(fixture.device.local), key), ...options.updates, tail]);
      assert.deepEqual(inspect(doc, fixture.device.anchors), inspect(oracle, fixture.device.anchors));
    } finally {
      doc.free();
      oracle.free();
    }
  } finally {
    source.free();
    await server.close();
  }
});

test("re-reading pending metadata discovers an interior gap in a single writer's disjoint export", async () => {
  const writer = new LoroDoc();
  writer.setPeerId("1");
  const t = writer.getText("markdown");
  for (const c of ["a", "b", "c", "d", "e"]) {
    t.insert(t.length, c);
    writer.commit();
  }
  const spans = (start, len) => ({ id: { peer: "1", counter: start }, len });
  const disjoint = writer.export({ mode: "updates-in-range", spans: [spans(1, 1), spans(4, 1)] });
  const receiver = new LoroDoc(),
    reads = [];
  try {
    const recovered = await recoverHistory({
      source: receiver,
      updates: [disjoint],
      archive: {
        async range(peer, start, end) {
          reads.push([peer, start]);
          return writer.export({ mode: "updates-in-range", spans: [spans(start, 1)] });
        },
      },
    });
    try {
      assert.equal(recovered.doc.getText("markdown").toString(), "abcde");
      assert.deepEqual(reads, [
        ["1", 0],
        ["1", 2],
        ["1", 3],
      ]);
    } finally {
      recovered.doc.free();
    }
  } finally {
    receiver.free();
    writer.free();
  }
});

test("a single multi-writer payload needs only its shared external predecessor", async () => {
  const root = new LoroDoc();
  root.setPeerId("11");
  root.getText("markdown").insert(0, "base");
  root.commit();
  const base = root.oplogVersion(),
    a = root.fork(),
    b = root.fork();
  a.setPeerId("21");
  b.setPeerId("22");
  a.getText("markdown").insert(0, "a");
  b.getText("markdown").insert(4, "b");
  a.import(b.export({ mode: "update", from: base }));
  const batch = a.export({ mode: "update", from: base });
  const meta = decodeImportBlobMeta(batch, true);
  assert.equal(meta.partialEndVersionVector.toJSON().size, 2);
  assert.deepEqual(meta.startFrontiers, [{ peer: "11", counter: 3 }]);
  const source = new LoroDoc(),
    requests = [];
  try {
    const { doc } = await recoverHistory({
      source,
      updates: [batch],
      archive: {
        async range(peer, start, end) {
          requests.push([peer, start, end]);
          return root.export({
            mode: "updates-in-range",
            spans: [{ id: { peer, counter: start }, len: end - start }],
          });
        },
      },
    });
    try {
      assert.deepEqual(requests, [["11", 0, 4]]);
      assert.equal(doc.oplogVersion().compare(a.oplogVersion()), 0);
      assert.deepEqual(doc.toJSON(), a.toJSON());
    } finally {
      doc.free();
    }
  } finally {
    for (const doc of [root, a, b, source]) doc.free();
  }
});

test("a directly rejected pre-root input survives restart in its original encrypted envelope", async () => {
  const fixture = await makeFixture({ size: 1024, beforeRoot: true });
  const dir = await mkdtemp(join(tmpdir(), "margin-pre-root-"));
  const first = await deviceSession(fixture);
  try {
    const path = join(dir, "incoming.elo");
    // The third archived edit was made by a device directly on the older root.
    const packet = fixture.objects.get(fixture.spanIds[2]);
    await writeFile(path, packet, { mode: 0o600 });
    const firstRaw = await openDelta(packet, first.key);
    assert.throws(() => first.source.import(firstRaw), /shallow history/i);
    assert.equal(first.source.exportPendingUpdates(), undefined);
  } finally {
    first.source.free();
    await first.server.close();
  }
  try {
    const resumed = await deviceSession(fixture);
    try {
      const incoming = await openDelta(await readFile(join(dir, "incoming.elo")), resumed.key);
      const result = await recoverHistory({ ...resumed.options, updates: [incoming] });
      const ref = fixture.device.initialFull;
      const expected = LoroDoc.fromSnapshot(
        await openCheckpoint(fixture.objects.get(ref.id), resumed.key, ref),
      );
      try {
        expected.importBatch([incoming, await openDelta(decode(fixture.device.local), resumed.key)]);
        assert.equal(result.from, "earlier shallow");
        assert.deepEqual(result.doc.toJSON(), expected.toJSON());
        assert.equal(result.doc.oplogVersion().compare(expected.oplogVersion()), 0);
      } finally {
        expected.free();
        result.doc.free();
      }
    } finally {
      resumed.source.free();
      await resumed.server.close();
    }
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});
