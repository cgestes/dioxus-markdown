# Spec: Agenda & Reminders (`agenda-and-reminders`) — V1

> Module id: `agenda-and-reminders` (see [capability-map.md](capability-map.md)).
> Depends on: `spaces-and-isolation` ([SPEC-spaces-and-isolation.md](SPEC-spaces-and-isolation.md)), `topics-and-wiki` ([SPEC-topics-and-wiki.md](SPEC-topics-and-wiki.md)), `channels-and-routing` ([SPEC-channels-and-routing.md](SPEC-channels-and-routing.md)).
> Status: approved.

## Objective

Keep every deadline, recurring chore, and exact time-of-day reminder inside each isolated Space **always up to date**, gently nudging on Telegram/Slack at the right moment without manual cron management — while providing quick **Done / Booked / Snooze / + Add** controls in the osom desktop UI and full parity from chat.

### Why
Maintaining separate daily cron jobs per destination (`reminder-sweeper`, `reminder-sweeper-car`, …) doesn't scale as topics grow, lags behind edits made during the day, and can't fire exact-time reminders (like *"15:00 call the vet"*). By driving a live, per-space reminder scheduler directly from `reminders.md` + `DeliveryTarget` resolution, every edit from desktop, Telegram, Slack, or git takes effect immediately and lands in the exact right channel.

### User Stories (V1)
1. **Single live source of truth per space (`reminders.md`):** Every space maintains its own `reminders.md` table holding both date-based deadlines and exact time-of-day reminders (`YYYY-MM-DD` or `YYYY-MM-DD HH:MM`), with strict zero cross-space leakage.
2. **Always-up-to-date scheduler (no daily lag, no per-destination crons):** A per-space scheduler reconciles immediately whenever `reminders.md` changes (on space commit) and evaluates due reminders continuously (minute resolution in the space's configured timezone, e.g. `Europe/Paris`), routing each due ping directly to its resolved `DeliveryTarget` (Telegram topic / group / Slack channel).
3. **Time-of-day & date-based reminders unified:** Date-only reminders fire at the space's default morning reminder time (e.g., `10:00` local time) with gentle tightening cadence; time-of-day reminders (`YYYY-MM-DD HH:MM`) fire at their exact scheduled minute.
4. **Gentle tightening cadence & clean lifecycle:**
   - Lead time scales with recurrence (`yearly+` → 3m, `~6m` → 6w, `quarterly` → 3w, `monthly` → 1w, `weekly/short` → 1–2d; overridable per item).
   - After each ping, `Next ping` tightens by `⅓` of remaining time (minimum 1 day, never past deadline); overdue items ping every 2 days (daily if recurrence ≤ weekly).
   - `booked` stays silent until the day before the appointment; `done` silences the item, logs completion to the topic's `log.md`, and creates the next occurrence if recurring; `cancelled` stops permanently.
   - Every channel ping ends with a brief check-in (*done? booked? when should I ping you again?*) and replying in any channel updates `reminders.md`.
5. **Desktop UI quick actions (offline-first):** In **Today** and each topic's **Calendar** facet, I can click **+ Add Reminder**, **Done**, **Booked** (picking date/time), or **Snooze** (e.g., +1d / +3d / +1w / custom date-time) — editing `reminders.md` cleanly in the local replica (preserving untouched rows and prose) and syncing automatically.
6. **Questions Bank integration:** Open questions in `<topic>/questions.md` with an `Ask by` date approaching are surfaced in the agenda and promoted into `reminders.md` so nothing slips through the cracks.

## Tech Stack

- **Pure core (`crates/osom-topics/src/{calendar,planning,reminders}.rs`, `crates/margin-core/src/dates.rs`):** Rust (edition 2024), pure Markdown table parser + literal-preserving row editor (add/update/mark-done/snooze/recurrence calculation), lead-time & `⅓` cadence math, date & time-of-day parsing.
- **Desktop UI (`crates/osom/src/{today,topic,view}.rs`, `crates/margin-ui/`):** Today view, Topic Calendar facet, inline Done/Booked/Snooze actions and Add Reminder dialog using `margin_ui::{DateBox, Row, Button, Popover, TextField}`.
- **Server (`~/src/osom/zeroclaw`):** Per-space live reminder scheduler reacting to `zeroclaw-spaces` commit notifications (and minute ticks), dispatching due items via the space's Telegram/Slack adapter with `message_thread_id` / `thread_ts`, and updating `Last ping` / `Next ping` under the space committer lock.

## Commands

```sh
# Type-check both binaries (0 warnings required)
cargo check --locked -p osom -p margin

# Run pure reminder parsing, cadence math, and literal-preserving edit tests
cargo test --locked -p osom-topics -p margin-core

# Run full workspace test suite
cargo test --locked --workspace

# Server: run reminder scheduler & dispatch tests
cargo test --manifest-path ~/src/osom/zeroclaw/Cargo.toml reminder
```

## Project Structure

```
spec/SPEC-agenda-and-reminders.md   → This specification
crates/osom-topics/src/calendar.rs  → Pure date/time parsing, timezone-aware comparison, weekday & DateBox chip formatting
crates/osom-topics/src/reminders.rs → Pure cadence math (lead time, ⅓ tightening, overdue, recurrence rollover), literal-preserving `reminders.md` table mutations
crates/osom-topics/src/planning.rs  → Reminder owner resolution (most-specific topic by Source file or ID stem) and Questions Bank `Ask by` bridge
crates/osom/src/today.rs            → Today view with overdue badge, + Add Reminder, Done (log popover), Booked, Snooze controls
crates/osom/src/topic.rs            → Topic Calendar facet
```

## Interface & Data Contract

### 1. `reminders.md` Table Schema (Backwards-Compatible)
Each space keeps `reminders.md` at its root with the standard Markdown table:
```markdown
| ID | What | Recurrence | Deadline | Start | Status | Next ping | Last ping | Deliver to | Source file | Notes |
|----|------|------------|----------|-------|--------|-----------|-----------|------------|-------------|-------|
| car-wof | WOF inspection | yearly | 2027-04-15 | 2027-01-15 | open | 2027-01-15 | — | — | car/maintenance.md | — |
| vet-call | Call vet for results | one-off | 2026-11-27 15:00 | 2026-11-27 15:00 | open | 2026-11-27 15:00 | — | — | dog/README.md | Ask about bloodwork |
```
- **Date vs. Date-Time formats:**
  - `YYYY-MM-DD`: date-based reminder; fires at the space's configured default morning hour (default `10:00` in space timezone).
  - `YYYY-MM-DD HH:MM`: exact time-of-day reminder; fires at that exact local minute.
- **`Status` values:** `open`, `booked`, `done`, `cancelled` (case-insensitive).
- **`Deliver to`:** Optional explicit override (e.g., `Telegram DM`, channel label, or `chat_id:thread_id`). When blank or `—`, falls back to the owning topic's resolved `DeliveryTarget` from `channels-and-routing`.
- **Literal-preserving mutations (`crates/osom-topics/src/reminders.rs`):** Adding a row or updating `Status` / `Next ping` / `Last ping` / `Notes` modifies only the targeted row (or appends one row to `## Active items`) without reformatting surrounding prose, headers, or other rows.

### 2. Pure Cadence & Lifecycle Rules (`crates/osom-topics/src/reminders.rs`)
- **Lead time (`Start = Deadline - lead`):**
  - `yearly` or longer → 3 months (90d)
  - `6 months` / `semi-annual` → 6 weeks (42d)
  - `quarterly` → 3 weeks (21d)
  - `monthly` → 1 week (7d)
  - `weekly` or short one-off → 1–2 days
  - Exact time-of-day one-off → `Start = Deadline` (or explicit lead if specified)
- **Next ping calculation after firing:**
  - For date-based `open` items before deadline: `next = today + max(1 day, remaining_days / 3)` (capped at `deadline`).
  - For overdue `open` items: `today + 1 day` if recurrence ≤ weekly (or exact-time), else `today + 2 days`.
  - For `booked`: silent until `appointment_date - 1 day` (or exact appointment reminder time if `HH:MM` is set).
  - For `done`: set `Status = done`; if recurring, compute next `Deadline` and `Start` and insert the next occurrence row; optionally append a dated entry to the topic's `log.md`.
- **Idempotent firing:** Before sending a ping, the scheduler checks `Last ping` against the due slot (`YYYY-MM-DD` for daily, `YYYY-MM-DD HH:MM` for time-of-day) and updates `Last ping` + `Next ping` atomically in `reminders.md` under the space lock so restarts never double-ping.

### 3. Live Per-Space Scheduler (Server)
- Each configured space specifies `timezone` (default e.g. `Europe/Paris`) and `reminder_hour` (default `10:00`).
- On startup and on every commit touching `reminders.md` (or `<topic>/questions.md`), the space's reminder scheduler reloads in-memory schedules immediately — no manual cron per topic or destination.
- Every minute, the scheduler checks for items where `status in {open, booked}` and `effective_next_ping <= now` (and not already pinged for that slot):
  1. Resolves `DeliveryTarget` (`Deliver to` override → topic's Telegram/Slack route → space default).
  2. Posts the concise reminder message (ending with *done? booked? snooze?*) via the space agent's channel adapter.
  3. Commits the updated `Last ping` and `Next ping` to `reminders.md` via `zeroclaw-spaces`.
- When nothing is due, zero messages are sent (completely silent).

### 4. Desktop UI Contract (Today & Topic Calendar)
- **Topic Calendar facet:** Shows all open/booked reminders owned by the topic (and its sub-topics), sorted by due date/time with overdue items highlighted (`Tone::Alert` / `Tone::Warn`), plus **+ Add Reminder** and per-row quick actions (**Done**, **Booked**, **Snooze**). Clicking **Done** opens a tiny optional note popover (details/cost to append to `<topic>/log.md`; pressing Enter confirms immediately with or without a note).
- **Today view & sidebar badge:** Shows overdue items + upcoming reminders within the rolling 3-week window (each shown once under its most-specific owning topic), plus any exact-time reminders due today at the top. The sidebar **Today** badge shows the count of overdue + due-today items, tinted `Tone::Alert` / `Tone::Warn` when any item is overdue.
- **Offline edits:** All UI actions mutate `reminders.md` via the space's local `Replica` immediately and sync via the outbox when back online.

## Code Style

Keep all date/time math, cadence calculation, and literal Markdown table editing pure and deterministic (passing `now` explicitly as a parameter — never calling `SystemTime::now()` inside core logic):

```rust
/// Compute the next ping date after firing an open reminder on `today`.
pub fn next_ping_after(reminder: &Reminder, today: NaiveDate) -> NaiveDate {
    if today >= reminder.deadline_date {
        let step = if reminder.is_frequent() { 1 } else { 2 };
        return today + Duration::days(step);
    }
    let remaining = (reminder.deadline_date - today).num_days();
    let advance = (remaining / 3).max(1);
    (today + Duration::days(advance)).min(reminder.deadline_date)
}
```

## Testing Strategy

| Layer | Location | What it verifies |
|---|---|---|
| Pure date/time & cadence unit tests | `crates/osom-topics/src/{calendar,reminders}.rs` | Date vs. `YYYY-MM-DD HH:MM` parsing, lead-time defaults, `⅓` tightening cadence, overdue 1d/2d cadence, `booked` day-before silence, `done` recurring rollover, idempotent double-ping guard |
| Literal-preserving Markdown edits | `crates/osom-topics/src/reminders.rs` | Add row, mark Done, mark Booked, Snooze update only targeted row while keeping untouched prose, comments, and column alignment intact |
| Ownership & Today rollup tests | `crates/osom-topics/src/tests.rs` | Reminder owned by most-specific topic (never duplicated in Today), `questions.md` `Ask by` surfacing |
| Server live scheduler tests | `zeroclaw` scheduler tests | Immediate schedule refresh on `reminders.md` commit, exact-minute firing for `HH:MM`, morning firing for date-only, routing to `DeliveryTarget` with `thread_id`, zero duplicate pings across restart |

## Boundaries

- **Always:**
  - Pass current date/time explicitly into pure functions (`crates/osom-topics/src/{calendar,reminders}.rs`) so every cadence and rollover test is 100% deterministic.
  - Preserve untouched bytes and prose in `reminders.md` on every programmatic edit.
  - Update `Last ping` and `Next ping` under the space committer lock to guarantee no duplicate pings on crash/restart.
  - Keep every reminder and scheduler strictly scoped to one Space (zero cross-space leakage).
- **Ask first:**
  - Changing the `reminders.md` column names or breaking backwards compatibility with existing `z-osom-tka/procedures/reminders.md`.
- **Never:**
  - Send "nothing due today" noise to Telegram/Slack — silence when nothing is due.
  - Require users to create manual cron jobs per topic or destination.
  - Silently drop a recurring reminder's next occurrence when marking the current one `done`.

## Success Criteria

1. **Always-up-to-date dispatch:** Adding or editing a reminder in `reminders.md` (via desktop UI, Telegram, or git) takes effect immediately on the space scheduler with zero manual cron setup.
2. **Exact time-of-day + date-based parity:** A `YYYY-MM-DD HH:MM` reminder fires within 1 minute of its scheduled time to the topic's resolved `DeliveryTarget` (including Telegram `message_thread_id`); a date-only reminder fires at the space's morning hour with `⅓` tightening cadence.
3. **Zero duplicate pings:** Restarting the server immediately after a ping never re-sends the same slot.
4. **Desktop UI quick actions work offline:** Clicking **+ Add Reminder**, **Done** (with recurring rollover), **Booked**, or **Snooze** updates `reminders.md` cleanly in the local replica without corrupting untouched lines, and syncs on reconnect.
5. **Questions Bank bridge:** An open question with an approaching `Ask by` date surfaces in the agenda so it isn't missed.

## Out of Scope (Deferred to V2)

- **Two-way external calendar write-back (Google/Apple Calendar sync)** — Reading external calendars for facts/agenda context lives in `connectors-and-import`; pushing osom reminders out as external calendar invites is deferred to V2.
- **Native macOS push notifications (Notification Center)** — V1 delivers active pings to Telegram/Slack and displays Today/Calendar in the desktop app; local macOS Notification Center alerts are deferred to V2.
- **Natural-language recurrence DSL beyond standard intervals** — V1 supports `one-off`, `daily`, `weekly`, `monthly`, `quarterly`, `6 months`, `yearly`, and `every N days/weeks/months`; complex RRULEs (e.g. "third Thursday of November") are deferred to V2.

## Open Questions

None — all V1 decisions are resolved (desktop **Done** opens a compact optional log-note popover with Enter to confirm; sidebar **Today** badge counts overdue + due-today items with overdue highlighting).
