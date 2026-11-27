# Implementation Plan: Osom V1

Source specs:
- [spec/capability-map.md](../spec/capability-map.md)
- [spec/SPEC-spaces-and-isolation.md](../spec/SPEC-spaces-and-isolation.md)
- [spec/SPEC-topics-and-wiki.md](../spec/SPEC-topics-and-wiki.md)
- [spec/SPEC-agents-and-chat.md](../spec/SPEC-agents-and-chat.md)
- [spec/SPEC-channels-and-routing.md](../spec/SPEC-channels-and-routing.md)
- [spec/SPEC-agenda-and-reminders.md](../spec/SPEC-agenda-and-reminders.md)
- [spec/SPEC-connectors-and-import.md](../spec/SPEC-connectors-and-import.md)
- [spec/SPEC-app-shell.md](../spec/SPEC-app-shell.md)

## Overview

Build **Osom V1** on top of the refactored Rust Cargo workspace (`crates/osom`, `crates/osom-hub`, `crates/osom-topics`, `crates/margin-core`, `crates/margin-projection`, `crates/margin-editor`, `crates/margin-ui`, `crates/margin-runtime`) and **one single server binary (`zeroclaw`)** extended via **ZeroClaw plugins** plus a small set of general-purpose **upstreamable ZeroClaw hooks** (such as `on_turn_end`, plugin hook dispatch, and plugin HTTP/SSE gateway routes) to replace SilverBullet and expose Osom's features with zero sidecar daemons. Implementation proceeds in vertical waves matching the approved capability-map DAG. `crates/margin` remains strictly isolated as the standalone Markdown editor and `margin ui` gallery harness.

## Architecture Decisions

1. **One Server Binary (`zeroclaw`) via Plugins + Upstreamable Hooks:**
   - **Zero sidecars, zero SilverBullet:** Only `zeroclaw` runs on the server.
   - **Small upstreamable ZeroClaw core hooks/extensions** (clean PRs suitable for upstream `zeroclaw`):
     - `HookHandler::on_turn_end` (fires after each agent turn with `{ agent, session_id, channel, modified_paths }`) — drives immediate post-turn space commits, reminder reconciliation, and wiki hygiene checks.
     - **Plugin Hook bridge** (dispatch `HookHandler` events — `on_turn_end`, `on_file_changed`, `on_heartbeat_tick`, `on_message_received` — to plugins).
     - **Plugin Gateway HTTP/SSE routes** (allow plugins to mount authenticated routes under the single `zeroclaw-gateway` Axum router without hardcoding Osom logic into ZeroClaw core).
     - **`Channel::create_room` for Telegram & Slack** (ZeroClaw already provides Telegram `message_thread_id` Forum Topics, Slack channels/threads, and the `channel_room` tool for Matrix — we only upstream `create_room` on `telegram.rs` via `createForumTopic` and `slack.rs` via `conversations.create`, with **no** separate `osom-channels` plugin needed).
   - **Osom ZeroClaw plugins (only two!):**
     - `osom-spaces` (replaces SilverBullet: `/api/spaces/*` HTTP + SSE, isolated git repos + `files_dir` + conflict SHAs, commits agent edits on `on_turn_end` / `on_file_changed`). *(Note: since WASI WASM components cannot `exec` host `git`, `osom-spaces` either runs as a native ZeroClaw plugin crate registered with the single binary or consumes a minimal host git/space capability — decided explicitly in Wave 1 Task 5a.)*
     - `osom-reminders` (hooks `on_turn_end` / `on_file_changed` + `on_heartbeat_tick` to evaluate `reminders.md` continuously and dispatch due pings to Telegram/Slack `DeliveryTarget`s).
2. **Workspace boundary (`osom` vs. `margin`):** `crates/margin` never depends on `osom*` crates. Shared editor primitives stay in `margin-core`, `margin-projection`, `margin-editor`, `margin-ui`, and `margin-runtime`; Osom-specific networking/sync lives in `crates/osom-hub`, pure Osom domain parsing/math lives in `crates/osom-topics`, and the desktop shell lives in `crates/osom`. Root `SPEC.md` moves to `crates/margin-editor/SPEC.md`.
3. **Offline-first Spaces over `/api/spaces`:** Replaces SilverBullet completely in `osom-hub`. Existing pre-split `src/spaces.rs` ports cleanly into `crates/osom-hub/src/spaces.rs`. Config and replicas live under `~/.config/osom/`.
4. **Strict zero cross-space isolation (V1):** Every space has its own git repo (Markdown & config only), its own server `files_dir` drop folder, its own primary agent, its own read-only `scribe-<space>` review agent, and its own read-only connector scopes. Existing `z-osom-tka` stays untouched as `personal`.
5. **Rename-immune Markdown comments:** `_osom/comments/<page-id>.md` keyed by lazily assigned YAML frontmatter `id:` in each page, with read-only fallback/migration from legacy `_margin/comments/<page>.json`.
6. **No `--demo` mode in `osom`:** `crates/osom/src/demo.rs` and `--demo` CLI flags are deleted; tests use deterministic in-memory `Workspace` fixtures.

## Dependency Graph & Parallelization

```
Wave 0 (Housekeeping & Playbook)
  └── Task 1 (SPEC.md move + drop demo.rs)
  └── Task 2 (docs/wiki-maintenance.md)        [parallel with Task 1]
        │
Wave 1 (spaces-and-isolation + ZeroClaw plugin/hook foundation)
  ├── Task 3 (osom-hub Replica + ~/.config/osom)
  └── Task 5a (Upstreamable ZeroClaw hooks: on_turn_end + plugin hook/HTTP-route surface)
        ├── Task 4 (Multi-space registry + SSE + PageFiles in osom-hub/osom)
        └── Task 5b (osom-spaces plugin: isolation, files_dir, conflict SHAs, on_turn_end commit) [parallel with Task 4]
              │
Wave 2 (topics-and-wiki)
  ├── Task 6 (Frontmatter id + _osom/comments/<page-id>.md)
  └── Task 7 (Wiki links + backlinks + in-app navigation)           [parallel with Task 6]
        └── Task 8 (Topic/doc create + Back/Forward stack)
              │
Wave 3 (agents-and-chat)
  ├── Task 9 (Per-space scribe + session IDs + Stop/abort)
  └── Task 10 (Offline chat outbox + transcript cache)              [parallel with Task 9]
        └── Task 11 (Automatic conflict rebase + Merge banner)
              │
Wave 4 (channels-and-routing)
  ├── Task 12 (Telegram topic thread_id + frontmatter routes + DeliveryTarget)
  └── Task 13 (Server Telegram createForumTopic + Slack channel tools) [parallel with Task 12]
        └── Task 14 (Discussion-driven Inbox + _osom/inbox.md)
              │
Wave 5 (agenda-and-reminders)
  ├── Task 15 (Pure reminders cadence + HH:MM + literal table editor)
  └── Task 16 (Server live per-space reminder scheduler)            [parallel with Task 15]
        └── Task 17 (Desktop Today/Calendar actions + Done popover + badge)
              │
Wave 6 (connectors-and-import)
  ├── Task 18 (Pure _osom/imports.md manifest + chat export parsers)
  └── Task 19 (Server read-only Mail/Calendar + cached Today calendar) [parallel with Task 18]
              │
Wave 7 (app-shell)
  └── Task 20 (Space switcher + ui-state.json + Sync footer + Connect/Pair UI)
        └── Task 21 (⌘O switcher + ⌘⇧F search + ⌘L chat + bundle & visual audit)
```

## Task List

Detailed tasks with acceptance criteria, verification commands, dependencies, and touched files live in [tasks/todo.md](todo.md).

### Wave 0: Housekeeping & Operational Playbook
- [x] Task 1: Relocate `SPEC.md` to `crates/margin-editor/SPEC.md` and remove `--demo` / `demo.rs` from `crates/osom`
- [x] Task 2: Write `docs/wiki-maintenance.md` (operational playbook & agent doctrine)
- [ ] **Checkpoint 0:** Workspace compiles with 0 warnings; `margin` and `osom` tests pass; playbook reviewed

### Wave 1: `spaces-and-isolation` (+ ZeroClaw Plugin/Hook Foundation)
- [ ] Task 3: Port `Replica` + durable outbox to `crates/osom-hub/src/spaces.rs` and switch config paths to `~/.config/osom/`
- [ ] Task 4: Implement multi-space registry, offline index cache, SSE `/events` catch-up, and `PageFiles` over `Space` (removing SilverBullet)
- [ ] Task 5a: Add upstreamable ZeroClaw hooks (`HookHandler::on_turn_end`) and plugin extension surface (plugin hook dispatch + authenticated gateway HTTP/SSE route mounting) in `~/src/osom/zeroclaw`
- [ ] Task 5b: Package `osom-spaces` as a ZeroClaw plugin (replacing SilverBullet under the single `zeroclaw` binary) with strict zero cross-space isolation, `files_dir`, conflict SHAs, and `on_turn_end` auto-commit
- [ ] **Checkpoint 1:** Single `zeroclaw` binary + `osom-spaces` plugin serves two isolated spaces; client discovers, syncs offline-first, catches up via SSE, and passes isolation + merge tests

### Wave 2: `topics-and-wiki`
- [ ] Task 6: Implement frontmatter `id:` helpers and rename-immune Markdown comment sidecars (`_osom/comments/<page-id>.md`) + legacy JSON migration
- [ ] Task 7: Implement `[[wikilinks]]`, relative space link resolution, space-wide backlinks index, and in-app `on_navigate` in `margin-editor` + `osom`
- [ ] Task 8: Add Topic & Doc creation actions and Back/Forward document navigation stack in `crates/osom`
- [ ] **Checkpoint 2:** Wiki navigation, backlinks, and rename-immune Markdown comments pass end-to-end tests

### Wave 3: `agents-and-chat`
- [ ] Task 9: Wire per-space `review_agent` (`scribe-<space>`), space-scoped session IDs, and `AgentChat::abort` (**Stop**)
- [ ] Task 10: Build durable offline chat outbox & cached transcript store (`crates/osom-hub/src/chat_outbox.rs`) and wire into `crates/osom/src/chat.rs`
- [ ] Task 11: Implement automatic conflict-rebase agent turn and fallback **"Merge with agent"** page banner
- [ ] **Checkpoint 3:** Streaming, Stop, per-space `scribe`, offline chat queue across restarts, and conflict rebase all pass

### Wave 4: `channels-and-routing`
- [ ] Task 12: Add Telegram `thread_id` session-key parsing, frontmatter `channels:` routes (with `README.md` fallback), and `DeliveryTarget` resolution in `crates/osom-topics`
- [ ] Task 13: Implement upstreamable `Channel::create_room` on ZeroClaw's existing Telegram (`createForumTopic`) and Slack (`conversations.create`) channels (reusing existing `message_thread_id` and `channel_room` tool)
- [ ] Task 14: Implement discussion-driven Inbox triage and `_osom/inbox.md` handled tracking in `crates/osom`
- [ ] **Checkpoint 4:** Telegram topics + Slack channels route cleanly, bot creates topics/channels, Inbox triage clears items

### Wave 5: `agenda-and-reminders`
- [ ] Task 15: Build pure cadence math (`YYYY-MM-DD` & `YYYY-MM-DD HH:MM`, lead time, `⅓` tightening, rollover) and literal-preserving `reminders.md` mutations in `crates/osom-topics`
- [ ] Task 16: Implement live per-space reminder scheduler reacting to `reminders.md` commits and minute ticks in `~/src/osom/zeroclaw`
- [ ] Task 17: Add desktop Today & Topic Calendar actions (**+ Add Reminder**, **Done** + optional log-note popover, **Booked**, **Snooze**) and sidebar Today overdue badge
- [ ] **Checkpoint 5:** Reminders update immediately on edit, fire at exact time/morning hour without duplicates, and work offline in desktop UI

### Wave 6: `connectors-and-import`
- [ ] Task 18: Build pure `_osom/imports.md` manifest, SHA-256 dedup, provenance tags, and WhatsApp/Telegram/Instagram export normalizers in `crates/osom-topics`
- [ ] Task 19: Enforce read-only per-space Mail & Calendar profiles in `~/src/osom/zeroclaw` and render cached read-only Calendar events in `osom` Today
- [ ] **Checkpoint 6:** Read-only Mail/Calendar enforced, re-imports are idempotent, Today shows cached calendar events

### Wave 7: `app-shell`
- [ ] Task 20: Add Space Switcher, persistent `ui-state.json`, live Sync status footer (`Synced` / `Syncing…` / `Offline · N pending`), and inline **Connect & Pair** onboarding
- [ ] Task 21: Add `⌘O` Quick Switcher, `⌘⇧F` active-space full-text search, `⌘L` collapsible Chat pane, and run full bundle + visual screenshot verification
- [ ] **Checkpoint 7 (V1 Complete):** All 7 module success criteria met, 0 warnings across workspace, `Osom.app` bundles cleanly, visual audit approved

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Two-repo drift (`dioxus-notes` vs. `~/src/osom/zeroclaw`) | High | Define JSON payloads and error codes up front in specs; test both sides with contract fixtures before live cutover |
| Breaking `crates/margin` or `crates/margin-editor` while updating `PageFiles` / `link_card` | High | Keep `PageFiles` changes minimal and optional (`on_navigate` callback); run `cargo test --locked -p margin -p margin-editor` at every checkpoint |
| Breaking backwards compatibility with live `z-osom-tka` Markdown tables (`README.md`, `reminders.md`, `questions.md`) | Medium | Include real anonymized `z-osom-tka` fixtures in `osom-topics` unit tests; preserve fallback parsing for root `README.md` routes and legacy `_margin/comments/*.json` |
| Stale pre-split prototype in `src/spaces.rs` & `git stash@{0}` | Low | Port tested `Replica` logic from `src/spaces.rs` cleanly into `crates/osom-hub/src/spaces.rs`, then delete top-level `src/spaces.rs` in Task 3 so root stays tidy |
| Double-firing reminders on server restart or clock jitter | Medium | Commit `Last ping` + `Next ping` atomically under the `zeroclaw-spaces` lock before/with dispatch and check slot idempotency |

## Open Questions

None — all 7 module specs are approved. Review `tasks/plan.md` and `tasks/todo.md`, and let me know if you want to adjust task order or start implementing Wave 0 / Wave 1.
