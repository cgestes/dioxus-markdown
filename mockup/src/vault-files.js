// Immutable, libsodium-compatible secretstreams. Only the encrypted document
// contains the original filename and the randomly generated file key.
import sodium from "libsodium-wrappers";
import { complete, request } from "./private-storage.js";

const block = 128 * 1024;
const utf8 = new TextEncoder();
// The JS wrapper allocates the secretstream state in libsodium's WASM heap.
// Its push and pull calls retain that allocation even after a final tag.
function releaseStream(address) {
  const ffi = sodium.libsodium;
  const size = ffi._crypto_secretstream_xchacha20poly1305_statebytes();
  sodium.memzero(ffi.HEAPU8.subarray(address, address + size));
  ffi._free(address);
}

async function storePart(db, id, part, data) {
  const tx = db.transaction("blobs", "readwrite"),
    done = complete(tx);
  tx.objectStore("blobs").add(data, [id, part]);
  await done;
}
export async function stageFile(db, file) {
  await sodium.ready;
  const id = crypto.randomUUID().replaceAll("-", "");
  const key = sodium.crypto_secretstream_xchacha20poly1305_keygen();
  const { state, header } =
    sodium.crypto_secretstream_xchacha20poly1305_init_push(key);
  const ad = utf8.encode(id);
  let parts = 0;
  try {
    await storePart(db, id, 0, header);
    for (let at = 0; at < file.size; at += block) {
      const bytes = new Uint8Array(
        await file.slice(at, at + block).arrayBuffer(),
      );
      const data = sodium.crypto_secretstream_xchacha20poly1305_push(
        state,
        bytes,
        ad,
        sodium.crypto_secretstream_xchacha20poly1305_TAG_MESSAGE,
      );
      await storePart(db, id, ++parts, data);
    }
    const end = sodium.crypto_secretstream_xchacha20poly1305_push(
      state,
      new Uint8Array(),
      ad,
      sodium.crypto_secretstream_xchacha20poly1305_TAG_FINAL,
    );
    await storePart(db, id, ++parts, end);
    return {
      id,
      manifest: {
        name: file.name,
        key: sodium.to_base64(key, sodium.base64_variants.URLSAFE_NO_PADDING),
        parts,
        size: file.size,
      },
    };
  } catch (e) {
    const tx = db.transaction("blobs", "readwrite"),
      done = complete(tx);
    tx.objectStore("blobs").delete(
      IDBKeyRange.bound([id, 0], [id, Number.MAX_SAFE_INTEGER]),
    );
    await done;
    throw e;
  } finally {
    releaseStream(state);
    sodium.memzero(key);
  }
}

export async function exportFile(db, id, manifest, handle = null) {
  await sodium.ready;
  const part = (n) =>
    request(db.transaction("blobs").objectStore("blobs").get([id, n]));
  const header = await part(0);
  if (!header) throw new Error("File is still syncing");
  const key = sodium.from_base64(
    manifest.key,
    sodium.base64_variants.URLSAFE_NO_PADDING,
  );
  const state = sodium.crypto_secretstream_xchacha20poly1305_init_pull(
    header,
    key,
  );
  sodium.memzero(key);
  let dest;
  const chunks = [],
    ad = utf8.encode(id);
  let size = 0;
  try {
    dest = await handle?.createWritable();
    if (!Number.isSafeInteger(manifest.parts) || manifest.parts < 1)
      throw new Error("Invalid file manifest");
    for (let n = 1; n <= manifest.parts; n++) {
      const cipher = await part(n);
      if (!cipher) throw new Error("File is still syncing");
      const msg = sodium.crypto_secretstream_xchacha20poly1305_pull(
        state,
        cipher,
        ad,
      );
      const tag =
        n === manifest.parts
          ? sodium.crypto_secretstream_xchacha20poly1305_TAG_FINAL
          : sodium.crypto_secretstream_xchacha20poly1305_TAG_MESSAGE;
      if (!msg || msg.tag !== tag)
        throw new Error("File authentication failed");
      size += msg.message.length;
      if (dest) await dest.write(msg.message);
      else chunks.push(msg.message);
    }
    if (size !== manifest.size) throw new Error("Incomplete file");
    if (dest) {
      await dest.close();
      return;
    }
    return new Blob(chunks);
  } catch (e) {
    if (dest) await dest.abort();
    throw e;
  } finally {
    releaseStream(state);
  }
}
