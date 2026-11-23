import { dragIntoGrid } from "./grid-pointer.js";
import { readTable, tableAt } from "./table-model.js";
import { GridWidget, extendFromCell, gridSession, pipeTable, activateCell, focusCellAt, setGrid } from "./grid.js";
import { Compartment, EditorSelection, EditorState, StateEffect, StateField } from "@codemirror/state";
import { Decoration, EditorView, WidgetType, highlightActiveLine, lineNumbers } from "@codemirror/view";
import { HighlightStyle, syntaxHighlighting, syntaxTree } from "@codemirror/language";
import { tags } from "@lezer/highlight";
import { agentState } from "./inline-agent.js";
import { literalMarker, markersIn } from "./checkbox.js";
import { button, el, icon } from "./ui.js";

const switchMode = StateEffect.define();
const targetCell = StateEffect.define();
const viewStyle = new Compartment();
export const mode = StateField.define({
  create: () => "live",
  update: (value, tr) => tr.effects.find(e => e.is(switchMode))?.value ?? value
});
// GFM pads short rows visually. Create the missing pipes on the first
// keystroke so merely visiting an implicit cell leaves the file untouched.
const pendingCell = StateField.define({
  create: () => null,
  update(value, tr) {
    const effect = tr.effects.find(e => e.is(targetCell));
    if (effect) return effect.value;
    if (!value || tr.selection) return null;
    if (!tr.docChanged) return value;
    const at = tr.changes.mapPos(value.at, -1);
    return tr.newDoc.lineAt(at).text === value.row ? { ...value, at } : null;
  }
});
const completeCell = EditorState.transactionFilter.of(tr => {
  const cell = tr.startState.field(pendingCell);
  if (!cell || !tr.docChanged || !tr.isUserEvent("input")) return tr;
  let added;
  tr.changes.iterChanges((from, to, _fromB, _toB, insert) => {
    if (from === cell.at && to === cell.at && insert.lines === 1) added = insert.length;
  });
  if (!added) return tr;
  return [tr, { sequential: true,
    changes: [{ from: cell.at, insert: cell.prefix }, { from: cell.at + added, insert: " |" }],
    selection: { anchor: cell.at + cell.prefix.length + added }, effects: targetCell.of(null) }];
});

// The caret may sit just outside a block widget at a file boundary. Preserve
// the table on the first edit there, without changing the note on navigation.
const tableEdgeInput = EditorState.transactionFilter.of(tr => {
  const state = tr.startState, caret = state.selection.main;
  if (!tr.docChanged || !caret.empty || state.field(mode) !== "live" || state.field(gridSession)
      || state.field(pipeTable) != null || state.field(draftingTable) != null
      || !(tr.isUserEvent("input.type") || tr.isUserEvent("input.paste"))) return tr;
  const pos = caret.head, doc = state.doc;
  if (pos !== 0 && pos !== doc.length) return tr;
  let inserted;
  tr.changes.iterChanges((from,to,_a,_b,text) => { if (from === pos && to === pos && text.length) inserted = text.toString(); });
  if (!inserted) return tr;
  const line = doc.lineAt(pos);
  const probe = pos && !line.length && line.number > 1 ? doc.line(line.number - 1).to : pos;
  const table = tableAt(state,probe);
  if (!table) return tr;
  if (pos === doc.length && pos >= table.to) {
    const separation = doc.sliceString(table.to,pos) + (inserted.match(/^\n*/)?.[0] || "");
    if (separation.length < 2) return [tr,{ sequential:true, changes:{from:pos,insert:"\n".repeat(2-separation.length)} }];
  } else if (pos === 0 && table.from === 0) {
    const n = inserted.match(/\n*$/)[0].length;
    if (n < 2) return [tr,{ sequential:true, changes:{from:inserted.length,insert:"\n".repeat(2-n)}, selection:tr.newSelection }];
  }
  return tr;
});

// A single dash can complete a GFM delimiter. Keep that line writable until
// the author finishes typing its alignment and chooses to enter the grid.
const draftingTable = StateField.define({
  create: () => null,
  update(value, tr) {
    if (tr.effects.some(e => e.is(setGrid) || e.is(switchMode))) return null;
    if (!tr.docChanged && !tr.selection) return value;
    if (tr.state.field(mode) !== "live" || tr.startState.field(gridSession)) return null;
    const head = tr.newSelection.main.head, line = tr.newDoc.lineAt(head);
    if (!/^[|: \t-]+$/.test(line.text)) return null;
    const table = tableAt(tr.state, head);
    if (!table || line.number !== tr.newDoc.lineAt(table.from).number + 1) return null;
    if (value != null && tr.changes.mapPos(value, -1) === table.from) return table.from;
    return tr.isUserEvent("input.type") && !tableAt(tr.startState, tr.startState.selection.main.head) ? table.from : null;
  }
});

function finishTableLine(view, enter) {
  const from = view.state.field(draftingTable);
  if (from == null) return false;
  let table = tableAt(view.state, from);
  if (!table) return false;
  if (enter && table.rows.length === 1) {
    const insert = "\n|" + "  |".repeat(table.rows[0].length);
    view.dispatch({ changes: { from: table.to, insert }, userEvent: "input.table" });
    table = tableAt(view.state, from);
  }
  activateCell(view, table, enter ? 1 : 0, 0);
  return true;
}

const sourceStyle = [
  EditorView.editorAttributes.of({ class: "source-mode" }), lineNumbers(), highlightActiveLine(),
  syntaxHighlighting(HighlightStyle.define([
    { tag: [tags.heading, tags.processingInstruction], color: "var(--accent-deep)" },
    { tag: tags.strong, fontWeight: "600" }, { tag: tags.emphasis, fontStyle: "italic" },
    { tag: [tags.url, tags.link], color: "var(--accent)" }, { tag: tags.monospace, color: "var(--muted)" }
  ]))
];
// The gutter is ink only. All editing takes place at the first visible letter.
function headingPrefix(state, pos) {
  const line = state.doc.lineAt(pos);
  const match = /^( {0,3})(#{1,6})([ \t]|$)/.exec(line.text);
  if (!match) return null;
  const from = line.from + match[1].length;
  if (syntaxTree(state).resolveInner(from + 1, -1).parent?.name?.startsWith("ATXHeading") !== true) return null;
  return { from, hashes: match[2].length, end: from + match[2].length + match[3].length };
}

export function editHeadingPrefix(view, text, backspace = false) {
  const { state } = view, sel = state.selection.main;
  if (state.field(mode) !== "live" || !sel.empty || state.selection.ranges.length !== 1) return false;
  const prefix = headingPrefix(state, sel.head);
  if (!prefix || sel.head < prefix.from || sel.head > prefix.end) return false;
  const { from, hashes, end } = prefix;
  let next, body = "";
  if (backspace) next = hashes - 1;
  else if (text === " ") next = hashes;
  else if (/^#+$/.test(text)) next = Math.min(6, hashes + text.length);
  else if (/^#{1,6} [^\r\n]*$/.test(text)) {
    next = text.indexOf(" ");
    body = text.slice(next + 1);
  }
  else return false;
  const insert = next ? "#".repeat(next) + (state.sliceDoc(from + hashes, end) || " ") + body : "";
  view.dispatch({ changes: state.sliceDoc(from, end) === insert ? undefined : { from, to: end, insert },
    selection: { anchor: from + insert.length }, userEvent: backspace ? "delete.backward" : "input.type" });
  return true;
}

const titleCaret = EditorState.transactionFilter.of(tr => {
  if (!tr.selection && !tr.effects.some(e => e.is(switchMode))) return tr;
  const sel = tr.newSelection.main;
  if (!sel.empty || tr.newSelection.ranges.length !== 1 || tr.state.field(mode) !== "live") return tr;
  const prefix = headingPrefix(tr.state, sel.head);
  if (!prefix || sel.head < prefix.from || sel.head >= prefix.end) return tr;
  return [tr, { sequential: true, selection: { anchor: prefix.end } }];
});

function leftFromHeading(view) {
  const state = view.state, sel = state.selection.main;
  if (state.field(mode) !== "live" || !sel.empty) return false;
  const prefix = headingPrefix(state, sel.head);
  if (!prefix || sel.head !== prefix.end) return false;
  const line = state.doc.lineAt(sel.head);
  if (line.number > 1) view.dispatch({ selection: { anchor: state.doc.line(line.number - 1).to }, scrollIntoView: true });
  return true;
}

const headingInput = EditorView.inputHandler.of((view, from, to, text) =>
  from === to && !view.composing && editHeadingPrefix(view, text));
const headingAtoms = EditorView.atomicRanges.of(view => {
  if (view.state.field(mode) !== "live") return Decoration.none;
  const ranges = [];
  syntaxTree(view.state).iterate({ enter(n) {
    if (n.name !== "HeaderMark" || n.node.parent?.firstChild?.from !== n.from) return;
    const prefix = headingPrefix(view.state, n.from);
    if (prefix) ranges.push(Decoration.replace({}).range(prefix.from, prefix.end));
  } });
  return Decoration.set(ranges);
});
export function setMode(view, value) {
  view.dispatch({ effects: [switchMode.of(value), setGrid.of(null), viewStyle.reconfigure(value !== "live" ? sourceStyle : [])] });
}

export function selectedTableCell(state, table) {
  const selection = state.selection.main, pending = state.field(pendingCell);
  for (const [r, row] of table.rows.entries()) {
    const c = row.findIndex(cell => pending
      ? cell.from === pending.at && cell.prefix === pending.prefix
      : !cell.prefix && selection.from >= cell.from && selection.to <= cell.to);
    if (c >= 0) return { table, r, c };
  }
  return null;
}

export function selectCell(view, cell, anchor = cell.from, head = cell.to) {
  const pending = cell.prefix ? { at: cell.from, prefix: cell.prefix, row: view.state.doc.lineAt(cell.from).text } : null;
  view.dispatch({ selection: EditorSelection.single(anchor, head),
    effects: targetCell.of(pending), scrollIntoView: true });
  view.focus();
}

class TableTools extends WidgetType {
  eq() { return true; }
  toDOM(view) {
    const tools = el("div", "table-tools");
    const close = button("Return to grid", "", () => focusCellAt(view, view.state.selection.main.head));
    close.prepend(icon("table-2"));
    close.append(el("kbd", "", "Esc"));
    tools.append(el("span", "", "Tab or Shift+Tab between cells"), close);
    return tools;
  }
  ignoreEvent() { return true; }
}

function decorate(state) {
  if (state.field(mode) !== "live") return Decoration.none;
  const ranges = [];
  const doc = state.doc, selections = state.selection.ranges;
  const agent = state.field(agentState);
  const checkboxes = markersIn(state);
  const literal = state.field(literalMarker);
  const caretAt = (from, to) => selections.some(sel => sel.empty && sel.head >= from && sel.head <= to);
  const mark = (from, to, cls) => { if (from < to) ranges.push(Decoration.mark({ class: cls }).range(from, to)); };
  const lineClass = (pos, cls) => ranges.push(Decoration.line({ class: cls }).range(doc.lineAt(pos).from));
  const hide = (from, to) => { if (from < to) ranges.push(Decoration.replace({}).range(from, to)); };
  const syntax = (from, to) => mark(from, to, "md-punctuation" + (caretAt(from, to) ? " md-punctuation-near" : ""));
  syntaxTree(state).iterate({ enter(n) {
    const node = n.node, line = doc.lineAt(n.from);
    if (literal && n.from >= literal.open && n.to <= literal.close + 1) return false;
    switch (n.name) {
      case "Table": {
        const agentInTable = agent && agent.from <= n.to && agent.to >= n.from;
        const session = state.field(gridSession);
        const inGrid = session?.table === n.from;
        const draft = state.field(draftingTable) === n.from;
        const active = !inGrid && (draft || state.field(pipeTable) === n.from || agentInTable);
        if (!active) {
          ranges.push(Decoration.replace({ widget: new GridWidget(readTable(state, node), inGrid ? session : null, state.selection.main),
            block: true, inclusiveStart: n.from !== 0, inclusiveEnd: n.to !== doc.length }).range(n.from, n.to));
        } else {
          for (let i = line.number; i <= doc.lineAt(n.to).number; i++) {
            const row = doc.line(i);
            lineClass(row.from, "md-source-table" + (caretAt(row.from, row.to) ? " md-table-current" : ""));
          }
          if (!agentInTable && !draft) ranges.push(Decoration.widget({ widget: new TableTools(), block: true, side: 1 }).range(n.to));
        }
        return false;
      }
      case "ATXHeading1": case "ATXHeading2": case "ATXHeading3": case "ATXHeading4": case "ATXHeading5": case "ATXHeading6": {
        const level = Number(n.name.at(-1));
        ranges.push(Decoration.line({
          class: "md-heading md-h" + level + (caretAt(line.from, line.to) ? " md-heading-current" : ""),
          attributes: { role: "heading", "aria-level": String(level), "data-prefix": "#".repeat(level) }
        }).range(line.from));
        break;
      }
      case "HeaderMark": {
        if (node.parent?.firstChild?.from === n.from) {
          const prefix = headingPrefix(state, n.from);
          if (prefix) hide(prefix.from, prefix.end);
        } else {
          const spaces = state.sliceDoc(line.from, n.from).match(/[ \t]*$/)[0].length;
          syntax(n.from - spaces, n.to);
        }
        break;
      }
      case "StrongEmphasis": mark(n.from, n.to, "md-strong"); break;
      case "Emphasis":
        mark(n.from, n.to, "md-em");
        if (line.number === 2 && n.from === line.from && n.to === line.to) lineClass(n.from, "md-meta");
        break;
      case "InlineCode": mark(n.from, n.to, "md-inline-code"); break;
      case "Strikethrough": mark(n.from, n.to, "md-strike"); break;
      case "EmphasisMark": case "CodeMark": case "StrikethroughMark":
        syntax(n.from, n.to);
        break;
      case "Link": {
        if (n.to === n.from + 3 && checkboxes.has(n.from)) return false;
        const marks = node.getChildren("LinkMark");
        if (marks.length >= 2) {
          mark(marks[0].to, marks[1].from, "md-link");
          syntax(n.from, marks[0].to);
          syntax(marks[1].from, n.to);
        }
        break;
      }
      case "ListMark": {
        const spaces = state.sliceDoc(n.to, line.to).match(/^[ \t]+/);
        if (!spaces) break;
        const open = n.to + spaces[0].length;
        if (!checkboxes.has(open) && literal?.open !== open) {
          if (/^[-+*]$/.test(state.sliceDoc(n.from,n.to))) mark(n.from, n.to, "md-bullet" + (caretAt(n.from,n.to) ? " md-bullet-near" : ""));
          break;
        }
        const checked = /^[xX]$/.test(state.sliceDoc(open + 1, literal?.open === open ? literal.close : open + 2));
        lineClass(n.from, "md-task" + (checked ? " done" : ""));
        hide(n.from, open);
        break;
      }
      case "QuoteMark": lineClass(n.from, "md-quote"); syntax(n.from, n.to); break;
      case "FencedCode": case "CodeBlock":
        for (const fence of node.getChildren("CodeMark")) syntax(fence.from, fence.to);
        for (let i = line.number; i <= doc.lineAt(n.to).number; i++) lineClass(doc.line(i).from, "md-code-line");
        return false;
    }
  } });
  return Decoration.set(ranges, true);
}

const presentation = StateField.define({
  create: decorate,
  update(value, tr) {
    if (tr.docChanged || tr.selection || tr.startState.field(mode) !== tr.state.field(mode) ||
        tr.startState.field(literalMarker) !== tr.state.field(literalMarker) ||
        tr.startState.field(draftingTable) !== tr.state.field(draftingTable) ||
        tr.startState.field(pipeTable) !== tr.state.field(pipeTable) ||
        tr.startState.field(gridSession) !== tr.state.field(gridSession) ||
        tr.startState.field(agentState) !== tr.state.field(agentState) ||
        syntaxTree(tr.startState) !== syntaxTree(tr.state)) return decorate(tr.state);
    return value;
  },
  provide: field => EditorView.decorations.from(field)
});

function moveCell(view, direction) {
  const sel = view.state.selection.main;
  const table = tableAt(view.state, sel.from);
  if (!table || sel.to > table.to) return false;
  if (view.state.field(mode) === "live" && view.state.field(pipeTable) !== table.from) return focusCellAt(view, sel.head);
  const flat = table.rows.flat();
  const pending = view.state.field(pendingCell);
  let i = pending ? flat.findIndex(c => c.from === pending.at && c.prefix === pending.prefix)
    : flat.findIndex(c => !c.prefix && sel.from >= c.start && sel.from <= c.end);
  if (i < 0) {
    const next = flat.findIndex(cell => cell.from > sel.from);
    i = next < 0 ? flat.length - 1 : direction > 0 ? next - 1 : next;
  }
  const target = flat[i + direction];
  if (target) { selectCell(view, target); return true; }
  if (direction < 0) return false;
  const row = "\n|" + "  |".repeat(table.rows[0].length);
  view.dispatch({ changes: { from: table.to, insert: row }, selection: { anchor: table.to + 3 }, userEvent: "input.table" });
  return true;
}

export function leaveTable(view) {
  if (view.state.field(mode) !== "live") return false;
  const pos = view.state.selection.main.head;
  return view.state.field(pipeTable) != null && focusCellAt(view, pos);
}

function moveVisualLine(view, down) {
  const { state } = view, sel = state.selection.main;
  if (!sel.empty || state.field(mode) !== "live" || state.field(pipeTable) != null || state.field(draftingTable) != null) return false;
  const doc = state.doc, line = doc.lineAt(sel.head);
  const heading = headingPrefix(state, line.from);
  const caret = view.coordsAtPos(sel.head, sel.assoc || 1);
  const edge = view.coordsAtPos(down ? line.to : (heading?.end ?? line.from), down ? -1 : 1);
  if (!caret || !edge) return false;
  const left = view.contentDOM.getBoundingClientRect().left;
  const goal = sel.goalColumn ?? caret.left - left;
  if (down ? caret.bottom < edge.top : caret.top > edge.bottom) {
    const next = view.moveVertically(sel, down, 1);
    view.dispatch({ selection: next, scrollIntoView: true, userEvent: "select" });
    return true;
  }
  const n = line.number + (down ? 1 : -1);
  if (n < 1 || n > doc.lines) return true;
  const other = doc.line(n), x = left + goal;
  const table = tableAt(state, other.from + Math.max(0, other.text.search(/\S/)));
  if (table) {
    const r = down ? 0 : table.rows.length - 1;
    const shell = [...view.dom.querySelectorAll(".md-grid-wrap")].find(el => el.gridModel?.from === table.from);
    const cells = shell?.querySelector("table")?.rows[r]?.cells;
    let col = cells ? [...cells].findIndex(cell => x < cell.getBoundingClientRect().right) : 0;
    if (col < 0) col = table.rows[0].length - 1;
    activateCell(view, table, r, col, false);
    return true;
  }
  const start = headingPrefix(state, other.from)?.end ?? other.from;
  const coords = view.coordsAtPos(down ? start : other.to, down ? 1 : -1);
  let hit = start;
  if (coords && other.length) hit = Math.max(start, Math.min(other.to,
    view.posAtCoords({ x, y: (coords.top + coords.bottom) / 2 }) ?? start));
  view.dispatch({ selection: EditorSelection.cursor(hit, down ? 1 : -1, undefined, goal), scrollIntoView: true, userEvent: "select" });
  return true;
}

export const tableKeys = [
  ...["ArrowLeft","ArrowRight","ArrowUp","ArrowDown"].map(key => ({
    key: "Shift-"+key, run: v => v.state.field(mode) === "live" && extendFromCell(v,key)
  })),
  { key: "ArrowLeft", run: leftFromHeading },
  { key: "#", run: v => editHeadingPrefix(v, "#") },
  { key: "Space", run: v => editHeadingPrefix(v, " ") },
  { key: "Backspace", run: v => editHeadingPrefix(v, "", true) },
  { key: "ArrowDown", run: v => moveVisualLine(v, true) },
  { key: "ArrowUp", run: v => moveVisualLine(v, false) },
  { key: "Enter", run: v => finishTableLine(v, true) },
  { key: "Tab", run: v => finishTableLine(v, false) || moveCell(v, 1), shift: v => moveCell(v, -1) },
  { key: "Escape", run: v => finishTableLine(v, false) || leaveTable(v) }
];

export const liveMarkdown = [EditorView.mouseSelectionStyle.of(dragIntoGrid), mode, gridSession, pipeTable, draftingTable, pendingCell, completeCell, tableEdgeInput, viewStyle.of([]), presentation, headingInput, headingAtoms, titleCaret];
