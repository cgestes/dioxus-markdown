import { EditorView } from "@codemirror/view";
import { ensureSyntaxTree } from "@codemirror/language";
import { mode, selectCell, selectedTableCell } from "./live-markdown.js";
import { offsetAt } from "./grid-pointer.js";
import { cellTextOffsets, tableAt } from "./table-model.js";
import { deleteColumn, insertColumn, insertRow } from "./table-commands.js";
import { pasteRectangle } from "./table-paste.js";
import { toggleMarker } from "./checkbox.js";
import { undoGroup } from "./inline-agent.js";
import { containsPoint, el } from "./ui.js";

function sourceAt(passage, offset) {
  for (const run of passage.runs) {
    if (offset < run.text.length) return run.literal ? run.from + offset : offset ? run.to : run.from;
    offset -= run.text.length;
  }
  return passage.runs.at(-1)?.to ?? passage.from;
}

function tableFor(view, start) {
  ensureSyntaxTree(view.state, Math.min(view.state.doc.length, start + 1), 100);
  return tableAt(view.state, start + 1);
}

function changeTable(view, start, changes, row, col) {
  const history = view.state.facet(undoGroup);
  history.stopCapturing();
  view.dispatch({ changes, userEvent: "input.table" });
  history.stopCapturing();
  const cell = tableFor(view, start)?.rows[row]?.[col];
  if (cell) selectCell(view, cell);
}

function add(view, table, column, col = 0) {
  const width = table.rows[0].length, height = table.rows.length;
  changeTable(view, table.from, column ? insertColumn(table, width) : insertRow(table, height),
    column ? 0 : height, column ? width : col);
}

function selectedCell(view) {
  if (view.state.field(mode) !== "split") return null;
  const sel = view.state.selection.main, table = tableAt(view.state, sel.from);
  return table && selectedTableCell(view.state, table);
}

function verticalCell(view, direction) {
  const hit = selectedCell(view);
  if (!hit) return false;
  const { table, r, c } = hit;
  if (r + direction < 0) {
    view.dispatch({ selection: { anchor: table.from }, scrollIntoView: true });
  } else if (r + direction === table.rows.length) add(view, table, false, c);
  else selectCell(view, table.rows[r + direction][c]);
  return true;
}

export const splitTableKeys = [{ key: "Enter", run: v => verticalCell(v, 1), shift: v => verticalCell(v, -1) }];
export const splitPaste = EditorView.domEventHandlers({
  paste(event, view) {
    const text = event.clipboardData?.getData("text/plain");
    if (!text || !/[\n\r\t]/.test(text)) return false;
    const hit = selectedCell(view);
    if (!hit) return false;
    const { table, r, c } = hit;
    const { changes } = pasteRectangle(view.state, table, r, c, text);
    event.preventDefault();
    changeTable(view, table.from, changes, r, c);
    return true;
  }
});

export function splitPreview(page, project) {
  let doc, current, active, passages = [];
  let pressedButton, activeDelete;
  const passageOf = new WeakMap(), locationOf = new WeakMap();

  function content(p, element) {
    passageOf.set(element, p);
    let offset = 0;
    for (const run of p.runs) {
      const s = el("span", run.style, run.text);
      if (run.style === "box") {
        s.dataset.box = run.from;
        s.title = "Toggle checkbox";
      }
      cellTextOffsets.set(s.firstChild, offset);
      offset += run.text.length;
      element.append(s);
    }
    return element;
  }

  function tableElement(block) {
    const shell = el("div", "split-table");
    const scroll = el("div", "read-scroll"), table = el("table"), body = el("tbody");
    const cols = el("colgroup");
    const total = block.widths.reduce((n, w) => n + w, 0);
    for (const w of block.widths) {
      const col = el("col"); col.style.width = (w / total * 100) + "%"; cols.append(col);
    }
    table.style.minWidth = (block.widths.length * 64) + "px";
    table.append(cols);
    for (const [r, row] of block.rows.entries()) {
      const tr = el("tr");
      for (const [c, p] of row.entries()) {
        const td = content(p, el(r ? "td" : "th", "reading-cell"));
        td.style.textAlign = block.align[c];
        locationOf.set(td, { start: block.from, r, c });
        tr.append(td);
      }
      body.append(tr);
    }
    table.append(body); scroll.append(table); shell.append(scroll);
    for (const [name, column] of [["Add column", true], ["Add row", false]]) {
      const rail = el("div", column ? "add-col-rail" : "add-row-rail");
      const button = el("button", "split-add", "+");
      button.type = "button"; button.title = name; button.setAttribute("aria-label", name);
      button.addEventListener("click", event => {
        if (event.detail && pressedButton !== button) return;
        pressedButton = null;
        const model = tableFor(current, block.from);
        if (model) add(current, model, column);
      });
      rail.append(button); shell.append(rail);
    }
    const remove = el("button", "split-delete", "−");
    remove.setAttribute("aria-label", "Delete column");
    remove.type = "button"; remove.hidden = true;
    remove.addEventListener("click", event => {
      if (event.detail && pressedButton !== remove) return;
      pressedButton = null;
      const hit = selectedCell(current);
      if (!hit || hit.table.from !== block.from || hit.table.rows[0].length < 2) return;
      const { table, r, c } = hit;
      changeTable(current, table.from, deleteColumn(current.state,table,c),r,Math.min(c,table.rows[0].length-2));
      document.querySelector("#announcement").textContent = "Deleted a column.";
    });
    shell.append(remove);
    return shell;
  }

  function render(blocks) {
    const children = [];
    passages = blocks.map(block => {
      const p = block.passage;
      let node;
      if (p) {
        const tag = /^h[1-6]$/.test(p.kind) ? p.kind : "div";
        node = content(p, el(tag, "reading-line " + p.kind));
        if (p.depth) node.style.paddingInlineStart = (p.depth * 1.25) + "em";
      } else node = tableElement(block);
      children.push(node);
      return { from: (p || block).from, to: (p || block).to, node };
    });
    page.replaceChildren(...children);
  }

  page.addEventListener("pointerdown", e => {
    if (e.button !== 0) return;
    // A browser can assign a click a fraction outside a button to that button.
    // Only its painted rectangle owns the action. The rest of the rail places the caret.
    const button = e.target.closest("button");
    pressedButton = button && containsPoint(button, e) ? button : null;
    if (pressedButton) return;
    let target = e.target.closest(".reading-line,.reading-cell");
    const shell = e.target.closest(".split-table");
    const scroll = shell?.querySelector(".read-scroll");
    let x = e.clientX, y = e.clientY;
    if (scroll && !target) {
      if (e.target === scroll && y > scroll.getBoundingClientRect().top + scroll.clientHeight) return;
      const frame = scroll.getBoundingClientRect();
      x = Math.max(frame.left + 0.5, Math.min(frame.right - 0.5, x));
      const rows = [...scroll.querySelector("table").rows];
      const row = rows.find(r => y < r.getBoundingClientRect().bottom) || rows.at(-1);
      const cells = [...row.cells];
      target = cells.find(c => x < c.getBoundingClientRect().right) || cells.at(-1);
      const rect = target.getBoundingClientRect();
      y = Math.max(rect.top + 1, Math.min(rect.bottom - 1, y));
    }
    if (!target) return;
    e.preventDefault();
    const box = e.target.closest("[data-box]");
    if (box && !e.altKey && !e.shiftKey) {
      const open = Number(box.dataset.box);
      toggleMarker(current, open);
      current.dispatch({ selection: { anchor: open + 1 }, scrollIntoView: true });
      current.focus(); return;
    }
    const pos = sourceAt(passageOf.get(target), offsetAt(target, x, y));
    const anchor = e.shiftKey ? current.state.selection.main.anchor : pos;
    const loc = locationOf.get(target);
    const cell = loc && tableFor(current, loc.start)?.rows[loc.r]?.[loc.c];
    if (cell) selectCell(current, cell, anchor, pos);
    else { current.dispatch({ selection: { anchor, head: pos }, scrollIntoView: true }); current.focus(); }
  });

  page.addEventListener("pointerup", e => {
    if (pressedButton && !containsPoint(pressedButton, e)) pressedButton = null;
  });
  page.addEventListener("pointercancel", () => { pressedButton = null; });

  return { update(view) {
    const split = view.state.field(mode) === "split";
    document.querySelector("#scroller").classList.toggle("is-split", split);
    document.querySelector("#source-label").hidden = !split;
    document.querySelector("#reading").hidden = !split;
    if (!split) { doc = null; return; }
    current = view;
    const edited = doc !== view.state.doc;
    if (edited) { doc = view.state.doc; render(project(doc)); active = null; }
    const head = view.state.selection.main.head;
    const next = passages.find(p => head >= p.from && head <= p.to)?.node;
    const target = next?.classList.contains("split-table") && selectedCell(view);
    const remove = target && target.table.rows[0].length > 1 ? next.querySelector(".split-delete") : null;
    if (activeDelete !== remove) {
      if (activeDelete) activeDelete.hidden = true;
      if (remove) remove.hidden = false;
      activeDelete = remove;
    }
    if (remove) {
      const header = target.table.rows[0][target.c];
      remove.title = "Delete column " + (target.c+1) + " · " + view.state.sliceDoc(header.from,header.to);
    }
    if (active !== next) {
      active?.classList.remove("at-caret"); next?.classList.add("at-caret");
      active = next;
      if (next) {
        const box = next.getBoundingClientRect(), frame = page.getBoundingClientRect();
        if (box.bottom < frame.top || box.top > frame.bottom) page.scrollTop += box.top - frame.top - 12;
      }
    }
  }};
}
