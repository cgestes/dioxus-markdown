import { EditorState, Prec } from "@codemirror/state";
import { EditorView, drawSelection, keymap } from "@codemirror/view";
import { defaultKeymap } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { syntaxTree } from "@codemirror/language";
import { collaboration, historyKeys, locate, markdownText, remember, sharedNote, undoHistory, writeChecked } from "./loro-editor.js";
import { openNotebook } from "./notebook.js";
import { agentState, openAgent, undoGroup, focusAgentCell } from "./inline-agent.js";
import { liveMarkdown, mode, setMode, tableKeys } from "./live-markdown.js";
import { activateCell, gridSession, paintGridPeers, focusCellAt } from "./grid.js";
import { splitPreview, splitTableKeys, splitPaste } from "./split.js";
import { tableAt } from "./table-model.js";
import { listKeys } from "./lists.js";
import { checkboxEditing, editAsText, keepLiteral, literalMarker, markerContext } from "./checkbox.js";
import { floatingFormatting } from "./formatting.js";
import { checkboxKernel } from "./checkbox-kernel.js";
import { button, el, icon } from "./ui.js";

const $ = id => document.getElementById(id);
// The deployed site keeps an offline copy for the installed web app (public/sw.js).
if (location.protocol === "https:" && "serviceWorker" in navigator)
  navigator.serviceWorker.register("sw.js").catch(() => {});
const isMac = /Mac|iPhone|iPad/.test(navigator.platform);
// iOS zooms into any focused field under 16px and stays zoomed. maximum-scale stops that;
// Safari still allows pinch zoom. Other browsers keep their default viewport.
if (/iPhone|iPad/.test(navigator.platform) || (isMac && navigator.maxTouchPoints > 1))
  document.querySelector('meta[name="viewport"]').content += ", maximum-scale=1";
for (const key of document.querySelectorAll("[data-key]")) key.textContent = (isMac ? "⌘ " : "Ctrl ") + key.dataset.key;

async function start() {
  const notebook = await openNotebook(status => { $("storage").textContent = status; });
  const { files, awareness, canShare, clientID } = notebook;
  const vaultLink = $("private-notebook"), link = new URL(location.href);
  if (notebook.privateMode) {
    $("vaultSync").hidden = false;
    $("vault-publish").hidden = !notebook.replica.paired();
    $("vault-pair-file").onchange = async event => {
      const file = event.target.files[0];
      if (!file) return;
      try {
        await notebook.flush();
        await notebook.replica.pair(await file.text());
      } catch (error) { $("relay-status").textContent = error.message; }
    };
    $("vault-publish").onclick = async () => {
      try { openNote(await notebook.publish(view.state.doc.toString())); }
      catch (error) { $("relay-status").textContent = error.message; }
    };
    vaultLink.textContent = "Lock notebook";
    vaultLink.onclick = async () => {
      try { await notebook.flush(); location.reload(); }
      catch (error) { $("storage").textContent = error.message; }
    };
    window.addEventListener("pageshow", event => { if (event.persisted) location.reload(); });
    $("folder").lastChild.textContent = "Private notebook";
    document.querySelector(".crumb-folder").textContent = "Private notebook";
  } else {
    link.searchParams.set("private", "1");
    link.hash = "";
    vaultLink.onclick = () => location.assign(link);
  }
  const wasm = Uint8Array.from(atob(MARGIN_CHECKBOX_WASM), c => c.charCodeAt(0));
  const kernel = checkboxKernel(wasm);
  const undo = new WeakMap(), bookmarks = new Map();
  let active, view, dialogKind;
  let preferredMode = new URLSearchParams(location.search).get("view") === "split" ? "split" : "live";
  const reading = splitPreview($("reading-page"), kernel.preview);

  function closeSidebar() {
    document.querySelector(".app").classList.remove("nav-open");
    $("scrim").hidden = true;
    $("nav-toggle").setAttribute("aria-expanded", "false");
  }
  $("nav-toggle").onclick = () => {
    const open = document.querySelector(".app").classList.toggle("nav-open");
    $("scrim").hidden = !open;
    $("nav-toggle").setAttribute("aria-expanded", String(open));
  };
  $("scrim").onclick = closeSidebar;
  $("folder").onclick = () => {
    $("notes").hidden = !$("notes").hidden;
    $("folder").setAttribute("aria-expanded", String(!$("notes").hidden));
  };
  function renderAttachments() {
    if (!notebook.privateMode || !active) return;
    $("vault-files").hidden = false;
    const list = $("vault-file-list");
    list.replaceChildren();
    for (const [id, manifest] of notebook.attachments(active)) {
      list.append(button(manifest.name, "file", async () => {
        try {
          const handle = await window.showSaveFilePicker?.({ suggestedName: manifest.name });
          const blob = await notebook.extract(id, manifest, handle);
          if (blob) {
            const url = URL.createObjectURL(blob), link = document.createElement("a");
            link.href = url;
            link.download = manifest.name;
            link.click();
            setTimeout(() => URL.revokeObjectURL(url), 1500);
          }
        } catch (error) {
          if (error.name !== "AbortError") $("vault-file-status").textContent = error.message;
        }
      }));
    }
  }
  $("vault-attach").onclick = () => $("vault-file-input").click();
  $("vault-file-input").onchange = async event => {
    const file = event.target.files[0], room = active;
    if (!file) return;
    try {
      await notebook.attach(room, file);
      $("vault-file-status").textContent = "";
      renderAttachments();
    } catch (error) { $("vault-file-status").textContent = error.message; }
    event.target.value = "";
  };
  function renderFiles() {
    renderAttachments();
    $("notes").replaceChildren();
    for (const [id, name] of files) {
      const item = button("", "file", () => { openNote(id); view.focus(); });
      if (id === active) item.setAttribute("aria-current", "page");
      item.append(icon("file-text"), el("span", "", name));
      $("notes").append(item);
    }
  }

  function renderOutline() {
    const links = [];
    syntaxTree(view.state).iterate({ enter(node) {
      if (!/^ATXHeading[1-6]$/.test(node.name)) return;
      const source = view.state.sliceDoc(node.from, node.to);
      const name = source.replace(/^#{1,6}\s+/, "").replace(/\s+#+\s*$/, "");
      const anchor = node.from;
      const item = button(name, "outline-link", () => {
        view.dispatch({ selection: { anchor }, effects: EditorView.scrollIntoView(anchor, { y: "start", yMargin: 20 }) });
        view.focus(); closeSidebar();
      });
      item.dataset.offset = node.from;
      links.push(item);
    } });
    $("outline").replaceChildren(...links);
    document.querySelector(".outline-group").hidden = !links.length;
  }
  function status(recount = false) {
    const { state } = view;
    const pos = state.selection.main.head;
    const line = state.doc.lineAt(pos);
    const marker = markerContext(state);
    $("marker-hint").hidden = !marker;
    $("marker-hint").parentElement.classList.toggle("has-marker-hint", Boolean(marker));
    if (marker) {
      $("marker-help").textContent = marker.raw ? "Editing bracket text" : "Space or X toggles";
      $("marker-action").textContent = marker.raw ? "Keep literal" : "Edit as text";
      $("marker-shortcut").textContent = marker.raw ? "" : "F2";
      view.contentDOM.setAttribute("aria-describedby", "marker-hint");
    } else view.contentDOM.removeAttribute("aria-describedby");
    $("position").textContent = `Ln ${line.number}, Col ${pos - line.from + 1}`;
    if (recount) {
      const words = state.doc.toString().match(/[\p{L}\p{N}]+(?:[’'-][\p{L}\p{N}]+)*/gu) || [];
      $("word-count").textContent = `${words.length} ${words.length === 1 ? "word" : "words"}`;
      renderOutline();
    }
    const links = [...$("outline").children];
    const current = links.filter(item => Number(item.dataset.offset) <= pos).at(-1) || links[0];
    for (const link of links) link.classList.toggle("current", link === current);
    for (const id of ["live", "split", "source"]) $(id).setAttribute("aria-pressed", String(id === state.field(mode)));
  }
  function renderPeers() {
    paintGridPeers(view);
    const users = [...awareness.getStates()].filter(([, peer]) => peer.note === active && peer.user);
    $("peer-count").hidden = users.length <= 1;
    $("peer-count").textContent = users.length;
    $("peer-list").replaceChildren(...users.map(([id, peer]) => {
      const li = el("li"), avatar = el("span", "avatar", peer.user.name.slice(-1));
      avatar.style.backgroundColor = peer.user.color;
      li.append(avatar, el("span", "", peer.user.name + (id === clientID ? " (you)" : "")));
      return li;
    }));
    const link = new URL(location.href);
    link.searchParams.set("editor", new URLSearchParams(location.search).get("editor") === "b" ? "a" : "b");
    link.hash = active;
    $("open-peer").href = link.href;
    $("open-peer").hidden = !canShare;
    if (!canShare) $("peers").querySelector("p").textContent = "Open the loopback preview to edit together in this browser.";
  }

  function openNote(id) {
    if (!files.has(id)) id = notebook.privateMode ? "inbox" : "walk";
    closeSidebar();
    const doc = notebook.getDoc(id);
    if (active === id && view?.state.facet(sharedNote).doc === doc) return;
    if (view) {
      const text = view.state.facet(sharedNote).text, selection = view.state.selection.main;
      bookmarks.set(active, {
        anchor: remember(text, selection.anchor),
        head: remember(text, selection.head),
        scroll: $("scroller").scrollTop
      });
      view.destroy();
    }
    active = id;
    history.replaceState(null, "", "#" + id);
    const name = files.get(id);
    document.title = notebook.privateMode ? "Margin · Private notebook" : name + " — Margin";
    $("file-name").textContent = name + ".md";
    const text = doc.getText("markdown");
    if (!undo.has(doc)) undo.set(doc, undoHistory(doc));
    const manager = undo.get(doc);
    const source = markdownText(text);
    const paragraph = source.match(/^(?![#*|\s>`-]).+/m);
    const saved = bookmarks.get(id);
    const anchor = saved && locate(doc, saved.anchor), head = saved && locate(doc, saved.head);
    const selection = anchor != null && head != null ? { anchor, head } : { anchor: paragraph?.index ?? source.length };
    awareness.setLocalStateField("cursor", null);
    awareness.setLocalStateField("note", id);
    view = new EditorView({
      parent: $("editor"),
      state: EditorState.create({
        doc: source, selection,
        extensions: [
          markdown({ base: markdownLanguage }), agentState, liveMarkdown, floatingFormatting(kernel.format), undoGroup.of(manager),
          checkboxEditing({ mode, ...kernel, manager, write: writeChecked }),
          focusAgentCell.of((currentView, pos) => currentView.state.field(mode) === "live" && focusCellAt(currentView, pos)),
          collaboration({ id, clientID, text, doc, awareness, manager }),
          splitPaste, Prec.highest(keymap.of([...splitTableKeys, ...tableKeys, ...listKeys, ...historyKeys])),
          keymap.of(defaultKeymap), EditorView.lineWrapping, drawSelection(),
          EditorView.contentAttributes.of({ "aria-label": "Edit Markdown", "aria-multiline": "true", spellcheck: "false" }),
          EditorView.updateListener.of(update => {
            if (view === update.view && (update.docChanged || update.selectionSet || update.transactions.some(t => t.reconfigured) ||
                update.startState.field(literalMarker) !== update.state.field(literalMarker))) {
              status(update.docChanged); reading.update(view);
            }
          })
        ]
      })
    });
    if (preferredMode !== "live") setMode(view, preferredMode);
    $("scroller").scrollTop = bookmarks.get(id)?.scroll ?? 0;
    reading.update(view); renderFiles(); status(true); renderPeers();
  }
  function chooseMode(value) {
    preferredMode = value;
    setMode(view, value); status();
    const selection = view.state.selection.main;
    if (value !== "live" || !focusCellAt(view,selection.head,selection)) view.focus();
  }
  $("live").onclick = () => chooseMode("live");
  $("split").onclick = () => chooseMode("split");
  $("source").onclick = () => chooseMode("source");
  $("marker-action").onmousedown = event => event.preventDefault();
  $("marker-action").onclick = () => {
    if (view.state.field(literalMarker)) keepLiteral(view);
    else editAsText(view);
    view.focus();
  };
  function askHere() {
    const session = view.state.field(gridSession);
    const input = view.dom.querySelector(".grid-cell-input");
    const target = session && input && input.selectionStart !== input.selectionEnd
      ? { from: session.from + input.selectionStart, to: session.from + input.selectionEnd } : session;
    return openAgent(view, target);
  }
  $("ask").onclick = askHere;
  $("insert-table").onclick = () => {
    const cursor = view.state.selection.main.to;
    const line = view.state.doc.lineAt(tableAt(view.state, cursor)?.to ?? cursor);
    const prefix = line.length ? "\n\n" : "";
    const table = "| Column A | Column B |\n| --- | --- |\n|  |  |";
    const suffix = line.to === view.state.doc.length || view.state.doc.line(line.number + 1).length ? "\n\n" : "\n";
    const from = line.to + prefix.length + 2;
    const manager = undo.get(active);
    manager.stopCapturing();
    view.dispatch({ changes: { from: line.to, insert: prefix + table + suffix },
      selection: { anchor: from, head: from + 8 }, scrollIntoView: true, userEvent: "input.table" });
    manager.stopCapturing();
    if (view.state.field(mode) === "live") activateCell(view, tableAt(view.state, from), 0, 0);
    else view.focus();
  };

  function closePeers() { $("peers").hidden = true; $("collaborate").setAttribute("aria-expanded", "false"); }
  $("collaborate").onclick = () => {
    $("peers").hidden = !$("peers").hidden;
    $("collaborate").setAttribute("aria-expanded", String(!$("peers").hidden));
    renderPeers();
  };
  document.addEventListener("click", e => { if (!e.target.closest(".peer-wrap")) closePeers(); });
  $("open-peer").addEventListener("click", closePeers);
  awareness.on("change", renderPeers);

  function search() {
    const query = $("note-query").value.trim().toLowerCase();
    const matches = [...files].filter(([, name]) => name.toLowerCase().includes(query));
    $("results").replaceChildren(...matches.map(([id, name]) => {
      const item = button("", "", () => { $("note-dialog").close(); openNote(id); view.focus(); });
      item.append(icon("file-text"), el("span", "", name + ".md"));
      return item;
    }));
    $("dialog-error").hidden = matches.length > 0;
    $("dialog-error").textContent = matches.length ? "" : "No matching notes.";
  }
  function showDialog(kind) {
    closePeers();
    dialogKind = kind;
    $("dialog-title").textContent = kind === "find" ? "Find a note" : "New Markdown note";
    $("note-query").value = "";
    $("note-query").placeholder = kind === "find" ? "Search by filename…" : "Filename";
    $("create").hidden = kind === "find";
    $("dialog-error").hidden = true;
    $("results").replaceChildren();
    if (kind === "find") search();
    $("note-dialog").showModal();
    $("note-query").focus();
  }
  $("search").onclick = () => showDialog("find");
  $("new-note").onclick = () => showDialog("new");
  $("dialog-close").onclick = () => $("note-dialog").close();
  $("note-query").oninput = () => { if (dialogKind === "find") search(); else $("dialog-error").hidden = true; };
  $("note-form").onsubmit = event => {
    event.preventDefault();
    if (dialogKind === "find") { $("results").querySelector("button")?.click(); return; }
    const name = $("note-query").value.trim().replace(/\.md$/i, "").trim();
    if (!name || /[\\/\x00-\x1f]/.test(name) || [...files.values()].some(n => n.toLowerCase() === name.toLowerCase())) {
      $("dialog-error").textContent = "Choose a unique filename without path separators.";
      $("dialog-error").hidden = false;
      return;
    }
    const id = crypto.randomUUID();
    notebook.create(id, name);
    $("note-dialog").close(); openNote(id); view.focus();
  };
  notebook.onFiles(() => {
    if (view && view.state.facet(sharedNote).doc !== notebook.getDoc(active)) openNote(active);
    renderFiles();
    if ($("note-dialog").open && dialogKind === "find") search();
  });
  $("download").onclick = () => {
    const blob = new Blob([view.state.doc.toString()], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url; link.download = files.get(active) + ".md"; link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1500);
  };
  document.addEventListener("keydown", event => {
    if (event.isComposing) return;
    const mod = isMac ? event.metaKey : event.ctrlKey;
    if (event.key === "Escape") { closePeers(); closeSidebar(); }
    if (!mod || event.altKey || event.shiftKey || $("note-dialog").open) return;
    if (event.key.toLowerCase() === "p") { event.preventDefault(); showDialog("find"); }
    if (event.key.toLowerCase() === "j") { event.preventDefault(); askHere(); }
  });
  window.addEventListener("hashchange", () => openNote(location.hash.slice(1)));
  openNote(location.hash.slice(1));
}
start().catch(error => {
  $("editor").textContent = "The editor could not open. " + error.message;
  $("storage").textContent = "Editor unavailable";
  console.error(error);
});
