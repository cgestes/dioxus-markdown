import { syntaxTree } from "@codemirror/language";
import { insertNewlineContinueMarkupCommand } from "@codemirror/lang-markdown";
import { mode } from "./live-markdown.js";
import { toggleMarker } from "./checkbox.js";

function itemAtLine(state, line) {
  const marker = /^([ \t]*)(?:[-+*]|\d+[.)])([ \t]+)/.exec(line.text);
  if (!marker) return null;
  let node = syntaxTree(state).resolveInner(line.from + marker[1].length + 1, -1);
  while (node && node.name !== "ListItem") node = node.parent;
  return node?.from === line.from + marker[1].length ? node : null;
}

function indentItems(view, outdent) {
  const { state } = view, { doc } = state, sel = state.selection.main;
  const first = doc.lineAt(sel.from);
  const item = itemAtLine(state, first);
  if (!item) return false;
  const parentItem = item.parent?.parent;
  const previous = item.prevSibling;
  if (outdent && parentItem?.name !== "ListItem") return false;
  if (!outdent && previous?.name !== "ListItem") return false;
  const leading = first.text.match(/^[ \t]*/)[0].length;
  const parentIndent = outdent ? parentItem.from - doc.lineAt(parentItem.from).from : 0;
  const width = outdent ? leading - parentIndent :
    doc.lineAt(previous.from).text.trimStart().match(/^(?:[-+*]|\d+[.)])\s+/)[0].length;
  if (width < 1) return false;
  const lastSelected = doc.lineAt(sel.empty ? sel.to : Math.max(sel.from, sel.to - 1)).number;
  let last = Math.max(lastSelected, doc.lineAt(item.to).number);
  const tailItem = itemAtLine(state, doc.line(lastSelected));
  if (tailItem) last = Math.max(last, doc.lineAt(tailItem.to).number);
  const changes = [];
  for (let n = first.number; n <= last; n++) {
    const line = doc.line(n);
    if (!line.length) continue;
    const count = outdent ? Math.min(width, line.text.match(/^[ \t]*/)[0].length) : 0;
    changes.push({ from: line.from, to: line.from + count, insert: outdent ? "" : " ".repeat(width) });
  }
  view.dispatch({ changes, userEvent: "input.list.indent", scrollIntoView: true });
  return true;
}

function toggleTask(view) {
  const line = view.state.doc.lineAt(view.state.selection.main.head);
  if (!itemAtLine(view.state, line)) return false;
  const marker = /^\s*(?:[-+*]|\d+[.)])\s+\[([ xX])\]/.exec(line.text);
  if (!marker) return false;
  const from = line.from + marker[0].length - 2;
  return toggleMarker(view, from - 1);
}

function finishEmptyItem(view) {
  const sel = view.state.selection.main, doc = view.state.doc, line = doc.lineAt(sel.head);
  if (!sel.empty || sel.head !== line.to || !/^\s*(?:[-+*]|\d+[.)])(?:\s+\[[ xX]\])?\s*$/.test(line.text)) return false;
  const item = itemAtLine(view.state, line);
  if (!item || item.parent?.parent?.name === "ListItem") return false;
  // A single newline permits a lazy continuation of the preceding Markdown item.
  const gap = line.number > 1 && doc.line(line.number - 1).text.trim() ? "\n" : "";
  view.dispatch({ changes: { from: line.from, to: line.to, insert: gap },
    selection: { anchor: line.from + gap.length }, userEvent: "input.list.end", scrollIntoView: true });
  return true;
}

function backspaceItem(view) {
  const { state } = view, sel = state.selection.main;
  if (!sel.empty || state.field(mode,false) === "source") return false;
  const line = state.doc.lineAt(sel.head);
  const prefix = line.text.match(/^[ \t]*(?:[-+*]|\d{1,9}[.)])(?:[ \t]+|$)(?:\[[ xX]\](?:[ \t]+|$))?/);
  if (!prefix || sel.head !== line.from + prefix[0].length) return false;
  const item = itemAtLine(state,line);
  if (!item) return false;
  if (item.parent?.parent?.name === "ListItem") return indentItems(view,true);
  view.dispatch({changes:{from:line.from,to:sel.head},selection:{anchor:line.from},userEvent:"delete.backward"});
  return true;
}

export const listKeys = [
  { key: "Backspace", run: backspaceItem },
  { key: "Enter", run: finishEmptyItem },
  { key: "Enter", run: insertNewlineContinueMarkupCommand({ nonTightLists: false }) },
  { key: "Tab", run: v => indentItems(v, false), shift: v => indentItems(v, true) },
  { key: "Mod-Enter", run: toggleTask }
];
