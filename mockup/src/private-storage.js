// The private notebook unlock key lives in memory. Each tab authenticates its own
// records and presence messages. The sample notebook uses an origin-local CryptoKey.
const enc = new TextEncoder();
const dec = new TextDecoder();
export const request = (req) =>
  new Promise((resolve, reject) => {
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
export const complete = (tx) =>
  new Promise((resolve, reject) => {
    tx.oncomplete = resolve;
    tx.onabort = () =>
      reject(tx.error || new Error("Storage transaction aborted"));
  });
const random = (size) => crypto.getRandomValues(new Uint8Array(size));
const keyOps = ["encrypt", "decrypt"];
function aes(key, domain) {
  const aad = (label) =>
    enc.encode(JSON.stringify(["Margin browser vault v1", domain, label]));
  return {
    async seal(label, bytes) {
      const iv = random(12);
      const data = await crypto.subtle.encrypt(
        { name: "AES-GCM", iv, additionalData: aad(label) },
        key,
        bytes,
      );
      return { iv, data };
    },
    async open(label, record) {
      return new Uint8Array(
        await crypto.subtle.decrypt(
          { name: "AES-GCM", iv: record.iv, additionalData: aad(label) },
          key,
          record.data,
        ),
      );
    },
  };
}

async function putOnce(db, name, candidate) {
  const tx = db.transaction("settings", "readwrite"),
    done = complete(tx);
  const store = tx.objectStore("settings"),
    read = store.get(name);
  read.onsuccess = () => {
    if (read.result === undefined) store.add(candidate, name);
  };
  await done;
  return read.result ?? candidate;
}

function askPassword(create, error) {
  const dialog = document.getElementById("vault-unlock"),
    form = dialog.querySelector("form");
  const pass = form.elements.pass,
    confirm = form.elements.confirm;
  dialog.querySelector("h2").textContent = create
    ? "Create private notebook"
    : "Unlock private notebook";
  dialog.querySelector(".vault-confirm").hidden = !create;
  dialog.querySelector("[role=status]").textContent = error;
  pass.value = "";
  confirm.value = "";
  pass.autocomplete = create ? "new-password" : "current-password";
  if (!dialog.open) dialog.showModal();
  pass.focus();
  return new Promise((resolve, reject) => {
    dialog.oncancel = () => reject(new Error("Private notebook is locked"));
    form.onsubmit = (event) => {
      event.preventDefault();
      if (create && (pass.value.length < 12 || pass.value !== confirm.value)) {
        dialog.querySelector("[role=status]").textContent =
          "Use at least 12 characters and enter the same passphrase twice.";
        return;
      }
      const value = pass.value;
      pass.value = "";
      confirm.value = "";
      resolve(value);
    };
  });
}

async function unlock(db, domain, privateMode) {
  const get = (k) =>
    request(db.transaction("settings").objectStore("settings").get(k));
  if (!privateMode) {
    const key =
      (await get("sample-key")) ??
      (await putOnce(
        db,
        "sample-key",
        await crypto.subtle.generateKey(
          { name: "AES-GCM", length: 256 },
          false,
          keyOps,
        ),
      ));
    return aes(key, domain);
  }
  // Installing the salt atomically also handles two first-run tabs.
  const salt = (await get("salt")) ?? (await putOnce(db, "salt", random(16)));
  let check = await get("check"),
    error = "";
  for (;;) {
    const password = await askPassword(!check, error);
    const material = await crypto.subtle.importKey(
      "raw",
      enc.encode(password),
      "PBKDF2",
      false,
      ["deriveKey"],
    );
    const key = await crypto.subtle.deriveKey(
      { name: "PBKDF2", hash: "SHA-256", iterations: 600000, salt },
      material,
      { name: "AES-GCM", length: 256 },
      false,
      keyOps,
    );
    const cipher = aes(key, domain);
    if (!check)
      check = await putOnce(
        db,
        "check",
        await cipher.seal("unlock", enc.encode("Margin private notebook")),
      );
    try {
      if (
        dec.decode(await cipher.open("unlock", check)) !==
        "Margin private notebook"
      )
        throw new Error();
      document.getElementById("vault-unlock").close();
      return cipher;
    } catch {
      error = "Could not unlock this notebook. Try the passphrase again.";
    }
  }
}

// Keep exact legacy Yjs updates, including deleted text and earlier versions.
// Each archived row and the removal of its plaintext commit in one transaction.
async function archivePreviousNotebook(name, restore, report) {
  const domain = "margin-md.1/" + name;
  const probe = indexedDB.open(domain);
  probe.onupgradeneeded = () => probe.transaction.abort();
  let db;
  try {
    db = await request(probe);
  } catch (error) {
    if (error.name === "AbortError") return [];
    throw error;
  }
  try {
    if (!db.objectStoreNames.contains("updates")) return [];
    if (!db.objectStoreNames.contains("encrypted-history")) {
      const version = db.version + 1;
      db.close();
      const upgrade = indexedDB.open(domain, version);
      upgrade.onblocked = () =>
        report("Close older notebook tabs to protect their history");
      upgrade.onupgradeneeded = () => {
        const legacy = upgrade.result;
        legacy.createObjectStore("encrypted-history");
        if (!legacy.objectStoreNames.contains("settings"))
          legacy.createObjectStore("settings");
      };
      db = await request(upgrade);
    }
    db.onversionchange = () => db.close();
    const keys = await request(
      db.transaction("updates").objectStore("updates").getAllKeys(),
    );
    if (!restore && !keys.length) return [];
    const cipher = await unlock(db, domain, false);
    const archive = "encrypted-history";
    for (const id of keys) {
      const raw = await request(
        db.transaction("updates").objectStore("updates").get(id),
      );
      if (raw === undefined) continue; // A second tab completed this row.
      const bytes = new Uint8Array(raw);
      const label = ["Yjs update", id];
      const row = await cipher.seal(label, bytes);
      if (!equalBytes(bytes, await cipher.open(label, row)))
        throw new Error("History verification failed");
      const tx = db.transaction(["updates", archive], "readwrite");
      const done = complete(tx),
        original = tx.objectStore("updates");
      const current = original.get(id);
      current.onsuccess = () => {
        if (current.result === undefined) return;
        if (!equalBytes(bytes, new Uint8Array(current.result))) {
          tx.abort();
          return;
        }
        tx.objectStore(archive).add(row, id);
        original.delete(id);
      };
      await done;
    }
    if (!restore) return [];
    const read = db.transaction(archive).objectStore(archive);
    const [ids, rows] = await Promise.all([
      request(read.getAllKeys()),
      request(read.getAll()),
    ]);
    return Promise.all(
      rows.map((row, i) => cipher.open(["Yjs update", ids[i]], row)),
    );
  } finally {
    db.close();
  }
}

function equalBytes(a, b) {
  return a.length === b.length && a.every((value, i) => value === b[i]);
}

export async function storage(
  name,
  report,
  privateMode,
  previousNotebook,
  seed,
) {
  if (!crypto.subtle)
    throw new Error("A secure origin is required for encrypted storage.");
  const dbName = (privateMode ? "margin-private.1/" : "margin-loro.1/") + name;
  const open = indexedDB.open(dbName, 3);
  open.onblocked = () => report("Close older notebook tabs to upgrade storage");
  open.onupgradeneeded = () => {
    const db = open.result;
    if (!db.objectStoreNames.contains("settings"))
      db.createObjectStore("settings");
    if (!db.objectStoreNames.contains("sealed"))
      db.createObjectStore("sealed", { autoIncrement: true }).createIndex(
        "note",
        "id",
      );
    if (!db.objectStoreNames.contains("outbox"))
      db.createObjectStore("outbox", { autoIncrement: true });
    if (!db.objectStoreNames.contains("blobs")) db.createObjectStore("blobs");
  };
  const db = await request(open);
  db.onversionchange = () => db.close();
  const cipher = await unlock(db, dbName, privateMode);
  const sealedRow = async (id, bytes) => ({
    id,
    ...(await cipher.seal(["document", id], bytes)),
  });
  const readRow = async (row) => ({
    id: row.id,
    bytes: await cipher.open(["document", row.id], row),
  });
  const installed = await request(
    db.transaction("settings").objectStore("settings").get("encrypted"),
  );
  const hasOld = db.objectStoreNames.contains("updates");
  const initialized =
    hasOld &&
    (await request(
      db.transaction("settings").objectStore("settings").get("initialized"),
    ));
  const importLegacy = !privateMode && !installed && !initialized;
  const legacy = privateMode
    ? []
    : await archivePreviousNotebook(name, importLegacy, report);
  if (!installed) {
    const original = hasOld
      ? await request(db.transaction("updates").objectStore("updates").getAll())
      : [];
    const rows =
      importLegacy && !original.length
        ? await previousNotebook(legacy)
        : original;
    const sealed = await Promise.all(
      rows.map(async (row) => {
        const s = await sealedRow(row.id, row.bytes),
          verified = await readRow(s);
        if (!equalBytes(verified.bytes, row.bytes))
          throw new Error("Migration verification failed");
        return s;
      }),
    );
    // The version change excludes v1 writers. Verification, installation,
    // and retiring the old Loro rows are crash-safe across racing unlocks.
    const tx = db.transaction(
      hasOld ? ["settings", "sealed", "updates"] : ["settings", "sealed"],
      "readwrite",
    );
    const done = complete(tx),
      settings = tx.objectStore("settings"),
      check = settings.get("encrypted");
    check.onsuccess = () => {
      if (check.result) return;
      for (const row of sealed) tx.objectStore("sealed").add(row);
      if (hasOld) tx.objectStore("updates").clear();
      settings.put(true, "encrypted");
    };
    await done;
  }
  const diskRows = await request(
    db.transaction("sealed").objectStore("sealed").getAll(),
  );
  const rows = await Promise.all(diskRows.map(readRow));
  const counts = new Map();
  for (const row of rows) counts.set(row.id, 1 + (counts.get(row.id) || 0));
  async function compact(id) {
    const tx = db.transaction("sealed"),
      idx = tx.objectStore("sealed").index("note");
    const [keys, records] = await Promise.all([
      request(idx.getAllKeys(id)),
      request(idx.getAll(id)),
    ]);
    const all = await Promise.all(records.map(readRow)),
      merged = seed(id);
    try {
      const status = merged.importBatch(all.map((r) => r.bytes));
      const awaiting = merged.exportPendingUpdates();
      if (merged.isShallow() || (status.pending?.size && !awaiting)) return;
      const row = await sealedRow(id, merged.export({ mode: "snapshot" }));
      const pendingRow = awaiting && await sealedRow(id, awaiting);
      const tx = db.transaction("sealed", "readwrite"),
        done = complete(tx),
        store = tx.objectStore("sealed");
      for (const key of keys) store.delete(key);
      store.add(row);
      if (pendingRow) store.add(pendingRow);
      await done;
    } finally {
      merged.free();
    }
  }
  const saved = privateMode
    ? "Encrypted on this device"
    : "Saved in this browser";
  const pending = [];
  let saving;
  async function commitEdit({ id, bytes, makePacket, fileJob }) {
    const row = await sealedRow(id, bytes);
    const packet = makePacket && (await makePacket(bytes));
    const tx = db.transaction(
      packet ? ["sealed", "outbox"] : ["sealed"],
      "readwrite",
    );
    const done = complete(tx);
    tx.objectStore("sealed").add(row);
    if (packet) {
      if (fileJob) tx.objectStore("outbox").add(fileJob);
      tx.objectStore("outbox").add(packet);
    }
    await done;
  }

  function drain() {
    if (saving) return saving;
    let retryScheduled = false;
    saving = (async () => {
      while (pending.length) {
        const edit = pending[0];
        await commitEdit(edit);
        pending.shift();
        edit.resolve();
        const n = 1 + (counts.get(edit.id) || 0);
        counts.set(edit.id, n);
        // Compaction is optional once the original row is durable.
        if (n > 150)
          await compact(edit.id).then(
            () => counts.set(edit.id, 1),
            () => {},
          );
      }
      report(saved);
    })()
      .catch((error) => {
        report("Save failed · keep this tab open");
        retryScheduled = true;
        setTimeout(() => {
          if (pending.length) void drain().catch(() => {});
        }, 3000);
        throw error;
      })
      .finally(() => {
        saving = undefined;
        if (pending.length && !retryScheduled)
          queueMicrotask(() => {
            void drain().catch(() => {});
          });
      });
    return saving;
  }

  function save(id, bytes, makePacket, fileJob) {
    report("Saving…");
    const accepted = new Promise((resolve) =>
      pending.push({ id, bytes: bytes.slice(), makePacket, fileJob, resolve }),
    );
    void drain().catch(() => {});
    return accepted;
  }
  report(saved);
  return {
    rows,
    save,
    cipher,
    db,
    sealedRow,
    readRow,
    flush: async () => {
      while (pending.length || saving) await (saving || drain());
    },
  };
}

export function encryptedChannel(name, cipher, onError) {
  const wire = new BroadcastChannel(name);
  const channel = { onmessage: null };
  let incoming = Promise.resolve(),
    outgoing = Promise.resolve();
  channel.postMessage = (message) => {
    const { bytes, update, ...fields } = message;
    const data = (bytes || update)?.slice();
    const head = enc.encode(
      JSON.stringify({
        ...fields,
        payload: bytes ? "bytes" : update ? "update" : null,
      }),
    );
    const packed = new Uint8Array(4 + head.length + (data?.length || 0));
    new DataView(packed.buffer).setUint32(0, head.length);
    packed.set(head, 4);
    if (data) packed.set(data, 4 + head.length);
    outgoing = outgoing
      .then(async () =>
        wire.postMessage(await cipher.seal("tab channel", packed)),
      )
      .catch(onError);
  };
  wire.onmessage = ({ data }) => {
    incoming = incoming
      .then(async () => {
        const packed = await cipher.open("tab channel", data);
        const n = new DataView(packed.buffer, packed.byteOffset).getUint32(0);
        const { payload, ...msg } = JSON.parse(
          dec.decode(packed.slice(4, 4 + n)),
        );
        if (payload === "bytes" || payload === "update")
          msg[payload] = packed.slice(4 + n);
        channel.onmessage?.({ data: msg });
      })
      .catch(onError);
  };
  return channel;
}
