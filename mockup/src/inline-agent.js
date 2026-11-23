import { Facet, StateEffect, StateField } from "@codemirror/state";
import { Decoration, EditorView, WidgetType } from "@codemirror/view";
import { syntaxTree } from "@codemirror/language";
import { button, el, icon } from "./ui.js";

export const undoGroup = Facet.define({ combine: values => values[0] });
export const focusAgentCell = Facet.define({ combine: values => values[0] });
const setAgent = StateEffect.define();

function localReply(source, prompt) {
  const literal = prompt.match(/^(?:replace (?:it |this )?with|insert|write)\s*:\s*([\s\S]*)$/i);
  if (literal) return { proposed: literal[1] };
  if (/checklist|task list/i.test(prompt)) {
    const tasks = source.split(/\n+|(?<=[.!?])\s+/u)
      .map(s => s.trim().replace(/^[-*]\s+(?:\[[ xX]\]\s*)?/, "")).filter(Boolean);
    if (tasks.length) return { proposed: tasks.map(s => "- [ ] " + s).join("\n") };
  }
  if (/shorten|concise|tighten/i.test(prompt)) {
    const first = source.trim().split(/(?<=[.!?])\s+|\n/u)[0];
    const shorter = first.replace(/\bin order to\b/gi, "to").replace(/\b(?:very|really|basically)\s+/gi, "");
    if (shorter && shorter !== source) return { proposed: shorter };
    return { answer: "This passage is already one short line. Use “Replace with: …” to try specific wording." };
  }
  if (/\?|describe|what is selected/i.test(prompt)) {
    const words = source.trim().split(/\s+/u).filter(Boolean).length;
    const opening = source.trim().split(/\n/u)[0] || "";
    return { answer: opening
      ? `The selected Markdown contains ${words} ${words === 1 ? "word" : "words"}. It opens with “${opening}”.`
      : "The caret is on an empty line. Use “Insert: …” to draft Markdown here." };
  }
  return { answer: "This demo can shorten a passage, turn it into a checklist, describe the selection, or use your exact Markdown after “Replace with: …”." };
}

function change(view, state) {
  view.dispatch({ effects: setAgent.of(state) });
}

function dismiss(view) {
  const target = view.state.field(agentState);
  change(view, null);
  if (!target?.cell || !view.state.facet(focusAgentCell)?.(view, target.from)) view.focus();
  return true;
}

function focusComposer(view) {
  requestAnimationFrame(() => view.dom.querySelector(".agent-form input")?.focus({ preventScroll: true }));
}

function ask(view, prompt) {
  const current = view.state.field(agentState);
  if (!current || !prompt.trim()) return;
  change(view, { ...current, prompt, ...localReply(current.original, prompt.trim()), stage: "reply" });
}

function apply(view) {
  const current = view.state.field(agentState);
  if (!current || current.stale || current.proposed == null ||
      view.state.sliceDoc(current.from, current.to) !== current.original) return;
  const undo = view.state.facet(undoGroup);
  undo.stopCapturing();
  view.dispatch({
    changes: { from: current.from, to: current.to, insert: current.proposed },
    selection: { anchor: current.from + current.proposed.length },
    effects: setAgent.of(null), userEvent: "input.agent"
  });
  undo.stopCapturing();
  if (!current.cell || !view.state.facet(focusAgentCell)?.(view, current.from)) view.focus();
  document.querySelector("#announcement").textContent = "Applied Markdown edit.";
}

function diff(label, content, removed) {
  const row = el("div", "diff " + (removed ? "old" : "new"));
  row.setAttribute("aria-label", label);
  row.append(el("span", "", removed ? "−" : "+"), el("span", "", content || "(empty)"));
  return row;
}

class AgentPanel extends WidgetType {
  constructor(state) { super(); this.model = state; }
  eq(other) {
    return ["stage", "stale", "original", "prompt", "answer", "proposed"].every(key => this.model[key] === other.model[key]);
  }
  toDOM(view) {
    const model = this.model;
    const box = el("section", "agent-panel");
    box.setAttribute("aria-label", "Inline agent");
    box.contentEditable = "false";
    const head = el("div", "agent-head");
    const close = button("", "icon-button", () => dismiss(view));
    close.append(icon("x"));
    close.setAttribute("aria-label", "Dismiss inline agent");
    head.append(icon("sparkles"), el("strong", "", "Inline agent"), el("small", "", "Local demo"), close);
    box.append(head);
    const actions = el("div", "agent-actions");
    const reopen = () => {
      const latest = view.state.field(agentState);
      const original = view.state.sliceDoc(latest.from, latest.to);
      change(view, { from: latest.from, to: latest.to, cell: latest.cell, original, prompt: "", stage: "compose", stale: false });
      focusComposer(view);
    };
    if (model.stale) {
      box.append(el("div", "agent-warning", "This passage changed. Review the current text before proposing another edit."));
      actions.append(button("Use current text", "primary", reopen), button("Dismiss", "", () => dismiss(view)));
    } else if (model.stage === "compose") {
      const form = el("form", "agent-form");
      const input = el("input");
      input.placeholder = "Ask about this text or request an edit…";
      input.setAttribute("aria-label", "Message the inline agent");
      const send = el("button", "primary");
      send.type = "submit";
      send.setAttribute("aria-label", "Send to inline agent");
      send.append(icon("arrow-up"));
      form.append(input, send);
      form.addEventListener("submit", event => { event.preventDefault(); ask(view, input.value); });
      box.append(form);
      for (const preset of ["Shorten", "Make a checklist", "Describe selection"]) {
        actions.append(button(preset, "", () => ask(view, preset)));
      }
    } else {
      box.append(el("p", "agent-question", model.prompt));
      if (model.proposed != null) {
        box.append(diff("Original Markdown", model.original, true), diff("Proposed Markdown", model.proposed, false));
        actions.append(button("Apply edit", "primary", () => apply(view)), button("Discard", "", () => dismiss(view)));
      } else {
        box.append(el("div", "agent-answer", model.answer));
        actions.append(button("Ask again", "", reopen));
      }
    }
    box.append(actions);
    box.addEventListener("keydown", event => {
      if (event.key === "Escape") { event.preventDefault(); dismiss(view); }
      event.stopPropagation();
    });
    return box;
  }
  ignoreEvent() { return true; }
}

export const agentState = StateField.define({
  create: () => null,
  update(value, tr) {
    for (const effect of tr.effects) if (effect.is(setAgent)) return effect.value;
    if (!value || !tr.docChanged) return value;
    const from = tr.changes.mapPos(value.from, 1);
    const to = Math.max(from, tr.changes.mapPos(value.to, value.from === value.to ? 1 : -1));
    return { ...value, from, to, stale: value.stale || tr.newDoc.sliceString(from, to) !== value.original };
  },
  provide: field => EditorView.decorations.compute([field], state => {
    const model = state.field(field);
    if (!model) return Decoration.none;
    const ranges = [];
    if (model.from < model.to) ranges.push(Decoration.mark({ class: "agent-range" }).range(model.from, model.to));
    let anchor = state.doc.lineAt(model.to).to;
    for (let node = syntaxTree(state).resolveInner(model.to, -1); node; node = node.parent) {
      if (node.name === "Table") { anchor = node.to; break; }
    }
    ranges.push(Decoration.widget({ widget: new AgentPanel(model), block: true, side: 1 }).range(anchor));
    return Decoration.set(ranges, true);
  })
});

export function openAgent(view, explicitTarget) {
  const sel = view.state.selection.main;
  const target = explicitTarget || (sel.empty ? view.state.doc.lineAt(sel.head) : sel);
  const original = view.state.sliceDoc(target.from, target.to);
  change(view, { from: target.from, to: target.to, cell: Boolean(explicitTarget), original, prompt: "", stage: "compose", stale: false });
  requestAnimationFrame(() => {
    view.dom.querySelector(".agent-panel")?.scrollIntoView({ block: "nearest" });
    focusComposer(view);
  });
  return true;
}
