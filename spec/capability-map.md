# Capability Map: osom

> Living index of **osom** — a personal assistant, living wiki, and agenda organized into isolated Spaces and Topics, backed by self-hosted ZeroClaw agents over Tailscale.
>
> The embedded Markdown editor remains specified separately in [crates/margin-editor/SPEC.md](../crates/margin-editor/SPEC.md).

## Modules

| Module id | Responsibility | Depends on | Spec |
|---|---|---|---|
| `spaces-and-isolation` | Isolated spaces (`Personal`, `Family`, `Project A`, …), strict zero cross-space isolation, offline-first git replica + outbox sync (Markdown & config only), and per-space local user-files folders | — | [SPEC-spaces-and-isolation.md](SPEC-spaces-and-isolation.md) |
| `topics-and-wiki` | Topics, sub-topics, people registries (with private cross-space notes that never leak), living facets (Overview, Docs, evolving Procedures + Logs, Lists, Questions Bank), and embedding the Markdown editor + anchored review comments | `spaces-and-isolation` | [SPEC-topics-and-wiki.md](SPEC-topics-and-wiki.md) |
| `agents-and-chat` | Per-space/audience agent isolation (memory, tools, risk profile), in-app chat (space-wide & per-topic), streaming, tool approvals, Questions Bank triggers, and read-only review agent (`scribe`) | `spaces-and-isolation`, `topics-and-wiki` | [SPEC-agents-and-chat.md](SPEC-agents-and-chat.md) |
| `channels-and-routing` | Messaging channel adapters (Telegram, Slack in V1; Instagram in V2), routing channels/groups/DMs to spaces & topics, Inbox triage for unrouted messages, and outbound delivery targets | `spaces-and-isolation`, `topics-and-wiki`, `agents-and-chat` | [SPEC-channels-and-routing.md](SPEC-channels-and-routing.md) |
| `agenda-and-reminders` | Deadlines & recurring cron schedules, gentle nudge lifecycle (advance notice → snooze / booked / done), channel reminder delivery & reply handling, and Today/Upcoming rollups | `spaces-and-isolation`, `topics-and-wiki`, `channels-and-routing` | [SPEC-agenda-and-reminders.md](SPEC-agenda-and-reminders.md) |
| `connectors-and-import` | External sources & ingestion: Mail, Calendar, and exports (chat exports, Apple Notes, attachments) → extracting facts, procedures, questions, and reminders with provenance | `spaces-and-isolation`, `topics-and-wiki`, `agents-and-chat`, `agenda-and-reminders` | [SPEC-connectors-and-import.md](SPEC-connectors-and-import.md) |
| `app-shell` | The `osom` macOS desktop app (`crates/osom` + `crates/margin-ui`: Space switcher, Sidebar, Today, Inbox, Topic workspace, Chat pane, global search, quick switcher, onboarding/pairing, sync status) — completely separated from `crates/margin` (standalone editor + UI gallery), no `--demo` | all above | [SPEC-app-shell.md](SPEC-app-shell.md) |

## Build Order

```
spaces-and-isolation
  → topics-and-wiki
  → agents-and-chat
  → channels-and-routing
  → agenda-and-reminders
  → connectors-and-import
  → app-shell
```

## Cross-Cutting Playbook Deliverable

- **[docs/wiki-maintenance.md](../docs/wiki-maintenance.md) (Operational Playbook & Agent Doctrine):** Living guide (also inherited/referenced by each space agent's `AGENTS.md`) defining how humans and agents maintain and evolve the wiki, review changes, prevent misses, detect edit & factual conflicts, stay proactive, and audit reminders.

## V1 Scope Decisions

- **Single Server Binary via ZeroClaw Plugins + Upstreamable Hooks:** Launch **one** server binary (`zeroclaw`) — zero SilverBullet, zero sidecar daemon. Osom server features (`osom-spaces` replacing SilverBullet, `osom-reminders` live scheduler, `osom-channels` topic/channel tools, read-only Calendar snapshot) live as **ZeroClaw plugins**, powered by a minimal set of general-purpose upstreamable ZeroClaw hooks & extension points (`HookHandler::on_turn_end`, plugin hook dispatch, plugin authenticated HTTP/SSE gateway routes, and Telegram `message_thread_id` / `createForumTopic`).
- **Users:** Single user across their own devices (V1); multi-user accounts and space-scoped device tokens deferred to V2. Space isolation and per-space agents are still strictly enforced in V1 so private spaces never leak into shared channels (e.g., family Telegram/Slack groups).
- **Clients:** macOS desktop (`Dioxus Native`) in V1; Web and mobile clients deferred to V2.
- **Channels:** Telegram and Slack in V1 (existing ZeroClaw adapters); Instagram deferred to V2.
- **Connectors:** Mail, Calendar, and chat/Apple Notes exports in V1 (building on existing tools/connectors).
- **Storage & Sync:** Offline-first git-backed Spaces for Markdown and config only in V1; user/binary files live in a per-space local folder read on demand. Git LFS, topic-to-space promotion, and live collaborative Loro sessions (Spaces Layer 2) are deferred to V2.
- **Isolation:** Strict zero cross-space access in V1 — each space has its own repo, user-files subfolder, and isolated agent(s).
- **Config Path:** `~/.config/osom/`.
- **Encryption / Loro:** No E2EE in V1 (`src/vault/` is out of V1 scope; Loro's broader role outside editor undo/editing is deferred).
- **Binaries & Editor Spec:** `crates/osom` (`osom`) is the main desktop app (dropping `demo.rs` / `--demo`); `crates/margin` (`margin`) stays completely separated as the test app for the Markdown editor and `margin ui` gallery. `SPEC.md` remains dedicated to the Markdown editor and will be relocated to `crates/margin-editor/SPEC.md`.
