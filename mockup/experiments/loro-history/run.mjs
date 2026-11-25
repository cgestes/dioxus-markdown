// Run from mockup with npm run experiment:history. Workers isolate client peak RSS
// from fixture generation and the ciphertext-only HTTP process.
import assert from "node:assert/strict";
import { execFile, fork } from "node:child_process";
import { mkdtemp, mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { arch, cpus, platform, tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { LoroDoc } from "loro-crdt";
import { archiveReader, documentKey, openCheckpoint, openDelta, serveArchive } from "./archive.mjs";
import { decode, fingerprint, inspect, makeFixture } from "./fixture.mjs";
import { recoverHistory } from "./recover.mjs";

const file = fileURLToPath(import.meta.url);
const execute = promisify(execFile);
const rounds = 5;

async function serve(dir, profile) {
  const objects = new Map();
  for (const id of await readdir(dir)) objects.set(id, await readFile(join(dir, id)));
  const server = await serveArchive(objects, profile);
  process.send(server.url);
  process.once("disconnect", async () => {
    await server.close();
  });
}

async function measure(dir, url, strategy, cache) {
  const info = JSON.parse(await readFile(join(dir, "journal.json"), "utf8"));
  const key = await documentKey(decode(info.key));
  const archive = await archiveReader(url, key, cache);
  if (strategy.startsWith("open-")) {
    const ref = strategy === "open-shallow" ? info.current : info.initialFull;
    const start = performance.now();
    const doc = LoroDoc.fromSnapshot(await archive.checkpoint(ref));
    try {
      doc.setPeerId("92");
      doc.getText("markdown").insert(0, "First offline edit · ");
      doc.commit({ timestamp: 0 });
      const ms = performance.now() - start,
        rssKiB = process.resourceUsage().maxRSS;
      return {
        ...archive.stats,
        ms,
        rssKiB,
        from: ref.label,
        result: fingerprint({
          state: doc.toJSON(),
          marks: doc.getText("markdown").toDelta(),
          vector: Object.fromEntries(doc.oplogVersion().toJSON()),
        }),
      };
    } finally {
      doc.free();
    }
  }

  const cached = await readFile(join(dir, "current.elo"));
  const source = LoroDoc.fromSnapshot(await openCheckpoint(cached, key, info.current));
  try {
    source.import(await openDelta(decode(info.local), key));
    const updates = [await openDelta(decode(info.incoming), key)];
    const anchors = info.anchors.map(decode),
      selective = strategy === "ranges";
    const start = performance.now();
    const { doc, from } = await recoverHistory({
      source,
      updates,
      anchors,
      archive,
      useCurrent: selective,
      checkpoints: selective ? [info.earlier, info.full] : [info.full],
    });
    try {
      const ms = performance.now() - start,
        rssKiB = process.resourceUsage().maxRSS;
      return { ...archive.stats, ms, rssKiB, from, result: inspect(doc, info.anchors) };
    } finally {
      doc.free();
    }
  } finally {
    source.free();
  }
}

function startRelay(dir, profile) {
  const child = fork(file, ["serve", dir, profile], { stdio: ["ignore", "ignore", "inherit", "ipc"] });
  return new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("exit", (code) => reject(new Error("Archive exited with " + code)));
    child.once("message", (url) =>
      resolve({
        url,
        stop: () =>
          new Promise((done) => {
            child.once("exit", done);
            child.disconnect();
          }),
      }),
    );
  });
}

async function benchmark() {
  const results = [];
  const workloads = [
    ["post-root", {}, "current"],
    ["pre-root", { beforeRoot: true, oldAnchor: true }, "earlier shallow"],
    ["missing anchor", { oldAnchor: true }, "earlier shallow"],
    ["pre-both", { beforeEarlier: true, oldAnchor: true }, "full"],
    ["visible text", { visible: true }, "current"],
  ];
  const profile = process.argv.includes("--constrained") ? "constrained" : "loopback";
  for (const [name, options, expectedPath] of workloads) {
    if (profile === "constrained" && !["post-root", "pre-root"].includes(name)) continue;
    const dir = await mkdtemp(join(tmpdir(), "margin-history-"));
    try {
      const { objects, device, expected } = await makeFixture(options);
      const archiveDir = join(dir, "archive"),
        deviceDir = join(dir, "device");
      await mkdir(archiveDir);
      await mkdir(deviceDir);
      for (const [id, bytes] of objects) await writeFile(join(archiveDir, id), bytes);
      // Only the device receives a key and the offline journal. The reference
      // result stays in the parent process throughout the comparison.
      await writeFile(join(deviceDir, "journal.json"), JSON.stringify(device), { mode: 0o600 });
      await writeFile(join(deviceDir, "current.elo"), objects.get(device.current.id));
      const relay = await startRelay(archiveDir, profile);
      try {
        const strategies = ["ranges", "full"];
        if (profile === "loopback" && (name === "post-root" || name === "visible text"))
          strategies.push("open-shallow", "open-full");
        const observations = new Map(strategies.map((strategy) => [strategy, []]));
        let coldState;
        for (let i = 0; i < rounds; i++) {
          const order = i % 2 ? strategies.toReversed() : strategies;
          for (const strategy of order) {
            const cache = join(deviceDir, "cache-" + i + "-" + strategy);
            const { stdout } = await execute(
              process.execPath,
              [file, "worker", deviceDir, relay.url, strategy, cache],
              { timeout: 30000 },
            );
            const sample = JSON.parse(stdout);
            if (strategy.startsWith("open-")) {
              coldState ??= sample.result;
              assert.equal(sample.result, coldState);
            } else {
              assert.deepEqual(sample.result, expected);
              assert.equal(sample.from, strategy === "ranges" ? expectedPath : "full");
            }
            observations.get(strategy).push(sample);
          }
        }
        for (const [strategy, samples] of observations) {
          const [first] = samples;
          for (const s of samples)
            assert.deepEqual(
              [s.bytes, s.requests, s.hits, s.missing],
              [first.bytes, first.requests, first.hits, first.missing],
            );
          const median = (field) =>
            [...samples].sort((a, b) => a[field] - b[field])[Math.floor(rounds / 2)][field];
          results.push({
            workload: name,
            strategy,
            checkpoint: first.from,
            bytes: first.bytes,
            requests: first.requests,
            cacheHits: first.hits,
            misses: first.missing,
            ms: +median("ms").toFixed(2),
            samplesMs: samples.map((s) => +s.ms.toFixed(2)),
            peakMiB: +(median("rssKiB") / 1024).toFixed(1),
          });
        }
      } finally {
        await relay.stop();
      }
    } finally {
      await rm(dir, { recursive: true, force: true });
    }
  }
  return {
    profile,
    version: process.version,
    system: platform() + "/" + arch(),
    cpu: cpus()[0].model,
    rounds,
    results,
  };
}

if (process.argv[2] === "serve") {
  await serve(process.argv[3], process.argv[4]);
} else if (process.argv[2] === "worker") {
  console.log(JSON.stringify(await measure(...process.argv.slice(3))));
} else {
  const report = await benchmark();
  if (process.argv.includes("--json")) console.log(JSON.stringify(report, null, 2));
  else {
    console.table(report.results);
    console.log(
      report.rounds + " runs per strategy in fresh " + report.version + " processes on " + report.cpu + ".",
    );
  }
}
