# Spec: Agents & Chat (`agents-and-chat`) — V1

> Module id: `agents-and-chat` (see [capability-map.md](capability-map.md)).
> Depends on: `spaces-and-isolation` ([SPEC-spaces-and-isolation.md](SPEC-spaces-and-isolation.md)), `topics-and-wiki` ([SPEC-topics-and-wiki.md](SPEC-topics-and-wiki.md)).
> Status: approved.

## Objective

Provide strictly isolated, per-space **ZeroClaw agents** and a resilient, streaming **in-app chat** experience (space-wide, per-topic, anchored document comments, and automatic conflict rebasing) that works cleanly alongside Telegram/Slack without ever impersonating the user in external channels — and queues user messages offline until the gateway reconnects.

### Why
Every space (`personal`, `family`, `climbing`, …) is a distinct privacy boundary with its own knowledge, procedures, and tone. Agents must act reliably across four distinct roles — conversational assistant, read-only document reviewer (`scribe`), automatic conflict rebaser, and proactive wiki steward — while keeping in-app sessions cleanly separated from Telegram/Slack group chats and never losing a prompt composed while offline.

### User Stories (V1)
1. **Isolated agents per space:** Each space has its own primary agent (e.g., `luc` for `personal`, `family` for `family`) and its own read-only review agent (`scribe-<space>`, optionally using a faster/cheaper model). Neither can read files, `files_dir`, sessions, or memory from any other space.
2. **Space-wide & per-topic in-app chat (separate from Telegram/Slack):** Each space has one persistent space-wide chat session (`osom-<space>`) and each topic has one persistent in-app chat session (`osom-<space>-<topic>`). Because osom cannot impersonate the user in Telegram, Telegram/Slack conversations remain their own sessions (visible read-only under Discussions) while sharing the space's wiki and memory as common ground.
3. **Streaming turns, tool visibility & Stop:** Replies stream token-by-token over `/ws/chat` through one `hub::AgentChat` trait, showing quiet tool-activity badges (`reading car/log.md…`) and supporting immediate cancellation (**Stop** → `POST /api/sessions/{id}/abort`).
4. **Scoped tool approvals:** Safe in-space operations (reading/editing the space's `.md` files, `questions.md`, `reminders.md`, `log.md`, reading `files_dir`, memory recall/store) run automatically. External or destructive actions (sending emails, posting to external channels outside the active reply, destructive shell commands) pause and present an inline **Approve / Deny** prompt.
5. **Offline chat outbox:** When the gateway is unreachable, I can still browse cached chat history and compose messages; unsent messages are saved durably in a local chat outbox, shown as *Queued (offline)* (editable or cancellable before delivery), and flushed automatically in order when the gateway reconnects.
6. **Anchored review comments (`scribe-<space>`):** Commenting on a passage in the Markdown editor opens a per-thread session (`osom-comment-<space>-<thread-id>`) with the space's read-only `scribe` agent, streaming a reply and optional ```` ```margin-edit ```` replacement proposal without ever modifying files directly.
7. **Automatic conflict rebase:** When `spaces-and-isolation` reports an overlapping text conflict (`base`, `head`, `client_commit`), the client runs a silent single-turn rebase via the space's agent to reconcile both edits cleanly (preserving the user's original intent and concurrent changes) and commits the result.
8. **Proactive wiki stewardship:** Every space agent follows the operational doctrine in `docs/wiki-maintenance.md` (Questions Bank context triggers, brief/doc upkeep, procedure refinement, factual contradiction detection, and reminder hygiene).

## Tech Stack

- **Client core (`crates/margin-core`, `crates/osom-hub`):** Rust (edition 2024), `margin_core::agent::{AgentChat, Ask, ChatEvent, Canned}`, `margin_core::comments::request`, `tungstenite` 0.29 (`native-tls`) for `/ws/chat`, `ureq` 3.5 for session REST & abort, `serde`/`serde_json`, `libc` (`flock`) for durable outbox files.
- **Client runtime & UI (`crates/margin-runtime/src/worker.rs`, `crates/margin-editor/src/comments/mod.rs`, `crates/osom/src/chat.rs`):** Background threads pushing `ChatEvent`s over `futures_channel::mpsc` into Dioxus `Signal`s; rendered with `margin_ui::{Message, Composer, Banner, Chip}`.
- **Server (`~/src/osom/zeroclaw`):** ZeroClaw gateway `/ws/chat`, `/api/sessions`, `/api/sessions/{id}/messages`, `/api/sessions/{id}/abort`, per-agent `[agents.<id>]` config and `[risk_profiles.readonly]`.

## Commands

```sh
# Type-check both binaries (0 warnings required)
cargo check --locked -p osom -p margin

# Run chat, outbox, and comment protocol unit tests
cargo test --locked -p osom-hub -p margin-core -p osom

# Run one live comment-agent round trip against a paired space (manual/tailnet)
cargo run --locked -p osom -- comment-test <page> "<quote>" "<request>"

# Run full workspace test suite
cargo test --locked --workspace
```

## Project Structure

```
spec/SPEC-agents-and-chat.md              → This specification
docs/wiki-maintenance.md                  → Operational playbook & agent doctrine (maintenance, reviews, contradictions, proactivity, reminders)
docs/comments-backend.md                  → Anchored comments wire & scribe contract reference
crates/margin-core/src/agent.rs           → AgentChat trait, Ask, ChatEvent, Canned test double
crates/margin-core/src/comments/request.rs → Comment turn builder, visible()/edit() streaming parsers, Proposal::fits
crates/osom-hub/src/lib.rs                → ZeroClaw WS/REST client, session abort
crates/osom-hub/src/chat_outbox.rs        → Pure durable offline message queue & cached transcript store under ~/.config/osom/chat/
crates/margin-runtime/src/worker.rs       → Background turn runner and batched event stream
crates/margin-editor/src/comments/mod.rs  → Editor comment pane wired to space-scoped scribe agent
crates/osom/src/chat.rs                   → Osom right-hand space/topic Chat pane
```

## Interface & Protocol Contract

### 1. Per-Space Agent Topology (Server Config)
Each space `<s_id>` pairs a primary agent and a read-only review agent:
```toml
[spaces.personal]
title = "Personal"
agent = "luc"
review_agent = "scribe-personal"
repo = "spaces/personal"
files_dir = "files/personal"

[agents.scribe-personal]
model_provider = "anthropic.fast"        # configurable faster/cheaper model
risk_profile = "readonly"                # file_read, file_list, memory_search, web_search, time only
channels = []                            # gateway only
[agents.scribe-personal.workspace]
access = { luc = "read" }                # read-only on personal space ONLY
read_memory_from = ["luc"]
```
- **Zero cross-space leakage:** `scribe-<space>` reads only its own space's owning agent workspace and memory. `GET /api/spaces` includes `review_agent` alongside `agent` so the client binds both automatically per space without manual configuration.

### 2. Session Naming & Separation
| Purpose | Session ID | Agent |
|---|---|---|
| Space-wide in-app chat | `osom-space-<space>` | `space.agent` |
| Topic in-app chat | `osom-topic-<space>-<topic-id>` | `space.agent` |
| Anchored comment thread | `osom-comment-<space>-<thread-id>` | `space.review_agent` |
| Automatic conflict rebase | `osom-rebase-<space>-<change-id>` (ephemeral, deleted after commit) | `space.agent` |
| External channels (Telegram / Slack) | Native ZeroClaw session keys (`telegram_…`, `slack_…`) | `space.agent` |

- **No Telegram impersonation:** In-app sessions never inject messages into Telegram/Slack groups or write into channel session keys.
- **First-turn preamble:** A fresh topic session (`message_count == 0`) sends a collapsed context preamble containing the space title, topic id, folder path, overview summary, and open questions count.

### 3. Unified Streaming Turn Contract (`hub::AgentChat`)
Every agent interaction (live ZeroClaw, `Canned` demo/test double, comment turns, conflict rebase) goes through one trait:
```rust
pub trait AgentChat: Send + Sync {
    fn chat(
        &self,
        ask: &Ask,
        on: &mut dyn FnMut(ChatEvent) -> bool,
        decide: &mut dyn FnMut() -> bool,
    );
    fn abort(&self, session: &str) -> Result<()>;
    fn inline(&self) -> bool { false }
}
```
- `chat()` emits `Chunk(String)`, `Tool(String)`, `Approval { tool, summary }`, `Tick`, and terminates with **exactly one** `Done(full)` or `Error(reason)` unless `on` returns `false` (cancelled/dropped).
- Pressing **Stop** drops the UI receiver and calls `abort(session)` (`POST /api/sessions/{id}/abort`), preserving any partial text clearly marked as stopped.

### 4. Tool Approval Boundary
- **Auto-allowed (no prompt):** Reading/writing/creating Markdown & config files inside the agent's own space repo, reading files in its own space `files_dir`, memory recall/store, reading open questions/reminders, web search (read-only).
- **Requires explicit Approve/Deny:** Sending emails, posting unsolicited messages to external channels outside the active reply, running shell commands with side effects, deleting files outside normal cleanup, or any action flagged by ZeroClaw's risk profile.
- **Read-only (`scribe`):** Enforced server-side by `risk_profile = "readonly"` — can never mutate files even if prompted to.

### 5. Offline Chat Outbox & Cached History (`crates/osom-hub/src/chat_outbox.rs`)
Stored under `~/.config/osom/chat/<encoded-gateway>/<encoded-space>/`:
- `cache/<encoded-session>.json` — Last fetched transcript for offline reading (updated atomically after each load or completed turn).
- `outbox/<encoded-session>.json` — Ordered queue of unsent `{ id, created_at, content, preamble?, state: Queued | Sending | Failed(reason) }`.
- **Rules:**
  - Sending while offline writes durably (`fsync` + atomic rename) to the outbox and displays the message immediately in the chat timeline with a *Queued (offline)* badge plus **Edit** / **Cancel** actions.
  - When connectivity returns (detected via SSE reconnect or explicit retry), queued entries drain strictly FIFO per session.
  - A message leaves the outbox **only** on `ChatEvent::Done`; on transient network failure it returns to `Queued` (with backoff); on permanent rejection it marks `Failed(reason)` with **Retry** / **Discard** so it never silently vanishes or blocks other sessions.

### 6. Automatic Conflict Rebase Turn
When `spaces-and-isolation` reports an overlapping text conflict `{ path, base, head, client_commit }`:
1. Client fetches the three versions (`base`, `head`, `client_commit`) via `GET /api/spaces/{s}/file?path=...&at=...`.
2. Client runs a non-interactive turn on ephemeral session `osom-rebase-<space>-<change-id>` instructing the space's agent to produce one clean reconciled Markdown document preserving the user's original edit alongside the concurrent changes (returning the full file in a fenced `osom-rebase` block).
3. Client validates non-empty output, commits it to the space replica, deletes the ephemeral session, and posts a quiet non-blocking banner ("Merged concurrent edit in `<path>` — full history kept in git").
4. If offline or the rebase turn fails, the server's union-merged version stays safely in the replica and osom displays a subtle banner on that page with a **"Merge with agent"** / **"Retry merge"** button so the space's agent reconciles it as soon as connectivity returns — zero data loss in all cases.

### 7. Agent Playbook & Proactivity Contract (`docs/wiki-maintenance.md`)
Every space's primary agent prompt (`AGENTS.md`) incorporates `docs/wiki-maintenance.md`:
- **Questions Bank:** Log new gaps to `<topic>/questions.md`; ask at most 1–2 relevant open questions naturally at the end of a reply when triggered (never repeat the same question the same day); move answered questions to *Answered* with date and source.
- **Contradiction detection:** When new information contradicts an existing doc or `reminders.md` (e.g., conflicting dates, outdated procedure steps, changed contacts), surface the contradiction explicitly and update or ask before overwriting.
- **Procedure evolution:** After a task or checklist is executed, suggest updating the procedure if steps changed and append a dated row to `log.md`.
- **Change visibility:** Every wiki edit produces a concise git commit message (`<topic>: <what changed>`) so the user can review changes cleanly in history.

## Code Style

Follow the synchronous `AgentChat` trait + `worker::turn` background bridge pattern in `crates/osom-hub/src/lib.rs` and `crates/margin-runtime/src/worker.rs`:
- Keep outbox state transitions, rebase prompt/response parsing, and comment `visible()`/`edit()` parsing 100% pure and unit-tested without sockets or Dioxus.
- Batch streaming chunks in `worker::batch` so fast token streams cause at most one signal update per frame.

```rust
/// Start one agent turn; dropping the receiver stops the turn.
pub(super) fn turn(
    agent: Arc<dyn AgentChat>,
    ask: Ask,
    mut decide: impl FnMut() -> bool + Send + 'static,
) -> UnboundedReceiver<ChatEvent> {
    let (tx, rx) = unbounded();
    let inline = agent.inline();
    let mut run = move || agent.chat(&ask, &mut |e| tx.unbounded_send(e).is_ok(), &mut decide);
    if inline { run(); } else { std::thread::spawn(run); }
    rx
}
```

## Testing Strategy

| Layer | Location | What it verifies |
|---|---|---|
| Outbox & cache unit tests | `crates/osom-hub/src/chat_outbox.rs` (`#[cfg(test)]`) | Durable enqueue across restart, FIFO drain per session, edit/cancel before send, transient failure stays queued, permanent failure → `Failed` without blocking other sessions, atomic cache read/write |
| Streaming & protocol unit tests | `crates/osom-hub/src/lib.rs`, `crates/margin-core/src/comments/request.rs` | `Canned` streaming chunks, tool badges, approval callback allow/deny, `Stop`/drop cancellation, `visible()` partial-fence holdback, `margin-edit` & `osom-rebase` block parsing |
| Conflict rebase unit tests | `crates/osom-hub/src/spaces.rs` | Simulated 3-version overlap triggers rebase turn, commits clean reconciled doc, falls back safely to union merge if offline or malformed output |
| Live opt-in smoke tests | `crates/osom/src/live/tests.rs` (`#[ignore]`), `osom comment-test` | Real `/ws/chat` streaming, tool call display, session abort, and read-only `scribe` isolation |

## Boundaries

- **Always:**
  - Route every agent call through `hub::AgentChat` so mock/demo and live ZeroClaw exercise the same code path.
  - Bind every session, comment agent, and conflict-rebase call strictly to the active space's `agent` / `review_agent`.
  - Keep `chat_outbox` and prompt/response parsers free of Dioxus/UI dependencies.
  - Persist cached transcripts and outbox entries atomically (`fsync` + rename).
- **Ask first:**
  - Changing the `/ws/chat` WebSocket framing or `margin-edit` / `osom-rebase` fenced block format.
  - Allowing any tool category to bypass Approve/Deny when it performs external side effects.
- **Never:**
  - Impersonate the user in Telegram or Slack or write into external channel session keys.
  - Allow `scribe-<space>` to write files or access another space's workspace/memory.
  - Drop a queued offline chat message without explicit user cancellation or confirmed `Done`.
  - Parse streaming chunks as Markdown while `pending` (render plain text while streaming, Markdown only on `Done`).

## Success Criteria

1. **Strict agent isolation:** Each space binds its own `agent` and read-only `review_agent` from `GET /api/spaces`; attempting to run a review comment in `family` uses `scribe-family` and cannot read `personal` files or memory.
2. **Clean session separation:** Sending messages in a topic's in-app chat writes to `osom-topic-<space>-<topic>` and never modifies Telegram/Slack sessions, while Telegram/Slack transcripts remain visible under Discussions.
3. **Offline chat outbox works end-to-end:** Composing messages while offline persists them across app restart, supports editing/cancelling before reconnect, and drains them in order upon reconnection with zero lost or duplicated prompts.
4. **Streaming, approvals & Stop:** Tokens stream smoothly via `worker::batch`, tool approvals block until Approve/Deny is clicked, and clicking Stop aborts the server turn cleanly.
5. **Automatic conflict rebase:** Given a simulated overlapping text conflict, the client invokes the space agent with `(base, head, client_commit)`, commits the reconciled Markdown, and falls back safely without data loss if the agent is offline or returns malformed output.

## Out of Scope (Deferred to V2)

- **Multiple named chat threads per topic** — V1 uses one persistent in-app chat per topic + one space-wide chat; branching/multiple threads per topic is deferred to V2.
- **Voice input / audio messages in desktop chat** — Deferred to V2.
- **Agent-initiated proactive comment threads** — Agents authoring `_osom/comments/<page-id>.md` review threads asynchronously (e.g., from Telegram review requests) is deferred to V2.
- **Per-message model/temperature picker in UI** — V1 uses each agent's configured model (`agent` vs faster `review_agent`); ad-hoc per-turn model switching in the UI is deferred to V2.

## Open Questions

None — all V1 decisions are resolved (failed automatic rebases surface a subtle banner to trigger an agent merge; topic chat composer stays minimal in V1).
