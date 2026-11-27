# Osom Wiki Maintenance & Agent Doctrine

> Living operational playbook for humans and space agents working inside an **Osom** Space.
> Every space agent's `AGENTS.md` should reference or include the **Agent Rules** section below.

---

## Core Philosophy

An Osom Space is **not** a static archive and **not** a chat dump — it is a **living wiki and agenda** co-maintained by you and your space's isolated agent.

- **Markdown & config in git; raw files in `files_dir`.** Keep the wiki lean, legible, and git-diffable. Raw PDFs, photos, and chat exports stay in `<files-root>/<space-id>/`.
- **Strict space isolation.** An agent operates strictly within its own space. Never reference, guess, or leak information across spaces.
- **Forgiving in-space capture.** Humans can be messy and post in the wrong channel/topic inside the same space. Answer helpfully in place, file extracted facts into the right topic, and leave a one-line footnote (*"Filed to `car/maintenance.md`"*).
- **Nothing is silently dropped.** Every edit becomes a git commit, every gap enters `questions.md`, every deadline enters `reminders.md`, and every contradiction is surfaced before overwriting.

---

## 1. Maintaining the Wiki & Evolving Procedures

### Keep One Canonical Home per Fact
Avoid copying the same phone number, policy number, or date into three files. Put the fact in its canonical doc and link to it with relative Markdown links (`[maintenance](car/maintenance.md)`) or `[[wikilinks]]`.

| Content type | Canonical file | Rule |
|---|---|---|
| Topic overview, contacts, summary | `<topic>/README.md` | Keep concise; link out to details |
| Repeatable checklist / playbook | `<topic>/maintenance.md` or `<topic>/procedures/*.md` | **What** to do and **how often** |
| Historical record of actions | `<topic>/log.md` | **What was done** — append-only, dated, with details & cost |
| Open gaps & follow-ups | `<topic>/questions.md` | Open table + Answered archive with date & source |
| Shopping / to-buy / wishlists | `<topic>/lists/*.md` or `tobuy/*.md` | Checkbox items (`- [ ] …`) |
| Deadlines & timed nudges | `reminders.md` (space root) | Single source of truth for all active reminders |
| People profiles | `people/<slug>.md` | One file per person; never put shared project plans inside a person file |

### Evolving Procedures Over Time
Procedures are **living documents**, not write-once notes:
1. **Before running a procedure:** Check `<topic>/questions.md` and the last entry in `<topic>/log.md` for lessons learned.
2. **After completing a run (or marking a reminder Done):**
   - Append a dated row/entry to `<topic>/log.md` (date, outcome, cost, provider/contact, provenance).
   - Ask one lightweight question: *"Did any step change, take longer, or need a new tool/contact?"*
   - If yes, update the procedure file right away so next time is smoother.
3. **Preventing rot:** When touching any doc, fix obviously outdated status lines, prune completed temporary bullets into `log.md`, and preserve human prose untouched unless asked to rewrite it.

---

## 2. Reviewing Changes (Human & Agent)

Trust comes from fast, low-friction visibility — not from blocking every edit:
- **Clear commit messages:** Every agent or app edit commits with `<topic>: <what changed>` (e.g. `car: log 2026-04-07 cabin filter service`, `wedding: answer venue deposit question`).
- **Reading diffs & history:** Review recent space commits in git history (`GET /api/spaces/{s}/history?path=<p>`) whenever you want to audit what changed on a page.
- **Scoped review via anchored comments (`scribe-<space>`):** When reviewing or tightening a page in Osom, select a passage (`⌘⇧M`) to ask the space's read-only `scribe` agent. `scribe` can read the whole space to fact-check dates, numbers, and procedures, and proposes exact ```` ```margin-edit ```` replacements that you review, **Apply**, or **Undo** with one click. Comment threads live in `_osom/comments/<page-id>.md` (plain Markdown, immune to renames).
- **External side-effect gate:** Agents edit in-space Markdown freely, but **never** send emails, mutate external calendars (read-only in V1), or run destructive commands without explicit approval.

---

## 3. Making Sure Nothing Is Missed

Four concrete safety nets guarantee nothing falls through the cracks:

1. **Discussion-Driven Inbox:** Every unrouted Telegram/Slack thread lands in the space's **Inbox**. During periodic review (or as soon as you chat about it), discuss the thread with the agent to extract facts into existing topics or spin up a new topic, then mark it handled (`_osom/inbox.md`).
2. **Questions Bank (`<topic>/questions.md`):**
   - Record a question the moment a gap appears with: `ID | Question | Why it matters | Ask when… | Ask by`.
   - Any open question with an approaching `Ask by` date promotes into `reminders.md` so it never expires silently.
   - When answered in any channel, file the fact in its canonical doc, move the row to `## Answered` with date + source, and commit.
3. **Idempotent Imports (`_osom/imports.md`):**
   - Every processed export (Apple Notes, WhatsApp/Telegram/Instagram chat exports, PDFs) and mail/calendar scan logs its `(Key, Hash / Cursor, Processed, Targets, Summary)` in `_osom/imports.md`.
   - Every extracted fact, log entry, question, or reminder carries concise provenance (e.g. `(via mail · 2026-04-07)`, `(via apple-notes)`).
   - Re-running an import skips unchanged hashes automatically and processes only what changed.
4. **End-of-Turn Commit Hook:** Agent workspace edits are committed automatically at `on_turn_end` under one lock — no manual `git commit` or fragile autosync cron can miss or clobber a turn.

---

## 4. Detecting Conflicts (Edit Conflicts & Factual Contradictions)

Two distinct kinds of conflict exist — handle both explicitly:

### A. Concurrent Edit Conflicts (Git Layer)
- Every client edit is committed on its exact `base` commit before merging (`refs/spaces/changes/<change_id>`), so raw edits are **never lost**.
- Non-overlapping text edits 3-way merge cleanly; an edit always beats a concurrent delete.
- When two edits overlap on the same lines, the server preserves both sides and returns `{ base, head, client_commit }`. The client's space agent automatically rebases the edit cleanly (keeping both intents); if offline or the rebase fails, Osom keeps the union-merged file safely and displays a **"Merge with agent"** banner on that page until reconciled.

### B. Factual & Logical Contradictions (Wiki Layer)
More dangerous than a git conflict is **silent drift** — e.g., `car/maintenance.md` says *"Next WOF: 2027-04"*, while a newer email or `car/log.md` says *"WOF done 2026-05-12, valid 12 months"*, or `reminders.md` has a deadline that disagrees with `pregnancy/appointments.md`.

**Agent rules for contradiction detection:**
1. **Check before writing:** Whenever updating a date, price, contact, status, or recurring interval, check the topic's `README.md`, `log.md`, and `reminders.md` for matching entries.
2. **Surface, don't silently overwrite:** If new information contradicts an existing canonical fact with unclear precedence, point out both values with their sources/dates and ask which is current (or log an open question in `questions.md`).
3. **Keep derived dates synchronized:** When a newer confirmed fact supersedes an older one (e.g., an invoice confirms a service was completed), update the canonical doc, append to `log.md`, **and** update the corresponding row in `reminders.md` in the same turn.
4. **Monthly contradiction sweep:** During periodic wiki hygiene (below), scan for:
   - `reminders.md` rows whose `Source file` no longer exists or lists a different deadline,
   - `questions.md` open questions already answered in a topic doc or `log.md`,
   - Duplicate topics or duplicate person files.

---

## 5. Staying Proactive (Without Being Annoying)

Proactivity is helpful only when it is **contextual, concise, and timed well**:
- **Questions Bank context triggers:** Before replying on a topic, check `<topic>/questions.md`. If the conversation matches an `Ask when…` trigger, ask **at most 1–2** relevant open questions briefly at the end of the reply. Never dump the whole list, and never ask the same question twice in one day.
- **Pre-deadline prep:** When a booking-heavy deadline enters its lead window (e.g., WOF, passport renewal, appointment), mention any prerequisites or contacts already stored in the topic's procedure alongside the nudge.
- **Brief upkeep:** When a topic's state changes materially (status change, upcoming milestone, resolved blocker), keep its `README.md` overview fresh so opening the topic gives an instant accurate picture.
- **Silence by default:** If nothing is due and no trigger matched, stay silent. Never send "all clear" noise to Telegram or Slack.

---

## 6. Reminder Hygiene & Audit Cadence

Reminders work only if you trust them completely:
1. **One live source of truth (`reminders.md`):** Every deadline (`YYYY-MM-DD`) and exact time-of-day nudge (`YYYY-MM-DD HH:MM`) lives in the space's `reminders.md`. The `osom-reminders` plugin reloads on every commit — never maintain manual per-topic crons.
2. **Right lead time & tightening cadence:**
   - Yearly+ → 3 months · ~6 months → 6 weeks · Quarterly → 3 weeks · Monthly → 1 week · Weekly/short → 1–2 days · Exact time → on the minute.
   - `Next ping` advances by `⅓` of remaining time (min 1 day); overdue items ping every 2 days (daily if recurrence ≤ weekly).
3. **Clean closure:** Every ping asks *done? booked? when should I ping again?*
   - `booked` → silent until the day before.
   - `done` → log in `<topic>/log.md`, spawn next occurrence if recurring.
   - `cancelled` → stop permanently.
4. **Periodic hygiene rhythm:**

| Cadence | What to check (human or agent-assisted) |
|---|---|
| **Per turn (automatic)** | File facts to canonical doc, cite provenance, sync `reminders.md`, check 1–2 `questions.md` triggers, commit on `on_turn_end` |
| **Weekly (~5 min in Today & Inbox)** | Triage unrouted Inbox items; clear or snooze overdue reminders; review upcoming 3-week agenda + cached Calendar events |
| **Monthly (~10 min wiki audit)** | Ask the space agent: *"Audit this space for stale reminders, answered questions still marked open, factual contradictions between docs and `reminders.md`, and procedures we should simplify."* |

---

## 7. Drop-In `AGENTS.md` Doctrine Block

Include this section in each Space agent's `AGENTS.md` so every isolated space follows the same standards:

```markdown
## Osom Wiki & Maintenance Rules

1. **Isolation:** Work strictly inside your assigned space repo and `files_dir`. Never reference or leak another space.
2. **Forgiving routing:** If the user asks about Topic B inside Topic A's channel (same space), answer helpfully in place, file facts into Topic B, and add a 1-line footnote (`Filed to <path>`).
3. **Canonical docs & provenance:** Keep procedures (`maintenance.md` / `procedures/`), history (`log.md`), open gaps (`questions.md`), and deadlines (`reminders.md`) in sync. Cite source and date on every extracted fact (`via mail · YYYY-MM-DD`, `via apple-notes`, `via chat-export`).
4. **No silent overwrites:** Before changing a date, price, or contact, check existing docs and `reminders.md`. Surface contradictions explicitly with both values and sources.
5. **Questions Bank:** Log new gaps with *why*, *ask when…*, and *ask by*. Ask at most 1–2 triggered open questions at the end of a relevant reply (never twice in one day). Move answered questions to *Answered* with date + source.
6. **Procedures evolve:** After logging a completed task in `log.md`, note any step or contact changes back into the procedure file.
7. **Reminders:** Keep `reminders.md` accurate (`YYYY-MM-DD` or `YYYY-MM-DD HH:MM`). On `done`, log to `log.md` and add the next recurring occurrence. Promote open questions with approaching `Ask by` dates into `reminders.md`.
8. **Read-only external connectors:** Mail and Calendar are read-only; notify actionable findings in Telegram only.
9. **Clean commits:** Summarize every edit clearly (`<topic>: <what changed>`). Never drop untouched user prose.
```
