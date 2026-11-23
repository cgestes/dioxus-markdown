import assert from "node:assert/strict";
import { test } from "node:test";
import { EditorState } from "@codemirror/state";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { tableAt } from "../src/table-model.js";
import { deleteColumn, insertColumn, swapColumns, swapRows } from "../src/table-commands.js";
import { rangePaste, rectangle } from "../src/cell-range.js";
import { pasteCells, pasteRectangle } from "../src/table-paste.js";
import { listKeys } from "../src/lists.js";

function documentState(doc) {
  return EditorState.create({ doc, selection: { anchor: doc.length }, extensions: [markdown({ base: markdownLanguage })] });
}

test("removing a column keeps an unbordered GFM table valid, including its escaped pipe", () => {
  const source = "Name | Time\n:--- | ---:\nA\\|B | 30\nCanal | 40\n\nTail\n";
  const state = documentState(source);
  const table = tableAt(state, 1);
  const next = state.update({ changes: deleteColumn(state, table, 1) }).state;
  assert.equal(tableAt(next, 1).rows[0].length, 1);
  assert.equal(next.doc.toString(), "| Name |\n| :--- |\n| A\\|B |\n| Canal |\n\nTail\n");
});

test("moving columns carries the alignment marker and moving rows carries the cells", () => {
  const state = documentState("| Name | Time |\n| :--- | ---: |\n| Quay | 20 |\n| Bridge | 30 |");
  const swapped = state.update({ changes: swapColumns(state, tableAt(state, 1), 0, 1) }).state;
  const table = tableAt(swapped, 1);
  assert.deepEqual(table.align, ["right", "left"]);
  const moved = swapped.update({ changes: swapRows(swapped, table, 1, 2) }).state;
  assert.equal(moved.doc.toString(), "| Time | Name |\n| ---: | :--- |\n| 30 | Bridge |\n| 20 | Quay |");
});

test("a rectangular paste expands a ragged table and one inverse restores the exact file", () => {
  const state = documentState("# Tape\n\n| A | B | C |\n| :- | :-: | -: |\n| left |\n\nOutside **markup**.\n");
  const at = state.doc.toString().indexOf("| A") + 1;
  const table = tableAt(state, at);
  const pasted = pasteRectangle(state, table, 1, 2, "x|y\t2\n🚲\t4\n");
  const after = state.update({ changes: pasted.changes }).state;
  const result = tableAt(after, at);
  assert.equal(result.rows.length, 3);
  assert.equal(result.rows[0].length, 4);
  assert.match(after.doc.toString(), /\| left \|  \| x\\\|y \| 2 \|/);
  assert.match(after.doc.toString(), /\n\nOutside \*\*markup\*\*\.\n$/);
  assert.equal(pasted.changes.invert(state.doc).apply(after.doc).toString(), state.doc.toString());
});

test("inserting a column inside a short row preserves its neighboring Markdown", () => {
  const state = documentState("| A | B | C |\n| --- | --- | --- |\n| *kept* |");
  const after = state.update({ changes: insertColumn(tableAt(state, 1), 1) }).state;
  assert.match(after.doc.toString(), /\| A \| Column 4 \| B \| C \|/);
  assert.match(after.doc.toString(), /\| \*kept\* \|  \|$/);
});

test("a checklist continues unchecked, nests, outdents, and exits into a real paragraph", () => {
  let state = documentState("- [x] Record");
  const command = (key, shift = false) => {
    const view = { state, dispatch: tr => { state = tr.state || state.update(tr).state; } };
    assert.ok(listKeys.some(binding => binding.key === key && (shift ? binding.shift : binding.run)?.(view)));
  };
  command("Enter");
  assert.equal(state.doc.toString(), "- [x] Record\n- [ ] ");
  state = state.update(state.replaceSelection("Listen")).state;
  command("Tab");
  assert.match(state.doc.toString(), /\n  - \[ \] Listen$/);
  command("Tab", true);
  command("Mod-Enter");
  command("Enter");
  command("Enter");
  state = state.update(state.replaceSelection("A separate paragraph.")).state;
  assert.equal(state.doc.toString(), "- [x] Record\n- [x] Listen\n\nA separate paragraph.");
});

test("heading prefix edits keep the caret at the displayed title and leave fenced hashes literal", async () => {
  const { editHeadingPrefix, mode } = await import("../src/live-markdown.js");
  let state = EditorState.create({
    doc: "## Études ##\n\n```md\n## literal\n```",
    selection: { anchor: 3 }, extensions: [markdown({ base: markdownLanguage }), mode]
  });
  const view = { get state() { return state; }, dispatch: tr => { state = state.update(tr).state; } };
  assert.equal(editHeadingPrefix(view, "#"), true);
  assert.equal(editHeadingPrefix(view, " "), true);
  assert.equal(state.doc.line(1).text, "### Études ##");
  assert.equal(state.selection.main.head, 4);
  for (let n = 0; n < 5; n++) editHeadingPrefix(view, "#");
  assert.equal(state.doc.line(1).text, "###### Études ##");
  assert.equal(editHeadingPrefix(view, "", true), true);
  assert.equal(state.doc.line(1).text, "##### Études ##");
  state = state.update({ selection: {anchor: state.doc.line(4).from + 3} }).state;
  assert.equal(editHeadingPrefix(view, "#"), false);
  state = state.update({ selection: {anchor: 7} }).state;
  assert.equal(editHeadingPrefix(view, " "), false);
});

test("a selected column pastes its final empty clipboard cell and keeps its neighbors", () => {
  const md="| A | B |\n| --- | --- |\n| one | keep |\n| two | safe |";
  const state=documentState(md), table=tableAt(state,1), range=rectangle([1,0],[2,0]);
  const edit=pasteCells(state,table,1,0,rangePaste(range,"filled\n\n"));
  assert.equal(state.update({changes:edit.changes}).state.doc.toString(),md.replace("one","filled").replace("two",""));
});
