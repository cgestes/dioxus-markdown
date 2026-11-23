import assert from "node:assert/strict";
import { test } from "node:test";
import { LoroDoc } from "loro-crdt/base64";
import { checkedStyle, equalCursor, locate, markdownText, remember } from "../src/loro-editor.js";

test("two authors can mark the same Markdown glyph while a UTF-16 caret follows independent edits", () => {
  const a = new LoroDoc();
  a.configTextStyle({ [checkedStyle]: { expand: "none" } });
  const md = "🎧 Ready [ ] now";
  const ta = a.getText("markdown");
  ta.insert(0, md);
  a.commit();
  const b = a.fork(), tb = b.getText("markdown");
  b.configTextStyle({ [checkedStyle]: { expand: "none" } });
  const at = md.indexOf("[") + 1, cursor = remember(ta, at), version = a.oplogVersion();
  assert.ok(equalCursor(cursor, remember(tb, at)));
  for (const t of [ta, tb]) t.mark({ start: at, end: at + 1 }, checkedStyle, true);
  tb.insert(0, "é ");
  a.commit(); b.commit();
  const au = a.export({ mode: "update", from: version }), bu = b.export({ mode: "update", from: version });
  a.import(bu); b.import(au); b.import(au);
  assert.equal(markdownText(ta), "é 🎧 Ready [x] now");
  assert.equal(markdownText(tb), markdownText(ta));
  assert.equal(locate(a, cursor), at + 2);
  tb.splice(at + 2, 1, "literal");
  b.commit();
  assert.equal(markdownText(tb), "é 🎧 Ready [literal] now");
});
