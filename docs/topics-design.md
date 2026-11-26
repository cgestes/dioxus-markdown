# Topics — browsing a ZeroClaw agent from Margin over Tailscale

Status: implemented as `osom topics` (2026-11-24). Mockup: [`mockup/topics.html`](../mockup/topics.html) · native: [`mockup/topics-native.png`](../mockup/topics-native.png).

## 0. What shipped, and how it differs from the plan below

The real setup already had most of the plan's pieces, so the implementation is smaller than §3–§7 describe:

- **No margin-hub.** The vault is the agent's own workspace (`~/.zeroclaw/agents/luc/workspace/procedures`), already served by SilverBullet on `:8443`. ZeroClaw stores every Telegram and Slack conversation with its chat id in the session key (`telegram_telegram_<chat>_<user>`, `slack_main_<channel>_<thread ts>_<user>`), and a paired device can list them (`GET /api/sessions`) and read them (`GET /api/sessions/{key}/messages`). Margin files them by matching chat ids against the space README's "Channel → folder" table, which the agent already maintains.
- **Reminders** come from `reminders.md` (the agent's single deadline table, pinged by its sweeper crons), not from per-reminder cron jobs.
- **Tailscale:** nothing to set up. ZeroClaw's `[tunnel] tunnel_provider = "tailscale"` (`funnel = false`) runs `tailscale serve --https=443 42617` itself; SilverBullet is a persistent `serve` on `:8443`.
- **Questions** (`questions.md`) became their own facet; **Known facts** (memory) and **Code** are rendered when present but not yet read from the live space.
- **Code:** `crates/osom-topics` (parsing, no I/O), `crates/margin-hub/src/lib.rs` (SilverBullet, ZeroClaw REST + `/ws/chat`, Keychain), `crates/osom/src/topics/` (window, CLI, live adapter, `view.rs` the shared view model, `demo.rs` the mockup's sample data).

**Later: anchored comments on the real backend.** Main has anchored AI comment threads (`crates/margin-editor/src/comments/mod.rs`) answered by `mock_llm_reply`, which returns a reply and an optional before/after edit proposal. Once vault docs open in the editor here, comments can go to the agent instead. To decide then:

- **Session per thread:** for example `margin-comment-<page>-<thread>`, so a follow-up keeps its context.
- **Prompt:** the page path, the quoted passage and the user's request.
- **Edit proposals:** a structured reply format the agent follows, such as one fenced `replace` block, parsed into an `EditProposal`.
- **Streaming:** the reply streams into the AI message using the same `hub::ZeroClaw::chat` path as the topic chat.
- **Persistence:** threads are in memory today. Options are a sidecar file in the space (not dot-prefixed, since SilverBullet refuses those) or Loro alongside the note.
- **Who applies the edit:** Margin, with undo as today, or the agent editing and committing the file itself.

The rest of this document is the original design, kept for the reasoning.

## 1. Goal

One place to see everything about a part of life (a **topic**): Mazda, Wedding N&C, the house
(Noisy Marae: hot-water cylinder, gutters…), Modular (the collection, the next module), People,
Nikki. For each topic: the documents, the upcoming reminders, every discussion about it — from
Telegram, from Slack, from the agent chat — the procedures, sometimes code, and lists such as
groceries ideas. And an agent chat scoped to that topic.

The agent is a self-hosted [ZeroClaw](https://github.com/zeroclaw-labs/zeroclaw) on a home
machine. Margin connects to it over the tailnet only.

## 2. What ZeroClaw gives us, and what it doesn't

Read from `crates/zeroclaw-gateway/src/lib.rs` (master, Nov 2026). Gateway default:
`127.0.0.1:42617`, `require_pairing = true`, `allow_public_bind = false`.

| Need | ZeroClaw route | Notes |
| --- | --- | --- |
| Pair a device | `POST /pair` with header `X-Pairing-Code` | Returns a bearer token (`zc_…`). Code from `zeroclaw gateway get-paircode` on the host. |
| Agent chat, streamed | `GET /ws/chat?agent=<alias>&session_id=<id>` | Frames: `session_start{resumed,message_count}` → send `{type:"connect"}` → `connected`; send `{type:"message",content}` → `chunk`* → `done{full_response,tokens…}`. Same `session_id` resumes. |
| Chat history | `GET /api/sessions/{id}/messages`, `POST /api/sessions/{id}/abort` | `GET /api/sessions` only lists *open dashboard* chats (`session_id, agent_alias, created_at`). |
| Live activity | `GET /api/events` (SSE), `GET /api/events/history` | Broadcast bus the dashboard uses; a golden test shows webhook turns on it. |
| Memory | `GET /api/memory?query=&category=&since=&until=&agent=`, `POST /api/memory {key,content,category}` | Categories: `core`, `daily`, `conversation`, or any custom string. |
| Reminders | `GET/POST /api/cron`, `PATCH/DELETE /api/cron/{id}`, `GET /api/cron/{id}/runs` | Job: `agent, name, schedule, tz, job_type (shell/agent), prompt, delivery{mode:"announce", channel:"telegram.<alias>", to, thread_id}`. |
| Files | `GET /api/browse?path=` | Directory listing under `<install>/shared/` only — no file contents. |

Auth: REST uses `Authorization: Bearer <token>`. WebSocket accepts the header,
`Sec-WebSocket-Protocol: zeroclaw.v1, bearer.<token>`, or `?token=` — we use the sub-protocol.

**Missing pieces:**

1. ZeroClaw has no notion of a topic.
2. Telegram/Slack conversation history can't be queried through the gateway.
3. The gateway can't serve file contents (docs, procedures).

So we add two things we own: a **topic vault** (folders of Markdown) and a small sidecar,
**margin-hub**, next to ZeroClaw.

## 3. Architecture

```
 Mac: Margin (Dioxus Native)          Phone: Margin web (later)
          │                                   │
          └──────── tailnet (WireGuard) ──────┘          Telegram · Slack (cloud)
                          │                                        │
   home server ───────────┼────────────────────────────────────────┼──────────
          tailscale serve  https://home.<tailnet>.ts.net           │
            :443  ──► 127.0.0.1:42617  ZeroClaw gateway ◄──► ZeroClaw daemon
            :8443 ──► 127.0.0.1:42620  margin-hub                 (agent, memory,
                          │   ▲ SSE /api/events                    cron, channels)
                          │   └──────────── gateway                     │
                          ▼                                             │
                    topic vault (~/topics, git) ◄─── agent reads/writes ┘
                    timeline.sqlite
```

| Component | Owns | Does not own |
| --- | --- | --- |
| **ZeroClaw** (unchanged, no fork) | Agent, Telegram/Slack channels, scheduler, memory, per-device tokens | Topics, file serving |
| **margin-hub** (new, ~1–2k lines of Rust, axum) | Capturing every turn into `timeline.sqlite`, filing turns into topics, serving the vault and timeline, Loro sync of vault docs (later) | Talking to LLMs directly — it asks ZeroClaw |
| **Topic vault** (git repo of Markdown) | Topic identity, routing rules, docs, procedures, lists, code | Messages (those live in the timeline) |
| **Margin** (this repo) | The UI: sidebar, topic pages, chat panel, editing vault notes with the existing Live editor | Anything server-side |

## 4. Connecting over Tailscale

ZeroClaw stays on loopback. `tailscale serve` terminates TLS with the MagicDNS certificate and
forwards tailnet traffic. Never use Funnel.

```toml
# ~/.zeroclaw/config.toml on the home server
[gateway]
host = "127.0.0.1"
port = 42617
require_pairing = true
trust_forwarded_headers = true   # rate limits key on the device's tailnet IP, not 127.0.0.1

[tunnel]
tunnel_provider = "none"         # we run `tailscale serve` ourselves (two ports)
```

```sh
tailscale serve --bg --https=443  http://127.0.0.1:42617   # ZeroClaw
tailscale serve --bg --https=8443 http://127.0.0.1:42620   # margin-hub
```

ZeroClaw's built-in `[tunnel] tunnel_provider = "tailscale"` with `[tunnel.tailscale] funnel = false`
is fine if the hub isn't deployed yet; switch to manual `serve` when it is.

A personal tailnet already limits access to your own devices. If the node is ever shared, add a
grant that only lets members reach `tag:home` on `tcp:443` and `tcp:8443`.

**Pairing from Margin:** *Settings → Agent → Connect*. Enter `https://home.<tailnet>.ts.net` and
the code printed by `zeroclaw gateway get-paircode`. Margin calls `POST /pair` and stores the
token in the macOS Keychain. Each device pairs separately, so each one can be revoked
(`DELETE /api/devices/{id}`) or rotated (`POST /api/devices/{id}/token/rotate`) on its own.

**Security notes:**

- **ZeroClaw can't tell devices apart by address.** Behind `serve`, every tailnet request reaches it
  from `127.0.0.1`, so localhost checks on the raw peer address (`require_localhost`) pass for any
  tailnet device. The pairing-code admin route also needs the gateway admin token, so it stays
  closed. Verify that `/admin/reload` and `/admin/shutdown` reject a phone on the tailnet
  (see §10).
- **Hub auth:** margin-hub listens on loopback only and accepts a request only when its
  `Tailscale-User-Login` header, set by `serve`, matches your login.
- **LLM exposure:** People and Nikki are sensitive. The vault and timeline never leave the
  server, but the LLM provider ZeroClaw uses sees whatever the agent reads. Choose the provider
  or a local model with that in mind.

## 5. Topic model — the vault

One folder per topic. `topic.md` holds the identity and routing rules in front matter. Its body
is a human-readable page with an agent-maintained brief.

```
~/topics/
  mazda/              topic.md  docs/  procedures/  lists/
  wedding-nc/
  house/              topic.md  hot-water/  gutters/  code/ …   (children are sub-topics)
  modular/
  people/             alex.md  mum.md …                          (kind: people)
  nikki/              topic.md  lists/groceries.md  lists/gifts.md (kind: person)
  .margin/filing.jsonl                                           (manual filing decisions)
```

```markdown
---
title: Noisy Marae
icon: home
hue: clay
aliases: [house, maison, noisy marae]
keywords: [chauffe-eau, hot water, cylinder, gouttière, gutter, plumber, roof]
route:
  telegram: { chat: "Home", thread: 12 }   # a forum topic in a Telegram supergroup
  slack: ["#house"]
agent: home
children: [hot-water, gutters]
---
# Noisy Marae
<!-- brief:start -->
Hot water trips about weekly — plumber booked Thu 10:00. South gutters overflow in heavy rain.
<!-- brief:end -->
```

The **facets** of a topic page come from the folder layout plus ZeroClaw. A facet with no
content is hidden.

| Facet | Source | Written by |
| --- | --- | --- |
| Overview | brief block in `topic.md` + memory `category=topic:<slug>` + top items of every facet | agent (brief, facts), you |
| Discussions | hub timeline, filed to this topic — Telegram, Slack, Margin chat | hub |
| Calendar | cron jobs named `[<slug>] …` + `dates:` in front matter | you (from Margin), agent |
| Docs | `docs/**` files and loose `*.md` notes | you, agent |
| Procedures | `procedures/*.md` — Markdown checklists, run in place with Margin checkboxes | you, agent |
| Lists | `lists/*.md` (groceries, wishlist, to-buy). `kind: groceries` lists roll up into one shopping list | you, agent |
| Code | `code/**` and repo links (e.g. Home Assistant automations) | you, agent |

Conventions taught to the agent through a ZeroClaw skill that the hub regenerates from the
vault whenever topics change:

- **Facts:** store with memory category `topic:<slug>`.
- **Reminders:** name cron jobs `[<slug>] <text>` and deliver them to that topic's Telegram
  thread.
- **Brief:** rewrite only the text between the `brief:` markers.
- **Procedures:** write new ones to `procedures/`.

## 6. Getting discussions into topics

**Capture.** margin-hub pairs as its own device and subscribes to `GET /api/events`. On start it
backfills from `/api/events/history`. Each turn becomes one append-only row:
`(id, source, chat, thread, author, text, ts, session_id, topics[])`.

If channel turns turn out not to be on the bus (spike 1, §10), the fallback is to read
ZeroClaw's SQLite session store read-only (`session_backend = "sqlite"`), polling by row id.

**Filing**, cheapest rule first. A turn can belong to several topics.

1. **Explicit route.** A Telegram forum topic or Slack channel listed under `route:`.
   *Recommended setup:* one Telegram supergroup "Home" with forum Topics mirroring Margin topics.
   Most messages then file deterministically, and reminders land in the right thread via cron
   `delivery.thread_id`.
2. **Explicit tag.** `#mazda` in the text, or a reply to an already-filed message.
3. **Thread continuity.** The same chat or thread within 6 hours inherits the previous topic.
4. **Aliases and keywords** from `topic.md`, scored.
5. **LLM fallback.** The hub asks ZeroClaw through an isolated `classifier` agent session. It
   gets back `[{topic, confidence}]`; at ≥ 0.8 the turn is filed, otherwise it goes to the
   **Inbox** with suggestions.

Filing by hand in the Inbox writes to `.margin/filing.jsonl`. Those decisions are used as
few-shot examples for rule 5.

## 7. Reminders

ZeroClaw's scheduler is the only scheduler. Margin creates a reminder with:

```json
POST /api/cron
{ "agent": "home", "name": "[mazda] WOF due", "schedule": "0 9 14 12 *", "tz": "<your tz>",
  "job_type": "agent", "prompt": "Remind me the WOF is due today and offer to book it.",
  "delivery": { "mode": "announce", "channel": "telegram.home", "thread_id": "<mazda thread>" } }
```

The Calendar facet lists jobs whose `[slug]` prefix matches. The next run is computed
client-side from `schedule` + `tz`, and recent runs come from `/api/cron/{id}/runs`. The
**Today** view merges every topic's calendar.

## 8. UI

See the mockup for the full layout. It reuses Margin's paper theme (`crates/margin-ui/assets/theme.css`):
Instrument Serif titles, DM Sans for UI, IBM Plex Mono for data, pine accent. Each topic gets a
single hue, used only on its icon tile.

```
┌ Sidebar ───────┬ Topic page ─────────────────────────────────┬ Chat · topic ─────────┐
│ Search    ⌘⇧F  │ [icon] Noisy Marae           + Reminder  +  │ session topic-house   │
│ Today       3  │ Overview Discussions Calendar Docs          │ ┌ context loaded ┐    │
│ Inbox       2  │ Procedures Lists Code                       │ you: is 60 °C ok?     │
│ TOPICS         │ ┌ Brief (agent) ──────┐ ┌ Next up ───────┐  │ agent: yes, …         │
│ ▸ Mazda   14/12│ │ …                   │ │ Thu Plumber    │  │                       │
│ ▾ Noisy Marae  │ └─────────────────────┘ └────────────────┘  │ [Summarize] [Remind]  │
│    Hot water   │ Recent discussions  ✈ Telegram  # Slack  ◎  │ ┌───────────────────┐ │
│    Gutters     │ Known facts (memory topic:house)            │ │ Ask about house…  │ │
│ ● home · 23 ms │                                             │ └───────────────────┘ │
└────────────────┴─────────────────────────────────────────────┴───────────────────────┘
```

- **Sidebar.** Today, Inbox, then topics with sub-topics. Each row shows the next reminder
  (e.g. `WOF · 14 Dec`) and an unread dot. The connection status sits at the bottom: host,
  latency, paired device name. When offline, it shows *read-only, last synced …*.
- **Topic page.** The header holds the icon tile, serif title, source counts, and *+ Reminder* /
  *+ Note*. Facet tabs sit below it. Docs and procedures open in the existing Live editor, so a
  procedure's checkboxes are real Markdown tasks.
- **Chat panel.** Bound to `session_id = topic-<slug>`. On a fresh session (`message_count == 0`),
  Margin sends a context message, shown collapsed: the topic, its vault path, the brief and the
  facts. Quick actions: *Summarize this week*, *Add reminder*, *What's next?*, *File into…*.
- **Discussions.** One thread list across sources, with filter chips (All · Telegram · Slack ·
  Chat) and a source glyph on each row. Opening a thread shows the original turns; there's no
  replying to Telegram from Margin in v1.
- **Inbox.** Unfiled turns, each with up to three suggested topics as one-click buttons, plus
  *Not a topic*.
- **Keys.** `⌘O` switches topic, `⌘⇧F` searches everything, `⌘L` focuses the topic chat.
  `⌘K`, `⌘J` and `⌘1–⌘6` are already editor shortcuts (link, ask inline, heading level), so they
  aren't reused.
- **Search.** Three sources merged: vault full-text, timeline full-text (SQLite FTS5), and
  `GET /api/memory?query=`.
- **Phone (later).** Same model. The topic list is the home screen, facets become a segmented
  control, and chat is a bottom sheet. The hub serves the Margin web build on `:8443` and proxies
  ZeroClaw calls, so the web client stays same-origin.

## 9. Implementation in Margin

- **`src/hub/`** (feature `hub`, native-only target; the wasm build stays as is):
  - `client.rs` — REST (`reqwest` + rustls) and `/ws/chat` (`tokio-tungstenite`), reconnect with
    backoff, token from `keyring`.
  - `topics.rs` — `Topic`, `Facet`, front-matter parsing (`serde_yaml`).
  - `cron.rs` — `[slug]` name convention and next-run computation (`cron` crate).
  - `cache.rs` — SQLite mirror of the topic index, timeline page and cron list for offline
    reading.
- **`crates/osom/src/topics/`** — `sidebar.rs`, `page.rs`, `facets/{overview,discussions,calendar,docs,procedures,lists,code}.rs`,
  `chat.rs`, `inbox.rs`. The existing note view hosts vault docs unchanged.
- **Data flow.** Hub client tasks run on tokio and push into Dioxus `Signal`s. Chat `chunk` frames
  append to the streaming message, and `done` commits it.
- **SPEC.md.** Add a "Topics" section once the layout is accepted (SPEC is the source of UX rules).

## 10. Build order and spikes

0. **Connect (½ day).** Set up `tailscale serve` and pairing. Check from the Mac and the phone:
   - `/health` returns 200.
   - `/api/status` returns 401 without a token and 200 with one.
   - `/admin/reload` and `/admin/shutdown` are rejected.
   - `/ws/chat` completes a turn.
1. **Topic chat (no hub).** Sidebar from a local clone of the vault, per-topic chat, Calendar
   from `/api/cron`, facts from `/api/memory`. Discussions shows Margin chat only.
2. **margin-hub capture.** Turn capture, rules 1–4 filing, Inbox, timeline endpoints, Tailscale
   identity check.
3. **Smart filing and agent upkeep.** LLM fallback, few-shot from manual filing, regenerated
   topics skill, brief upkeep, groceries roll-up in Today.
4. **Live docs and phone.** Loro sync of vault docs through the hub (README build-order step 3),
   web build served by the hub.

**Spikes, before step 2:**

1. Do Telegram and Slack turns appear on `/api/events`, with channel, chat and thread ids?
2. Does ZeroClaw's Telegram channel read and post to forum topics (`message_thread_id`)?
3. Which `schedule` formats does cron accept — cron expressions only, or one-shot times too?

## 11. Open questions

- Which machine runs ZeroClaw? From this Mac, the home server had no gateway on
  42617, 3000 or 8080, and its SSH host key wasn't trusted yet.
- Is Telegram a 1:1 chat with the bot today, or a group that could switch on forum Topics?
- Is Slack a workspace the bot can join (Socket Mode), or only DMs?
- Is "Wedding N&C" our own event, or friends' (which changes whether People links in)?
