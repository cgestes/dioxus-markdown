import { Facet, Prec, StateEffect, StateField } from "@codemirror/state";
import { Decoration, EditorView } from "@codemirror/view";

const config = Facet.define({ combine: values => values[0] });
const literal = StateEffect.define();

export const literalMarker = StateField.define({
  create: () => null,
  update(value, tr) {
    const set = tr.effects.find(e => e.is(literal));
    if (set) return set.value;
    if (!value || tr.reconfigured) return null;
    const open = tr.changes.mapPos(value.open, 1), close = tr.changes.mapPos(value.close, 1);
    const sel = tr.newSelection;
    if (sel.ranges.length !== 1 || sel.main.from <= open || sel.main.to > close ||
        tr.newDoc.sliceString(open, open + 1) !== "[" || tr.newDoc.sliceString(close, close + 1) !== "]") return null;
    return value.open === open && value.close === close ? value : { open, close };
  }
});

export function markersIn(state) {
  return state.facet(config).index(state.doc);
}

export function markerAt(state, from = state.selection.main.from, to = state.selection.main.to) {
  const markers = markersIn(state);
  const token = markers.get(from - 1) ?? markers.get(from - 2);
  return token && from > token.open && to <= token.close ? token : null;
}

// The adjacent caret reveals the Markdown. Only the interior owns toggle keys.
export function markerBeside(state, range = state.selection.main) {
  const inside = markerAt(state, range.from, range.to);
  if (inside || !range.empty) return inside;
  const markers = markersIn(state);
  return markers.get(range.head) ?? markers.get(range.head - 3) ?? null;
}

export function markerContext(state, from = state.selection.main.from, to = state.selection.main.to) {
  const options = state.facet(config);
  if (!options || state.field(options.mode) === "source" || state.selection.ranges.length !== 1) return null;
  const raw = state.field(literalMarker);
  return raw && from > raw.open && to <= raw.close ? { ...raw, raw: true } : markerAt(state, from, to);
}

export function editAsText(view) {
  const marker = markerContext(view.state);
  if (!marker) return false;
  view.dispatch({ effects: literal.of(marker.raw ? null : { open: marker.open, close: marker.close }) });
  return true;
}

export function keepLiteral(view) {
  const marker = view.state.field(literalMarker);
  if (!marker) return false;
  const manager = view.state.facet(config).manager;
  manager.stopCapturing();
  view.dispatch({ changes: { from: marker.open, insert: "\\" }, effects: literal.of(null), userEvent: "input.task.escape" });
  manager.stopCapturing();
  return true;
}

function replaceState(view, open, target) {
  view.dispatch({ changes: { from: open + 1, to: open + 2, insert: target },
    selection: view.state.selection, effects: EditorView.announce.of(target === "x" ? "Checked" : "Unchecked"), userEvent: "input.task" });
}

function setMarker(view, open, target) {
  const write = view.state.facet(config)?.write;
  if (write) write(view, open, target);
  else replaceState(view, open, target);
  return true;
}

export function toggleMarker(view, open) {
  if (view.state.facet(config) && !markerAt(view.state, open + 1, open + 1)) return false;
  return setMarker(view, open, view.state.sliceDoc(open + 1, open + 2) === " " ? "x" : " ");
}

function handleText(view, value, repeat = false, from = view.state.selection.main.from, to = view.state.selection.main.to) {
  const { state } = view, marker = markerContext(state, from, to);
  if (!marker || marker.raw) return false;
  const typed = value != null && [...value].length === 1 ? value.codePointAt(0) : 0xffffffff;
  const result = state.facet(config).rule(state.sliceDoc(marker.open + 1, marker.close).charCodeAt(0),
    from - marker.open, to - marker.open, typed, false, repeat);
  if (result > 0) setMarker(view, marker.open, String.fromCharCode(result));
  return result >= 0;
}

function editorTarget(event, view) {
  return event.target === view.contentDOM || (view.contentDOM.contains(event.target) &&
    !event.target.closest?.("input, textarea, button, [contenteditable=false]"));
}

function checkboxMarks(state, options) {
  if (state.field(options.mode) !== "live") return Decoration.none;
  const markers = markersIn(state), raw = state.field(literalMarker);
  const near = new Set(state.selection.ranges.map(range => markerBeside(state, range)?.open));
  const marks = [];
  for (const { open, close } of markers.values()) {
    if (raw?.open === open) continue;
    const line = state.doc.lineAt(open);
    const before = state.sliceDoc(line.from, open).trim();
    const after = state.sliceDoc(close + 1, line.to).trim();
    const list = /^(?:>\s*)*(?:[-*+]|\d+[.)])$/.test(before);
    const label = (list ? after : before.split(/[.!?]\s+/).at(-1)?.trim() || after) || "Checkbox";
    marks.push(Decoration.mark({
      class: "md-checkbox" + (near.has(open) ? " md-checkbox-near" : ""),
      attributes: {
        role: "checkbox", tabindex: "0", "aria-label": label,
        "aria-checked": String(state.sliceDoc(open + 1, close) !== " "),
        title: "Toggle checkbox · Alt-click to place the caret inside"
      }
    }).range(open, close + 1));
  }
  if (raw) marks.push(Decoration.mark({
    class: "md-checkbox md-checkbox-literal" + (markers.has(raw.open) ? "" : " md-checkbox-free")
  }).range(raw.open, raw.close + 1));
  return Decoration.set(marks, true);
}

function checkboxTarget(event) {
  return event.target.closest?.(".md-checkbox[role=checkbox]");
}

function placeInCheckbox(view, target) {
  const open = view.posAtDOM(target);
  view.dispatch({ selection: { anchor: open + 1 } });
  view.focus();
  return open;
}

export function checkboxEditing(options) {
  return [
    literalMarker, config.of(options),
    // Presence decorations may cross the token. Keep a single box around all three characters.
    EditorView.outerDecorations.compute(["selection", "doc", literalMarker, options.mode], state => checkboxMarks(state, options)),
    Prec.highest(EditorView.domEventHandlers({
      mousedown(event) {
        // Keep the target in place until click. Shift-click still selects document text.
        return event.button === 0 && !event.shiftKey && !event.metaKey && !event.ctrlKey && !!checkboxTarget(event);
      },
      click(event, view) {
        const target = checkboxTarget(event);
        if (!target || event.button !== 0 || event.shiftKey || event.metaKey || event.ctrlKey) return false;
        const open = placeInCheckbox(view, target);
        if (!event.altKey) toggleMarker(view, open);
        return true;
      },
      keydown(event, view) {
        if (!editorTarget(event, view) || event.isComposing || view.composing || event.keyCode === 229 ||
            event.ctrlKey || event.metaKey || event.altKey) return false;
        const target = checkboxTarget(event);
        if (target && ["Enter", " ", "x", "X", "F2"].includes(event.key)) {
          if (event.repeat) return true;
          const open = placeInCheckbox(view, target);
          return event.key === "F2" ? editAsText(view) : toggleMarker(view, open);
        }
        if (target && (event.key.length === 1 || ["Backspace", "Delete"].includes(event.key))) return true;
        if (event.key === "F2") return event.repeat ? !!markerContext(view.state) : editAsText(view);
        if (event.key === "Escape" && view.state.field(literalMarker)) return editAsText(view);
        if (["Backspace", "Delete", "Enter"].includes(event.key)) return handleText(view, null);
        return [...event.key].length === 1 && handleText(view, event.key, event.repeat);
      },
      beforeinput(event, view) {
        if (!editorTarget(event, view) || !/^(insert|delete)/.test(event.inputType)) return false;
        const typed = event.inputType === "insertText" && !event.isComposing && !view.composing ? event.data : null;
        return handleText(view, typed);
      },
      paste: (event, view) => editorTarget(event, view) && handleText(view, null),
      cut: (event, view) => editorTarget(event, view) && handleText(view, null),
      drop(event, view) {
        const at = view.posAtCoords({ x: event.clientX, y: event.clientY });
        return at != null && editorTarget(event, view) && handleText(view, null, false, at, at);
      }
    })),
    EditorView.inputHandler.of((view, from, to, text) => handleText(view, view.composing ? null : text, false, from, to))
  ];
}
