// Margin signs one encrypted record at a time. Its signature travels in the
// preceding text frame so the binary document frames use the upstream codec.
import {
  BytesReader,
  CrdtType,
  MAX_MESSAGE_SIZE,
  MessageType,
  bytesToHex,
  decode,
  decodeEloContainer,
  encode,
  encodeEloContainer,
} from "loro-protocol";

const maxRecord = 16 * 1024 * 1024;
const fragmentBytes = MAX_MESSAGE_SIZE - 256;

function requireValue(ok, message) {
  if (!ok) throw new Error(message);
}

export const bytes = (s) =>
  Uint8Array.from(atob(s.replaceAll("-", "+").replaceAll("_", "/")), (c) =>
    c.charCodeAt(0),
  );

export function b64(b) {
  let str = "";
  for (let i = 0; i < b.length; i += 8192)
    str += String.fromCharCode(...b.subarray(i, i + 8192));
  return btoa(str)
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replaceAll("=", "");
}

export function sendPacket(ws, packet) {
  if (packet.part !== -1) {
    ws.send(JSON.stringify({ type: "push", packet }));
    return null;
  }
  const { data, ...signed } = packet;
  const record = bytes(data);
  requireValue(
    record.length > 0 && record.length <= maxRecord,
    "Invalid document record size",
  );
  const batchId = bytesToHex(crypto.getRandomValues(new Uint8Array(8)));
  const envelope = { crdt: CrdtType.Elo, roomId: packet.room, batchId };
  ws.send(JSON.stringify({ type: "push", packet: signed }));
  if (record.length <= fragmentBytes) {
    ws.send(
      encode({ ...envelope, type: MessageType.DocUpdate, updates: [record] }),
    );
  } else {
    const payload = encodeEloContainer([record]);
    const fragmentCount = Math.ceil(payload.length / fragmentBytes);
    ws.send(
      encode({
        ...envelope,
        type: MessageType.DocUpdateFragmentHeader,
        fragmentCount,
        totalSizeBytes: payload.length,
      }),
    );
    for (let index = 0; index < fragmentCount; index++) {
      const fragment = payload.subarray(
        index * fragmentBytes,
        (index + 1) * fragmentBytes,
      );
      ws.send(
        encode({
          ...envelope,
          type: MessageType.DocUpdateFragment,
          index,
          fragment,
        }),
      );
    }
  }
  return batchId;
}

export function decodeFrame(data) {
  const frame = new Uint8Array(data);
  requireValue(frame.length <= MAX_MESSAGE_SIZE, "Oversized Loro frame");
  const r = new BytesReader(frame);
  requireValue(
    r.readBytes(4).every((b, i) => b === "%ELO".charCodeAt(i)),
    "Expected %ELO frame",
  );
  requireValue(/^[0-9a-f]{32}$/.test(r.readVarString()), "Invalid Loro room");
  if (r.readByte() === MessageType.DocUpdate)
    requireValue(r.readUleb128() === 1, "Expected one signed record");
  return decode(frame);
}

// WebSocket preserves fragment order. Each signed header starts a fresh reader.
export function documentReader(room) {
  let batch,
    buf,
    offset = 0,
    next = 0,
    count = 0;
  return (message) => {
    requireValue(message.roomId === room, "Document room mismatch");
    if (message.type === MessageType.DocUpdate && !buf) {
      const [record] = message.updates;
      requireValue(record?.length > 0, "Empty document record");
      return record;
    }
    if (message.type === MessageType.DocUpdateFragmentHeader && !buf) {
      const { totalSizeBytes, fragmentCount } = message;
      requireValue(
        totalSizeBytes > 0 &&
          totalSizeBytes <= maxRecord + 5 &&
          fragmentCount > 0 &&
          fragmentCount <= Math.min(4096, totalSizeBytes),
        "Invalid Loro fragment bounds",
      );
      buf = new Uint8Array(totalSizeBytes);
      batch = message.batchId;
      count = fragmentCount;
      return null;
    }
    requireValue(
      buf &&
        message.type === MessageType.DocUpdateFragment &&
        message.batchId === batch &&
        message.index === next &&
        message.fragment.length > 0 &&
        offset + message.fragment.length <= buf.length,
      "Invalid Loro fragment",
    );
    buf.set(message.fragment, offset);
    offset += message.fragment.length;
    if (++next < count) return null;
    requireValue(
      offset === buf.length && buf[0] === 1,
      "Incomplete Loro container",
    );
    const [record] = decodeEloContainer(buf);
    requireValue(
      record?.length > 0 && record.length <= maxRecord,
      "Invalid document record size",
    );
    return record;
  };
}
