# Margin — Product & UX Specification

> Living specification for **Margin**, a native Rust Markdown notebook editor built with **Dioxus Native** (`blitz-dom` / `parley`) and **Loro** CRDT.
> This document captures all active UX, interaction, architecture, and quality rules. New features and UX changes should be added here first, then implemented and verified against tests.

---

## 1. Core Principles

### Personal vaults

Built with the editor's `vault` feature, the desktop opens *Field notes* in a fresh vault,
with its checklist, listening table and two anchored discussions saved on the device.
Without the feature it opens the same note in memory. Desktop note selection is local
and survives a restart. An existing vault with notes resumes its own content.

Devices edit Loro in memory and persist authenticated ciphertext. New personal notes,
captured history, prompts, and diagnostics are device-only by default. Publishing a
note creates a new document from its current Markdown. Files use immutable encrypted
streams. Paired devices subscribe once from their last durable relay cursor.
The relay replays missed records and pushes live edits on the same WebSocket.
Document deltas and checkpoints use binary Loro `%ELO` update frames, including
standard fragments for large records. The relay acknowledges durable writes and accepts
verified full checkpoints from enrolled devices. Pruning follows causal coverage and
preserves concurrent edits and a recovery window.

1. **Literal Markdown as Single Source of Truth**
   - The note’s Markdown lives in one Loro `LoroText` (`markdown`) managed by `Note` (`crates/margin-note/src/note.rs`). Encrypted attachment manifests live in the document’s `attachments` map.
   - Editing must preserve untouched bytes exactly (line endings `LF`/`CRLF`, trailing newlines, spacing, escapes). Never re-serialize the whole document from an AST.
   - Undo/redo captures directed UTF-8 byte selections via Loro cursors; structural commands (table moves, pastes, formatting) commit as a single undoable step.
   - Shared core (`src/lib.rs`) remains portable across Native and Web/WASM (`mockup/`).
2. **Calm, Distraction-Free Paper Canvas**
   - No heavy branded top header, no permanent formatting ribbon.
   - Contextual UI appears only where and when needed (inline empty-line icons, selection format bar, docked table action bar).
3. **One Design System**
   - Every window is built from the components in `crates/margin-ui/src/` (buttons, chips, segmented switch, tabs, fields, composer, checkboxes, cards, rows, messages, read-only Markdown, popovers, menus, toolbars, icons and marks), including the editor's own chrome (mode bar, format bar, slash/block/table menus, empty-line tools, status bar). Headings, prose and tables share one type scale (`--font-h*`, `--font-prose*`, `--font-table*`), so a read-only page reads like the page in Rich mode, styled by `crates/margin-ui/assets/ui.css` from the tokens in `crates/margin-ui/assets/theme.css`. Features compose them and add layout only: no per-feature button, card, row, Markdown or checkbox styles. `margin ui` shows every component in every state. See `docs/design-system.md`.
4. **Out of Scope (for UX polish)**
   - **Split view** (`View::Split`) is retained only as a regression baseline; do not spend UX effort polishing Split view, but keep existing Split tests passing.

---

## 2. Chrome, Layout & Typography

- **Top-Anchored Mode Bar (`#mode-bar`):**
  - Slim, sticky top bar (`height: 36px`), borderless `--page` background, centered.
  - Contains the segmented view/mode switch (`ui::Segmented`: `Rich | Live | Split | Source`), compact icon buttons (`ui::Button` `Icon` variant: `Table`, `Undo`, `Redo`, `Comments`), and the **Debug Display toggle** (`#toggle-debug`, tooltip `Toggle debug display (F8)`). A notebook editing a page of the space ends the bar with the page's path, its state and **Save** (`#save-page`).
  - Must fit cleanly on narrow viewports down to `375px` without wrapping or overflowing.
- **Debug Display / Inspector (`F8` or `#toggle-debug`):**
  - Toggles field outlines, active caret highlight, live status strip, and JSON report export (`F9` / Save button).
  - Positioned fixed directly below `#mode-bar` (`top: 36px`) so toggling it causes **zero layout shift** in the document.
  - Clicking `#toggle-debug` while in Source/Split switches to Live and enables debug display.
- **Bottom Status Bar (`.statusbar`):**
  - Quiet footer in normal flex flow (`margin-top: auto`; never `position: fixed` so it never overlaps bottom table controls).
  - Shows word count, contextual hints (`Space or X toggles` / `Edit as text · F2` / `Press Enter or type to edit`), caret `Ln / Col`, and total line count.
- **Typography & Bundled Fonts (`crates/margin-ui/src/typefaces.rs`):**
  - Modern Claude-inspired pairing — every bundled `.woff2` font is registered with an explicit `FontInfoOverride` (`family_name`, `weight`, `style`) so CSS font matching never splits weights into separate families:
    - `Geist` (`400 Normal`, `500 Normal`, `600 Normal`, `400 Italic`) — headings (`600`, tight `-0.03em` tracking), Rich mode prose (`15.5px/26px`), UI controls, menus, and table labels/headers/body
    - `JetBrains Mono` (`400 Normal`, `500 Normal`, `600 Normal`, `400 Italic`) — Live mode prose (`14px/25px`), inline code, code blocks, Source mode, numeric table columns, and status hints

---

## 3. Editing Modes

### 3.1 Live Mode (`EditMode::Markdown` — Keyboard-First)
- **Tight rendering at rest:** Unfocused inline delimiters (`**`, `*`, `~~`, `` ` ``, `[`, `](url)`) and task-list `- ` prefixes collapse completely via `Projection` (`crates/margin-editor/src/notebook/projection.rs`) with zero ghost whitespace.
- **Cursor-only inline expansion:** Placing the caret inside or touching a formatted span expands **only that span's** Markdown delimiters inline so it can be edited directly with the keyboard; moving the caret away collapses it tightly again.
- **Checkboxes (Mouse vs. Keyboard):**
  - **Mouse left-click** toggles the checkbox directly in Loro and **keeps it rendered as a visual togglebox** (repeatable click-to-toggle; never flashes into `[ ]`/`[x]` text).
  - **Keyboard navigation** into or immediately beside a checkbox reveals its Markdown `[ ]`/`[x]` brackets (`Space` or `X` toggles once; `F2` / `Edit as text` enters literal mode; `Keep literal` escapes with `\`).
  - **`Alt+Click`** on a checkbox reveals its brackets for direct keyboard editing.

### 3.2 Rich Mode (`EditMode::Rich` — Traditional)
- Clean modern sans prose (`Geist`, `15.5px/26px`) with `JetBrains Mono` inline code.
- Inline Markdown delimiters **never** reveal on cursor entry; edits inside a styled span stay styled, and deleting all visible characters of a styled span cleans up its hidden delimiters automatically.
- Checkboxes always remain visual toggleboxes.
- No global formatting toolbar: formatting uses the floating selection `FormatBar`, inline empty-line block icons, and keyboard shortcuts.

### 3.3 Source Mode (`View::Source`)
- Full-viewport raw Markdown `<textarea>` sharing the same Loro document, undo history, and caret/selection round-tripping.
- Supports the same formatting shortcuts (`⌘B`, `⌘I`, `⌘⇧X`, `⌘E`, `⌘K`) and list/blockquote continuation (`Enter`, `Tab`, `Shift+Tab`, `Backspace`).

---

## 4. Contextual Controls

- **Inline Empty-Line Icons (`#empty-line-tools`) & Slash Menu (`#slash-menu`):**
  - On an empty line (collapsed caret in prose or `TitleGap`), subtle inline icons appear beside the caret (`H1`, `H2`, `H3`, Bullet, Checklist, Quote, Code, Table, Divider, Image) and disappear immediately on typing or moving off.
  - **Slash Command (`/`):** Typing `/` on an empty line opens a floating, filterable command menu (`#slash-menu`) right below the caret:
    - Typing after `/` filters commands live (e.g., `/h2`, `/task`, `/table`, `/code`, `/img`, `/quote`, `/rule`).
    - `ArrowUp` / `ArrowDown` navigate items, `Enter` / `Tab` / click executes the item (replacing `/query`), `Escape` dismisses.
    - Supports: `Heading 1`, `Heading 2`, `Heading 3`, `Bullet list`, `Checklist`, `Blockquote`, `Callout` (`/callout`, `/tip`, `/important`, `/warning`, `/caution`), `Code block`, `Table`, `Divider`, `Image` (`![alt](url)`).
- **Left Block Handle (`⋮⋮`, `.block-handle`) & Drag-and-Drop Reordering:**
  - Every top-level block (headings, individual paragraphs, blockquotes, rules, lists, code blocks, tables) renders its own subtle `⋮⋮` grip handle in the left margin (`cursor: grab`).
  - **Drag** any block's `⋮⋮` handle (headings, paragraphs, blockquotes, dividers, tables, fenced code blocks, and individual top-level list/checklist items) vertically to reorder it to any position in the document with full-window pointer tracking, source dimming, and a crisp accent-green drop indicator line (`#block-drop-line`); releasing commits one byte-exact, undoable Loro edit. Zero dead hover gap between block content and handle.
  - **Click** `⋮⋮` to open the compact block menu (`#block-menu`) with **Move Up**, **Move Down**, **Duplicate**, and **Delete**.
  - **Keyboard:** `⌥⇧↑` / `⌥⇧↓` (`Alt+Shift+ArrowUp/Down`) moves the current block up or down directly.
- **Floating Selection FormatBar (`#format-bar`):**
  - Appears near non-empty text selections in headings, prose, and table cells (deferred until pointer-up during drags; clears `#mode-bar` at the top edge).
  - Actions: Bold (`⌘B`), Italic (`⌘I`), Strikethrough (`⌘⇧X`), Inline Code (`⌘E`), Link (`⌘K`). `⌘⇧H` toggles `==highlight==`, and pasting an `http(s)://` URL onto selected text wraps it as `[selection](url)`.
- **Callouts & Code Blocks:**
  - All 5 GFM/Obsidian callouts (`> [!NOTE]`, `> [!TIP]`, `> [!IMPORTANT]`, `> [!WARNING]`, `> [!CAUTION]`, with optional custom titles) render as tinted cards with a 3.5px accent bar, clickable type-switcher badge, `/` menu entries, and `Enter` continuation.
  - Fenced code blocks (` ```lang `) render inside a rounded `--rail` card with an optional language label and Copy button (`#code-copy-*`).
  - Bare `http(s)://` URLs autolink and open the Link Preview Card.
- **Floating Link Preview & Action Card (`#link-card`):**
  - Appears directly below the active Markdown link (`[label](target)` or `<url>`) when a collapsed caret sits inside it in Rich or Live mode (in headings, prose, or table cells), flipping above if near the bottom viewport edge.
  - **Header bar:** clickable target path/URL (`#link-card-open`, opening external URLs, Space pages, or jumping to in-document `#heading` anchors) plus compact icon actions: **Edit** (`#link-card-edit`), **Copy** (`#link-card-copy`, with checkmark feedback), and **Remove / Unlink** (`#link-card-remove`, unwrapping `[label](url)` back to plain `label` in one undoable Loro edit).
  - **Preview body:** displays the resolved title, folder/host row, excerpt (from Space page content, in-document section prose, Markdown title attribute, or URL), and metadata footer (page/link kind and backlink count).
  - **Inline Edit form:** clicking Edit switches `#link-card` into an inline form (`#link-edit-label`, `#link-edit-url`, Save/Cancel, `Enter` to apply, `Escape` to cancel) committing one undoable Loro edit. `Escape` in preview mode dismisses the card until the caret leaves or re-clicks the link.

---

## 5. Text Rendering, Caret & Selection

- **Always-Visible Caret (`.caret-ink`):**
  - Styled runs painted as `.md-ink` overlays sit above `<textarea>` with opaque backgrounds. A dedicated `.caret-ink` overlay (`z-index: 3`, exact Parley `cursor_geometry(1.5)`) paints **above** `.md-ink` so the cursor is always sharp and visible inside bold, italic, strikethrough, completed tasks, inline code, links, quotes, headings, and table cells (including light caret on dark table headers).
  - Long single-line table cells maintain horizontal scroll margin (`keep_caret_in_view`) and scroll-aware caret tracking (`TrackedKey` includes `scroll_offset`) so typing, arrowing, or wheel-scrolling inside a long cell never loses or clips the caret on 1× or 2× Retina displays.
- **Consistent Selection Color:**
  - Single-field selection uses Blitz's native highlight (`rgb(180, 213, 255)`).
  - Cross-block/multiline `.selection-ink` uses `rgba(81, 161, 255, 0.42)` (and `.marker-face.selected` uses `#7ab3fa`) so single-field and cross-block selections match seamlessly with no color shift.
- **Continuous Document Selection & Navigation:**
  - Drag, `Shift+Click`, `Shift+Arrows`, `⌘A`, copy, cut, paste, and arrow keys cross seamlessly across title, headings, prose, blank lines, and tables.
  - `Escape` parks focus on `body-surface`; pressing `Enter`, any arrow key, or typing resumes editing immediately.
  - Caret scrolling maintains comfortable vertical scroll margin (`scroll_caret`).
- **Projection Sync Safety:**
  - `absolute_selection` and `track_field` guard against stale DOM buffers while a projection expansion/collapse (`focus`) is pending, preventing double-event (`pointerup` + `click`) caret drift.
  - `WindowLayout::RedrawRequested` syncs caret expansion on auto-repeating arrows and macOS Cocoa key bindings.

---

## 6. Headings, Lists, Checkboxes & Images

- **Headings:**
  - Render as wrapping `<textarea>` fields auto-sized to content height.
  - **Editable inline `#` prefix in Live mode:** The `# ` / `## ` prefix is a true projected inline delimiter inside the heading `<textarea>` itself (exact heading font, size, and baseline; dimmed when cursor is elsewhere, full ink when cursor touches it). Users can arrow onto/before `#`, click between `#`s, add/remove `#` directly, or delete `# ` to convert the heading into a paragraph. At rest and in Rich mode, the `# ` prefix collapses tightly.
  - Keyboard: `⌘1`–`⌘6` set heading level, `⌘0` converts to paragraph; in Rich mode (when `#` is hidden), `#` / `Backspace` at offset 0 still adjusts heading level; `Enter` splits heading cleanly.
- **Lists & Checklists:**
  - `Enter` continues bullet, numbered, and task lists (new task starts unchecked); `Enter` on an empty item exits to a paragraph.
  - `Tab` / `Shift+Tab` indents/outdents; `Backspace` at label start outdents then strips the list prefix.
  - `⌘Enter` / `Ctrl+Enter` toggles the checkbox on the current line (or prepends `- [ ] `).
- **Checkboxes:**
  - Crisp `15×15px` square togglebox (`CHECKBOX_SLOT_MONO = "  "` in monospace Live mode; `CHECKBOX_SLOT_RICH = "\u{2003}"` in proportional Rich mode), with native `cursor: pointer` on hover.
  - Task list items (`- [ ] ...`, `- [x] ...`) keep their list bullet marker (`•`) visible just like ordinary bullet lists — `- ` is never hidden or collapsed.
  - Structured toggles write the Loro `markdown.checked` attribute so concurrent peer toggles converge cleanly.
  - Whole-field replacement (Select All + type) cleans up leading/trailing hidden syntax.
- **Inline Images:**
  - `![alt](url)` collapses syntax tightly at rest (styled as an inline image chip) and expands inline when focused in Live mode.

---

## 7. Tables

- **Visual Design:**
  - Light header (`--wash` background, small `--muted` 600 labels, a 1px `--line-strong` rule under it) with `6px` rounded corners, rows divided by `--line` hairlines; the accent appears only for focus and selection (cell ring, range cells, current cell), no stray bottom line on the last row (with matching `6px` rounded bottom corners), steady column widths while typing (no jitter), aligned cell hit areas (`left`, `center`, `right`).
  - No floating column/row toolbars, no edge `+` column/row buttons, and no `"Cell xx"` single-cell footer clutter.
- **Craft-Inspired Edge Pill Controls (Hovered/Active Column & Row Only):**
  - Controls appear **only for the currently hovered (or focused/menu-open) column and row** — keeping all other columns and rows completely clean, and never stealing width from header or cell text.
  - **Column pill handle (`•••` on top border):** A compact horizontal triple-dot pill (`ui::Icon::DotsHorizontal`) sits centered on and straddling the top border edge of the hovered/active column. Clicking it highlights the column and opens the column menu directly underneath with **alignment icons (`AlignLeft` / `AlignCenter` / `AlignRight`)**, Move Left/Right, Insert Left/Right, and Delete Column.
  - **Row pill handle (`⋮` on left border):** A compact vertical triple-dot pill (`ui::Icon::DotsVertical`) sits centered on and straddling the left border edge of the hovered/active data row (clear of both the outer `⋮⋮` block drag handle and column 0 text). Clicking it highlights the row and opens the row menu beside it with Move Up/Down, Insert Above/Below, and Delete Row.
  - **Bottom range strip (`#table-bar-{t}`):** Hidden for single-cell editing; appears only when a multi-cell rectangular range is selected (`B2 – C3 · N cells  Clear  Del rows  Del cols`).
- **Keyboard & Pointer:**
  - `Tab` / `Shift+Tab` move horizontally (growing a new row at the last cell); `Enter` / `Shift+Enter` move vertically.
  - `Alt+ArrowUp` / `Alt+ArrowDown` move the current row; `Alt+ArrowLeft` / `Alt+ArrowRight` move the current column.
  - Drag, `Shift+Click`, or `Alt+Shift+Arrow` selects a rectangular range; TSV copy/paste (including ragged tables) commits in one undoable edit.
- **Below-Table Boundary:**
  - Clicking below a table's `#grid-scroll-{t}` bottom border always places the caret on the prose line underneath (never selects the table).
  - Typing on the line immediately below a table always preserves a blank-line separator in Markdown (in both raw and projected input paths) so GFM never absorbs typed prose into the table as a new row.

---

## 8. Performance, Stability & Verification Rules

- **Blitz Hit-Test & Layout Safety:**
  - Never pass raw viewport coordinates to `doc.hit`: always add `doc.viewport_scroll()`.
  - Guard against `blitz-dom` `paint_children` panics (`node.rs:1483`) using a `dom_dirty` flag + `resolve_dom(&handle)` before hit-testing and after Dioxus commits DOM mutations.
- **Low-Latency Selection & Input:**
  - Memoize Markdown parsing (`ParsedMarkdown` via `Memo` — `pulldown_cmark` runs once per edit, never per mouse-move frame).
  - Intra-field drags take a bounding-rect fast path (zero layout resolve, zero hit-test, zero projection) so Parley handles selection natively.
  - Cross-field drags deduplicate unchanged spans, guard unchanged signal writes in `Writing::open`, defer `FormatBar` until mouse-up, and skip height measurements while dragging.
  - Non-render paths use `projection_peek` (reading `Memo`s via `.read()` so dirty memos recompute, while reading `Signal`s via `.peek()` to avoid reactive subscriptions).
- **Quality Bar:**
  - `cargo check --locked --workspace --all-targets` must pass with **0 warnings**, with and without `--all-features` (the editor's `vault` feature).
  - `cargo test --locked --workspace` must pass 100%, both with and without `--all-features` (only the build without the vault compiles `personal/off.rs`).
  - Visual changes must be verified by inspecting rendered PNG screenshots from `MARGIN_TEST_ARTIFACTS`.

---

## 9. Anchored Inline Comments & Scoped LLM Actions

- **CRDT-Anchored Storage (Zero Markdown Pollution):**
  - Comment threads live in memory (`Writing::comments`) anchored via Loro `Cursor` pairs (`Note::anchor_range` / `Note::resolve_anchor`), so editing prose above or around an anchor keeps highlights and replacement ranges accurate without modifying literal Markdown.
  - The desktop starter and the in-memory `--demo` each open with two discussions anchored to the same phrases in *Field notes*. In a vault, editing or removing a discussion persists across launches.
- **Inline Highlights (`CommentInk`):**
  - Anchored spans in headings, prose blocks, and table cells render warm amber underlines/washes (stronger when active, hidden once resolved unless selected).
- **Triggers & Right-Hand Drawer (`CommentsPane`):**
  - Select text and click **Comment** (`#format-comment`) in `FormatBar`, or press `⌘⇧M` / `Ctrl+Shift+M`, to open a new comment composer anchored to the selection (falls back to the current line if collapsed).
  - Toggle the `304px` right-hand Comments drawer from `#toggle-comments` in `#mode-bar` (with an amber badge dot when open threads exist).
  - Filter by `Open`, `Resolved`, or `All`; click any thread's quoted anchor to jump & select it in the document; `Resolve` / `Reopen` or `Delete` threads.
- **Interactive Mock LLM Replies & Actionable Diffs:**
  - Submitting a new comment or replying in an active thread (or tapping quick chips like `Shorten`, `Polish`, `Try another`) appends a mock Claude response with a reviewable inline diff (`- old` / `+ new`).
  - **Apply to doc** commits the replacement to Loro Markdown (safely encoding table cell pipes/newlines), re-anchors the thread to the new span, and switches the button to **✓ Applied · Undo** (plus a quick **Resolve** button) for one-click reversal.

---

- **One reply path:** the mock and the real agent both implement `hub::AgentChat` and stream through `worker::turn`; the reply streams into its message (only that message re-renders), and a final ` ```margin-edit ` block (label, `---`, replacement) becomes the proposal and is never shown as text.
- **Real backend (pages of the space):** an editor opened from the Topics window on a live, paired page asks the read-only ZeroClaw agent `scribe` (`comment_agent` in `topics.json`) instead of the mock. Each thread is its own session (`margin-comment-<id>`). The first turn carries the reply contract and the page; every turn carries the section or table cell, the selection and the request. **Apply** and **Undo** act only while the text under the thread is what the proposal expects; otherwise the thread offers *Ask again* (staleness is derived, never stored). Threads are saved to `_margin/comments/<page>.json` in the space, anchored by quote and context and merged on 412; conversation changes save at once, anchors and applied states are saved with the page. They re-attach after edits made outside Margin. A comment file that cannot be read or written shows a banner in the pane. See `docs/comments-backend.md`.

## 10. Topics Window (`osom topics`)

- **Sources:** the SilverBullet space is the source of truth for topics and documents (`GET/PUT /.fs`); ZeroClaw provides conversations (`GET /api/sessions`, `GET /api/sessions/{key}/messages`) and chat (`/ws/chat`). Both are reached over the tailnet; the device token lives in the Keychain in release builds and in a private file next to `topics.json` in dev builds (every rebuild is a binary the Keychain no longer trusts), never in a file in the repo.
- **Topics:** every top-level folder except `Library/` and `Repositories/` is a topic; sub-folders are sub-topics; each page in `people/` and every page a README route targets is a topic of its own. Titles come from the page's first heading, up to an em dash.
- **Facets:** Overview renders the topic's README (else its main page, never `log.md` or `questions.md`). Discussions are the sessions whose key contains a chat id routed to the topic in the space README, plus Margin's `topic-<id>` chat; tool results and tool-calling steps are never shown. Calendar lists open rows of `reminders.md` whose source file is in the folder or whose id starts with the topic's stem. Questions read `questions.md`. A facet without content is hidden.
- **Today** shows the next three weeks of key dates and deadline reminders. Dated list entries under `Key dates` in `planning/README.md` appear first, with their original details and a link to the source. Each deadline reminder appears once under its owning topic. The page also shows recent discussions and shopping lists. Opening Topics, returning to Today, or pressing Refresh reads the live sources.
- **Editing:** a doc opens in the Live editor, which owns the page (`NotebookOptions::page`); **Save** in its bar writes with `If-Match` and reports a conflict instead of overwriting a newer version, then saves the comment anchors for the saved text.
- **Chat:** one WebSocket per turn; the first message of a fresh topic session carries the topic's title and space path; tool approvals are answered from the panel.
- **Demo:** `--demo` renders the same window from `topics_demo.rs`; both modes build one `topics_view::Workspace`, so every UX change applies to both.

## 11. Upcoming Rules & Additions (Backlog)

- Global right-hand AI Chat tab & real LLM provider backend
- Persistent sidecar storage for comments alongside `.md` files
- Open / save real `.md` files + workspace file sidebar
- Heading outline panel
- Dark mode theme toggle
- Fenced code block syntax highlighting
- Native multi-style rich-text editing surface (eliminating `.md-ink` / `.caret-ink` overlays)
