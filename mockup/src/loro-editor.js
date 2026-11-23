import { Annotation, Facet } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, WidgetType } from "@codemirror/view";
import { diff } from "@codemirror/merge";
import { Cursor, UndoManager } from "loro-crdt/base64";

export const sharedNote = Facet.define({ combine: v => v[0] });
const reflected = Annotation.define();
export const checkedStyle = "markdown.checked";

export function markdownText(text) {
  return text.toDelta().map(({ insert, attributes }) => {
    const checked = attributes?.[checkedStyle];
    return typeof checked === "boolean" ? insert.replace(/[ xX]/g, checked ? "x" : " ") : insert;
  }).join("");
}

export function remember(text, index) {
  const cursor = text.getCursor(index, 0);
  if (!cursor) return null;
  try { return cursor.encode(); } finally { cursor.free(); }
}
export function locate(doc, bytes) {
  if (!bytes) return null;
  const cursor = Cursor.decode(bytes);
  try {
    const result = doc.getCursorPos(cursor);
    result?.update?.free();
    return result?.offset ?? null;
  } catch {
    // Ephemeral cursor messages can arrive before their document updates.
    return null;
  } finally { cursor.free(); }
}
export function equalCursor(a, b) {
  return a === b || Boolean(a && b && a.length === b.length && a.every((v, i) => v === b[i]));
}

export function undoHistory(doc) {
  const manager = new UndoManager(doc, { mergeInterval: 500 });
  let isolated = false;
  return {
    stopCapturing() { manager.setMergeInterval(0); isolated = true; },
    committed() {
      if (!isolated) return;
      // The following commit also begins a new undo group.
      isolated = false;
      manager.setMergeInterval(500);
    },
    undo: () => manager.undo(), redo: () => manager.redo()
  };
}

class PeerCaret extends WidgetType {
  constructor(user) { super(); this.user = user; }
  eq(other) { return other.user.name === this.user.name && other.user.color === this.user.color; }
  toDOM() {
    const node = document.createElement("span");
    node.className = "peer-caret"; node.style.setProperty("--peer", this.user.color);
    node.title = this.user.name; node.setAttribute("aria-hidden", "true");
    return node;
  }
}

const sync = ViewPlugin.fromClass(class {
  constructor(view) {
    this.view = view;
    this.note = view.state.facet(sharedNote);
    this.decorations = Decoration.none;
    this.unsubscribe = this.note.text.subscribe(() => queueMicrotask(() => this.pull()));
    this.changed = () => {
      if (this.redraw || this.closed) return;
      this.redraw = true;
      queueMicrotask(() => { this.redraw = false; if (!this.closed) view.dispatch({}); });
    };
    this.note.awareness.on("change", this.changed);
  }
  pull() {
    if (this.closed) return;
    const { view } = this, before = view.state.doc.toString(), after = markdownText(this.note.text);
    if (before === after) return;
    const changes = diff(before, after).map(c => ({ from: c.fromA, to: c.toA, insert: after.slice(c.fromB, c.toB) }));
    view.dispatch({ changes, annotations: reflected.of(true) });
  }
  update(update) {
    const { doc, text, awareness, id, clientID, manager } = this.note;
    if (update.selectionSet && !update.docChanged && !update.startState.selection.eq(update.state.selection)) {
      manager.stopCapturing();
    }
    if (update.docChanged && !update.transactions.some(tr => tr.annotation(reflected))) {
      let shift = 0;
      update.changes.iterChanges((from, to, _start, _end, insert) => {
        text.splice(from + shift, to - from, insert.toString());
        shift += insert.length - (to - from);
      });
      doc.commit({ origin: "editor" });
      manager.committed();
    }
    if ((update.selectionSet || update.focusChanged) && update.view.hasFocus &&
        !update.view.dom.querySelector(".grid-cell-input:focus")) {
      const sel = update.state.selection.main;
      const cursor = { anchor: remember(text, sel.anchor), head: remember(text, sel.head) };
      const last = awareness.getLocalState()?.cursor;
      if (!last || !equalCursor(cursor.anchor, last.anchor) || !equalCursor(cursor.head, last.head)) {
        awareness.setLocalStateField("cursor", cursor);
      }
    }
    const ranges = [];
    for (const [peer, state] of awareness.getStates()) {
      if (peer === clientID || !state.user || state.note !== id || !state.cursor) continue;
      const anchor = locate(doc, state.cursor.anchor), head = locate(doc, state.cursor.head);
      if (head == null || head > update.state.doc.length || anchor == null || anchor > update.state.doc.length) continue;
      if (head !== anchor) ranges.push(Decoration.mark({ attributes: { style: "background:" + state.user.colorLight } }).range(Math.min(head, anchor), Math.max(head, anchor)));
      ranges.push(Decoration.widget({ widget: new PeerCaret(state.user), side: 1 }).range(head));
    }
    this.decorations = Decoration.set(ranges, true);
  }
  destroy() { this.closed = true; this.unsubscribe(); this.note.awareness.off("change", this.changed); }
}, { decorations: v => v.decorations });

export function collaboration(note) { return [sharedNote.of(note), sync]; }

export function writeChecked(view, open, target) {
  const note = view.state.facet(sharedNote);
  note.manager.stopCapturing();
  note.text.mark({ start: open + 1, end: open + 2 }, checkedStyle, target === "x");
  note.doc.commit({ origin: "checkbox" });
  view.dispatch({ changes: { from: open + 1, to: open + 2, insert: target }, selection: view.state.selection,
    annotations: reflected.of(true), effects: EditorView.announce.of(target === "x" ? "Checked" : "Unchecked") });
  note.manager.stopCapturing();
}

export function applyHistory(view, redo) {
  const manager = view.state.facet(sharedNote).manager;
  if (redo) manager.redo(); else manager.undo();
  view.plugin(sync).pull();
  return true;
}
export const historyKeys = [
  { key: "Mod-z", run: v => applyHistory(v, false), shift: v => applyHistory(v, true) },
  { key: "Mod-y", run: v => applyHistory(v, true) }
];
