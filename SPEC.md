# Margin — Product & UX Specification

> Living specification for **Margin**, a native Rust Markdown notebook editor built with **Dioxus Native** (`blitz-dom` / `parley`) and **Loro** CRDT.
> This document captures all active UX, interaction, architecture, and quality rules. New features and UX changes should be added here first, then implemented and verified against tests.

---

## 1. Core Principles

1. **Literal Markdown as Single Source of Truth**
   - All content lives in one Loro `LoroText` (`markdown`) managed by `Note` (`src/note.rs`).
   - Editing must preserve untouched bytes exactly (line endings `LF`/`CRLF`, trailing newlines, spacing, escapes). Never re-serialize the whole document from an AST.
   - Undo/redo captures directed UTF-8 byte selections via Loro cursors; structural commands (table moves, pastes, formatting) commit as a single undoable step.
   - Shared core (`src/lib.rs`) remains portable across Native and Web/WASM (`mockup/`).
2. **Calm, Distraction-Free Paper Canvas**
   - No heavy branded top header, no permanent formatting ribbon.
   - Contextual UI appears only where and when needed (inline empty-line icons, selection format bar, docked table action bar).
3. **Out of Scope (for UX polish)**
   - **Split view** (`View::Split`) is retained only as a regression baseline; do not spend UX effort polishing Split view, but keep existing Split tests passing.

---

## 2. Chrome, Layout & Typography

- **Top-Anchored Mode Bar (`#mode-bar`):**
  - Slim, sticky top bar (`height: 36px`), borderless `--page` background, centered.
  - Contains the segmented view/mode switch (`Rich | Live | Split | Source`), compact icon buttons (`Table`, `Undo`, `Redo`), and the **Debug Display toggle** (`#toggle-debug`, tooltip `Toggle debug display (F8)`).
  - Must fit cleanly on narrow viewports down to `375px` without wrapping or overflowing.
- **Debug Display / Inspector (`F8` or `#toggle-debug`):**
  - Toggles field outlines, active caret highlight, live status strip, and JSON report export (`F9` / Save button).
  - Positioned fixed directly below `#mode-bar` (`top: 36px`) so toggling it causes **zero layout shift** in the document.
  - Clicking `#toggle-debug` while in Source/Split switches to Live and enables debug display.
- **Bottom Status Bar (`.statusbar`):**
  - Quiet footer in normal flex flow (`margin-top: auto`; never `position: fixed` so it never overlaps bottom table controls).
  - Shows word count, contextual hints (`Space or X toggles` / `Edit as text · F2` / `Press Enter or type to edit`), caret `Ln / Col`, and total line count.
- **Typography & Bundled Fonts (`src/native/typefaces.rs`):**
  - Every bundled `.woff2` font is registered with an explicit `FontInfoOverride` (`family_name`, `weight`, `style`) so CSS font matching never splits weights into separate families:
    - `Instrument Serif` (`400 Normal`) — headings & table first-column labels
    - `DM Sans` (`400 Normal`, `600 Normal`) — UI controls & table body/headers
    - `IBM Plex Mono` (`400 Normal`, `600 Normal`, `400 Italic`) — Live mode prose, code, Source mode, numeric table columns
  - Rich mode prose uses editorial serif (`var(--body)`).

---

## 3. Editing Modes

### 3.1 Live Mode (`EditMode::Markdown` — Keyboard-First)
- **Tight rendering at rest:** Unfocused inline delimiters (`**`, `*`, `~~`, `` ` ``, `[`, `](url)`) and task-list `- ` prefixes collapse completely via `Projection` (`src/native/projection.rs`) with zero ghost whitespace.
- **Cursor-only inline expansion:** Placing the caret inside or touching a formatted span expands **only that span's** Markdown delimiters inline so it can be edited directly with the keyboard; moving the caret away collapses it tightly again.
- **Checkboxes (Mouse vs. Keyboard):**
  - **Mouse left-click** toggles the checkbox directly in Loro and **keeps it rendered as a visual togglebox** (repeatable click-to-toggle; never flashes into `[ ]`/`[x]` text).
  - **Keyboard navigation** into or immediately beside a checkbox reveals its Markdown `[ ]`/`[x]` brackets (`Space` or `X` toggles once; `F2` / `Edit as text` enters literal mode; `Keep literal` escapes with `\`).
  - **`Alt+Click`** on a checkbox reveals its brackets for direct keyboard editing.

### 3.2 Rich Mode (`EditMode::Rich` — Traditional)
- Editorial serif prose (`16px/27px`).
- Inline Markdown delimiters **never** reveal on cursor entry; edits inside a styled span stay styled, and deleting all visible characters of a styled span cleans up its hidden delimiters automatically.
- Checkboxes always remain visual toggleboxes.
- No global formatting toolbar: formatting uses the floating selection `FormatBar`, inline empty-line block icons, and keyboard shortcuts.

### 3.3 Source Mode (`View::Source`)
- Full-viewport raw Markdown `<textarea>` sharing the same Loro document, undo history, and caret/selection round-tripping.
- Supports the same formatting shortcuts (`⌘B`, `⌘I`, `⌘⇧X`, `⌘E`, `⌘K`) and list continuation (`Enter`, `Tab`, `Shift+Tab`, `Backspace`).

---

## 4. Contextual Controls

- **Inline Empty-Line Icons (`#empty-line-tools`) & Slash Menu (`#slash-menu`):**
  - On an empty line (collapsed caret in prose or `TitleGap`), subtle inline icons appear beside the caret (`H1`, `H2`, `H3`, Bullet, Checklist, Quote, Code, Table, Divider, Image) and disappear immediately on typing or moving off.
  - **Slash Command (`/`):** Typing `/` on an empty line opens a floating, filterable command menu (`#slash-menu`) right below the caret:
    - Typing after `/` filters commands live (e.g., `/h2`, `/task`, `/table`, `/code`, `/img`, `/quote`, `/rule`).
    - `ArrowUp` / `ArrowDown` navigate items, `Enter` / `Tab` / click executes the item (replacing `/query`), `Escape` dismisses.
    - Supports: `Heading 1`, `Heading 2`, `Heading 3`, `Bullet list`, `Checklist`, `Blockquote`, `Code block`, `Table`, `Divider`, `Image` (`![alt](url)`).
- **Left Block Handle (`⋮⋮`, `.block-handle`) & Drag-and-Drop Reordering:**
  - Every top-level block (headings, individual paragraphs, blockquotes, rules, lists, code blocks, tables) renders its own subtle `⋮⋮` grip handle in the left margin (`cursor: grab`).
  - **Drag** any block's `⋮⋮` handle vertically to reorder it to any position in the document with a crisp accent-green drop indicator line (`#block-drop-line`); releasing commits one byte-exact, undoable Loro edit.
  - **Click** `⋮⋮` to open the compact block menu (`#block-menu`) with **Move Up**, **Move Down**, **Duplicate**, and **Delete**.
  - **Keyboard:** `⌥⇧↑` / `⌥⇧↓` (`Alt+Shift+ArrowUp/Down`) moves the current block up or down directly.
- **Floating Selection FormatBar (`#format-bar`):**
  - Appears near non-empty text selections in headings, prose, and table cells (deferred until pointer-up during drags; clears `#mode-bar` at the top edge).
  - Actions: Bold (`⌘B`), Italic (`⌘I`), Strikethrough (`⌘⇧X`), Inline Code (`⌘E`), Link (`⌘K`).

---

## 5. Text Rendering, Caret & Selection

- **Always-Visible Caret (`.caret-ink`):**
  - Styled runs painted as `.md-ink` overlays sit above `<textarea>` with opaque backgrounds. A dedicated `.caret-ink` overlay (`z-index: 3`, exact Parley `cursor_geometry(1.5)`) paints **above** `.md-ink` so the cursor is always sharp and visible inside bold, italic, strikethrough, completed tasks, inline code, links, quotes, headings, and table cells (including light caret on dark table headers).
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
  - Pine header, subtle row rules, steady column widths while typing (no jitter), aligned cell hit areas (`left`, `center`, `right`).
  - No ugly floating bordered `+` / `+ Row` pills and no detached popup menu.
- **Unified Flat Axis Bars (Top Column Bar + Left Row Bar):**
  - All table controls are **flat** (borderless, shadowless, transparent ghost buttons on `--page`, with soft hover tints) and appear only when the table is hovered or focused — no duplicate right-edge or bottom-right growth buttons.
  - **Top column bar (`#col-strip-{t}`):** Every column header has a flat `⋯` menu trigger (`L / C / R` alignment, Move Left/Right, Insert Left/Right, Delete Column) + quick `×` delete directly above it, and a coherent flat `+` button at the right end of the bar to append a column.
  - **Left row bar (`#row-strip-{t}`):** Every data row (`r > 0`) has a flat `⋯` menu trigger (Move Up/Down, Insert Above/Below, Delete Row) + quick `×` delete directly beside it, and a coherent flat `+` button at the bottom end of the bar to append a row.
  - **Bottom scope strip (`#table-bar-{t}`):** Shows only the minimal flat cell/range scope label (`Cell B2`, or `B2 – C3 · N cells  Clear  Del rows  Del cols` when a rectangular range is selected).
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
  - `cargo check --locked --features native --bin margin` must pass with **0 warnings**.
  - `cargo test --locked --features native` must pass 100% (currently 79 tests: 17 core + 62 native).
  - Visual changes must be verified by inspecting rendered PNG screenshots from `MARGIN_TEST_ARTIFACTS`.

---

## 9. Upcoming Rules & Additions (Backlog)

- Open / save real `.md` files + workspace file sidebar
- Heading outline panel
- Dark mode theme toggle
- Fenced code block syntax highlighting
- Native multi-style rich-text editing surface (eliminating `.md-ink` / `.caret-ink` overlays)
