import assert from "node:assert/strict";
import { test } from "node:test";
import { EditorState, StateField } from "@codemirror/state";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { buildCheckbox } from "../build-checkbox.mjs";
import { checkboxKernel } from "../src/checkbox-kernel.js";
import { checkboxEditing, editAsText, keepLiteral, markerAt, markerBeside, markerContext } from "../src/checkbox.js";

const kernel = checkboxKernel(buildCheckbox());
const { rule } = kernel;
const mode = StateField.define({ create: () => "live", update: v => v });
function editor(doc, selection = { anchor: 0 }) {
  const view = {
    state: EditorState.create({ doc, selection, extensions: [markdown({ base: markdownLanguage }), mode,
      checkboxEditing({ mode, ...kernel, manager: { stopCapturing() {} } })] }),
    dispatch(spec) { view.state = view.state.update(spec).state; }
  };
  return view;
}

test("the inline module calls the Rust command with token-relative offsets", () => {
  for (const [from, to] of [[1, 1], [1, 2], [2, 2]]) {
    for (const key of [" ", "x", "X"]) {
      assert.equal(rule(32, from, to, key.charCodeAt(0), false, false), 120);
      assert.equal(rule(88, from, to, key.charCodeAt(0), false, false), 32);
    }
  }
  assert.equal(rule(32, 1, 1, 97, false, false), 0);
  assert.equal(rule(32, 1, 1, 120, false, true), 0);
  assert.equal(rule(32, 1, 1, 32, true, false), -1);
  assert.equal(rule(32, 0, 2, 32, false, false), -1);
});

test("the Rust index maps tasks and prose to browser UTF-16 offsets and protects Markdown links and code", () => {
  const source = "## 音 🎧\n\n-   [ ] ready\n  - [x] nested\n> 1. [X] quoted\n\n- \\[x] escaped\n\n`[ ]`, [ ] prose, [x](url)\n\n~~~md\n- [ ] example\n~~~\n\n- [X]";
  const { state } = editor(source);
  const recognized = [...source.matchAll(/\[[ xX]\]/g)].map(m => Boolean(markerAt(state, m.index + 1, m.index + 2)));
  assert.deepEqual(recognized, [true, true, true, false, false, true, false, false, true]);
  const at = source.indexOf("[ ]");
  assert.equal(markerAt(state, at, at + 3), null);
});

test("literal mode stays with its brackets and can escape them in the actual Markdown", () => {
  const view = editor("- [ ] listen", { anchor: 3 });
  assert.ok(editAsText(view));
  view.dispatch(view.state.replaceSelection("raw"));
  assert.equal(view.state.doc.toString(), "- [raw ] listen");
  assert.equal(markerContext(view.state).raw, true);
  keepLiteral(view);
  assert.equal(view.state.doc.toString(), "- \\[raw ] listen");
  assert.equal(markerContext(view.state), null);
  const second = editor("- [x] listen", { anchor: 4 });
  editAsText(second);
  second.dispatch({ selection: { anchor: 5 } });
  second.dispatch({ selection: { anchor: 3 } });
  assert.ok(markerContext(second.state));
  assert.equal(Boolean(markerContext(second.state).raw), false);
  editAsText(second);
  keepLiteral(second);
  assert.equal(second.state.doc.toString(), "- \\[x] listen");
  assert.equal(markerAt(second.state, 4, 4), null);
});

test("only the adjacent checkbox reveals its Markdown and only its interior consumes typing", () => {
  const source = "音 🎧 [ ] then [x].\n- [ ] next";
  const view = editor(source);
  for (const match of source.matchAll(/\[[ x]\]/g)) {
    const open = match.index;
    for (let offset = -1; offset <= 4; offset++) {
      view.dispatch({ selection: { anchor: open + offset } });
      assert.equal(markerBeside(view.state)?.open, offset >= 0 && offset <= 3 ? open : undefined);
      assert.equal(markerContext(view.state)?.open, offset === 1 || offset === 2 ? open : undefined);
    }
    view.dispatch({ selection: { anchor: open + 2, head: open + 1 } });
    assert.equal(markerBeside(view.state)?.open, open);
  }
  view.dispatch({ selection: { anchor: 0, head: source.length } });
  assert.equal(markerBeside(view.state), null);
  const touching = editor("[ ][x]", { anchor: 3 });
  assert.equal(markerBeside(touching.state).open, 3);
});
