import { syntaxTree } from "@codemirror/language";
import { el } from "./ui.js";

function children(node) {
  const list = [];
  for (let child = node.firstChild; child; child = child.nextSibling) list.push(child);
  return list;
}

// The Markdown parser identifies separator pipes, including escaped-pipe cases.
// Keep a source span for blank cells too, since those have no TableCell node.
function readRow(state, node, divider = false) {
  const parts = children(node);
  const rowText = state.sliceDoc(node.from, node.to);
  const pipes = divider
    ? [...rowText.matchAll(/\|/g)].map(m => ({ from: node.from + m.index, to: node.from + m.index + 1 }))
    : parts.filter(part => part.name === "TableDelimiter");
  const leading = pipes.length > 0 && rowText.trimStart().startsWith("|");
  const trailing = pipes.length > 0 && rowText.slice(pipes.at(-1).to - node.from).trim() === "";
  const start = leading ? pipes[0].to : node.from;
  const end = Math.max(start, trailing ? pipes.at(-1).from : node.to);
  const separators = pipes.filter(pipe => pipe.from >= start && pipe.from < end);
  const spans = [];
  let left = start;
  for (const right of [...separators.map(pipe => pipe.from), end]) {
    const raw = state.sliceDoc(left, right);
    const from = left + (raw.trim() ? raw.length - raw.trimStart().length : Math.floor(raw.length / 2));
    const to = Math.max(from, left + raw.trimEnd().length);
    spans.push({ from, to, start: left, end: right, node: parts.find(p => p.name === "TableCell" && p.from >= left && p.to <= right) });
    left = right + 1;
  }
  return { from: node.from, to: node.to, cells: spans, trailing };
}

export function readTable(state, node) {
  const headerNode = node.getChild("TableHeader"), bodyNodes = node.getChildren("TableRow");
  const delimiter = node.getChild("TableDelimiter");
  const lines = [headerNode, delimiter, ...bodyNodes].map((n, i) => readRow(state, n, i === 1));
  const width = lines[0].cells.length;
  const rows = lines.filter((_, i) => i !== 1).map(line => {
    if (line.cells.length === width) return line.cells;
    const cells = line.cells.slice(0, width);
    for (let col = cells.length; col < width; col++) {
      const prefix = (line.trailing ? "" : " |") + "  |".repeat(col - line.cells.length) + " ";
      cells.push({ from: line.to, to: line.to, start: line.to, end: line.to, prefix });
    }
    return cells;
  });
  const align = lines[1].cells.map(cell => {
    const value = state.sliceDoc(cell.from, cell.to);
    if (value.startsWith(":") && value.endsWith(":")) return "center";
    return value.endsWith(":") ? "right" : "left";
  });
  return { from: node.from, to: node.to, rows, lines, align, source: state.sliceDoc(node.from, node.to) };
}

// Store offsets relative to this cell. An edit earlier in the note can move
// the table without invalidating a cached row's DOM text.
export const cellTextOffsets = new WeakMap();
export function appendInline(parent, state, node, base = node.from) {
  function appendText(target, from, to) {
    if (from >= to) return;
    const text = document.createTextNode(state.sliceDoc(from, to));
    cellTextOffsets.set(text, from - base);
    target.append(text);
  }
  function punctuation(target, from, to) {
    const mark = el("span","grid-delimiter");
    mark.setAttribute("aria-hidden","true");
    appendText(mark,from,to); target.append(mark);
  }
  const name = node.name;
  if (["EmphasisMark", "StrikethroughMark", "CodeMark", "LinkMark", "LinkTitle"].includes(name)) {
    punctuation(parent,node.from,node.to); return;
  }
  if (name === "Escape") {
    punctuation(parent,node.from,node.from+1); appendText(parent,node.from+1,node.to); return;
  }
  const tag = { StrongEmphasis: "strong", Emphasis: "em", InlineCode: "code", Strikethrough: "s", Link: "span" }[name];
  const target = tag ? el(tag, name === "Link" ? "md-link" : "") : parent;
  if (tag) parent.append(target);
  if (name === "Link") {
    const marks = node.getChildren("LinkMark");
    if (marks.length >= 2) {
      punctuation(target,node.from,marks[0].to);
      appendText(target,marks[0].to,marks[1].from);
      punctuation(target,marks[1].from,node.to);
      return;
    }
  }
  let pos = node.from;
  for (const child of children(node)) {
    if (pos < child.from) appendText(target, pos, child.from);
    appendInline(target, state, child, base);
    pos = child.to;
  }
  if (pos < node.to) appendText(target, pos, node.to);
}

export function tableAt(state, pos) {
  for (const side of [-1, 1]) {
    let node = syntaxTree(state).resolveInner(pos, side);
    while (node) {
      if (node.name === "Table") return readTable(state, node);
      node = node.parent;
    }
  }
  return null;
}

