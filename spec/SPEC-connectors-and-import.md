# Spec: Connectors & Import (`connectors-and-import`) — V1

> Module id: `connectors-and-import` (see [capability-map.md](capability-map.md)).
> Depends on: `spaces-and-isolation` ([SPEC-spaces-and-isolation.md](SPEC-spaces-and-isolation.md)), `topics-and-wiki` ([SPEC-topics-and-wiki.md](SPEC-topics-and-wiki.md)), `agents-and-chat` ([SPEC-agents-and-chat.md](SPEC-agents-and-chat.md)), `agenda-and-reminders` ([SPEC-agenda-and-reminders.md](SPEC-agenda-and-reminders.md)).
> Status: approved.

## Objective

Let each Space's agent read **Mail**, **Calendar**, and **user-dropped exports** (chat exports, Apple Notes exports, PDFs/documents in `files_dir`) **read-only and on demand**, distilling useful facts, people details, procedures, open questions, and reminders into the space's Markdown wiki with clear **source provenance** and **idempotent deduplication** — notifying the user in Telegram only.

### Why
Real life's facts are scattered across emails (invoices, booking confirmations, vet/garage receipts), external calendars, Apple Notes, and old chat exports (WhatsApp, Telegram, Instagram). Agents need a safe, read-only way to search those sources and ingest exports into the right space without polluting git with raw binary dumps, duplicating entries on re-runs, or ever mutating external mail/calendar accounts.

### User Stories (V1)
1. **Strict read-only Mail & Calendar per space:** Each space's agent can search and read only the Mail and Calendar accounts bound to that space (`google_workspace`, `email_imap`/`email_read`/`email_search`) — never sending email, deleting messages, or writing calendar events in V1. Any actionable finding is surfaced in Telegram and/or filed into the space's wiki and `reminders.md`.
2. **On-demand ingestion from `files_dir`:** I can drop chat exports (WhatsApp `.txt`/`.zip`, Telegram JSON/HTML, Instagram JSON), Apple Notes exports (`apple-notes-export`), PDFs, or receipts into `<files-root>/<space-id>/`, then ask the agent (in Telegram, Slack, or osom chat) — or trigger **Import** in the app — to inspect and extract facts into the right topic(s).
3. **Raw sources stay out of git; distilled knowledge enters git:** Raw archives, attachments, and PDFs remain in `files_dir` (never committed to git); only structured Markdown (topic docs, `log.md`, `people/*.md`, `questions.md`, `reminders.md`) is written to the space repo.
4. **Provenance on every extracted item:** Every fact, log row, question, or reminder extracted from an external source cites its provenance (e.g., `mail · 2026-04-07`, `calendar`, `apple-notes`, `chat-export · <file>`) so the user can always trace where information came from and catch stale or conflicting facts.
5. **Idempotent re-imports (`_osom/imports.md`):** Processed files (with path + content SHA-256 + timestamp + target topics) and mail/calendar scan cursors are logged in `_osom/imports.md`, so re-running an incremental export like `apple-notes-export` skips unchanged items and never duplicates notes, questions, or reminders.
6. **Contradiction & gap handling during import:** When an imported source contradicts an existing fact in the wiki (e.g., a newer appointment date or changed service interval), the agent flags the contradiction explicitly (or logs a question in `questions.md`) rather than silently overwriting.

## Tech Stack

- **Server connectors (`~/src/osom/zeroclaw`):** Existing `google_workspace` (Gmail read, Google Calendar read, Drive read), `email_imap` / `email_read` / `email_search`, scoped read-only per space agent via tool allowlists.
- **Export helpers:** Existing `tools/apple-notes-export` (Python 3 stdlib, idempotent Apple Notes → Markdown + `_attachments` mirror), plus pure Rust/script parsers for common chat export formats (WhatsApp `.txt`, Telegram `result.json`, Instagram `message_*.json`).
- **Client pure core (`crates/osom-topics/src/imports.rs`):** Pure Rust parser/writer for `_osom/imports.md` (manifest table, SHA-256 skip check, provenance formatting) and cached calendar event model.

## Commands

```sh
# Type-check both binaries (0 warnings required)
cargo check --locked -p osom -p margin

# Run pure import manifest, provenance, and dedup unit tests
cargo test --locked -p osom-topics imports

# Run Apple Notes exporter tests (synthetic fixture, no real notes touched)
python3 -m unittest discover -s ~/src/osom/z-osom-tka/tools/apple-notes-export/tests

# Run full workspace test suite
cargo test --locked --workspace

# Server: run read-only mail & google workspace connector tests
cargo test --manifest-path ~/src/osom/zeroclaw/Cargo.toml -p zeroclaw-tools email
cargo test --manifest-path ~/src/osom/zeroclaw/Cargo.toml -p zeroclaw-tools google_workspace
```

## Project Structure

```
spec/SPEC-connectors-and-import.md              → This specification
crates/osom-topics/src/imports.rs               → Pure `_osom/imports.md` manifest parser/writer, SHA-256 dedup helper, provenance tags, chat-export normalizer
crates/osom/src/live.rs                         → Loads cached calendar events & import status for the active space
crates/osom/src/today.rs                        → Renders upcoming read-only Calendar events alongside reminders
~/src/osom/z-osom-tka/tools/apple-notes-export/ → Existing idempotent Apple Notes → folder exporter
~/src/osom/zeroclaw/crates/zeroclaw-tools/src/  → Existing `google_workspace.rs`, `email_imap.rs`, `email_read.rs`, `email_search.rs`
```

## Interface & Data Contract

### 1. Per-Space Read-Only Connector Binding (Server Config)
Connectors are declared strictly per space agent with read-only scopes:
```toml
[agents.luc.connectors]
mail = { provider = "gmail", account = "personal", mode = "readonly" }
calendar = { provider = "google_calendar", calendars = ["primary"], mode = "readonly" }

[agents.family.connectors]
calendar = { provider = "google_calendar", calendars = ["family@group.calendar.google.com"], mode = "readonly" }
```
- **Enforcement:** Write tools on external Mail and Calendar providers (`send_email`, `create_event`, `delete_event`, `modify_event`) are disabled in V1 agent tool profiles. All notifications go to Telegram only (via `channels-and-routing`).
- **Today view calendar cache:** The desktop app displays upcoming read-only Calendar events for the active space alongside `reminders.md` in the **Today** view from a cached snapshot (so Today works offline too).
- **Isolation:** An agent can query only connectors declared under its own `[agents.<id>.connectors]` block — zero cross-space credential sharing.

### 2. Local `files_dir` Drop Folder Conventions
Inside `<files-root>/<space-id>/` (configured as `files_dir` in `spaces-and-isolation`):
```
<files-root>/<space-id>/
  apple-notes/          # Output of `apple-notes-export` (idempotent Markdown + _attachments)
  chats/                # User-dropped WhatsApp, Telegram, Instagram, Messenger exports
  docs/                 # PDFs, receipts, manuals, scans
  ...                   # Any free-form folders dropped by the user
```
- Users can drop files anywhere inside their space's `files_dir`; osom and the space agent treat `files_dir` as **read-only input** (never deleting or mutating user-dropped files).

### 3. Import Manifest & Deduplication (`_osom/imports.md`)
Lives inside the space's git repo under `_osom/` (hidden from Topics, readable by humans and agents):
```markdown
# Imports

Tracks processed files and connector scans so re-runs stay idempotent.

| Key | Source | Hash / Cursor | Processed | Targets | Summary |
|---|---|---|---|---|---|
| apple-notes/Car/WOF.md | apple-notes | sha256:9f3a1c8b… | 2026-11-26 | car/maintenance.md, reminders.md | Extracted WOF schedule & garage contact |
| chats/whatsapp-nikki.txt | chat-export | sha256:4d2e7a10… | 2026-11-26 | people/nikki-kergozou.md | Added gift ideas & dietary notes |
| mail:personal | gmail | cursor:193a8f12 | 2026-11-26 | car/log.md | Logged 2026-04-07 service invoice |
```
- **Skip rule:** Before ingesting any file in `files_dir`, compute its SHA-256 hash; if `(Key, Hash)` matches an existing row in `_osom/imports.md`, skip it unless the user explicitly asks to re-process.
- **Incremental updates:** When a file's hash changes (e.g., an Apple Note was edited and re-exported), only the updated item is re-processed and its row in `_osom/imports.md` is updated in place.

### 4. Provenance Convention
Every extracted item written to the wiki includes a concise, consistent provenance marker:
- **In `log.md` / `reminders.md` (`Notes` or `Source` column):** `(via mail · 2026-04-07)`, `(via calendar)`, `(via apple-notes · Car/WOF)`, `(via chat-export · whatsapp-nikki)`.
- **In `questions.md` (`Answered` table):** `2026-11-26 · mail (invoice #4812)`.
- **In topic prose / people pages:** Inline parenthetical or quiet footnote citing source and date — never pasting raw email headers or full raw chat dumps into git.

### 5. On-Demand Ingestion Flow
1. User drops file(s) into `<files-root>/<space-id>/` (or asks to check Mail/Calendar for a topic/date range).
2. User triggers ingestion via Telegram/Slack/osom chat (e.g., *"check my mail for garage invoices"*, *"import `files/personal/chats/whatsapp-nikki.txt`"*) or via an **Import** action in the desktop app.
3. Agent checks `_osom/imports.md` to skip already-processed items, extracts structured facts, procedures, open questions, and reminders, detects contradictions with existing docs, commits the distilled Markdown + updated `_osom/imports.md`, and replies with a concise summary of what was added/updated/skipped (and any contradictions or new questions).

## Code Style

Keep manifest parsing, SHA-256 skip detection, chat-export text normalization, and provenance formatting pure in `crates/osom-topics/src/imports.rs`:

```rust
/// Decide whether a file in `files_dir` needs ingestion based on `_osom/imports.md`.
pub fn needs_import(manifest: &[ImportRecord], key: &str, sha256: &str) -> bool {
    !manifest.iter().any(|r| r.key == key && r.hash_or_cursor == sha256)
}
```

## Testing Strategy

| Layer | Location | What it verifies |
|---|---|---|
| Pure import manifest unit tests | `crates/osom-topics/src/imports.rs` (`#[cfg(test)]`) | `_osom/imports.md` parse/round-trip, SHA-256 skip logic, updated-hash re-import detection, literal-preserving row update, provenance tag formatting |
| Chat-export normalization tests | `crates/osom-topics/src/imports.rs` (`#[cfg(test)]`) | WhatsApp `.txt`, Telegram `result.json`, Instagram JSON parsing into clean `(ts, author, text)` turns without leaking media blobs |
| Apple Notes exporter tests | `tools/apple-notes-export/tests` | Synthetic SQLite NoteStore fixture export, idempotency, safety guards |
| Server connector read-only tests | `zeroclaw-tools` | Read-only tool profile blocks send-mail / write-calendar calls while permitting search & read within the agent's bound account |

## Boundaries

- **Always:**
  - Enforce read-only mode on all Mail and Calendar connectors in V1.
  - Record every processed import and cursor in `_osom/imports.md` with its hash so re-runs are idempotent.
  - Cite source provenance on every extracted fact, log entry, question answer, and reminder.
  - Treat `files_dir` as read-only input (never delete or overwrite user-dropped files).
- **Ask first:**
  - Enabling any write/send tool on Mail or Calendar (explicitly deferred to V2).
  - Adding a new external OAuth/API connector beyond Mail, Calendar, Apple Notes, and local exports.
- **Never:**
  - Commit raw binary exports, email archives, or PDFs from `files_dir` into the space's git repo.
  - Share a connector credential or search Mail/Calendar across Space boundaries.
  - Silently overwrite an existing wiki fact when an imported source contradicts it — flag the contradiction explicitly.

## Success Criteria

1. **Read-only Mail & Calendar enforced:** Space agents can search and read their bound Mail/Calendar accounts, notify findings in Telegram, and are blocked server-side from sending email or modifying calendar events.
2. **Idempotent on-demand import:** Running an import twice over the same `files_dir` export (including `apple-notes-export`) skips unchanged files on the second run via `_osom/imports.md` and produces zero duplicate wiki entries or reminders.
3. **Incremental re-import:** Modifying one exported note updates its SHA-256, re-processes only that note, and updates its manifest row cleanly.
4. **Provenance preserved:** Every extracted reminder, log entry, and answered question carries a traceable source tag (`mail`, `calendar`, `apple-notes`, or `chat-export`).
5. **Strict isolation:** `family`'s agent cannot query `personal`'s Gmail/Calendar or read `files/personal/`.

## Out of Scope (Deferred to V2)

- **Mail sending & Calendar write-back** — Creating/editing Google or Apple Calendar events and sending emails from osom is deferred to V2 (V1 is read-only; notifications go to Telegram only).
- **Automatic background folder watcher (`files_dir/inbox/`)** — V1 triggers ingestion on demand; automatic file-drop watching is deferred to V2.
- **Scheduled periodic Mail/Calendar digest sweeps** — Automated proactive background polling of Gmail/Calendar (beyond on-demand checks or morning brief calls) can be layered once read-only tools are proven; fine-grained email rule automation is deferred to V2.
- **OCR / audio transcription pipeline for scanned PDFs and voice notes** — Deferred to V2 (text-based PDFs and existing Apple Notes handwriting transcriptions work in V1).

## Open Questions

None — all V1 decisions are resolved (upcoming read-only Calendar events appear alongside `reminders.md` in the desktop Today view from cache; export parsers stay scoped to Apple Notes + WhatsApp/Telegram/Instagram and grow on demand).
