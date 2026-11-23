// Hit-test the rendered letters. A formatted label can omit Markdown bytes,
// wrap, align right, or be clipped by the horizontal scroller.
import { EditorSelection } from "@codemirror/state";
import { cellTextOffsets } from "./table-model.js";

const graphemes = new Intl.Segmenter(undefined, { granularity: "grapheme" });
const distance = (n, a, b) => Math.max(a - n, n - b, 0);

export function offsetAt(label, x, y) {
  const caret = document.caretPositionFromPoint?.(x, y);
  const range = !caret && document.caretRangeFromPoint?.(x, y);
  const node = caret?.offsetNode || range?.startContainer;
  const box = label.getBoundingClientRect();
  if (x >= box.left && x <= box.right && y >= box.top && y <= box.bottom
      && node?.nodeType === Node.TEXT_NODE && label.contains(node)) {
    return (cellTextOffsets.get(node) ?? 0) + (caret?.offset ?? range.startOffset);
  }
  // Outside the glyphs (cell padding or a rail), choose an actual caret on
  // the nearest displayed line. Range geometry also works on a hidden label.
  const walker = document.createTreeWalker(label, NodeFilter.SHOW_TEXT);
  const measure = document.createRange();
  let best = 0, dy = Infinity, dx = Infinity;
  while (walker.nextNode()) {
    const text = walker.currentNode;
    const base = cellTextOffsets.get(text) ?? 0;
    const end = text.length;
    for (const at of [...graphemes.segment(text.data)].map(s => s.index).concat(end)) {
      measure.setStart(text, at); measure.collapse(true);
      const rect = measure.getBoundingClientRect();
      if (!rect.height) continue;
      const vertical = distance(y, rect.top, rect.bottom), horizontal = Math.abs(rect.x - x);
      if (vertical < dy || (vertical === dy && horizontal < dx)) {
        dy = vertical; dx = horizontal; best = base + at;
      }
    }
  }
  return best;
}

export function cellAtPoint(shell, x, y) {
  const grid = shell.querySelector("table");
  const frame = shell.querySelector(".grid-scroll").getBoundingClientRect();
  const visibleX = Math.min(Math.max(frame.left + 0.5, x), frame.right - 0.5);
  const rows = [...grid.rows];
  const row = rows.find(tr => y < tr.getBoundingClientRect().bottom) || rows.at(-1);
  const cells = [...row.cells];
  const td = cells.find(td => visibleX < td.getBoundingClientRect().right) || cells.at(-1);
  const cell = shell.gridModel.rows[row.rowIndex][td.cellIndex];
  const label = td.querySelector(".grid-value"), rect = td.getBoundingClientRect();
  const pos = offsetAt(label, visibleX, Math.min(Math.max(rect.top + 1, y), rect.bottom - 1));
  return { row: row.rowIndex, col: td.cellIndex, offset: Math.max(0, Math.min(cell.to - cell.from, pos)), from: cell.from };
}

// Let CodeMirror own a drag that starts in prose, including remapping both
// endpoints through edits from another writer. Resolve the far end inside a
// replacement widget using the same glyph hit test as a click on that widget.
export function dragIntoGrid(view, down) {
  if (down.button !== 0 || down.detail > 1 || down.altKey || down.metaKey || down.ctrlKey
      || down.target.closest(".md-grid-wrap") || !view.dom.querySelector(".md-grid-wrap")) return null;
  let start = view.posAtCoords({ x: down.clientX, y: down.clientY });
  if (start == null) return null;
  let initial = view.state.selection;
  return {
    update(update) {
      if (update.docChanged) { start = update.changes.mapPos(start); initial = initial.map(update.changes); }
    },
    get(event, extend) {
      const grid = document.elementFromPoint(event.clientX,event.clientY)?.closest(".md-grid-wrap");
      const hit = grid && view.dom.contains(grid) ? cellAtPoint(grid,event.clientX,event.clientY) : null;
      const head = hit ? hit.from + hit.offset : view.posAtCoords({ x: event.clientX, y: event.clientY }, false) ?? start;
      return EditorSelection.single(extend ? initial.main.anchor : start, head);
    }
  };
}

export function paintCellSelection(shell, range) {
  if (!globalThis.Highlight || !globalThis.CSS?.highlights) return;
  let highlight = CSS.highlights.get("margin-grid-selection");
  if (!highlight) { highlight = new Highlight(); CSS.highlights.set("margin-grid-selection",highlight); }
  for (const r of shell.selectionRanges || []) highlight.delete(r);
  shell.selectionRanges = [];
  if (!range) return;
  const [from,to] = range;
  for (const td of shell.querySelectorAll("th,td")) {
    const cell = shell.gridModel.rows[td.parentElement.rowIndex][td.cellIndex];
    if (to <= cell.from || from >= cell.to) continue;
    const walker = document.createTreeWalker(td.querySelector(".grid-value"),NodeFilter.SHOW_TEXT);
    while (walker.nextNode()) {
      const text = walker.currentNode, base = cell.from + (cellTextOffsets.get(text) ?? 0);
      const a = Math.max(0,from-base), b = Math.min(text.length,to-base);
      if (b <= a) continue;
      const r = document.createRange(); r.setStart(text,a); r.setEnd(text,b);
      shell.selectionRanges.push(r); highlight.add(r);
    }
  }
}
