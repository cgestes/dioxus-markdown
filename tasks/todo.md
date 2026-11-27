# Task Breakdown: Osom V1

Plan: [tasks/plan.md](plan.md) · Capability Map: [spec/capability-map.md](../spec/capability-map.md)

---

## Wave 0: Housekeeping & Operational Playbook

## Task 1: Relocate `SPEC.md` and remove `--demo` from `crates/osom`

**Description:** Move the Markdown editor specification from repo root `SPEC.md` into `crates/margin-editor/SPEC.md` (updating links in `README.md`, `docs/`, and `spec/`), and remove `crates/osom/src/demo.rs` and `--demo` flags from `crates/osom` so `osom` is strictly a live/offline-replica app with zero demo workspace baggage. Replace demo-dependent unit tests in `crates/osom` with minimal in-memory `Workspace` fixtures.

**Acceptance criteria:**
- [x] `SPEC.md` lives at `crates/margin-editor/SPEC.md` and all repo links resolve cleanly
- [x] `crates/osom/src/demo.rs` and `--demo` CLI flag are removed from `crates/osom`; `osom --help` lists no demo flag
- [x] `crates/margin` remains untouched and compiles independently

**Verification:**
- [x] Tests pass: `cargo test --locked -p osom -p margin -p margin-editor`
- [x] Build succeeds: `cargo check --locked -p osom -p margin` (0 warnings)
- [x] Manual check: `cargo run --locked -p osom -- --help`

**Dependencies:** None

**Files likely touched:**
- `SPEC.md` → `crates/margin-editor/SPEC.md`
- `crates/osom/src/demo.rs` (delete)
- `crates/osom/src/cli.rs`
- `crates/osom/src/live.rs`
- `crates/osom/src/lib.rs`

**Estimated scope:** Medium (5 files)

---

## Task 2: Author `docs/wiki-maintenance.md` (Operational Playbook & Agent Doctrine)

**Description:** Write the living operational playbook for humans and space agents covering all six pillars: (1) maintaining and evolving the wiki and procedures without rot/duplication, (2) reviewing human and agent changes via git history and anchored comments, (3) making sure nothing is missed (Inbox triage, Questions Bank, unlogged procedure runs, imports), (4) detecting both edit conflicts and factual contradictions across docs, (5) proactive agent behavior (Questions Bank context triggers, brief upkeep, gap surfacing), and (6) reminder hygiene and audit routines.

**Acceptance criteria:**
- [x] Covers all 6 operational pillars with concrete rules, checklists, and anti-patterns
- [x] Includes a copyable/includable `AGENTS.md` snippet that each space agent inherits
- [x] Aligns with all 7 approved module specs (strict zero cross-space isolation, `_osom/` files, forgiving in-space routing, read-only connectors)

**Verification:**
- [ ] Manual check: Human reviews `docs/wiki-maintenance.md` for tone, completeness, and fit with daily workflow

**Dependencies:** None (can run in parallel with Task 1)

**Files likely touched:**
- `docs/wiki-maintenance.md`
- `spec/capability-map.md` (link to created doc)

**Estimated scope:** Small (2 files)

---

## Checkpoint 0: After Tasks 1–2
- [ ] `cargo check --locked --workspace` passes with 0 warnings
- [ ] `cargo test --locked --workspace` passes
- [ ] `docs/wiki-maintenance.md` reviewed by human before starting Wave 1

---

## Wave 1: `spaces-and-isolation`

## Task 3: Port offline-first `Replica` + outbox into `crates/osom-hub/src/spaces.rs` and `~/.config/osom/`

**Description:** Move the offline-first `Replica`, durable outbox, `Remote` trait, and `flock` locking from top-level `src/spaces.rs` into `crates/osom-hub/src/spaces.rs` (removing the orphaned root `src/spaces.rs`), and switch `Endpoints` config/token/replica paths in `crates/osom-hub/src/lib.rs` to `~/.config/osom/` (with `OSOM_CONFIG` env override).

**Acceptance criteria:**
- [ ] `crates/osom-hub/src/spaces.rs` compiles cleanly and all offline replica/outbox unit tests pass
- [ ] Root `src/spaces.rs` is removed
- [ ] `Endpoints::path()` resolves under `~/.config/osom/` and replicas store under `~/.config/osom/spaces/<gateway>/<space>/`

**Verification:**
- [ ] Tests pass: `cargo test --locked -p osom-hub`
- [ ] Build succeeds: `cargo check --locked -p osom-hub` (0 warnings)

**Dependencies:** Task 1

**Files likely touched:**
- `crates/osom-hub/Cargo.toml`
- `crates/osom-hub/src/spaces.rs` (new, ported from `src/spaces.rs`)
- `crates/osom-hub/src/lib.rs`
- `src/spaces.rs` (delete)

**Estimated scope:** Medium (4 files)

---

## Task 4: Multi-space registry, SSE catch-up, and `PageFiles` over `Space` (remove SilverBullet)

**Description:** Replace SilverBullet HTTP access in `crates/osom-hub/src/lib.rs` with `SpaceApi` (`/api/spaces/{id}`), multi-space discovery (`GET /api/spaces` + offline `index.json` cache), SSE `/api/spaces/{id}/events` background catch-up with backoff, and `PageFiles` implemented over local `Space` replicas. Update `crates/osom` to drop SilverBullet browser buttons and read/write via local `Space` replicas.

**Acceptance criteria:**
- [ ] `SpaceRegistry` lists spaces from `GET /api/spaces`, persists offline `index.json`, and boots offline with zero network calls
- [ ] SSE event parser triggers `Replica::sync` on `head` change without overwriting pending outbox paths
- [ ] `Space` implements `margin_core::files::PageFiles` without SilverBullet URLs; SilverBullet buttons removed from `crates/osom`

**Verification:**
- [ ] Tests pass: `cargo test --locked -p osom-hub -p osom`
- [ ] Build succeeds: `cargo check --locked -p osom -p margin` (0 warnings)

**Dependencies:** Task 3

**Files likely touched:**
- `crates/osom-hub/src/lib.rs`
- `crates/osom-hub/src/sse.rs` (new)
- `crates/osom/src/live.rs`
- `crates/osom/src/pages.rs`
- `crates/osom/src/topic.rs`

**Estimated scope:** Medium (5 files)

---

## Task 5a: Upstreamable ZeroClaw hooks (`on_turn_end`) & plugin gateway/hook surface (`zeroclaw`)

**Description:** In `~/src/osom/zeroclaw`, add minimal, general-purpose extension points suitable for upstreaming to ZeroClaw: (1) `HookHandler::on_turn_end` firing after each agent turn completes with `{ agent, session_id, channel, modified_paths }`, (2) plugin hook dispatch so plugins can subscribe to `on_turn_end`, `on_file_changed`, and `on_heartbeat_tick`, and (3) an authenticated gateway route-registration hook so plugins can mount HTTP/SSE routes inside the single `zeroclaw` binary without embedding Osom-specific business logic in ZeroClaw core.

**Acceptance criteria:**
- [ ] `HookHandler::on_turn_end` fires reliably at the end of gateway WS, Telegram, and Slack turns (covered by unit/integration tests)
- [ ] Plugins can register authenticated HTTP/SSE routes under the single `zeroclaw-gateway` router and receive `on_turn_end` / `on_file_changed` / `on_heartbeat_tick` hooks
- [ ] Changes stay generic (zero Osom-specific names or logic in `zeroclaw-api` / `zeroclaw-gateway` / `zeroclaw-plugins`), ready for upstream PRs

**Verification:**
- [ ] Tests pass: `cargo test --manifest-path ~/src/osom/zeroclaw/Cargo.toml -p zeroclaw-api -p zeroclaw-plugins -p zeroclaw-gateway`

**Dependencies:** None (can run in parallel with Task 3)

**Files likely touched:**
- `~/src/osom/zeroclaw/crates/zeroclaw-api/src/hook.rs`
- `~/src/osom/zeroclaw/crates/zeroclaw-runtime/src/` (turn-end dispatch)
- `~/src/osom/zeroclaw/crates/zeroclaw-plugins/src/` (hook/route capability registration)
- `~/src/osom/zeroclaw/crates/zeroclaw-gateway/src/` (authenticated plugin route mount)

**Estimated scope:** Medium (4 files)

---

## Task 5b: `osom-spaces` ZeroClaw plugin — strict isolation, `files_dir`, conflict SHAs & `on_turn_end` commit

**Description:** Package `zeroclaw-spaces` + `/api/spaces/*` as the `osom-spaces` plugin inside the single `zeroclaw` binary (replacing SilverBullet cleanly): enforce strict zero cross-space isolation across `repo`, `files_dir`, and agent workspace/memory, include `review_agent` in `GET /api/spaces`, return addressable `{ base, head, client_commit }` SHAs on overlapping text conflicts, and commit dirty agent workspace edits automatically on `on_turn_end` (and `on_file_changed`).

**Acceptance criteria:**
- [ ] Single `zeroclaw` binary loads `osom-spaces` plugin and serves `/api/spaces/*` (HTTP + SSE) with zero SilverBullet dependency
- [ ] Automated test proves an agent in Space A is denied reading git files, `files_dir`, and memory in Space B
- [ ] Overlapping text conflict response includes `base`, `head`, and `client_commit` SHAs; agent edits commit automatically on `on_turn_end`

**Verification:**
- [ ] Tests pass: `cargo test --manifest-path ~/src/osom/zeroclaw/Cargo.toml -p zeroclaw-spaces -p zeroclaw-gateway spaces`
- [ ] Manual check: `curl` `/api/spaces` on single `zeroclaw` binary returns `agent` and `review_agent`

**Dependencies:** Task 5a (can run in parallel with Task 4)

**Files likely touched:**
- `~/src/osom/zeroclaw/crates/zeroclaw-spaces/src/lib.rs`
- `~/src/osom/zeroclaw/crates/zeroclaw-spaces/src/merge.rs`
- `~/src/osom/zeroclaw/crates/zeroclaw-spaces/tests/spaces.rs`
- `~/src/osom/zeroclaw/crates/zeroclaw-gateway/src/api_spaces.rs`

**Estimated scope:** Medium (4 files)

---

## Checkpoint 1: After Tasks 3–5b (`spaces-and-isolation`)
- [ ] Single `zeroclaw` binary + `osom-spaces` plugin passes all isolation, merge, and `on_turn_end` commit tests
- [ ] All client space tests pass (`cargo test --locked -p osom-hub -p osom`)
- [ ] Offline boot, multi-space discovery, SSE catch-up, and strict zero cross-space isolation verified
- [ ] Review with human before proceeding to Wave 2

---

## Wave 2: `topics-and-wiki`

## Task 6: Frontmatter `id:` and rename-immune Markdown comment sidecars (`_osom/comments/<page-id>.md`)

**Description:** Update `crates/margin-core/src/comments.rs` with pure YAML frontmatter `id:` extraction and lazy insertion, Markdown comment sidecar serialization/parsing at `_osom/comments/<page-id>.md` (including ```` ```margin-edit ```` proposal status and deterministic thread/message deduplication after merge), and read-only migration fallback from legacy `_margin/comments/<page>.json`. Wire `crates/margin-editor/src/comments/mod.rs` to load/save through the new format.

**Acceptance criteria:**
- [ ] Moving or renaming a page preserves all anchored comment threads via its frontmatter `id:`
- [ ] Pages without comments remain untouched until the first thread is created (lazy `id:` assignment)
- [ ] Legacy `_margin/comments/<page>.json` files load cleanly and migrate to `_osom/comments/<page-id>.md` on next save

**Verification:**
- [ ] Tests pass: `cargo test --locked -p margin-core -p margin-editor`
- [ ] Build succeeds: `cargo check --locked -p osom -p margin` (0 warnings)

**Dependencies:** Task 4

**Files likely touched:**
- `crates/margin-core/src/comments.rs`
- `crates/margin-editor/src/comments/mod.rs`
- `crates/osom-topics/src/space.rs` (hide `_osom` root alongside `_margin`)

**Estimated scope:** Medium (3 files)

---

## Task 7: Wiki links (`[[wikilinks]]` + relative `.md`), space backlinks index, and in-app navigation

**Description:** Add pure `[[wikilinks]]` and relative Markdown link extraction + space-wide backlink indexing to `crates/osom-topics/src/page.rs`. Extend `crates/margin-projection/src/link_preview.rs` and `crates/margin-editor/src/notebook/link_card.rs` with an optional in-app `on_navigate(String)` callback (keeping `crates/margin` unaffected), and wire `crates/osom/src/pages.rs` so clicking links in either `MarkdownView` or `#link-card-open` navigates directly to the target page inside the active space.

**Acceptance criteria:**
- [ ] Relative links (`car/log.md`, `../people/nikki`), `[[wikilinks]]`, and `#heading` anchors resolve within the active space and reject `..` traversal outside space root
- [ ] Backlink index computes exact referring pages and counts across the active space (excluding `_osom/`)
- [ ] Clicking `#link-card-open` or a `MarkdownView` wiki link opens the target page in-app inside `crates/osom`

**Verification:**
- [ ] Tests pass: `cargo test --locked -p osom-topics -p margin-projection -p margin-editor -p osom`
- [ ] Build succeeds: `cargo check --locked -p osom -p margin` (0 warnings)

**Dependencies:** Task 4 (can run in parallel with Task 6)

**Files likely touched:**
- `crates/osom-topics/src/page.rs`
- `crates/osom-topics/src/tests.rs`
- `crates/margin-projection/src/link_preview.rs`
- `crates/margin-editor/src/notebook/link_card.rs`
- `crates/osom/src/pages.rs`

**Estimated scope:** Medium (5 files)

---

## Task 8: Topic/Doc creation actions and Back/Forward doc navigation stack in `crates/osom`

**Description:** Add UI actions in `crates/osom` to create a new Topic (`<slug>/README.md` with frontmatter) and create a new Doc/Procedure/List inside a topic (writing directly to the space's local `Replica`), plus a Back/Forward navigation stack (`⌘[` / `⌘]`) in `crates/osom/src/pages.rs` so wiki link hops return cleanly to previous docs or topic tabs.

**Acceptance criteria:**
- [ ] Creating a topic writes `<slug>/README.md` with valid frontmatter and `# <Title>`, immediately appearing in the sidebar offline
- [ ] Creating a doc or following a link to an uncreated page offers to create it inside that topic
- [ ] Navigating A → B → C via wiki links supports Back (`⌘[`) and Forward (`⌘]`) cleanly

**Verification:**
- [ ] Tests pass: `cargo test --locked -p osom -p osom-topics`
- [ ] Build succeeds: `cargo check --locked -p osom` (0 warnings)

**Dependencies:** Task 6, Task 7

**Files likely touched:**
- `crates/osom-topics/src/space.rs`
- `crates/osom/src/lib.rs`
- `crates/osom/src/topic.rs`
- `crates/osom/src/pages.rs`

**Estimated scope:** Medium (4 files)

---

## Checkpoint 2: After Tasks 6–8 (`topics-and-wiki`)
- [ ] `cargo test --locked --workspace` passes with 0 warnings
- [ ] Rename-safe `_osom/comments/<page-id>.md`, wiki navigation, backlinks, and offline topic/doc creation verified
- [ ] Review with human before proceeding to Wave 3

---

## Wave 3: `agents-and-chat`

## Task 9: Per-space `review_agent`, space-scoped session IDs, and `AgentChat::abort` (**Stop**)

**Description:** Update `margin_core::agent::AgentChat` with `abort(&self, session: &str)` (`POST /api/sessions/{id}/abort` in `osom-hub`), switch in-app session IDs to `osom-space-<space>`, `osom-topic-<space>-<topic>`, and `osom-comment-<space>-<thread>` (never touching Telegram/Slack session keys), bind `space.review_agent` per space for anchored comments, and wire a **Stop** button during streaming in `crates/osom/src/chat.rs` and `crates/margin-editor/src/comments/mod.rs`.

**Acceptance criteria:**
- [ ] Each space binds its own `agent` and `review_agent` from `SpaceInfo` (zero cross-space reuse)
- [ ] In-app sessions use `osom-space-*` / `osom-topic-*` / `osom-comment-*` prefixes
- [ ] Clicking **Stop** cancels the stream immediately, calls `abort(session)`, and marks partial text cleanly

**Verification:**
- [ ] Tests pass: `cargo test --locked -p margin-core -p osom-hub -p osom-topics -p osom`
- [ ] Build succeeds: `cargo check --locked -p osom -p margin` (0 warnings)

**Dependencies:** Task 4, Task 5b

**Files likely touched:**
- `crates/margin-core/src/agent.rs`
- `crates/osom-hub/src/lib.rs`
- `crates/osom-topics/src/chats.rs`
- `crates/osom/src/chat.rs`
- `crates/osom/src/pages.rs`

**Estimated scope:** Medium (5 files)

---

## Task 10: Durable offline chat outbox & transcript cache (`crates/osom-hub/src/chat_outbox.rs`)

**Description:** Implement `ChatOutbox` and `TranscriptCache` under `~/.config/osom/chat/<gateway>/<space>/` with atomic `fsync` + rename persistence, FIFO per-session drain, edit/cancel before send, and retry/failed state handling. Wire `crates/osom/src/chat.rs` so offline messages display immediately with a *Queued (offline)* badge (Edit/Cancel) and flush automatically on reconnect.

**Acceptance criteria:**
- [ ] Messages composed offline persist across app restart and render immediately as *Queued (offline)*
- [ ] Queued messages can be edited or cancelled before delivery and drain strictly FIFO on reconnect
- [ ] Permanent send failures mark `Failed(reason)` with Retry/Discard without blocking other sessions or dropping prompts

**Verification:**
- [ ] Tests pass: `cargo test --locked -p osom-hub chat_outbox`
- [ ] Build succeeds: `cargo check --locked -p osom` (0 warnings)

**Dependencies:** Task 3 (can run in parallel with Task 9)

**Files likely touched:**
- `crates/osom-hub/src/chat_outbox.rs` (new)
- `crates/osom-hub/src/lib.rs`
- `crates/osom/src/chat.rs`
- `crates/osom/src/live.rs`

**Estimated scope:** Medium (4 files)

---

## Task 11: Automatic conflict-rebase agent turn + fallback `"Merge with agent"` banner

**Description:** When `Replica::sync` returns an overlapping text conflict `{ path, base, head, client_commit }`, fetch the 3 versions and run an automatic single-turn rebase via the space's agent on ephemeral session `osom-rebase-<space>-<change-id>`, parsing the fenced ```` ```osom-rebase ```` block and committing the reconciled file. If offline or the rebase turn fails, persist the pending conflict flag and display a subtle banner on that page with a **"Merge with agent"** / **"Retry merge"** button.

**Acceptance criteria:**
- [ ] Simulated 3-version overlap invokes the space agent with `(base, head, client_commit)`, validates non-empty output, and commits reconciled Markdown
- [ ] Failed/offline rebase preserves the union-merged file safely (zero data loss) and surfaces the `"Merge with agent"` banner
- [ ] Ephemeral rebase session is deleted after a successful commit

**Verification:**
- [ ] Tests pass: `cargo test --locked -p osom-hub rebase` and `cargo test --locked -p osom`
- [ ] Build succeeds: `cargo check --locked -p osom` (0 warnings)

**Dependencies:** Task 5b, Task 9

**Files likely touched:**
- `crates/osom-hub/src/rebase.rs` (new)
- `crates/osom-hub/src/lib.rs`
- `crates/osom/src/live.rs`
- `crates/osom/src/pages.rs`

**Estimated scope:** Medium (4 files)

---

## Checkpoint 3: After Tasks 9–11 (`agents-and-chat`)
- [ ] `cargo test --locked --workspace` passes with 0 warnings
- [ ] Streaming, Stop, per-space `scribe`, offline chat outbox across restart, and conflict rebase verified
- [ ] Review with human before proceeding to Wave 4

---

## Wave 4: `channels-and-routing`

## Task 12: Telegram `thread_id`, frontmatter `channels:` routes, and `DeliveryTarget` in `crates/osom-topics`

**Description:** Extend `crates/osom-topics/src/routes.rs` and `chats.rs` to parse `channels:` YAML frontmatter on topic `README.md` (with fallback to space root `README.md` tables), parse Telegram `message_thread_id` in `Origin::Telegram { chat, thread }`, allow one channel to route to multiple topics, and implement deterministic 3-step `DeliveryTarget` resolution.

**Acceptance criteria:**
- [ ] Both frontmatter `channels:` and legacy `z-osom-tka` root `README.md` route tables parse with frontmatter taking precedence
- [ ] `Origin::Telegram` distinguishes whole-group chats from Forum/DM topic threads (`thread_id`)
- [ ] `DeliveryTarget` resolves deterministically (`Deliver to` override → topic primary Telegram/Slack route → space default)

**Verification:**
- [ ] Tests pass: `cargo test --locked -p osom-topics`
- [ ] Build succeeds: `cargo check --locked -p osom` (0 warnings)

**Dependencies:** Task 7

**Files likely touched:**
- `crates/osom-topics/src/routes.rs`
- `crates/osom-topics/src/chats.rs`
- `crates/osom-topics/src/tests.rs`

**Estimated scope:** Small (3 files)

---

## Task 13: Upstream `Channel::create_room` on ZeroClaw's Telegram (`createForumTopic`) & Slack (`conversations.create`)

**Description:** ZeroClaw already implements Telegram `message_thread_id` Forum Topics (`telegram.rs`), Slack channels/threads (`slack.rs`), and the built-in `channel_room` tool (`Channel::create_room`, currently implemented for Matrix). Implement `Channel::create_room` on `telegram.rs` (calling Telegram `createForumTopic` inside the space's configured chat/DM and returning `chat_id:message_thread_id`) and on `slack.rs` (calling Slack `conversations.create` when scopes permit) — no separate `osom-channels` plugin needed.

**Acceptance criteria:**
- [ ] Existing ZeroClaw `channel_room` tool (`action = "create_room"`) creates a Telegram Forum/DM topic via `createForumTopic` and returns `chat_id:message_thread_id`
- [ ] Existing ZeroClaw `channel_room` tool creates a Slack channel via `conversations.create` when permitted and returns a clear error without scopes
- [ ] Existing Telegram `message_thread_id` session routing and outbound replies remain intact (verified against ZeroClaw's existing tests)

**Verification:**
- [ ] Tests pass: `cargo test --manifest-path ~/src/osom/zeroclaw/Cargo.toml -p zeroclaw-channels telegram` and `slack`

**Dependencies:** Task 5b (can run in parallel with Task 12)

**Files likely touched:**
- `~/src/osom/zeroclaw/crates/zeroclaw-channels/src/telegram.rs`
- `~/src/osom/zeroclaw/crates/zeroclaw-channels/src/slack.rs`

**Estimated scope:** Small (2 files)

---

## Task 14: Discussion-driven Inbox triage & `_osom/inbox.md` handled tracking in `crates/osom`

**Description:** Update `crates/osom-topics/src/chats.rs` and `crates/osom/src/topic.rs` so unrouted Telegram/Slack sessions appear in the space's Inbox, let the user discuss an Inbox thread with the space agent to discover/file/create its topic, and persist handled/dismissed session keys in `_osom/inbox.md`.

**Acceptance criteria:**
- [ ] Unrouted sessions appear in Inbox ordered by `last_activity`; comment and rebase sessions never appear in Inbox or Discussions
- [ ] Clicking "Discuss in chat" opens the space chat pre-contextualized with the Inbox thread so the agent can file facts or create a topic
- [ ] Marking an item handled writes to `_osom/inbox.md` in the local replica and removes it from the active Inbox

**Verification:**
- [ ] Tests pass: `cargo test --locked -p osom-topics -p osom`
- [ ] Build succeeds: `cargo check --locked -p osom` (0 warnings)

**Dependencies:** Task 9, Task 12

**Files likely touched:**
- `crates/osom-topics/src/chats.rs`
- `crates/osom/src/live.rs`
- `crates/osom/src/topic.rs`
- `crates/osom/src/chat.rs`

**Estimated scope:** Medium (4 files)

---

## Checkpoint 4: After Tasks 12–14 (`channels-and-routing`)
- [ ] All client & server routing tests pass
- [ ] Telegram Forum/DM topic routing, `DeliveryTarget`, and Inbox triage verified
- [ ] Review with human before proceeding to Wave 5

---

## Wave 5: `agenda-and-reminders`

## Task 15: Pure reminder cadence math (`YYYY-MM-DD HH:MM`) & literal-preserving `reminders.md` edits

**Description:** Create `crates/osom-topics/src/reminders.rs` (and extend `calendar.rs` / `planning.rs`) with pure, deterministic cadence math (date-only + `YYYY-MM-DD HH:MM` time-of-day, recurrence lead times, `⅓` tightening, overdue 1d/2d cadence, `booked` day-before rule, `done` recurring rollover, `questions.md` `Ask by` surfacing) and literal-preserving `reminders.md` table mutations (Add, Done, Booked, Snooze).

**Acceptance criteria:**
- [ ] Both `YYYY-MM-DD` and `YYYY-MM-DD HH:MM` parse, sort, and compute `next_ping` deterministically with explicit `now`
- [ ] Add / Done (with recurring rollover) / Booked / Snooze mutate only the targeted row while preserving untouched bytes, prose, and headers
- [ ] `questions.md` items with approaching `Ask by` dates surface cleanly in the agenda

**Verification:**
- [ ] Tests pass: `cargo test --locked -p osom-topics`
- [ ] Build succeeds: `cargo check --locked -p osom-topics` (0 warnings)

**Dependencies:** Task 12

**Files likely touched:**
- `crates/osom-topics/src/reminders.rs` (new)
- `crates/osom-topics/src/calendar.rs`
- `crates/osom-topics/src/planning.rs`
- `crates/osom-topics/src/lib.rs`
- `crates/osom-topics/src/tests.rs`

**Estimated scope:** Medium (5 files)

---

## Task 16: `osom-reminders` ZeroClaw plugin — live per-space reminder scheduler

**Description:** Implement `osom-reminders` as a ZeroClaw plugin inside the single `zeroclaw` binary (replacing manual per-destination reminder crons): subscribe to `on_turn_end` and `on_file_changed` to reload `reminders.md` / `questions.md` immediately, evaluate due items on `on_heartbeat_tick` (minute tick in the space's timezone, firing `YYYY-MM-DD HH:MM` at exact minute and date-only items at `reminder_hour`), dispatch directly to the topic's `DeliveryTarget`, and atomically commit `Last ping` / `Next ping` under the space lock to prevent duplicate pings.

**Acceptance criteria:**
- [ ] Editing `reminders.md` refreshes the in-memory schedule immediately without restarting the server
- [ ] Exact time-of-day reminders fire within 1 minute of their scheduled time; nothing-due ticks produce zero channel messages
- [ ] Restarting immediately after a ping never double-sends the same slot

**Verification:**
- [ ] Tests pass: `cargo test --manifest-path ~/src/osom/zeroclaw/Cargo.toml reminder`

**Dependencies:** Task 5a, Task 5b, Task 13 (can run in parallel with Task 15)

**Files likely touched:**
- `~/src/osom/zeroclaw/crates/zeroclaw-runtime/src/spaces_reminders.rs` (new)
- `~/src/osom/zeroclaw/crates/zeroclaw-runtime/src/spaces_sync.rs`
- `~/src/osom/zeroclaw/crates/zeroclaw-runtime/src/lib.rs`

**Estimated scope:** Medium (3 files)

---

## Task 17: Desktop Today & Topic Calendar quick actions + Done log popover + overdue badge

**Description:** Wire **+ Add Reminder**, **Done** (opening a tiny optional log-note popover with Enter to confirm and append to `<topic>/log.md`), **Booked**, and **Snooze** controls in `crates/osom/src/today.rs` and `crates/osom/src/topic.rs` against the local `Replica`, and show the overdue + due-today count on the sidebar **Today** badge (tinted `Tone::Alert` / `Tone::Warn` when overdue).

**Acceptance criteria:**
- [ ] All four reminder actions (Add, Done + log popover, Booked, Snooze) work offline-first by mutating `reminders.md` (and `log.md`) in the local replica
- [ ] Marking a recurring reminder Done creates its next occurrence automatically
- [ ] Sidebar Today badge displays overdue + due-today count with overdue alert tint

**Verification:**
- [ ] Tests pass: `cargo test --locked -p osom -p osom-topics`
- [ ] Build succeeds: `cargo check --locked -p osom` (0 warnings)

**Dependencies:** Task 15

**Files likely touched:**
- `crates/osom/src/today.rs`
- `crates/osom/src/topic.rs`
- `crates/osom/src/lib.rs`
- `crates/osom/src/view.rs`

**Estimated scope:** Medium (4 files)

---

## Checkpoint 5: After Tasks 15–17 (`agenda-and-reminders`)
- [ ] All reminder cadence, scheduler, and desktop UI tests pass
- [ ] Live exact-time & morning reminder dispatch verified with zero duplicate pings
- [ ] Review with human before proceeding to Wave 6

---

## Wave 6: `connectors-and-import`

## Task 18: Pure `_osom/imports.md` manifest, provenance tags, and chat-export parsers

**Description:** Create `crates/osom-topics/src/imports.rs` implementing `_osom/imports.md` table parsing/literal-preserving updates, SHA-256 skip/re-import checks, concise provenance tag formatting, and pure normalizers for WhatsApp `.txt`, Telegram `result.json`, and Instagram `message_*.json` exports into `(ts, author, text)` turns.

**Acceptance criteria:**
- [ ] `_osom/imports.md` round-trips cleanly; unchanged `(key, sha256)` pairs are skipped while modified hashes trigger incremental re-import
- [ ] WhatsApp, Telegram, and Instagram export fixtures normalize cleanly without media blobs
- [ ] Provenance helpers format consistent source tags for `log.md`, `reminders.md`, and `questions.md`

**Verification:**
- [ ] Tests pass: `cargo test --locked -p osom-topics imports`
- [ ] Tests pass: `python3 -m unittest discover -s ~/src/osom/z-osom-tka/tools/apple-notes-export/tests`

**Dependencies:** Task 6

**Files likely touched:**
- `crates/osom-topics/src/imports.rs` (new)
- `crates/osom-topics/src/lib.rs`
- `crates/osom-topics/src/tests.rs`

**Estimated scope:** Small (3 files)

---

## Task 19: Server read-only Mail/Calendar profiles (`zeroclaw`) & cached Calendar events in `osom` Today

**Description:** Enforce read-only tool profiles on per-space Mail and Calendar connectors in `~/src/osom/zeroclaw` (blocking send-email and calendar-write tools in V1 while allowing search/read + Telegram notification), expose a read-only upcoming events snapshot per space, and display cached read-only Calendar events alongside `reminders.md` in `crates/osom/src/today.rs`.

**Acceptance criteria:**
- [ ] Server tests verify read-only Mail/Calendar profiles allow search/read within the agent's bound account and reject send/write calls
- [ ] Upcoming Calendar events cache locally under `~/.config/osom/` and render in `osom` Today even while offline
- [ ] Strict zero cross-space credential isolation verified

**Verification:**
- [ ] Tests pass: `cargo test --manifest-path ~/src/osom/zeroclaw/Cargo.toml -p zeroclaw-tools email google_workspace`
- [ ] Tests pass: `cargo test --locked -p osom-hub -p osom`

**Dependencies:** Task 5b, Task 17 (can run in parallel with Task 18)

**Files likely touched:**
- `~/src/osom/zeroclaw/crates/zeroclaw-gateway/src/api_spaces.rs`
- `crates/osom-hub/src/lib.rs`
- `crates/osom/src/live.rs`
- `crates/osom/src/today.rs`
- `crates/osom/src/view.rs`

**Estimated scope:** Medium (5 files)

---

## Checkpoint 6: After Tasks 18–19 (`connectors-and-import`)
- [ ] All connector, manifest, and Today calendar tests pass
- [ ] Read-only enforcement & idempotent re-import verified
- [ ] Review with human before proceeding to Wave 7

---

## Wave 7: `app-shell`

## Task 20: Space Switcher, persistent `ui-state.json`, live Sync status footer, and Connect/Pair UI

**Description:** Add the sidebar Space Switcher (swapping the full active-space workspace cleanly), persist `last_space` / `last_place` / chat collapsed state in `~/.config/osom/ui-state.json`, render live sync state (`Synced`, `Syncing…`, `Offline · N pending` including both space and chat outboxes, plus manual Refresh `↻`) in the sidebar footer, and render the inline **Connect & Pair** onboarding screen when `~/.config/osom/` is unconfigured or unpaired.

**Acceptance criteria:**
- [ ] Switching spaces swaps sidebar, Today, Inbox, Search, and Chat cleanly with zero cross-space bleed
- [ ] Launching offline restores the last active space immediately from local replicas and shows accurate pending counts
- [ ] Empty `~/.config/osom/` opens the inline Connect & Pair screen and enters Today immediately upon pairing

**Verification:**
- [ ] Tests pass: `cargo test --locked -p osom`
- [ ] Build succeeds: `cargo check --locked -p osom -p margin` (0 warnings)

**Dependencies:** Task 4, Task 10, Task 17, Task 19

**Files likely touched:**
- `crates/osom/src/lib.rs`
- `crates/osom/src/live.rs`
- `crates/osom/src/cli.rs`
- `crates/osom/src/commands.rs`
- `crates/osom/src/live/tests.rs`

**Estimated scope:** Medium (5 files)

---

## Task 21: `⌘O` Quick Switcher, `⌘⇧F` active-space search, `⌘L` Chat toggle, bundle & visual audit

**Description:** Implement `crates/osom/src/search.rs` for `⌘O` Quick Switcher (spaces, topics, docs in active space) and `⌘⇧F` full-text search across the active space's local Markdown files and cached transcripts, wire `⌘L` collapsible Chat pane (auto-collapsing below `960px` width) without colliding with editor shortcuts, and verify the bundled `Osom.app` (`./scripts/bundle-osom.sh`) + rendered PNG screenshots against `docs/design-system.md`.

**Acceptance criteria:**
- [ ] `⌘O`, `⌘⇧F`, `⌘L`, `⌘[`, `⌘]`, and `Escape` work cleanly and never collide with `margin-editor` shortcuts
- [ ] `⌘⇧F` searches only the active space's local replica + cached transcripts (zero cross-space results, works offline)
- [ ] `cargo check --locked --workspace` has 0 warnings, `cargo test --locked --workspace` passes 100%, and `crates/margin` has zero dependencies on `osom*`

**Verification:**
- [ ] Tests pass: `cargo test --locked --workspace`
- [ ] Build succeeds: `cargo check --locked --workspace` (0 warnings) and `./scripts/bundle-osom.sh`
- [ ] Manual check: Inspect rendered PNG screenshots of Today, Topic Overview, Embedded Editor, Chat, and Onboarding

**Dependencies:** Task 20

**Files likely touched:**
- `crates/osom/src/search.rs` (new)
- `crates/osom/src/lib.rs`
- `crates/osom/src/chat.rs`
- `crates/osom/src/live/tests.rs`

**Estimated scope:** Medium (4 files)

---

## Checkpoint 7: V1 Complete
- [ ] All 21 tasks checked off and all 7 module specs' Success Criteria verified
- [ ] `cargo check --locked --workspace` passes with 0 warnings
- [ ] `cargo test --locked --workspace` passes 100%
- [ ] Visual screenshot audit & `Osom.app` bundle verified with human
