import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import test from "node:test";
import {
  CrdtType,
  MAX_MESSAGE_SIZE,
  MessageType,
  decode,
  decodeEloContainer,
  encode,
  encryptDeltaSpan,
  parseEloRecordHeader,
} from "loro-protocol";
import {
  b64,
  decodeFrame,
  documentReader,
  sendPacket,
} from "../src/vault-stream.js";

const room = "ab".repeat(16);
async function frames(size) {
  const { record } = await encryptDeltaSpan(
    randomBytes(size),
    {
      peerId: Uint8Array.of(7),
      start: 0,
      end: 1,
      keyId: "1",
    },
    randomBytes(32),
  );
  const sent = [];
  const id = sendPacket(
    { send: (f) => sent.push(f) },
    { room, part: -1, signature: "test", data: b64(record) },
  );
  const meta = JSON.parse(sent.shift());
  assert.deepEqual(meta, {
    type: "push",
    packet: { room, part: -1, signature: "test" },
  });
  const messages = sent.map((f) => {
    assert.ok(f.length <= MAX_MESSAGE_SIZE);
    const m = decode(f);
    assert.equal(m.roomId, room);
    assert.equal(m.crdt, CrdtType.Elo);
    assert.equal(m.batchId, id);
    return m;
  });
  return { record, messages, sent };
}

test("the upstream codec reads small and fragmented encrypted document uploads", async () => {
  for (const size of [80, 700000]) {
    const { record, messages, sent } = await frames(size);
    let received;
    if (messages[0].type === MessageType.DocUpdate) {
      received = messages[0].updates[0];
    } else {
      const [header, ...fragments] = messages;
      assert.equal(header.type, MessageType.DocUpdateFragmentHeader);
      assert.equal(fragments.length, header.fragmentCount);
      const body = Buffer.concat(fragments.map((f) => f.fragment));
      assert.equal(body.length, header.totalSizeBytes);
      [received] = decodeEloContainer(body);
    }
    assert.equal(parseEloRecordHeader(received).keyId, "1");
    assert.deepEqual(Buffer.from(received), Buffer.from(record));
    const read = documentReader(room);
    for (const f of sent.slice(0, -1)) assert.equal(read(decodeFrame(f)), null);
    assert.deepEqual(
      Buffer.from(read(decodeFrame(sent.at(-1)))),
      Buffer.from(record),
    );
  }
});

test("a signed document waits for every fragment of its own room and batch", async () => {
  const {
    messages: [header, first, second],
  } = await frames(700000);
  const reader = () => {
    const read = documentReader(room);
    assert.equal(read(header), null);
    return read;
  };
  assert.throws(() => reader()(second), /fragment/);
  assert.throws(() => reader()({ ...first, roomId: "cd".repeat(16) }), /room/);
  assert.throws(
    () => reader()({ ...first, batchId: "0x0123456789abcdef" }),
    /fragment/,
  );
  assert.throws(
    () => documentReader(room)({ ...header, totalSizeBytes: 100000000 }),
    /bounds/,
  );
  assert.throws(
    () =>
      decodeFrame(
        encode({
          crdt: CrdtType.Elo,
          roomId: room,
          batchId: header.batchId,
          type: MessageType.DocUpdate,
          updates: [new Uint8Array(8), new Uint8Array(8)],
        }),
      ),
    /one signed record/,
  );
});
