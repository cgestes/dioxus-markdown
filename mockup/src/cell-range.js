// Rows include the header. The anchor stays fixed as the moving corner crosses it.
export function rectangle(anchor, head) {
  if (anchor[0] === head[0] && anchor[1] === head[1]) return null;
  const top = Math.min(anchor[0],head[0]), bottom = Math.max(anchor[0],head[0]);
  const left = Math.min(anchor[1],head[1]), right = Math.max(anchor[1],head[1]);
  return { anchor, head, top, bottom, left, right, count: (bottom-top+1)*(right-left+1) };
}

export function nextCell(rect, row, col, backwards) {
  const width = rect.right-rect.left+1;
  const index = ((row-rect.top)*width + col-rect.left + (backwards ? rect.count-1 : 1)) % rect.count;
  return [rect.top + Math.floor(index/width), rect.left + index%width];
}

export function rangeText(state, table, rect) {
  return table.rows.slice(rect.top,rect.bottom+1).map(row => row.slice(rect.left,rect.right+1)
    .map(cell => state.sliceDoc(cell.from,cell.to).replaceAll("\\|","|")).join("\t")).join("\n");
}

export function rangePaste(rect, text) {
  const value = text.replace(/\r\n?/g,"\n").replace(/\n$/,"");
  const width = rect.right-rect.left+1, height = rect.bottom-rect.top+1;
  if (!/[\t\n]/.test(value)) return Array.from({length:height},()=>Array(width).fill(value));
  return value.split("\n").slice(0,height).map(line => line.split("\t").slice(0,width));
}

export function cellName(row, col) {
  let letters = "";
  for (let n=col+1;n>0;n=Math.floor((n-1)/26)) letters=String.fromCharCode(65+(n-1)%26)+letters;
  return letters+(row+1);
}
