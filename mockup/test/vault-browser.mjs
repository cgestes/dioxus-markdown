// Real browser, on-disk vaults, native CLI, and a keyless relay on an ephemeral port.
import assert from "node:assert/strict";
import { execFile, spawn } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import { createServer } from "node:http";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { createInterface } from "node:readline";
import { chromium } from "playwright-core";
import { build } from "esbuild";
import * as Y from "yjs";
import { LoroDoc } from "loro-crdt";
import {
  CrdtType,
  MAX_MESSAGE_SIZE,
  MessageType,
  decode,
  parseEloRecordHeader,
} from "loro-protocol";

const run = promisify(execFile);
const root = await mkdtemp(join(tmpdir(), "margin-vault-browser-"));
const binary =
  process.env.MARGIN_VAULT_BIN ||
  fileURLToPath(new URL("../../target/debug/margin-vault", import.meta.url));
const passFile = join(root, "unlock");
const args = ["--dir", join(root, "owner"), "--passphrase-file", passFile];
const cli = async (...command) =>
  (await run(binary, [...args, ...command], { timeout: 30000 })).stdout.trim();
const database = join(root, "relay.sqlite3");
const invitation = join(root, "browser.invite");
const html = await readFile(new URL("../index.html", import.meta.url));
const site = createServer((_, res) => {
  res.setHeader("Content-Type", "text/html; charset=utf-8");
  res.end(html);
});
let browser, relay, page, follower;
const wire = {
  subscriptions: [],
  checkpoints: 0,
  received: 0,
  fragmentEvents: [],
  lastEvent: 0,
  deltaDigests: [],
  pulls: 0,
};
const wireErrors = [];
function observeFrame(payload, sent) {
  try {
    if (typeof payload === "string") {
      if (payload === "ping" || payload === "pong") return;
      const m = JSON.parse(payload);
      if (m.type === "subscribe" && sent) wire.subscriptions.push(m.after);
      if (m.type === "event" && !sent) wire.lastEvent = m.seq;
      if (m.type === "next" || m.type === "changed") wire.pulls++;
      if (m.packet?.part === -1) assert.equal(m.packet.data, undefined);
      return;
    }
    assert.ok(payload.byteLength <= MAX_MESSAGE_SIZE);
    const m = decode(new Uint8Array(payload));
    assert.equal(m.crdt, CrdtType.Elo);
    if (m.type === MessageType.DocUpdate) {
      assert.equal(m.updates.length, 1);
      const record = parseEloRecordHeader(m.updates[0]);
      if (sent && record.kind === 1) wire.checkpoints++;
      if (sent && record.kind === 0)
        wire.deltaDigests.push(
          createHash("sha256").update(m.updates[0]).digest("hex"),
        );
      if (!sent) wire.received++;
    }
    if (!sent && m.type === MessageType.DocUpdateFragmentHeader)
      wire.fragmentEvents.push(wire.lastEvent);
  } catch (e) {
    wireErrors.push(e);
  }
}
async function until(condition) {
  const deadline = Date.now() + 30000;
  while (!(await condition())) {
    assert.ok(Date.now() < deadline, "stream check timed out");
    await new Promise((resolve) => setTimeout(resolve, 75));
  }
}

async function unlock(first = false, value = "browser fixture passphrase") {
  const dialog = page.locator("#vault-unlock[open]");
  await dialog.locator("[name=pass]").fill(value);
  if (first) await dialog.locator("[name=confirm]").fill(value);
  await dialog.getByRole("button").click();
}

async function saved() {
  await page.waitForFunction(
    () =>
      document.querySelector("#storage").textContent ===
      "Encrypted on this device",
  );
}
async function drained() {
  await saved();
  await page.waitForFunction(async () => {
    const [info] = (await indexedDB.databases()).filter((db) =>
      db.name.startsWith("margin-private."),
    );
    const db = await new Promise((resolve) => {
      const r = indexedDB.open(info.name);
      r.onsuccess = () => resolve(r.result);
    });
    try {
      const request = db.transaction("outbox").objectStore("outbox").count();
      return await new Promise((resolve) => {
        request.onsuccess = () => resolve(request.result === 0);
      });
    } finally {
      db.close();
    }
  });
}
async function edit(text, append = false) {
  await page.locator("#source").click();
  const editor = page.locator(".cm-content");
  await editor.focus();
  await editor.press(append ? "ControlOrMeta+End" : "ControlOrMeta+a");
  await page.keyboard.insertText(text);
  await saved();
}

try {
  await writeFile(passFile, randomBytes(32), { mode: 0o600 });
  const owner = await cli("init", "--recovery", join(root, "recovery"));
  relay = spawn(
    binary,
    [
      "serve",
      "--database",
      database,
      "--owner",
      owner,
      "--listen",
      "127.0.0.1:0",
      "--checkpoint-after",
      "2",
    ],
    { stdio: ["ignore", "ignore", "pipe"] },
  );
  const endpoint = await new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error("Relay startup timed out")),
      12000,
    );
    relay.once("error", reject);
    relay.once("exit", (code) => {
      clearTimeout(timer);
      reject(new Error("Relay exited with " + code));
    });
    createInterface({ input: relay.stderr }).on("line", (line) => {
      const found = /Listening on (127\.0\.0\.1:\d+)/.exec(line);
      if (found) {
        clearTimeout(timer);
        resolve("ws://" + found[1]);
      }
    });
  });
  await cli("connect", endpoint);
  await cli("invite", invitation);
  const title = "Native interoperability diary";
  const nativeMarkdown = "# " + title + "\n\nFirst device.\n";
  const source = join(root, "source.md");
  await writeFile(source, nativeMarkdown);
  const room = await cli("new", "--synced", "--input", source);
  const nativeFile = join(root, "native-private.bin"),
    nativeBytes = randomBytes(260001);
  await writeFile(nativeFile, nativeBytes);
  await cli("attach", room, nativeFile);
  await cli("sync");
  await new Promise((resolve) => site.listen(0, "127.0.0.1", resolve));
  browser = await chromium.launch({
    channel: process.env.MARGIN_CHROME_CHANNEL || "chrome",
  });
  page = await browser.newPage({
    viewport: { width: 1100, height: 780 },
    acceptDownloads: true,
  });
  const errors = [];
  page.on("pageerror", (err) => errors.push(err.message));
  page.on("websocket", (socket) => {
    socket.on("framesent", ({ payload }) => observeFrame(payload, true));
    socket.on("framereceived", ({ payload }) => observeFrame(payload, false));
  });
  const codec = await build({
    stdin: {
      contents:
        'export { decode, MessageType, EloRecordKind, parseEloRecordHeader } from "loro-protocol";',
      resolveDir: fileURLToPath(new URL("..", import.meta.url)),
    },
    bundle: true,
    write: false,
    format: "iife",
    globalName: "marginTestCodec",
  });
  await page.addInitScript({
    content:
      codec.outputFiles[0].text +
      "\nglobalThis.marginTestCodec = marginTestCodec;",
  });
  await page.addInitScript(() => {
    window.showSaveFilePicker = undefined;
    // Install the cut before the application's message handler. Ignore already
    // queued frames on that socket once the simulated link has dropped.
    window.WebSocket = new Proxy(window.WebSocket, {
      construct(WebSocket, args) {
        const ws = new WebSocket(...args);
        let cut = false,
          dropBatch;
        const { decode, MessageType, EloRecordKind, parseEloRecordHeader } =
          window.marginTestCodec;
        const send = ws.send.bind(ws);
        ws.send = (frame) => {
          if (window.dropAckRoom && ArrayBuffer.isView(frame)) {
            const m = decode(frame);
            if (
              m.roomId === window.dropAckRoom &&
              m.type === MessageType.DocUpdate &&
              parseEloRecordHeader(m.updates[0]).kind ===
                EloRecordKind.DeltaSpan
            )
              dropBatch = m.batchId;
          }
          return send(frame);
        };
        ws.addEventListener("message", (e) => {
          if (cut) {
            e.stopImmediatePropagation();
            return;
          }
          if (!(e.data instanceof ArrayBuffer)) return;
          const m = decode(new Uint8Array(e.data));
          if (
            window.interruptFragment &&
            m.type === MessageType.DocUpdateFragment &&
            m.index === 1
          ) {
            window.interruptFragment = false;
            window.interrupted = true;
          } else if (
            dropBatch &&
            m.type === MessageType.Ack &&
            m.refId === dropBatch
          ) {
            window.dropAckRoom = undefined;
            window.droppedAck = true;
          } else return;
          cut = true;
          e.stopImmediatePropagation();
          ws.close();
        });
        return ws;
      },
    });
  });
  await page.goto("http://127.0.0.1:" + site.address().port + "/?private=1");
  assert.ok(
    await page.evaluate(
      () => typeof window.marginTestCodec?.decode === "function",
    ),
  );
  await unlock(true);
  await edit("only-this-browser-private-capture");
  await page.locator("#collaborate").click();
  await page.locator("#vault-pair-file").setInputFiles(invitation);
  await unlock();
  await page.locator("#notes").getByText(title, { exact: true }).click();
  assert.ok(
    (await page.locator(".cm-content").innerText()).includes("First device."),
  );
  const downloaded = page.waitForEvent("download");
  await page
    .locator("#vault-file-list")
    .getByRole("button", { name: "native-private.bin" })
    .click();
  assert.deepEqual(
    await readFile(await (await downloaded).path()),
    nativeBytes,
  );
  await edit("Browser contribution.", true);
  await edit(" Second browser edit.", true);
  await drained();
  await until(() => wire.checkpoints > 0);
  follower = spawn(binary, [...args, "sync", "--watch"], { stdio: "ignore" });
  const liveText = join(root, "live.md");
  await writeFile(liveText, "\nNative live broadcast.\n");
  await cli("append", room, liveText);
  await page.waitForFunction(() =>
    document
      .querySelector(".cm-content")
      ?.textContent.includes("Native live broadcast."),
  );
  await edit(" Reached the watching native device.", true);

  const webBytes = randomBytes(310123);
  await page.locator("#vault-file-input").setInputFiles({
    name: "browser-private.bin",
    mimeType: "application/octet-stream",
    buffer: webBytes,
  });
  await page
    .locator("#vault-file-list")
    .getByRole("button", { name: "browser-private.bin" })
    .waitFor();
  await drained();
  await until(async () =>
    (await cli("read", room)).includes("Reached the watching native device."),
  );
  assert.deepEqual(wire.subscriptions, [0]);
  assert.ok(wire.received > 0);
  assert.match(await cli("read", room), /Browser contribution/);
  assert.doesNotMatch(await cli("list"), /only-this-browser-private-capture/);
  let line;
  await until(async () => {
    line = (await cli("files", room))
      .split("\n")
      .find((s) => s.endsWith("\tbrowser-private.bin"));
    return line;
  });
  const output = join(root, "decrypted");
  await until(async () => {
    try {
      await cli("extract", room, line.split("\t")[0], output);
      return true;
    } catch (error) {
      if (/missing|incomplete/i.test(error.stderr)) return false;
      throw error;
    }
  });
  assert.deepEqual(await readFile(output), webBytes);

  // Drop a real subscription after its first data fragment, then replay from durable
  // storage. The interrupted record must survive the relay and appear on reconnect.
  const largeSource = join(root, "large.md");
  const largeTitle = "Fragmented recovery diary";
  await writeFile(
    largeSource,
    "# " + largeTitle + "\n" + randomBytes(370000).toString("base64"),
  );
  await page.evaluate(() => {
    window.interruptFragment = true;
  });
  await cli("new", "--synced", "--input", largeSource);
  await page.waitForFunction(() => window.interrupted);
  await page.locator("#notes").getByText(largeTitle, { exact: true }).waitFor();
  await until(() => wire.subscriptions.length >= 2);
  assert.equal(wire.subscriptions.length, 2);
  assert.ok(wire.subscriptions[1] > 0);
  assert.ok(wire.fragmentEvents.length >= 2);
  assert.ok(wire.subscriptions[1] < wire.fragmentEvents[0]);
  // Lose a durable ACK. The browser retries the identical encrypted record.
  const beforeRetry = wire.deltaDigests.length;
  await page.evaluate((room) => {
    window.dropAckRoom = room;
  }, room);
  await edit(" One durable retry.", true);
  await page.waitForFunction(() => window.droppedAck);
  await until(() => wire.subscriptions.length === 3);
  await drained();
  await until(() => {
    const sent = wire.deltaDigests.slice(beforeRetry);
    return new Set(sent).size < sent.length;
  });
  await until(async () =>
    (await cli("read", room)).includes("One durable retry."),
  );
  assert.equal((await cli("read", room)).split("One durable retry.").length, 2);
  assert.equal(wire.pulls, 0);
  assert.deepEqual(wireErrors, []);

  await page.locator("#private-notebook").click();
  await unlock(false, "an incorrect passphrase");
  await page
    .locator("#vault-unlock [role=status]")
    .getByText(/Could not unlock/)
    .waitFor();
  await unlock();
  await page.locator("#notes").getByText(title, { exact: true }).click();
  await page.waitForFunction(() =>
    document
      .querySelector(".cm-content")
      ?.textContent.includes("Browser contribution."),
  );
  const localRows = await page.evaluate(async () => {
    const [info] = (await indexedDB.databases()).filter((db) =>
      db.name.startsWith("margin-private."),
    );
    const db = await new Promise((resolve) => {
      const r = indexedDB.open(info.name);
      r.onsuccess = () => resolve(r.result);
    });
    try {
      const rows = await Promise.all(
        ["settings", "sealed", "blobs", "outbox"].map(
          (store) =>
            new Promise((resolve) => {
              const r = db.transaction(store).objectStore(store).getAll();
              r.onsuccess = () => resolve(r.result);
            }),
        ),
      );
      return JSON.stringify(rows, (_, v) =>
        v instanceof Uint8Array || v instanceof ArrayBuffer
          ? new TextDecoder("latin1").decode(v)
          : v,
      );
    } finally {
      db.close();
    }
  });
  for (const marker of [
    title,
    "Browser contribution.",
    "browser-private.bin",
    "only-this-browser-private-capture",
  ]) {
    assert.ok(!localRows.includes(marker));
    for (const path of [database, database + "-wal"]) {
      const data = await readFile(path).catch((e) => {
        if (e.code === "ENOENT") return Buffer.alloc(0);
        throw e;
      });
      assert.ok(!data.includes(marker));
    }
  }
  // Upgrade both historical browser storage formats at the same origin.
  const old = new Y.Doc(),
    previous = [];
  old.on("update", (bytes) => previous.push(Array.from(bytes)));
  old.getMap("files").set("legacy-note", "Archived Yjs diary");
  old.getText("legacy-note").insert(0, "original Yjs content");
  const oldLoro = new LoroDoc();
  oldLoro.getText("markdown").insert(0, "original Loro content");
  const snapshot = Array.from(oldLoro.export({ mode: "snapshot" }));
  await page.evaluate(
    async ({ previous, snapshot }) => {
      const seed = (name, rows, loro) =>
        new Promise((resolve, reject) => {
          const opening = indexedDB.open(name, 1);
          opening.onupgradeneeded = () => {
            const db = opening.result;
            const updates = db.createObjectStore("updates", {
              autoIncrement: true,
            });
            if (loro) {
              updates.createIndex("note", "id");
              db.createObjectStore("settings").put(true, "initialized");
            }
            for (const row of rows)
              updates.add(
                loro
                  ? { id: "walk", bytes: new Uint8Array(row) }
                  : new Uint8Array(row),
              );
          };
          opening.onsuccess = () => {
            opening.result.close();
            resolve();
          };
          opening.onerror = () => reject(opening.error);
        });
      await seed("margin-md.1///archived-yjs", previous, false);
      await seed("margin-loro.1///archived-loro", [snapshot], true);
    },
    { previous, snapshot },
  );
  await page.goto(
    "http://127.0.0.1:" +
      site.address().port +
      "/?room=archived-yjs#legacy-note",
  );
  await page.getByRole("button", { name: "Archived Yjs diary" }).click();
  assert.ok(
    (await page.locator(".cm-content").innerText()).includes(
      "original Yjs content",
    ),
  );
  const archive = await page.evaluate(async () => {
    const name = "margin-md.1///archived-yjs";
    const get = (r) =>
      new Promise((resolve, reject) => {
        r.onsuccess = () => resolve(r.result);
        r.onerror = () => reject(r.error);
      });
    const db = await get(indexedDB.open(name));
    const remaining = await get(
      db.transaction("updates").objectStore("updates").count(),
    );
    const key = await get(
      db.transaction("settings").objectStore("settings").get("sample-key"),
    );
    const store = db
      .transaction("encrypted-history")
      .objectStore("encrypted-history");
    const [keys, rows] = await Promise.all([
      get(store.getAllKeys()),
      get(store.getAll()),
    ]);
    db.close();
    const restored = await Promise.all(
      rows.map(async (row, i) => {
        const additionalData = new TextEncoder().encode(
          JSON.stringify([
            "Margin browser vault v1",
            name,
            ["Yjs update", keys[i]],
          ]),
        );
        return Array.from(
          new Uint8Array(
            await crypto.subtle.decrypt(
              { name: "AES-GCM", iv: row.iv, additionalData },
              key,
              row.data,
            ),
          ),
        );
      }),
    );
    return { remaining, restored };
  });
  assert.equal(archive.remaining, 0);
  assert.deepEqual(archive.restored, previous);
  await page.goto(
    "http://127.0.0.1:" + site.address().port + "/?room=archived-loro#walk",
  );
  await page.waitForFunction(() =>
    document
      .querySelector(".cm-content")
      ?.textContent.includes("original Loro content"),
  );
  assert.equal(
    await page.evaluate(async () => {
      const db = await new Promise((resolve) => {
        const r = indexedDB.open("margin-loro.1///archived-loro");
        r.onsuccess = () => resolve(r.result);
      });
      const n = await new Promise((resolve) => {
        const r = db.transaction("updates").objectStore("updates").count();
        r.onsuccess = () => resolve(r.result);
      });
      db.close();
      return n;
    }),
    0,
  );
  assert.deepEqual(errors, []);
  console.log(
    "Passed live browser/native %ELO streaming, fragmented reconnect, encrypted checkpoints, multi-chunk attachments, pairing, lock and legacy storage migration.",
  );
} catch (error) {
  console.error({
    subscriptions: wire.subscriptions,
    fragmentEvents: wire.fragmentEvents,
    uploadedDeltas: wire.deltaDigests.length,
  });
  if (page)
    console.error(
      await page
        .locator(
          "#storage,#relay-status,#vault-file-status,#vault-unlock [role=status]",
        )
        .allTextContents(),
    );
  throw error;
} finally {
  await browser?.close();
  follower?.kill();
  relay?.kill();
  site.close();
  await rm(root, { recursive: true, force: true });
}
