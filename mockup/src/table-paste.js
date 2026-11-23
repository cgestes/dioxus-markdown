import { ChangeSet, EditorState } from "@codemirror/state";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { ensureSyntaxTree } from "@codemirror/language";
import { tableAt } from "./table-model.js";
import { insertColumn, escapeCell, textChange } from "./table-commands.js";

// Compose expansion and cell writes against a temporary Markdown document.
// The live editor receives one changeset and the undo stack receives one edit.
export function pasteRectangle(state, initial, row, col, text) {
  const records = text.replace(/\r\n?/g, "\n").replace(/\n$/, "").split("\n").map(line => line.split("\t"));
  return pasteCells(state,initial,row,col,records);
}

export function pasteCells(state,initial,row,col,records) {
  const width = records.reduce((n, cells) => Math.max(n, cells.length), 0);
  let draft = EditorState.create({ doc: state.doc, extensions: [markdown({ base: markdownLanguage })] });
  let combined = ChangeSet.empty(draft.doc.length), table = initial;
  function stage(changes) {
    const set = draft.changes(changes);
    combined = combined.compose(set);
    draft = draft.update({ changes: set }).state;
    ensureSyntaxTree(draft, Math.min(draft.doc.length, set.mapPos(table.to, 1) + 1), 1000);
    table = tableAt(draft, initial.from + 1);
  }
  const extraCols = col + width - table.rows[0].length;
  if (extraCols > 0) stage(insertColumn(table, table.rows[0].length, extraCols));
  const extraRows = row + records.length - table.rows.length;
  if (extraRows > 0) {
    const blank = "\n|" + "  |".repeat(table.rows[0].length);
    stage({ from: table.to, insert: blank.repeat(extraRows) });
  }
  const changes = [];
  records.forEach((values, r) => {
    const cells = table.rows[row + r];
    let tail = null;
    values.forEach((value, c) => {
      const cell = cells[col + c], encoded = escapeCell(value);
      if (cell.prefix) {
        if (!tail) tail = { from: cell.from, insert: cell.prefix + encoded };
        else tail.insert += " | " + encoded;
      } else {
        const source = draft.sliceDoc(cell.from, cell.to);
        if (source !== encoded) changes.push(textChange(source, encoded, cell.from));
      }
    });
    if (tail) { tail.insert += " |"; changes.push(tail); }
  });
  return { changes: combined.compose(draft.changes(changes)), rows: records.length, cols: width };
}
