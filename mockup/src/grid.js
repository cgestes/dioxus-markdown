import { findClusterBreak, StateEffect, StateField } from "@codemirror/state";
import { WidgetType } from "@codemirror/view";
import { applyHistory, equalCursor, locate, remember, sharedNote } from "./loro-editor.js";
import { tableAt, appendInline } from "./table-model.js";
import { textChange, escapeCell, insertRow, deleteRow, insertColumn, deleteColumn, swapRows, swapColumns } from "./table-commands.js";
import { cellAtPoint, paintCellSelection } from "./grid-pointer.js";
import { pasteRectangle, pasteCells } from "./table-paste.js";
import { undoGroup } from "./inline-agent.js";
import { rectangle, nextCell, rangePaste, rangeText, cellName } from "./cell-range.js";
import { button, el, icon } from "./ui.js";

export const setGrid = StateEffect.define();
const showPipes = StateEffect.define();
export const pipeTable = StateField.define({
  create: () => null,
  update(value, tr) {
    const open = tr.effects.find(e => e.is(showPipes));
    if (open) return open.value;
    if (value == null || tr.effects.some(e => e.is(setGrid))) return null;
    if (!tr.docChanged && !tr.selection) return value;
    const from = tr.changes.mapPos(value, -1);
    return tableAt(tr.state, tr.newSelection.main.head)?.from === from ? from : null;
  }
});
export const gridSession = StateField.define({
  create: () => null,
  update(value, tr) {
    const effect = tr.effects.find(e => e.is(setGrid));
    if (effect) return effect.value;
    if (!value || tr.selection) return null;
    if (!tr.docChanged) return value;
    const oldTable = tableAt(tr.startState, value.from);
    const oldCell = oldTable?.rows[value.row]?.[value.col];
    if (oldCell) {
      let removed = false;
      tr.changes.iterChangedRanges((from, to) => {
        if (from <= oldCell.start && to >= oldCell.end && (from < oldCell.start || to > oldCell.end)) removed = true;
      });
      if (removed) return null;
    }
    const at = tr.changes.mapPos(value.table, 1);
    const point = tr.changes.mapPos(Math.floor((value.from + value.to) / 2), 1);
    const table = tableAt(tr.state, point);
    if (!table || table.from !== at) return null;
    const row = table.rows.findIndex(cells => tr.newDoc.lineAt(cells[0].from).number === tr.newDoc.lineAt(point).number);
    if (row < 0) return null;
    const cells = table.rows[row];
    let col = value.implicit && cells[value.col]?.prefix ? value.col : cells.findIndex(c => !c.prefix && point >= c.start && point <= c.end);
    if (col < 0) col = Math.min(value.col, cells.length - 1);
    const cell = cells[col];
    const from = Math.max(cell.start, Math.min(cell.from, tr.changes.mapPos(value.from, -1)));
    const to = Math.min(cell.end, Math.max(cell.to, tr.changes.mapPos(value.to, 1)));
    const rect = table.rows.length === oldTable?.rows.length && table.rows[0].length === oldTable.rows[0].length &&
      row === value.row && col === value.col ? value.rect : null;
    return { table: at, row, col, from, to, implicit: Boolean(cell.prefix), rect, editing: value.editing, textSelected: value.textSelected };
  }
});

function announce(text) { document.getElementById("announcement").textContent = text; }

export function activateCell(view, table, row, col, select = true, rect = null) {
  if (!table) return;
  row = Math.min(Math.max(0, row), table.rows.length - 1);
  col = Math.min(Math.max(0, col), table.rows[0].length - 1);
  const cell = table.rows[row][col];
  view.state.facet(undoGroup).stopCapturing();
  const session = { table: table.from, row, col, from: cell.from, to: cell.to, implicit: Boolean(cell.prefix), rect };
  const length = cell.to - cell.from;
  const pos = Array.isArray(select) ? select[0] : select === "end" ? length : typeof select === "number" ? Math.min(select, length) : 0;
  const head = Array.isArray(select) ? select[1] : select === true ? length : pos;
  view.dispatch({ selection: { anchor: cell.from + pos, head: cell.from + head }, effects: setGrid.of(session) });
  const input = view.dom.querySelector(".grid-cell-input");
  if (input) {
    const shell = input.closest(".md-grid-wrap");
    shell.closeMenu();
    input.focus({ preventScroll: true });
    input.setSelectionRange(Math.min(pos,head), Math.max(pos,head), head < pos ? "backward" : "forward");
    input.scrollIntoView({ block: "nearest", inline: "nearest" });
    publishCursor(view, input);
  }
}

export function focusCellAt(view, pos, selection) {
  const table = tableAt(view.state, pos);
  if (!table) return false;
  const row = Math.max(0, table.rows.findIndex(cells => view.state.doc.lineAt(cells[0].from).number === view.state.doc.lineAt(pos).number));
  const col = Math.max(0, table.rows[row].findIndex(c => pos >= c.start && pos <= c.end));
  const cell = table.rows[row][col];
  if (selection && (selection.from < cell.from || selection.to > cell.to)) return false;
  activateCell(view, table, row, col,
    selection ? [selection.anchor - cell.from, selection.head - cell.from] : true);
  return true;
}

function publishCursor(view, input) {
  const current = view.state.field(gridSession);
  if (!current || input !== document.activeElement) return;
  const config = view.state.facet(sharedNote);
  const [start, end] = bodySelection(input);
  const backwards = input.selectionDirection === "backward";
  const anchor = Math.min(current.to, current.from + (backwards ? end : start));
  const head = Math.min(current.to, current.from + (backwards ? start : end));
  const selection = view.state.selection.main;
  if (selection.anchor !== anchor || selection.head !== head) {
    view.dispatch({ selection: { anchor, head }, effects: setGrid.of({...current,textSelected:anchor!==head}), userEvent: "select" });
  }
  const cursor = { anchor: remember(config.text, anchor), head: remember(config.text, head) };
  const prior = config.awareness.getLocalState()?.cursor;
  if (!prior || !equalCursor(cursor.anchor, prior.anchor) || !equalCursor(cursor.head, prior.head))
    config.awareness.setLocalStateField("cursor", cursor);
}

export function paintGridPeers(view) {
  if (!view) return;
  const conf = view.state.facet(sharedNote);
  for (const shell of view.dom.querySelectorAll(".md-grid-wrap")) {
    if (!shell.gridModel) continue;
    for (const td of shell.querySelectorAll("[data-peer]")) delete td.dataset.peer;
    const table = shell.gridModel;
    for (const [id, peer] of conf.awareness.getStates()) {
      if (id === conf.clientID || peer.note !== conf.id || !peer.cursor || !peer.user) continue;
      let pos;
      try { pos = locate(conf.doc, peer.cursor.head); } catch { continue; }
      if (pos == null || pos < table.from || pos > table.to) continue;
      for (let row = 0; row < table.rows.length; row++) {
        const col = table.rows[row].findIndex(c => !c.prefix && pos >= c.start && pos <= c.end);
        if (col < 0) continue;
        const td = shell.querySelector("table").rows[row]?.cells[col];
        if (td) { td.dataset.peer = peer.user.name; td.style.setProperty("--peer", peer.user.color); }
        break;
      }
    }
  }
}

function bodySelection(input) {
  return [input.selectionStart ?? input.value.length, input.selectionEnd ?? input.value.length];
}

function commitValue(view, shell, input, previous = null) {
  const session = view.state.field(gridSession);
  if (!session) return;
  const cell = shell.gridModel.rows[session.row]?.[session.col];
  if (!cell) return;
  const current = view.state.sliceDoc(session.from, session.to);
  // Keep an in-progress IME composition private until commit. If the same
  // passage changed remotely, retain both drafts for an explicit choice.
  if (previous != null && previous !== current) {
    const draft = input.value;
    input.value = current;
    const notice = shell.querySelector(".grid-recovery");
    notice.replaceChildren(el("span", "", "This cell changed during composition."), button("Copy my draft", "", async () => {
      try { await navigator.clipboard.writeText(draft); notice.hidden = true; }
      catch { const copy = el("input"); copy.value = draft; notice.replaceChildren(copy); copy.select(); }
    }), button("Dismiss", "", () => { notice.hidden = true; }));
    notice.hidden = false;
    return;
  }
  const [start, end] = bodySelection(input);
  // Wrapped cells still occupy one Markdown source line, including text
  // committed by a dictation or drop provider that bypasses beforeinput.
  const draft = input.value.replaceAll("\n", " ");
  const value = escapeCell(draft);
  const a = escapeCell(draft.slice(0, start)).length, h = escapeCell(draft.slice(0, end)).length;
  if (input.value !== value) { input.value = value; input.setSelectionRange(a, h); }
  if (current === value) return;
  const prefix = cell.prefix || "";
  const change = prefix ? { from: cell.from, insert: prefix + value + " |" } : textChange(current, value, session.from);
  const from = (prefix ? cell.from : session.from) + prefix.length;
  view.dispatch({ changes: change, selection: { anchor: from + a, head: from + h },
    effects: setGrid.of({ ...session, from, to: from + value.length, implicit: false, editing: true }), userEvent: "input.table.cell" });
  publishCursor(view, input);
}

function structuralEdit(view, table, changes, row, col, message) {
  const manager = view.state.facet(undoGroup);
  manager.stopCapturing();
  view.dispatch({ changes, userEvent: "input.table.structure" });
  manager.stopCapturing();
  activateCell(view, tableAt(view.state, table.from + 1), row, col);
  announce(message);
}

function runCommand(view, shell, command) {
  const table = shell.gridModel, active = view.state.field(gridSession);
  const row = active?.table === table.from ? active.row : 0;
  const col = active?.table === table.from ? active.col : 0;
  const width = table.rows[0].length, height = table.rows.length;
  shell.closeMenu();
  const rect = active?.rect;
  if (command === "clear" && rect) {
    fillRange(view,table,active,"");
  } else if (["row", "above", "below"].includes(command)) {
    const idx = command === "row" ? height : command === "above" ? Math.max(1, row) : row + 1;
    structuralEdit(view, table, insertRow(table, idx), idx, command === "row" ? 0 : col, "Added a row.");
  } else if (["column", "left", "right"].includes(command)) {
    const idx = command === "column" ? width : col + (command === "right" ? 1 : 0);
    structuralEdit(view, table, insertColumn(table, idx), 0, idx, "Added a column.");
  } else if (command === "delete-row" && (rect?.top ?? row) > 0) {
    const first = rect?.top ?? row, last = rect?.bottom ?? row;
    structuralEdit(view, table, deleteRow(table,first,last), Math.min(first,height-(last-first)-2), col, "Deleted selected rows.");
  } else if (command === "delete-column" && active?.table === table.from && col < width && width > 1) {
    const first = rect?.left ?? col, last = rect?.right ?? col;
    if (last-first+1 < width) structuralEdit(view, table, deleteColumn(view.state,table,first,last), row, Math.min(first,width-(last-first)-2), "Deleted selected columns.");
  } else if (command === "move-up" || command === "move-down") {
    const next = row + (command === "move-up" ? -1 : 1);
    if (row > 0 && next > 0 && next < height)
      structuralEdit(view, table, swapRows(view.state, table, row, next), next, col, "Moved the row.");
  } else if (command === "move-left" || command === "move-right") {
    const next = col + (command === "move-left" ? -1 : 1);
    if (next >= 0 && next < width)
      structuralEdit(view, table, swapColumns(view.state, table, col, next), row, next, "Moved the column.");
  } else if (command === "pipes") {
    const cell = table.rows[row][col];
    view.dispatch({ selection: { anchor: cell.from, head: cell.to }, effects: [setGrid.of(null), showPipes.of(table.from)] });
    view.focus();
  }
}

function leaveGrid(view, table, before = false) {
  const doc = view.state.doc;
  const n = doc.lineAt(before ? table.from : table.to).number;
  const anchor = before && n > 1 ? doc.line(n - 1).to : !before && n < doc.lines ? doc.line(n + 1).from : before ? table.from : table.to;
  view.dispatch({ selection: { anchor }, effects: setGrid.of(null) });
  view.focus();
}

// Extend from an input into the Markdown selection. Further Shift+arrows
// can keep moving its head inside a projected table after the input closes.
export function extendFromCell(view, key, input = null) {
  const session = view.state.field(gridSession);
  const selected = view.state.selection.main;
  let { anchor, head } = selected;
  if (input && session) {
    const [a,b] = bodySelection(input), reverse = input.selectionDirection === "backward";
    anchor = session.from + (reverse ? b : a); head = session.from + (reverse ? a : b);
  }
  const table = tableAt(view.state,head);
  if (!table || view.state.field(pipeTable) === table.from) return false;
  const width = table.rows[0].length, flat = table.rows.flat();
  const index = session ? session.row*width + session.col : flat.findIndex(c => head >= c.from && head <= c.to);
  if (index < 0) return false;
  const cell = flat[index], dir = key === "ArrowRight" || key === "ArrowDown" ? 1 : -1;
  const horizontal = key === "ArrowLeft" || key === "ArrowRight";
  let end;
  if (horizontal && (dir > 0 ? head < cell.to : head > cell.from)) {
    if (input) return false;
    end = cell.from + findClusterBreak(view.state.sliceDoc(cell.from,cell.to),head-cell.from,dir>0);
  } else {
    const next = flat[index + dir*(horizontal ? 1 : width)];
    if (next && horizontal) end = dir > 0 ? next.from : next.to;
    else if (next) {
      const text = view.state.sliceDoc(next.from,next.to);
      let offset = Math.min(head-cell.from,text.length);
      if (offset < text.length) offset = findClusterBreak(text,findClusterBreak(text,offset,true),false);
      end = next.from + offset;
    } else {
      const doc = view.state.doc, n = doc.lineAt(dir < 0 ? table.from : table.to).number + dir;
      end = n < 1 ? table.from : n > doc.lines ? table.to : dir < 0 ? doc.line(n).to : doc.line(n).from;
    }
  }
  view.dispatch({ selection: {anchor,head:end}, effects:setGrid.of(null), scrollIntoView:true, userEvent:"select" });
  view.focus();
  return true;
}

function fillRange(view, table, session, value) {
  const {rect,row,col} = session;
  let changes;
  if (value === "") {
    changes = table.rows.slice(rect.top,rect.bottom+1).flatMap(cells =>
      cells.slice(rect.left,rect.right+1).filter(cell => cell.to > cell.from).map(cell => ({from:cell.from,to:cell.to})));
  } else changes = pasteCells(view.state,table,rect.top,rect.left,rangePaste(rect,value)).changes;
  const manager = view.state.facet(undoGroup);
  manager.stopCapturing();
  view.dispatch({changes,userEvent:"input.table.range"});
  manager.stopCapturing();
  activateCell(view,tableAt(view.state,table.from+1),row,col,true,rect);
}

function keyInCell(view, shell, input, event) {
  if (event.isComposing || event.keyCode === 229) return;
  const table = shell.gridModel, current = view.state.field(gridSession);
  if (!current) return;
  const { row, col } = current, width = table.rows[0].length, height = table.rows.length;
  const mod = event.metaKey || event.ctrlKey;
  const consume = () => { event.preventDefault(); event.stopPropagation(); };
  const rect = current.rect;
  if (!mod && event.shiftKey && (event.altKey || rect) && ["ArrowLeft","ArrowRight","ArrowUp","ArrowDown"].includes(event.key)) {
    const [r,c] = rect?.head ?? [row,col];
    const end = [Math.max(0,Math.min(height-1,r+Number(event.key==="ArrowDown")-Number(event.key==="ArrowUp"))),
      Math.max(0,Math.min(width-1,c+Number(event.key==="ArrowRight")-Number(event.key==="ArrowLeft")))];
    consume(); activateCell(view,table,...end,true,rectangle(rect?.anchor ?? [row,col],end)); return;
  }
  if (rect && !mod && !event.altKey) {
    if (event.key === "Tab") { consume(); activateCell(view,table,...nextCell(rect,row,col,event.shiftKey),true,rect); return; }
    if (event.key === "Escape") { consume(); activateCell(view,table,row,col,"end"); return; }
    if (!current.editing && ["Backspace","Delete"].includes(event.key)) { consume(); fillRange(view,table,current,""); return; }
    if (event.key.startsWith("Arrow") || event.key === "Enter") view.dispatch({effects:setGrid.of({...current,rect:null})});
  }
  if (mod && ["z", "y"].includes(event.key.toLowerCase())) {
    event.preventDefault(); event.stopPropagation();
    applyHistory(view, event.shiftKey || event.key.toLowerCase() === "y");
    activateCell(view, tableAt(view.state, Math.min(table.from + 1, view.state.doc.length)), row, col);
    return;
  }
  if (mod && event.key.toLowerCase() === "a") {
    event.preventDefault(); event.stopPropagation();
    view.dispatch({ selection: { anchor: 0, head: view.state.doc.length }, effects: setGrid.of(null), userEvent: "select" });
    view.focus(); return;
  }
  if (mod && ["Home", "End"].includes(event.key)) {
    event.preventDefault(); event.stopPropagation();
    const last = event.key === "End";
    activateCell(view, table, last ? height - 1 : 0, last ? width - 1 : 0);
    return;
  }
  if (mod || event.altKey) return;
  if (event.shiftKey && event.key.startsWith("Arrow") && extendFromCell(view,event.key,input)) {
    consume(); return;
  }
  const start = input.selectionStart, end = input.selectionEnd, len = input.value.length;
  let r = row, c = col, landing = true;
  switch (event.key) {
    case "Escape": consume(); leaveGrid(view, table); return;
    case "Tab": c += event.shiftKey ? -1 : 1; break;
    case "Enter": r += event.shiftKey ? -1 : 1; break;
    case "ArrowDown": case "ArrowUp":
      if (event.shiftKey) return;
      r += event.key === "ArrowDown" ? 1 : -1; landing = end; break;
    case "ArrowLeft":
      if (event.shiftKey || !(start === 0 && end === 0)) return;
      c--; landing = "end"; break;
    case "ArrowRight":
      if (event.shiftKey || !(start === len && end === len)) return;
      c++; landing = false; break;
    default: return;
  }
  consume();
  if (c < 0) { r--; c = width - 1; }
  if (c >= width) { r++; c = 0; }
  landInRow(view, table, r, c, landing, event.key === "Enter" || event.key === "Tab");
}

function landInRow(view, table, r, c, landing, grow) {
  const height = table.rows.length;
  if (r < 0) { leaveGrid(view, table, true); return; }
  if (r >= height) {
    if (grow) structuralEdit(view, table, insertRow(table, height), height, c, "Added a row.");
    else leaveGrid(view, table);
  } else activateCell(view, table, r, c, landing);
}

// The mask follows the same line breaks and advances as the editable text.
function cellMasks(node, offset) {
  const ranges = [];
  node?.cursor().iterate(n => {
    if (["EmphasisMark", "StrikethroughMark", "CodeMark"].includes(n.name)) ranges.push([n.from-offset,n.to-offset]);
    if (n.name === "Escape") ranges.push([n.from-offset,n.from+1-offset]);
    if (n.name === "Link") {
      const marks = n.node.getChildren("LinkMark");
      if (marks.length >= 2) ranges.push([n.from-offset,marks[0].to-offset],[marks[1].from-offset,n.to-offset]);
    }
  });
  return ranges.sort((a,b) => a[0]-b[0]);
}

function showCellMasks(input) {
  const mask = input.mask;
  if (!mask) return;
  const [a,b] = bodySelection(input);
  mask.line.style.transform = `translate(${-input.scrollLeft}px, ${-input.scrollTop}px)`;
  for (const [from,to,mark] of mask.parts) {
    mark.classList.toggle("cover", input.compositionBase == null && (b < from || a > to));
  }
}

function updateCellMasks(entry, input, cell, source) {
  if (!cell.node || !/[*_`~[\]<\\]/.test(source)) { input.mask?.box.remove(); input.mask = null; return; }
  if (input.mask?.source !== source) {
    input.mask?.box.remove();
    const box = el("span","grid-mask"), line = el("span","grid-mask-line");
    box.setAttribute("aria-hidden","true"); box.append(line);
    let last = 0;
    const parts = [];
    for (const [from,to] of cellMasks(cell.node,cell.from)) {
      if (from < last || to <= from || to > source.length) continue;
      line.append(document.createTextNode(source.slice(last,from)));
      const mark = el("span","",source.slice(from,to)); line.append(mark);
      parts.push([from,to,mark]); last = to;
    }
    line.append(document.createTextNode(source.slice(last)));
    input.mask = { source, box, line, parts };
  }
  if (input.mask.box.parentElement !== entry) entry.append(input.mask.box);
  showCellMasks(input);
}

function cellInput(view, shell) {
  const input = el("textarea", "grid-cell-input");
  input.rows = 1; input.autocomplete = "off"; input.spellcheck = false;
  input.addEventListener("beforeinput", event => {
    const session = view.state.field(gridSession);
    if (!session || event.isComposing || !["insertLineBreak","insertParagraph"].includes(event.inputType)) return;
    event.preventDefault();
    landInRow(view, shell.gridModel, session.row + 1, session.col, true, true);
  });
  input.addEventListener("input", event => { if (!event.isComposing && input.compositionBase == null) commitValue(view, shell, input); });
  input.addEventListener("compositionstart", () => { input.compositionBase = input.value; input.setAttribute("data-composing",""); });
  input.addEventListener("compositionend", () => {
    const base = input.compositionBase; input.compositionBase = null;
    input.removeAttribute("data-composing");
    commitValue(view, shell, input, base);
  });
  for (const name of ["keyup","mouseup","select","scroll","selectionchange","compositionstart"]) {
    input.addEventListener(name, () => showCellMasks(input));
  }
  input.addEventListener("keyup", () => publishCursor(view, input));
  for (const name of ["cut","copy"]) input.addEventListener(name,event => {
    const session = view.state.field(gridSession);
    if (!session?.rect || session.editing || !event.clipboardData) return;
    event.preventDefault(); event.stopPropagation();
    event.clipboardData.setData("text/plain",rangeText(view.state,shell.gridModel,session.rect));
    if (name === "cut") fillRange(view,shell.gridModel,session,"");
  });
  input.addEventListener("paste", event => {
    const text = event.clipboardData?.getData("text/plain");
    const session = view.state.field(gridSession);
    if (session?.rect && !session.editing && text) {
      event.preventDefault(); event.stopPropagation(); fillRange(view,shell.gridModel,session,text); return;
    }
    if (!session || !text || !/[\t\n\r]/.test(text)) return;
    event.preventDefault(); event.stopPropagation();
    const table = shell.gridModel;
    const pasted = pasteRectangle(view.state, table, session.row, session.col, text);
    structuralEdit(view, table, pasted.changes, session.row + pasted.rows - 1, session.col + pasted.cols - 1,
      "Pasted " + pasted.rows + " rows and " + pasted.cols + " columns.");
  });
  input.addEventListener("mouseup", () => publishCursor(view, input));
  input.addEventListener("keydown", event => keyInCell(view, shell, input, event));
  return input;
}

const operations = {
  Row: [["Insert above", "above"], ["Insert below", "below"], ["Move up", "move-up"], ["Move down", "move-down"], ["Delete row", "delete-row"]],
  Column: [["Insert left", "left"], ["Insert right", "right"], ["Move left", "move-left"], ["Move right", "move-right"], ["Delete column", "delete-column"]]
};

function createShell(view) {
  const shell = el("div", "md-grid-wrap ledger");
  shell.contentEditable = "false";
  const scroll = el("div", "grid-scroll");
  const grid = el("table", "md-grid");
  grid.setAttribute("aria-label", "Editable Markdown table");
  grid.append(el("colgroup")); grid.createTHead(); grid.createTBody();
  scroll.append(grid);
  // Fixed-layout tables accept pixel column widths across the supported engines.
  // Only a change to the viewport or the set of columns should redistribute space.
  let columns, fittedWidth = -1;
  function fitColumns(width) {
    if (!columns || !width || width === fittedWidth) return;
    fittedWidth = width;
    const extra = Math.max(0, width - columns.minimum);
    columns.items.forEach(({element, floor, share}) => { element.style.width = (floor + extra * share) + "px"; });
  }
  const observer = new ResizeObserver(([entry]) => fitColumns(entry.contentRect.width));
  observer.observe(scroll);
  shell.stopMeasuring = () => observer.disconnect();
  shell.setColumns = (weights, alignment) => {
    const sum = weights.reduce((a,b) => a + b, 0);
    const items = weights.map((w,c) => ({
      element: el("col"), floor: alignment[c] === "right" ? 68 : c === 0 ? 92 : 100, share: w / sum
    }));
    columns = {items, minimum: items.reduce((sum,col) => sum + col.floor, 0)};
    grid.style.minWidth = columns.minimum + "px";
    grid.querySelector("colgroup").replaceChildren(...items.map(col => col.element));
    fittedWidth = -1;
    fitColumns(scroll.clientWidth);
  };
  const bar = el("div", "grid-bar ledger-tools"), menu = el("div", "grid-menu ledger-menu");
  menu.hidden = true; menu.setAttribute("role", "group"); menu.setAttribute("aria-label", "Table actions");
  const tool = (label, command, glyph, aria) => {
    const b = button(label, "", () => runCommand(view, shell, command));
    b.setAttribute("aria-label", aria || label); b.prepend(icon(glyph)); return b;
  };
  const toggle = button("Table actions", "cell-menu-toggle ledger-actions", () => {
    if (!menu.hidden) { shell.closeMenu(); return; }
    menu.hidden = false;
    toggle.setAttribute("aria-expanded", "true");
    placeMenu();
    for (const control of menu.querySelectorAll("button:not(:disabled)")) {
      if (control.getClientRects().length) { control.focus({preventScroll:true}); break; }
    }
    window.addEventListener("resize", placeMenu);
    document.addEventListener("scroll", placeMenu, true);
  });
  toggle.setAttribute("aria-expanded", "false"); toggle.prepend(icon("ellipsis")); toggle.title = "Table actions (Shift+F10)";
  toggle.setAttribute("aria-keyshortcuts", "Shift+F10");
  const pipes = tool("Edit Markdown pipes", "pipes", "code-xml", "Edit table as Markdown pipes");
  pipes.className = "edit-pipes";
  const rowAdd = tool("Row", "row", "plus", "Add row"), colAdd = tool("", "column", "plus", "Add column");
  rowAdd.className = "grid-add-row ledger-grow ledger-grow-row"; colAdd.className = "grid-add-column ledger-grow ledger-grow-column";
  rowAdd.title = "Add row"; colAdd.title = "Add column";
  const frame = el("div", "grid-frame");
  frame.append(scroll, colAdd);
  const count = el("span","cell-count");
  bar.append(toggle, rowAdd, count);
  const scope = el("div","grid-scope ledger-scope");
  const clear = button("Clear selected cells","",() => runCommand(view,shell,"clear"));
  clear.dataset.command = "clear";
  menu.append(scope, clear);
  for (const [groupName, entries] of Object.entries(operations)) {
    const group = el("div", "grid-menu-group"); group.append(el("strong", "", groupName));
    for (const [label, cmd] of entries) {
      const b = button(label, cmd.startsWith("delete-") ? "remove" : "", () => runCommand(view, shell, cmd)); b.dataset.command = cmd; group.append(b);
    }
    menu.append(group);
  }
  menu.append(pipes);
  const recovery = el("div", "grid-recovery"); recovery.hidden = true;
  shell.append(frame, bar, menu, recovery);
  // Position in document coordinates so opening the tools never moves a row.
  function placeMenu() {
    const clip = view.dom.closest("main").getBoundingClientRect();
    const top = Math.max(0, clip.top) + 5, bottom = Math.min(innerHeight, clip.bottom) - 5;
    menu.style.maxHeight = Math.max(1, bottom - top) + "px";
    const bounds = shell.getBoundingClientRect(), height = menu.offsetHeight;
    const above = bounds.top - height - 5, below = bounds.bottom;
    const y = below + height <= bottom || (above < top && bottom - below >= bounds.top - top) ? below : above;
    menu.style.top = Math.round(Math.max(top, Math.min(bottom - height, y)) - bounds.top) + "px";
  }
  shell.closeMenu = () => {
    menu.hidden = true;
    toggle.setAttribute("aria-expanded", "false");
    window.removeEventListener("resize", placeMenu);
    document.removeEventListener("scroll", placeMenu, true);
  };
  shell.addEventListener("mousedown", event => {
    const control = event.target.closest("button,.grid-menu,.grid-recovery");
    if (event.button !== 0 || control) return;
    const scroller = shell.querySelector(".grid-scroll");
    if (event.target === scroller && event.clientY >= scroller.getBoundingClientRect().top + scroller.clientHeight) return;
    const native = event.target === shell.editorInput;
    const point = cellAtPoint(shell, event.clientX, event.clientY);
    let origin = event.shiftKey ? view.state.selection.main.anchor : point.from + point.offset;
    let readNativeAnchor = native && !event.shiftKey;
    const selectDocument = head => {
      view.dispatch({ selection: { anchor: origin, head }, effects: setGrid.of(null), userEvent: "select.pointer" });
      view.focus();
    };
    const current = view.state.field(gridSession);
    const prior = current?.table === shell.gridModel.from ? current : null;
    if (event.shiftKey && prior && (prior.rect || prior.row !== point.row || prior.col !== point.col)) {
      event.preventDefault();
      const end = [point.row,point.col];
      activateCell(view,shell.gridModel,...end,true,rectangle(prior.rect?.anchor ?? [prior.row,prior.col],end));
      return;
    }
    if (prior?.rect) view.dispatch({effects:setGrid.of({...prior,rect:null})});
    if (event.shiftKey && (origin < point.from || origin > shell.gridModel.rows[point.row][point.col].to)) {
      event.preventDefault(); selectDocument(point.from + point.offset);
    } else if (!native) {
      event.preventDefault();
      activateCell(view, shell.gridModel, point.row, point.col, point.offset);
      if (event.shiftKey) {
        const anchor = origin - point.from;
        shell.editorInput.setSelectionRange(Math.min(anchor,point.offset), Math.max(anchor,point.offset),
          anchor > point.offset ? "backward" : "forward");
        publishCursor(view, shell.editorInput);
      }
    }
    // Within an already active input the platform owns word and drag selection.
    // Once the gesture leaves it, extend the same selection in the Markdown.
    const controller = new AbortController();
    const options = { signal: controller.signal, capture: true };
    window.addEventListener("blur", () => controller.abort(), { signal: controller.signal });
    window.addEventListener("mouseup", () => controller.abort(), options);
    window.addEventListener("mousemove", move => {
      if (!(move.buttons & 1)) { controller.abort(); return; }
      const over = document.elementFromPoint(move.clientX, move.clientY);
      const grid = over?.closest(".md-grid-wrap");
      if (readNativeAnchor) {
        const i = shell.editorInput;
        origin = point.from + (i.selectionDirection === "backward" ? i.selectionEnd : i.selectionStart);
        readNativeAnchor = false;
      }
      if (native && over === shell.editorInput && !view.state.field(gridSession)?.rect && view.state.field(gridSession)) return;
      move.preventDefault();
      if (grid) {
        const hit = cellAtPoint(grid, move.clientX, move.clientY);
        if (grid === shell && hit.row === point.row && hit.col === point.col && view.state.field(gridSession)) {
          if (view.state.field(gridSession)?.rect) activateCell(view,shell.gridModel,hit.row,hit.col,hit.offset);
          const a = origin - hit.from, h = hit.offset;
          shell.editorInput.setSelectionRange(Math.min(a,h), Math.max(a,h), a > h ? "backward" : "forward");
          publishCursor(view, shell.editorInput);
        } else if (grid === shell) {
          const rect = rectangle([point.row,point.col],[hit.row,hit.col]);
          const prior = view.state.field(gridSession);
          if (!prior?.rect || prior.row !== hit.row || prior.col !== hit.col) activateCell(view,shell.gridModel,hit.row,hit.col,true,rect);
        } else selectDocument(hit.from + hit.offset);
      } else {
        const head = view.posAtCoords({ x: move.clientX, y: move.clientY }, false);
        if (head != null) selectDocument(head);
      }
    }, options);
  });
  shell.addEventListener("keydown", event => {
    if (!toggle.hidden && event.key === "F10" && event.shiftKey && !event.altKey && !event.metaKey && !event.ctrlKey) {
      event.preventDefault(); event.stopPropagation(); toggle.click(); return;
    }
    const td = event.target.closest("td,th");
    if (event.target === td && td && ["Enter", " ", "F2"].includes(event.key)) {
      event.preventDefault(); activateCell(view, shell.gridModel, td.parentElement.rowIndex, td.cellIndex);
    }
    if (event.key === "Escape" && !menu.hidden) {
      event.preventDefault(); event.stopPropagation(); shell.closeMenu();
      (shell.editorInput?.isConnected ? shell.editorInput : toggle).focus({preventScroll:true});
    }
  });
  shell.addEventListener("focusout", event => {
    // During a pointer-driven focus transfer activeElement can briefly be body
    // before the next menu button receives focus. Keep the menu under the pointer.
    if (event.relatedTarget && shell.contains(event.relatedTarget)) return;
    queueMicrotask(() => {
      if (!shell.contains(document.activeElement)) shell.closeMenu();
    });
  });
  return shell;
}

function syncValue(input, next) {
  if (input.value === next || input.compositionBase != null) return;
  const [a, b] = bodySelection(input), change = textChange(input.value, next);
  const map = pos => pos <= change.from ? pos : pos >= change.to ? pos + change.insert.length - (change.to - change.from) : change.from + change.insert.length;
  input.value = next;
  input.setSelectionRange(map(a), map(b));
}

export class GridWidget extends WidgetType {
  constructor(table, session, selection) {
    super(); this.table = table; this.session = session;
    this.range = !session && !selection.empty && selection.from < table.to && selection.to > table.from
      ? [Math.max(table.from,selection.from), Math.min(table.to,selection.to)] : null;
  }
  eq(other) { return this.table.from === other.table.from && this.table.source === other.table.source && this.session === other.session &&
    this.range?.[0] === other.range?.[0] && this.range?.[1] === other.range?.[1]; }
  toDOM(view) { const shell = createShell(view); this.updateDOM(shell, view); return shell; }
  updateDOM(shell, view) {
    const { table, session } = this;
    shell.gridModel = table;
    const grid = shell.querySelector("table"), group = grid.querySelector("colgroup"), width = table.rows[0].length;
    const alignment = table.align.join(",");
    if (group.children.length !== width || group.dataset.alignment !== alignment) {
      group.dataset.alignment = alignment;
      const weights = table.rows[0].map((_, c) => Math.min(34, Math.max(7, ...table.rows.map(row => row[c].to - row[c].from))) + 5);
      shell.setColumns(weights, table.align);
    }
    const input = shell.editorInput ||= cellInput(view, shell);
    const wasFocused = document.activeElement === input;
    while (grid.rows.length > table.rows.length) grid.deleteRow(-1);
    const rect = session?.rect;
    table.rows.forEach((cells, r) => {
      const tr = grid.rows[r] || (r === 0 ? grid.tHead : grid.tBodies[0]).insertRow();
      tr.classList.toggle("ledger-last", r === table.rows.length - 1);
      const line = table.lines[r === 0 ? 0 : r + 1];
      const rowSource = view.state.sliceDoc(line.from, line.to);
      const activeCol = session?.row === r ? session.col : -1;
      const entry = r === 0 && !session;
      const rowRange = rect && r >= rect.top && r <= rect.bottom ? rect : null;
      if (tr.lastSource === rowSource && tr.lastAlignment === alignment && tr.lastActive === activeCol &&
          tr.lastEntry === entry && tr.lastRange === rowRange && activeCol < 0) return;
      tr.lastRange = rowRange;
      tr.lastSource = rowSource; tr.lastAlignment = alignment; tr.lastActive = activeCol; tr.lastEntry = entry;
      while (tr.cells.length > width) tr.deleteCell(-1);
      cells.forEach((cell, c) => {
        let td = tr.cells[c];
        if (!td) {
          td = el(r === 0 ? "th" : "td"); if (!r) td.scope = "col";
          const entry = el("div", "grid-entry");
          entry.append(el("span", "grid-value")); td.append(entry); tr.append(td);
        }
        const active = session?.row === r && session.col === c;
        td.tabIndex = !session && r === 0 && c === 0 ? 0 : -1;
        td.className = "ledger-cell cell-" + table.align[c] + (r ? " ledger-data" : " ledger-head");
        if (r && c === 0 && table.align[c] === "left") td.classList.add("ledger-label");
        if (r && table.align[c] === "right") td.classList.add("ledger-number");
        if (active) td.classList.add("is-editing");
        if (rowRange && c >= rect.left && c <= rect.right) {
          td.classList.add("range-cell");
          td.setAttribute("aria-selected","true");
          td.style.setProperty("--range-top",r === rect.top ? "2px" : "0");
          td.style.setProperty("--range-right",c === rect.right ? "2px" : "0");
          td.style.setProperty("--range-bottom",r === rect.bottom ? "2px" : "0");
          td.style.setProperty("--range-left",c === rect.left ? "2px" : "0");
        } else td.removeAttribute("aria-selected");
        const entry = td.firstChild, label = entry.firstChild, source = view.state.sliceDoc(cell.from, cell.to);
        if (active) label.setAttribute("aria-hidden","true"); else label.removeAttribute("aria-hidden");
        if (td.dataset.source !== source) {
          label.replaceChildren();
          if (cell.node) appendInline(label, view.state, cell.node); else label.textContent = source;
          td.dataset.source = source;
        }
        if (active) {
          if (input.parentElement !== entry) entry.append(input);
          input.toggleAttribute("data-range-target", Boolean(rect && !session.editing));
          input.setAttribute("aria-label", (r === 0 ? "Header" : "Row " + r) + ", column " + (c + 1));
          syncValue(input, view.state.sliceDoc(session.from, session.to));
          updateCellMasks(entry,input,cell,source);
        }
      });
    });
    if (!session) { input.remove(); input.mask?.box.remove(); input.mask = null; }
    const count = shell.querySelector(".cell-count");
    count.hidden = !rect;
    if (rect) count.textContent = rect.count+" cells";
    shell.querySelector(".cell-menu-toggle").hidden = !session;
    if (!session) shell.closeMenu();
    shell.querySelector(".grid-menu").classList.toggle("scope-actions",Boolean(rect));
    shell.querySelector(".grid-scope").textContent = rect ? cellName(rect.top,rect.left)+" – "+cellName(rect.bottom,rect.right) :
      session ? "Cell "+cellName(session.row,session.col) : "";
    shell.querySelector('[data-command="clear"]').hidden = !rect;
    const delRow = shell.querySelector('[data-command="delete-row"]');
    delRow.disabled = !(rect ? rect.top : session?.row);
    delRow.textContent = rect && rect.top !== rect.bottom ? "Delete selected rows" : "Delete row";
    const delCol = shell.querySelector('[data-command="delete-column"]');
    delCol.disabled = (rect ? rect.right-rect.left+1 : 1) >= width;
    delCol.textContent = rect && rect.left !== rect.right ? "Delete selected columns" : "Delete column";
    shell.querySelector('[data-command="above"]').disabled = !session?.row;
    shell.querySelector('[data-command="move-up"]').disabled = !(session?.row > 1);
    shell.querySelector('[data-command="move-down"]').disabled = !session?.row || session.row === table.rows.length - 1;
    shell.querySelector('[data-command="move-left"]').disabled = !session?.col;
    shell.querySelector('[data-command="move-right"]').disabled = !session || session.col === width - 1;
    paintCellSelection(shell,this.range);
    if (wasFocused && session && document.activeElement !== input) input.focus({ preventScroll: true });
    return true;
  }
  destroy(shell) { shell.closeMenu(); shell.stopMeasuring(); paintCellSelection(shell,null); }
  ignoreEvent() { return true; }
}
