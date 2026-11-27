# Spec: Channels & Routing (`channels-and-routing`) — V1

> Module id: `channels-and-routing` (see [capability-map.md](capability-map.md)).
> Depends on: `spaces-and-isolation` ([SPEC-spaces-and-isolation.md](SPEC-spaces-and-isolation.md)), `topics-and-wiki` ([SPEC-topics-and-wiki.md](SPEC-topics-and-wiki.md)), `agents-and-chat` ([SPEC-agents-and-chat.md](SPEC-agents-and-chat.md)).
> Status: approved.

## Objective

Connect **Telegram** (groups, supergroup Forum Topics, and threaded bot DM topics) and **Slack** channels to isolated **osom** Spaces and Topics — supporting **bot-created Telegram topics**, **forgiving multi-topic routing** for real-world messy human conversations, **discussion-driven Inbox discovery**, and reliable outbound delivery targets for reminders.

### Why
Users live in chat apps on their phone all day. They shouldn't have to remember strict folder rules or switch apps just to capture a thought, start a new topic, or follow up on a task. Channels must feel effortless and forgiving inside a space — letting the bot spin up Telegram topics automatically and file off-topic remarks into the right topic — while still enforcing the hard privacy wall between isolated Spaces.

### User Stories (V1)
1. **Space-bound channels (zero cross-space leakage):** Every Telegram group, bot DM, or Slack channel connects to **one** Space's agent. Private `personal` data can never leak into a `family` or `project` channel.
2. **Telegram Forum Topics & threaded DM topics (bot-created):** Each Space has a Telegram supergroup or bot DM with Telegram Topics enabled (`chat_id` + `message_thread_id`). Creating a topic (from Telegram or from the desktop app) can have the bot create the corresponding Telegram topic thread automatically and record its `(chat_id, thread_id)` binding in the topic's `README.md` frontmatter. Standalone Telegram groups (`chat_id` only) and Slack channels (`channel_id`) remain fully supported.
3. **Flexible, forgiving routing within a space:** A channel maps to a default topic (or multiple topics, like `people/nikki` + `wedding`), **not** a rigid fence. If the user asks about `pregnancy` inside the `car` group or in DM, the agent answers helpfully right there, files any facts/questions/reminders into the right topic(s) within the same space, and notes briefly where it was filed.
4. **Discussion-driven Inbox discovery:** Unrouted sessions (e.g., general DM, ad-hoc group threads, or newly arrived conversations whose topic isn't clear yet) surface in the space's **Inbox**. Topics are discovered through conversation with the agent — as you discuss, the agent extracts facts into existing topics or proposes creating a new topic (and its Telegram thread), clearing the Inbox item once triaged.
5. **Read-only Discussions facet in osom:** Every topic's **Discussions** tab shows all routed Telegram, Slack, and in-app chat sessions ordered by recent activity, filtered cleanly (no raw tool-call JSON), with source glyphs (`Telegram` / `Slack` / `Chat`) and source filter chips.
6. **Deterministic outbound delivery target:** Each topic exposes a resolved delivery target (`channel` + `chat_id` + optional `thread_id`) consumed by `agenda-and-reminders` so scheduled reminders land in the exact right Telegram topic or Slack channel.

## Tech Stack

- **Client pure core (`crates/osom-topics/src/{routes,chats}.rs`):** Rust (edition 2024), pure route parsing (frontmatter + root `README.md` fallback), session-key `Origin` parsing (including Telegram `message_thread_id`), thread-to-topic matching, and Inbox classification.
- **Client adapter (`crates/osom-hub/src/lib.rs`, `crates/osom/src/live.rs`):** Reads `/api/sessions` and `/api/sessions/{key}/messages` per space agent, caches transcripts locally for offline browsing, and exposes delivery target resolution.
- **Server (`~/src/osom/zeroclaw`):** Existing Telegram and Slack channel adapters, extended to preserve `message_thread_id` in session keys and outbound delivery, plus a bot tool to create Telegram Forum/DM topics (`createForumTopic`).

## Commands

```sh
# Type-check both binaries (0 warnings required)
cargo check --locked -p osom -p margin

# Run pure routing, origin parsing, and inbox tests
cargo test --locked -p osom-topics

# Run full workspace test suite
cargo test --locked --workspace

# Server: run Telegram/Slack channel & routing tests
cargo test --manifest-path ~/src/osom/zeroclaw/Cargo.toml telegram
cargo test --manifest-path ~/src/osom/zeroclaw/Cargo.toml slack
```

## Project Structure

```
spec/SPEC-channels-and-routing.md   → This specification
crates/osom-topics/src/routes.rs    → Route, ChannelRef (Telegram chat+thread, Slack channel), DeliveryTarget resolver
crates/osom-topics/src/chats.rs     → Origin parsing, session filing, transcript filtering, Inbox builder
crates/osom-hub/src/lib.rs          → Per-agent session listing & transcript fetching
crates/osom/src/live.rs             → Merges space routes + agent sessions into TopicView Discussions & Inbox
crates/osom/src/view.rs             → Discussions & Inbox view models
```

## Interface & Routing Contract

### 1. Channel Identifiers & Session Keys
Every external conversation carries a structured `ChannelRef`:
- **Telegram group/DM:** `{ kind: "telegram", chat_id: "-5318909829", thread_id: None }`
- **Telegram Forum / DM Topic:** `{ kind: "telegram", chat_id: "-5318909829", thread_id: Some("42") }`
- **Slack channel / thread:** `{ kind: "slack", channel_id: "C0CLHUN1UQ1", thread_ts: Option<String> }`

`osom_topics::chats::origin(key)` parses ZeroClaw session keys deterministically:
- `telegram_<alias>_<chat_id>_t<thread_id>_<user>` (or `telegram_<alias>_<chat_id>_<user>` without topic) → `Origin::Telegram { chat, thread }`
- `slack_<alias>_<channel_id>_<thread_ts>_<user>` → `Origin::Slack { channel, thread }`
- `gw_osom-space-<space>` / `gw_osom-topic-<space>-<topic>` → `Origin::Space` / `Origin::Topic`
- `gw_osom-comment-<space>-<thread>` / `gw_margin-comment-<thread>` → `Origin::Comment` (hidden from Discussions & Inbox)

### 2. Route Storage & Precedence
Routes belong inside each space's git repo (Markdown + frontmatter only):
1. **Primary — Topic `README.md` YAML frontmatter:**
   ```yaml
   ---
   id: 01jdq8z6v9k3
   title: Car
   channels:
     - kind: telegram
       chat_id: "-5318909829"
       thread_id: "14"         # optional: omit for whole group
       label: "Telegram · Car"
     - kind: slack
       channel_id: "C0CLHUN1UQ1"
       label: "#car"
   ---
   ```
2. **Fallback — Space root `README.md` "Channel → folder" table:** Continues working seamlessly with existing `z-osom-tka` tables; frontmatter bindings take precedence when both exist.
3. **One channel → many topics allowed:** Multiple topics may reference the same `(chat_id, thread_id)` or `channel_id` (e.g., `people/nikki` and `wedding`), with optional `primary: true` marking the default delivery target.

### 3. Bot-Created Telegram Topics & Slack Channels/Threads
- **Telegram — Forum/DM Topics (not standalone groups):** Because Telegram bots cannot create standalone groups, the primary Telegram model is **one supergroup (with Forum Topics enabled) or threaded bot DM per Space** (`telegram_chat_id`, created once by the user). When a topic is created (in Telegram or from the desktop app while online), the space agent calls `telegram_create_topic(chat_id, name, icon_color?)` (`createForumTopic`), gets `message_thread_id`, and writes the `channels:` entry into `<topic>/README.md` frontmatter in the same commit. Existing standalone Telegram groups (e.g., legacy `z-osom-tka` groups) remain supported via route binding.
- **Slack — Bot-created channels / threads (when permitted):** When a space is connected to Slack and the bot has channel management scopes (`channels:manage` / `groups:write`), creating a topic can likewise create a matching Slack channel (or thread under a space channel) and record its `channel_id` in `<topic>/README.md` frontmatter. Existing Slack channels remain bindable manually or via chat.
- **Offline resilience:** If offline or no channel container is configured on the space, the topic is still created locally without blocking; its Telegram topic / Slack channel can be provisioned or linked later.

### 4. Forgiving In-Space Routing Rules (Agent Behavior)
When a user sends a message in any channel bound to Space `S`:
1. **Default context:** The agent loads the topic(s) mapped to that `(chat_id, thread_id)` or `channel_id` first.
2. **Messy/off-topic tolerance (same space only):** If the user's request clearly belongs to a different topic inside Space `S` (or spans two topics, e.g., mentioning a car WOF date inside the general DM or Nikki chat), the agent:
   - Answers directly in the channel where the user spoke (never refusing with "wrong channel"),
   - Writes extracted facts, questions, or reminders into the appropriate topic's files inside Space `S`,
   - Adds a brief 1-line footnote noting where it filed the update (e.g. *Filed to `car/maintenance.md`*),
   - Records a cross-topic filing hint in `_osom/routing.md` (optional) so Discussions can surface relevant context without duplicating transcripts.
3. **Hard space wall:** If the user asks about data belonging to another Space `S2`, the agent in Space `S` has zero access to `S2` and simply notes that it belongs to the other space.

### 5. Discussion-Driven Inbox
- Any Telegram or Slack session belonging to the space's agent that does **not** match a topic route (and is not a comment or ephemeral rebase session) appears in the space's **Inbox** ordered by `last_activity`.
- **Workflow:** The user discusses the unrouted thread with the agent (in Telegram/Slack or in the desktop Inbox view) to figure out what topic(s) it belongs to — or whether it warrants creating a new topic.
- Once the agent files the extracted content into a topic (or creates a new topic + binds the channel) or the user marks the thread **Dismissed / Handled** (stored in `_osom/inbox.md`), it leaves the active Inbox list.

### 6. Outbound Delivery Target Contract (for `agenda-and-reminders`)
```rust
pub struct DeliveryTarget {
    pub channel: String,           // e.g. "telegram.personal" or "slack.main"
    pub chat_id: String,           // Telegram chat_id or Slack channel_id
    pub thread_id: Option<String>, // Telegram message_thread_id or Slack thread_ts
}
```
Resolution order for a topic `T` in Space `S`:
1. Explicit `deliver_to` override on a specific reminder row (if present),
2. Topic `T`'s primary Telegram channel binding (`chat_id` + `thread_id`), else Slack binding,
3. Space `S`'s default Telegram container / DM fallback.

## Code Style

Keep route parsing, session key classification, Inbox filtering, and delivery target resolution pure in `crates/osom-topics` with zero network I/O:

```rust
/// Where a session's messages come from, read from its ZeroClaw key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    Space(String),
    Topic { space: String, topic: String },
    Comment(String),
    Telegram { chat: String, thread: Option<String> },
    Slack { channel: String, thread: String },
    Gateway,
    Other,
}
```

## Testing Strategy

| Layer | Location | What it verifies |
|---|---|---|
| Pure routing & origin unit tests | `crates/osom-topics/src/tests.rs` | Frontmatter `channels:` parsing, root `README.md` fallback, precedence rules, Telegram `chat_id` + `thread_id` key parsing, Slack thread key parsing, multi-topic channel routing, Inbox unrouted filtering + handled state, `DeliveryTarget` resolution order |
| Transcript filtering unit tests | `crates/osom-topics/src/tests.rs` | Tool-call / tool-result messages stripped cleanly from Discussions view; human + assistant messages preserved |
| Server Telegram topic tests | `zeroclaw` Telegram adapter tests | `message_thread_id` round-trip in session keys, outbound `sendMessage` with `message_thread_id`, `createForumTopic` tool payload |

## Boundaries

- **Always:**
  - Keep session-key parsing, route matching, and `DeliveryTarget` resolution 100% pure and deterministic in `crates/osom-topics`.
  - Preserve backwards compatibility with existing `README.md` "Channel → folder" tables (such as `z-osom-tka`).
  - Filter raw tool-call/result turns out of the Discussions UI so transcripts stay readable.
- **Ask first:**
  - Changing ZeroClaw's Telegram or Slack session-key naming format (ensure backwards-compatible parsing of existing session keys).
  - Adding new external messaging channel adapters beyond Telegram and Slack.
- **Never:**
  - Route or read a session across Space boundaries in V1.
  - Refuse a user's request just because they posted it in the wrong channel inside the same space.
  - Impersonate the user by posting user-authored text into Telegram or Slack from the desktop app.

## Success Criteria

1. **Telegram topics (supergroup + DM) end-to-end:** ZeroClaw routes incoming Telegram messages with `message_thread_id` to their exact topic session, and `telegram_create_topic` creates a new Telegram topic and saves its `(chat_id, thread_id)` in `<topic>/README.md` frontmatter.
2. **Backwards-compatible routing:** Both frontmatter `channels:` entries and existing `z-osom-tka` `README.md` route tables resolve deterministically, including one channel mapped to multiple topics.
3. **Forgiving in-space routing:** Asking about Topic B inside Topic A's channel (same space) answers in place, updates Topic B's files, and notes where it was filed — without ever leaking across spaces.
4. **Discussion-driven Inbox:** Unrouted Telegram/Slack sessions appear in Inbox, allow conversational triage with the agent to file or create topics, and clear cleanly once handled.
5. **Delivery target resolution:** Given any topic (with a Telegram topic, standalone Telegram group, Slack channel, or no binding), `DeliveryTarget` resolves deterministically using the 3-step precedence order.

## Out of Scope (Deferred to V2)

- **Instagram channel adapter** — Deferred to V2 (V1 covers Telegram + Slack).
- **Automated LLM confidence-scored background classifier** — V1 uses explicit routes + forgiving agent filing + discussion-driven Inbox; background auto-filing classifiers with few-shot training are deferred to V2.
- **Replying to Telegram/Slack directly from osom as the user** — Impossible without user-account impersonation; out of scope.
- **Multi-user per-channel permissions inside a shared group** — Deferred to V2 alongside multi-user Spaces.

## Open Questions

None — all V1 decisions are resolved (Telegram uses bot-created Forum/DM topics inside each space's existing container rather than separate groups per topic; Slack supports bot-created channels/threads when permitted).
