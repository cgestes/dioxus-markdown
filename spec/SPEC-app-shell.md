# Spec: App Shell (`app-shell`) — V1

> Module id: `app-shell` (see [capability-map.md](capability-map.md)).
> Depends on: `spaces-and-isolation`, `topics-and-wiki`, `agents-and-chat`, `channels-and-routing`, `agenda-and-reminders`, `connectors-and-import`.
> Status: approved.

## Objective

Deliver **`osom`** (`crates/osom`) as the standalone macOS desktop application (`Dioxus Native`) tying together multi-space switching, active-space Today, discussion-driven Inbox, Topic workspaces, the embedded `margin-editor`, and the right-hand Chat pane — completely separated from **`margin`** (`crates/margin`, the standalone test app for the Markdown editor and `margin ui` gallery) and with `--demo` (`crates/osom/src/demo.rs`) removed.

### Why
The Cargo workspace already cleanly separates `crates/osom`, `crates/osom-hub`, and `crates/osom-topics` from `crates/margin`, `crates/margin-editor`, `crates/margin-core`, `crates/margin-ui`, and `crates/margin-runtime` (`topics` is already removed from `crates/margin`). What remains for V1 is removing `--demo` (`crates/osom/src/demo.rs`), switching `crates/osom` from single-space SilverBullet to multi-space ZeroClaw Spaces with live sync status, adding Space switching, `⌘O` quick switcher, `⌘⇧F` local search, inline Connect & Pair onboarding, and relocating root `SPEC.md` into `crates/margin-editor/SPEC.md`.

### User Stories (V1)
1. **Clean `osom` vs. `margin` separation (no `--demo`):** `crates/osom` (`osom`) is the real multi-space desktop app backed by `~/.config/osom/`, with `crates/osom/src/demo.rs` and `--demo` removed completely. `crates/margin` (`margin`) stays completely independent as the test harness for `crates/margin-editor` (whose spec moves to `crates/margin-editor/SPEC.md`) and `margin ui` (`crates/margin-ui` gallery).
2. **Space Switcher (isolated spaces):** The top of the left sidebar lets me switch between configured spaces (`Personal`, `Family`, `Climbing`, …) discovered from `GET /api/spaces` (and cached offline). Switching spaces swaps the entire sidebar, Today, Inbox, Search, and Chat context cleanly with zero cross-space mixing.
3. **Simple, focused Today view (active space):** Opening a space lands on **Today** for the active space: overdue & due-today items (plus exact-time reminders), upcoming 3-week agenda from `reminders.md`, cached read-only Calendar events, recent discussions, and rolled-up shopping lists — with quick **Done** (optional log-note popover) / **Booked** / **Snooze** / **+ Add Reminder** actions.
4. **Topic workspace & embedded editor:** Clicking any topic or sub-topic opens its living facets (**Overview** with read-only comments, **Discussions**, **Calendar**, **Questions**, **Docs**, **Procedures**, **Lists**, hidden when empty) plus **+ New Topic / Doc** creation. Clicking any doc, procedure, list, or wiki link opens it inside `margin-editor` with a **Back** navigation stack, space-wide backlinks, and anchored `scribe-<space>` comments.
5. **Discussion-driven Inbox:** Clicking **Inbox** lists unrouted Telegram/Slack sessions for the active space so I can discuss them with the agent to file into existing topics, create new topics, or mark handled.
6. **Collapsible Chat pane (`⌘L`):** The right pane hosts the persistent in-app chat for the active space (`osom-space-<space>`) or active topic (`osom-topic-<space>-<topic>`), with streaming chunks, tool badges, inline Approve/Deny, Stop, and offline queued messages.
7. **Keyboard navigation (`⌘O`, `⌘⇧F`, `⌘L`):** `⌘O` opens a fast quick switcher for spaces, topics, and docs; `⌘⇧F` searches full-text across the active space's local replica and cached transcripts; `⌘L` focuses or toggles the Chat pane (never colliding with editor shortcuts `⌘B`, `⌘I`, `⌘K`, `⌘1–6`, `⌘⇧M`).
8. **Clear sync status & inline Connect/Pair onboarding:** The sidebar footer shows live space sync state (`Synced`, `Syncing…`, `Offline · N pending`, plus pending offline chat messages), gateway pairing status, and a manual Refresh/Sync button. When `~/.config/osom/` is not yet configured or paired, `osom` opens an inline **Connect & Pair** screen (gateway URL + one-time paircode).

## Tech Stack

- **Desktop runtime:** Rust (edition 2024), Dioxus Native (`blitz-dom` / `parley`), `margin-runtime` (window launch, `worker`, `restart`, macOS integration), `clap` for CLI subcommands.
- **Design system:** `crates/margin-ui` (`Button`, `Chip`, `Segmented`, `Tabs`, `NavItem`, `Popover`, `Menu`, `TextField`, `Composer`, `Checkbox`, `CheckItem`, `Card`, `Row`, `Message`, `MarkdownView`, `Icon`, `Tile`, `Badge`, `Banner`, `DateBox`, `SourceGlyph`), styled exclusively via `crates/margin-ui/assets/{theme,ui,table}.css` + layout-only `crates/osom/src/osom.css`.

## Commands

```sh
# Build & type-check both binaries (0 warnings required)
cargo check --locked -p osom -p margin

# Launch the osom desktop app
cargo run --locked -p osom

# Headless osom CLI helpers
cargo run --locked -p osom -- setup <zeroclaw-url>
cargo run --locked -p osom -- pair <code>
cargo run --locked -p osom -- status

# Bundle macOS Osom.app (with brushtail possum icon)
./scripts/bundle-osom.sh

# Launch the completely separate Markdown editor & UI gallery test app
cargo run --locked -p margin
cargo run --locked -p margin -- ui

# Run full workspace test suite
cargo test --locked --workspace
```

## Project Structure

```
spec/SPEC-app-shell.md             → This specification
crates/margin-editor/SPEC.md       → Markdown editor spec (relocated from root `SPEC.md`)
crates/margin/                     → Standalone `margin` test binary (Markdown editor + `margin ui` gallery; zero dependency on `osom`)
crates/margin-ui/                  → Shared design-system components, tokens, fonts & gallery
crates/margin-runtime/             → Window launch, background worker, auto-restart, macOS integration
crates/osom-hub/                   → Multi-space offline Replica, Spaces HTTP/SSE client, ZeroClaw chat & offline chat outbox
crates/osom-topics/                → Pure domain core (topics, wiki links/backlinks, routes, reminders, imports)
crates/osom/                       → Main `osom` desktop app crate:
  src/main.rs                      → Binary entry point (CLI + desktop launch)
  src/cli.rs                       → `osom` CLI (`setup`, `pair`, `status`, `--open`, `--tab`, `--doc`; no `--demo`)
  src/commands.rs                  → Headless CLI command handlers
  src/lib.rs                       → Shell root, Space switcher, keyboard shortcuts (⌘O, ⌘⇧F, ⌘L), onboarding/pairing, sidebar & sync footer
  src/live.rs                      → Multi-space backend adapter (replicas, SSE sync, cached transcripts/calendar)
  src/today.rs                     → Active-space Today view (reminders, cached calendar events, recent discussions, shopping rollup)
  src/topic.rs                     → Topic header, facet tabs, Inbox triage, conflict-merge banner
  src/pages.rs                     → Embedded `margin-editor` host + Back/Forward navigation stack + space-scoped `scribe` comments
  src/chat.rs                      → Right-hand space/topic Chat pane (streaming, approvals, Stop, offline outbox queue)
  src/search.rs                    → ⌘O quick switcher & ⌘⇧F active-space full-text search overlay
  src/view.rs                      → Shared Workspace / TopicView model
  src/osom.css                     → Layout-only styles (no custom button/card/row/Markdown/checkbox styling)
```

*(Note: `crates/osom/src/demo.rs` and `--demo` flags are removed; automated tests construct deterministic in-memory `Workspace` fixtures directly.)*

## Interface & Layout Contract

### 1. Three-Pane Window Anatomy
```
┌ Sidebar (240px) ─────┬ Workspace (flex) ───────────────────────┬ Chat (340px, ⌘L) ────┐
│ [Space ▾ Personal]   │ Today / Inbox / Topic / Embedded Editor │ Space or Topic chat  │
│ Search          ⌘⇧F  │                                         │                      │
│ Today         [3 !]  │ (When a doc opens: HostBar with Back,   │ Streaming turns,     │
│ Inbox           [2]  │  crumb, Save/Sync state, optional       │ tool badges, inline  │
│ TOPICS         [+]   │  "Merge with agent" conflict banner)    │ Approve/Deny,        │
│ ▸ Car      WOF 14/12 │                                         │ Queued (offline)     │
│ ▾ Wedding            │                                         │                      │
│ ▸ People             │                                         │ ┌──────────────────┐ │
│ ───────────────────  │                                         │ │ Ask…             │ │
│ ● Synced · 2s ago ↻  │                                         │ └──────────────────┘ │
└──────────────────────┴─────────────────────────────────────────┴──────────────────────┘
```
- **Zero manual file-upload UI in V1:** User files are uploaded/dropped directly on the server's per-space `files_dir`; the desktop app does not implement a local file-upload picker in V1.
- **Responsive narrow windows:** Below `960px` width, the Chat pane collapses cleanly behind the `⌘L` toggle button rather than crushing the Workspace or embedded editor.
- **Zero feature restyling:** Every button, card, row, tab, checkbox, badge, banner, popover, and Markdown block comes from `crates/margin-ui/`.

### 2. Window State & Persistence (`~/.config/osom/`)
- Remembers last active space (`last_space`), last open place (`Today` / `Inbox` / `Topic`), and Chat pane collapsed state in `~/.config/osom/ui-state.json`.
- Restores immediately from local replicas on launch (instant offline startup), then runs background SSE + sync catch-up without blocking UI interaction.

### 3. Keyboard Shortcuts (Active-Space Scoped)
| Shortcut | Action | Conflict Check |
|---|---|---|
| `⌘O` | Open Quick Switcher (switch space, jump to any topic or doc in active space) | Does not collide with editor |
| `⌘⇧F` | Open full-text search across active space's Markdown files + cached transcripts | Does not collide with editor |
| `⌘L` | Focus or toggle right-hand Chat pane | Does not collide with editor |
| `⌘[` / `⌘]` | Navigate Back / Forward in doc navigation stack | Standard macOS navigation |
| `Escape` | Close active overlay (`⌘O`, `⌘⇧F`, popover) or return focus to editor | Consistent with `crates/margin-editor/SPEC.md` |

### 4. Onboarding & Connection States
- **Not configured / unpaired:** Full-window calm **Connect & Pair** card (`Gateway URL` + `Pair code` from `zeroclaw gateway get-paircode --new`) saving to `~/.config/osom/config.json` and storing the bearer token in Keychain (release) / `~/.config/osom/token-<host>` (dev).
- **Configured & offline:** Launches straight into cached spaces with sidebar footer showing `Offline` (or `Offline · N pending` when space outbox or chat outbox has unsent items) and read-only cached chat history + offline chat queueing enabled.
- **Configured & online:** Sidebar footer shows `● Synced` (or `Syncing…`), pending count when flushing, and a quiet manual Refresh (`↻`) button.

## Code Style

Compose exclusively from `margin_ui` components and keep view-model assembly (`crates/osom/src/view.rs`, `search.rs`) pure and testable without launching a window:

```rust
/// Filter quick-switcher candidates (topics and docs) within the active space.
pub fn filter_quick_open<'a>(ws: &'a Workspace, query: &str) -> Vec<QuickItem<'a>> {
    let q = query.trim().to_ascii_lowercase();
    // Pure subsequence / prefix ranking over ws.topics and their docs
    ...
}
```

## Testing Strategy

| Layer | Location | What it verifies |
|---|---|---|
| Pure shell helpers | `crates/osom/src/search.rs`, `crates/osom/src/view.rs` | `⌘O` quick-switcher ranking, `⌘⇧F` full-text search across active space only, Today rollup ordering & overdue badge counts, UI state persistence |
| Headless Dioxus shell tests | `crates/osom/src/live/tests.rs` | Space switching swaps workspace cleanly, Connect/Pair onboarding state, offline boot from local replicas, sync status footer transitions (`Synced` / `Offline · N pending`), doc Back-stack navigation, zero `--demo` dependency |
| Binary separation check | `cargo check --locked -p margin` | `crates/margin` compiles and passes all editor + `margin ui` gallery tests with zero dependency on `crates/osom`, `crates/osom-hub`, or `crates/osom-topics` |
| Visual verification | Headless screenshot harness | Render key `osom` views (Today, Topic Overview, Topic + Embedded Editor, Chat, Onboarding) to PNG and inspect against [design-system.md](../docs/design-system.md) |

## Boundaries

- **Always:**
  - Build every UI surface strictly from `crates/margin-ui` components and `crates/margin-ui/assets/theme.css` tokens.
  - Keep `crates/margin` (Markdown editor + `margin ui` gallery) completely decoupled from `crates/osom`, `crates/osom-hub`, and `crates/osom-topics`.
  - Keep `cargo check --locked -p osom -p margin` at **0 warnings** and `cargo test --locked --workspace` passing.
  - Relocate root `SPEC.md` to `crates/margin-editor/SPEC.md` and update cross-references cleanly.
- **Ask first:**
  - Adding new top-level components or tokens to `crates/margin-ui` / `theme.css` (must also be added to `margin ui` gallery and `docs/design-system.md`).
  - Changing global keyboard shortcuts that could conflict with `margin-editor`.
- **Never:**
  - Re-introduce `--demo` or `crates/osom/src/demo.rs`.
  - Mix data from two spaces inside a single agent prompt, search result, or Today view in V1.
  - Add per-feature CSS overrides for buttons, cards, rows, checkboxes, or Markdown rendering.
  - Add a local file-upload picker in V1 (user files live on the server's per-space `files_dir`).

## Success Criteria

1. **Clean binary separation:** `crates/osom` launches the multi-space app directly (`demo.rs` and `--demo` deleted); `crates/margin` builds and passes all editor and `margin ui` tests independently with zero dependency on `osom*` crates; `SPEC.md` lives at `crates/margin-editor/SPEC.md`.
2. **Multi-space switching & instant offline boot:** Launching `osom` offline immediately renders the last active space from `~/.config/osom/spaces/`, lets the user switch between cached spaces, edit docs, mark reminders Done/Booked/Snooze, and queue chat messages — showing accurate `Offline · N pending` sync status.
3. **Today, Inbox, Topic & Editor flow:** Active-space Today displays overdue/upcoming reminders, cached Calendar events, recent discussions, and shopping lists; Inbox supports conversational triage; clicking any doc or wiki link opens `margin-editor` with working Back navigation and `scribe-<space>` comments.
4. **Keyboard navigation:** `⌘O` switches spaces/topics/docs, `⌘⇧F` searches active-space Markdown & cached transcripts, and `⌘L` toggles/focuses Chat without conflicting with any editor shortcut.
5. **Onboarding works without CLI:** Starting `osom` with an empty `~/.config/osom/` presents the inline Connect & Pair flow, pairs via `POST /pair`, discovers spaces via `GET /api/spaces`, and enters Today seamlessly.

## Out of Scope (Deferred to V2)

- **Cross-space "All spaces" Today rollup** — V1 keeps Today simple and scoped to the active space; a client-side cross-space visual rollup is deferred to V2.
- **Desktop file upload UI** — V1 uploads/drops user files directly on the server's per-space `files_dir`; an in-app drag-and-drop uploader (or Git LFS upload) is deferred to V2.
- **Dark mode theme toggle** — Deferred to V2 across `crates/margin-ui`.
- **Web and mobile (iOS/Android) shells** — V1 is macOS desktop (`Dioxus Native`) only.
- **Multi-window / tear-off tabs** — V1 is a single main window per launch; multi-window workspace management is deferred to V2.
- **Native macOS Notification Center & menu-bar companion** — Deferred to V2.

## Open Questions

None — `crates/margin-editor/SPEC.md` is the natural home beside `crates/margin-editor/`, and `topics` is already removed from `crates/margin`.
