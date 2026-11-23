export function checkboxKernel(bytes) {
  const wasm = new WebAssembly.Instance(new WebAssembly.Module(bytes)).exports;
  const encoder = new TextEncoder();
  const cache = new WeakMap();
  const previews = new WeakMap();
  const decoder = new TextDecoder();
  function index(doc) {
    if (cache.has(doc)) return cache.get(doc);
    const source = encoder.encode(doc.toString());
    const address = wasm.marker_source(source.length);
    new Uint8Array(wasm.memory.buffer, address, source.length).set(source);
    const count = wasm.index_markers();
    const offsets = new Uint32Array(wasm.memory.buffer, wasm.marker_positions(), count);
    const markers = new Map();
    for (const open of offsets) markers.set(open, { open, close: open + 2 });
    cache.set(doc, markers);
    return markers;
  }
  function preview(doc) {
    if (previews.has(doc)) return previews.get(doc);
    const markdown = doc.toString();
    const bytes = encoder.encode(markdown);
    const ptr = wasm.marker_source(bytes.length);
    new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
    const len = wasm.render_preview();
    const blocks = JSON.parse(decoder.decode(new Uint8Array(wasm.memory.buffer, wasm.preview_bytes(), len)));
    // Rust and Parley use UTF-8. CodeMirror and DOM Range use UTF-16.
    const positions = new Uint32Array(bytes.length + 1);
    let i = 0, character = 0;
    for (const ch of markdown) {
      const cp = ch.codePointAt(0);
      i += cp <= 0x7f ? 1 : cp <= 0x7ff ? 2 : cp <= 0xffff ? 3 : 4;
      character += ch.length;
      positions[i] = character;
    }
    const convert = range => { range.from = positions[range.from]; range.to = positions[range.to]; };
    const passage = p => { convert(p); p.runs.forEach(convert); };
    for (const block of blocks) {
      if (block.type === "text") passage(block.passage);
      else { convert(block); block.rows.forEach(row => row.forEach(passage)); }
    }
    previews.set(doc, blocks);
    return blocks;
  }
  function format(doc, anchor, head, action = 4) {
    const bytes = encoder.encode(doc.toString());
    const ptr = wasm.marker_source(bytes.length);
    new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
    const len = wasm.inline_format(anchor, head, action);
    return len ? JSON.parse(decoder.decode(new Uint8Array(wasm.memory.buffer, wasm.preview_bytes(), len))) : null;
  }
  return { rule: wasm.checkbox_input, index, preview, format };
}
