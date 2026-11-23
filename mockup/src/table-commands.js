export function textChange(before, after, at = 0) {
  let start = 0, oldEnd = before.length, newEnd = after.length;
  while (start < oldEnd && start < newEnd && before[start] === after[start]) start++;
  if (start && /[\uD800-\uDBFF]/.test(before[start - 1]) && start < oldEnd) start--;
  while (oldEnd > start && newEnd > start && before[oldEnd - 1] === after[newEnd - 1]) { oldEnd--; newEnd--; }
  if (oldEnd < before.length && /[\uDC00-\uDFFF]/.test(before[oldEnd])) { oldEnd++; newEnd++; }
  return { from: at + start, to: at + oldEnd, insert: after.slice(start, newEnd) };
}

export function escapeCell(text) {
  return text.replace(/(\\*)\|/g, (match, slashes) => slashes.length % 2 ? match : "\\" + match);
}

export function insertRow(table, index) {
  const row = "|" + "  |".repeat(table.rows[0].length);
  const next = table.lines[index + 1];
  return next ? { from: next.from, insert: row + "\n" } : { from: table.to, insert: "\n" + row };
}

export function deleteRow(table, index, last = index) {
  return { from: table.lines[index + 1].from - 1, to: table.lines[last + 1].to };
}

export function swapRows(state, table, a, b) {
  const first = table.lines[Math.min(a, b) + 1], second = table.lines[Math.max(a, b) + 1];
  const x = state.sliceDoc(first.from, first.to), y = state.sliceDoc(second.from, second.to);
  return [textChange(x, y, first.from), textChange(y, x, second.from)];
}

export function swapColumns(state, table, a, b) {
  const changes = [], left = Math.min(a, b), right = Math.max(a, b);
  for (const { cells } of table.lines) {
    const x = cells[left], y = cells[right];
    if (!x) continue;
    if (!y) { changes.push({ from: x.start, insert: "  |" }); continue; }
    const first = state.sliceDoc(x.from, x.to), second = state.sliceDoc(y.from, y.to);
    if (first !== second) changes.push(textChange(first, second, x.from), textChange(second, first, y.from));
  }
  return changes;
}

export function insertColumn(table, index, count = 1) {
  const changes = [];
  table.lines.forEach(({ cells, to, trailing }, row) => {
    const parts = Array.from({ length: count }, (_, n) => {
      const value = row === 0 ? "Column " + (table.rows[0].length + n + 1) : row === 1 ? "---" : "";
      return " " + value + " ";
    }).join("|");
    if (index < cells.length) changes.push({ from: cells[index].start, insert: parts + "|" });
    else if (index === cells.length) changes.push({ from: cells.at(-1).end, insert: "|" + parts });
    else changes.push({ from: to, insert: (trailing ? "" : " |") + "  |".repeat(index - cells.length) + parts + "|" });
  });
  return changes;
}

export function deleteColumn(state, table, index, last = index) {
  const width = table.rows[0].length;
  if (index < 0 || last >= width || last < index || last - index + 1 >= width) return [];
  const changes = [];
  for (const line of table.lines) {
    const { cells } = line;
    const end = Math.min(last+1,cells.length);
    if (index >= end) continue;
    const survivors = cells.length - (end-index);
    if (!survivors) changes.push(textChange(state.sliceDoc(line.from,line.to),"|  |",line.from));
    else if (survivors === 1 && cells[0].start === line.from && !line.trailing) {
      const keep = cells[index === 0 ? end : 0], value = state.sliceDoc(keep.start,keep.end);
      changes.push({ from: line.from, to: keep.start, insert: /^[ \t]/.test(value) ? "|" : "| " },
        { from: keep.end, to: line.to, insert: /[ \t]$/.test(value) ? "|" : " |" });
    } else if (cells[end]) changes.push({ from: cells[index].start, to: cells[end].start });
    else changes.push({ from: cells[index-1].end, to: cells[end-1].end });
  }
  return changes;
}
