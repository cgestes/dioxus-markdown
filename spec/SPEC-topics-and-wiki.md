# Spec: Topics & Living Wiki (`topics-and-wiki`) — V1

> Module id: `topics-and-wiki` (see [capability-map.md](capability-map.md)).
> Depends on: `spaces-and-isolation` ([SPEC-spaces-and-isolation.md](SPEC-spaces-and-isolation.md)).
> Status: approved.

## Objective

Turn each isolated Space's Markdown repository into a structured, evolving **living wiki** organized by **Topics** — with first-class parity between **Telegram** (where each space has a group and each topic has its own group) and the **osom desktop app** (where docs open in the embedded Markdown editor from [SPEC.md](../SPEC.md)).

### Why
A raw folder of `.md` files is hard to navigate as life and projects accumulate notes, checklists, people profiles, shopping lists, and open questions. By establishing deterministic conventions over plain Markdown files, both humans and agents can grow each topic's knowledge base, refine procedures over time, track unanswered questions, navigate wiki links and backlinks, and review anchored comments — from either Telegram or the desktop app.

### User Stories (V1)
1. **Topics & sub-topics inside every space:** Every top-level folder in a space (excluding `_`-prefixed system folders like `_osom/`) is a Topic (`car/`, `wedding/`, `pregnancy/`, `me/`, `tobuy/`, …); nested folders are sub-topics; pages under registry folders like `people/` (or directly targeted by a channel route) are Page-Topics.
2. **Telegram + UI parity for topic lifecycle:** I can create a new topic, add or update docs/procedures/lists, and answer open questions **directly from Telegram** (in the space's Telegram group to create topics, or inside a topic's own Telegram group) **or** from the osom desktop UI — both write the same Markdown files in the space.
3. **Living facets (hidden when empty):** Each topic surfaces only the facets that have content:
   - **Overview** (`README.md`, or the folder's main page — never `log.md` or `questions.md`)
   - **Docs** (all other `.md` pages in the topic + references to files in the space's `files_dir`)
   - **Procedures & Logs** (`procedures/*.md` or `maintenance.md` checklists improved over time + `log.md` recording dated runs, details, and costs)
   - **Lists** (`lists/*.md` or checklist pages like `tobuy/list.md`, with shopping lists rolling up into Today)
   - **Questions Bank** (`questions.md` with open questions carrying *why*, *ask when…*, *ask by*, and an *Answered* archive with date + source)
   - *(Discussions & Calendar facets are populated by `channels-and-routing` and `agenda-and-reminders`.)*
4. **In-app wiki navigation & backlinks:** Relative Markdown links (`[label](car/log.md)`, `[label](people/nikki)`, `#heading` anchors) and `[[wikilinks]]` resolve within the active space, show live page previews in the editor's Link Preview Card, navigate in-app to the target doc on click, and compute incoming backlinks across the space.
5. **Markdown editor & rename-safe Markdown comment sidecars:** Opening any doc embeds the Markdown editor ([SPEC.md](../SPEC.md)). Anchored review comment threads are stored as one plain Markdown file per page under `_osom/comments/<page-id>.md` keyed by a stable `id` in the page's YAML frontmatter (human-readable, agent-readable, git-diffable, and immune to page renames or folder moves).
6. **People registries per space:** Each space can maintain its own `people/` registry (e.g., friends in `personal/people/`, shared contacts in `family/people/`) with strict zero leakage across spaces.

## Tech Stack

- **Pure core (no I/O):** `crates/osom-topics/src/` (`space.rs`, `page.rs`, `planning.rs`), `crates/margin-core/src/comments.rs`, `crates/margin-projection/src/` (Rust edition 2024, `pulldown-cmark`, `serde`).
- **Desktop presentation & editor host:** `crates/osom/src/` (`topic.rs`, `pages.rs`, `view.rs`, `live.rs`), `crates/margin-editor/src/` (`notebook/link_card.rs`, `comments/mod.rs`), `crates/margin-ui/` (Dioxus Native design system).
- **Embedded editor:** Defined in [crates/margin-editor/SPEC.md](../crates/margin-editor/SPEC.md), integrated via `PageFiles` + navigation callback.

## Commands

```sh
# Type-check both binaries (0 warnings required)
cargo check --locked -p osom -p margin

# Run pure topic, wiki-link, backlink, and comment-sidecar unit tests
cargo test --locked -p osom-topics -p margin-core -p margin-projection

# Run editor & link-card integration tests
cargo test --locked -p margin-editor link_card
cargo test --locked -p osom

# Run full workspace test suite
cargo test --locked --workspace
```

## Project Structure

```
spec/SPEC-topics-and-wiki.md                → This specification
crates/osom-topics/src/space.rs             → Pure space-to-topic tree & facet classifiers
crates/osom-topics/src/page.rs              → Page titles, frontmatter, wiki links & backlink index
crates/osom-topics/src/planning.rs          → Questions Bank, Procedures + Logs, Lists parsers
crates/margin-core/src/comments.rs          → Anchors (TextQuoteSelector), frontmatter `id:` helpers, Markdown comment sidecar parser/serializer/merger (`_osom/comments/<page-id>.md`)
crates/margin-projection/src/link_preview.rs → Wiki link + relative link + heading anchor resolution and preview summaries
crates/margin-editor/src/notebook/link_card.rs → Link popover card wired to in-app page navigation callback
crates/margin-editor/src/comments/mod.rs    → Embedded editor CommentsPane reading/writing Markdown comment sidecars
crates/osom/src/view.rs                     → Shared Workspace / TopicView model
crates/osom/src/live.rs                     → Adapter building Workspace from a space's local Replica
```

## Interface & Schema Contract

### 1. Space & Topic File Layout
Inside any space's git repo:
```
README.md                        # Space overview + fallback Channel → topic table
reminders.md                     # Space-wide agenda table (consumed by agenda-and-reminders)
_osom/
  comments/<page-id>.md          # Anchored review threads for a page (Markdown, keyed by frontmatter `id`, see §4)
people/
  README.md                      # People registry table (Relationship, File, …)
  _template.md                   # Template (ignored as a topic)
  <person-slug>.md               # Page-Topic (one per person)
<topic-slug>/
  README.md                      # Topic overview + optional YAML frontmatter (title, icon, hue, channel)
  questions.md                   # Open & Answered Questions Bank
  log.md                         # Append-only chronological execution/event log
  maintenance.md / procedures/   # Living checklists & playbooks improved over time
  lists/                         # Checklists & shopping lists
  <other>.md                     # Additional topic docs
  <subtopic-slug>/               # Nested sub-topic (same structure recursively)
```
- **Hidden paths:** Top-level folders starting with `_` (such as `_osom/`, legacy `_margin/`) or `.`, plus `Library/` and `Repositories/`, never appear as topics. Templates prefixed with `_` (like `_template.md`) are hidden from Docs and Topic lists.
- **Topic identity & title:**
  - `id` is the space-relative path without `.md` (e.g., `car`, `house/hot-water`, `people/nikki-kergozou`).
  - Title comes from optional YAML frontmatter `title:`, else the overview page's first H1 split at ` — ` / ` – ` / `: ` / ` (` into `(title, subtitle)`, else title-cased slug.
  - Optional frontmatter fields: `title`, `subtitle`, `icon`, `hue`, `channel` (Telegram/Slack group binding consumed by `channels-and-routing`).

### 2. Creating Topics & Docs (Telegram + UI Parity)
Both the desktop UI and the space's agent (from Telegram/Slack or in-app chat) create topics and docs via the same deterministic structure:
- **Create Topic (`<slug>`):** Creates `<slug>/README.md` with frontmatter (including `title` and optional `channel` binding) and `# <Title>` heading, committing through the space's local replica or agent workspace.
- **Create Doc / Procedure / List / Question:** Writes the Markdown file under the topic folder (or appends to `questions.md` / `log.md`) using the standard conventions so both UI and agent see it immediately on sync.
- **Telegram-first creation:** Asking the agent in the **space's Telegram group** to create a topic (or linking a new topic Telegram group) creates the topic folder + `README.md` + channel route entry in one commit.

### 3. Wiki Links, In-App Navigation & Backlinks
- **Supported link forms (within the active space):**
  - Standard relative Markdown links: `[Label](car/maintenance.md)`, `[Label](car/maintenance)`, `[Label](./log.md#2026-04)`, `[Label](../people/nikki.md)`.
  - Wikilinks: `[[car/maintenance]]`, `[[car/maintenance|Maintenance]]`, `[[nikki]]` (unique stem match within the space).
  - In-document heading anchors: `[Section](#listening-stops)`.
- **Navigation contract (`OpenTarget::SpacePage`):**
  - Clicking a Space link in read-only `MarkdownView` **or** clicking `#link-card-open` in the embedded editor navigates in-app to the target doc within the active space (pushing onto a back/forward navigation stack so Back returns cleanly to the previous doc or topic tab).
  - Clicking a link to a non-existent valid space path offers to create the page inside that topic.
  - Parent-directory traversal escaping the space root (`../../etc/passwd`) is rejected.
- **Backlinks index (pure, in `crates/osom-topics/src/page.rs`):**
  - Computed in memory across all `.md` files in the active space (excluding `_osom/`): maps each normalized page path to all referring pages and link counts.
  - Displayed in the editor's Link Preview Card (`N backlinks`) and as a quiet Backlinks section on a doc/topic view.

### 4. Markdown Comment Sidecars (`_osom/comments/<page-id>.md`)
Comment threads for a page live together in one plain Markdown file per page at `_osom/comments/<page-id>.md`, where `<page-id>` is a stable short id (e.g., ULID or 12-char base32/hex id) stored in the page's YAML frontmatter (`id: <page-id>`).
- **Rename immunity:** Because the sidecar filename tracks the page's frontmatter `id` rather than its filesystem path, renaming or moving a page across folders never orphans its comments.
- **Lazy assignment:** Existing pages without an `id` frontmatter field are left untouched until the first comment thread is created on them (at which point `id: <page-id>` is inserted cleanly into the page's YAML frontmatter); newly created pages from the UI/agent may assign `id:` upfront.
- **Format:** Plain Markdown with YAML frontmatter (`page:` kept updated as a human/agent-readable hint) so git, agents, and humans can read, diff, and merge threads natively:
  ```markdown
  ---
  version: 1
  id: 01jdq8z6v9k3
  page: car/maintenance.md
  ---

  ## Thread 01JDQ8Z6V9K3M2N4P5R6S7T8U9
  - session: margin-comment-01JDQ8Z6V9K3M2N4P5R6S7T8U9
  - status: open
  - created: 2026-11-24T22:41:07Z
  - updated: 2026-11-24T22:41:30Z
  - anchor.exact: "2027-04"
  - anchor.prefix: "| Cabin filter | yearly | 2026-04-07 / 155,681 | "
  - anchor.suffix: " |"
  - anchor.start: 1834

  ### you · 2026-11-24T22:41:07Z · 01JDQ8Z600000001
  Is this right given the last service?

  ### scribe · ai · 2026-11-24T22:41:30Z · 01JDQ8Z700000002
  Yes: yearly, last done 2026-04-07 (`car/log.md`).
  ```
- **Proposals** (when present on an AI message) are stored as a fenced ```` ```margin-edit ```` block with status metadata (`pending` / `applied` / `undone`) inside that message section.
- **Legacy compatibility:** On load, if the page has no frontmatter `id` (or `_osom/comments/<page-id>.md` does not exist yet), the client checks legacy `_margin/comments/<page>.json` read-only and migrates to `_osom/comments/<page-id>.md` (assigning `id:` in frontmatter) on the next save.
- **Re-attachment & merge:** Anchors use W3C `TextQuoteSelector` (`exact` + `prefix` + `suffix` + `start`), re-attaching after external edits by agents or git. Because the file is Markdown, non-overlapping edits merge via git 3-way merge; any duplicate thread/message IDs after union/rebase are deduplicated deterministically by `Sidecar::parse`.

### 5. Editor Integration Boundary
- [SPEC.md](../SPEC.md) owns all Markdown editing, selection, formatting, tables, checkboxes, and rendering rules.
- `topics-and-wiki` hands the editor a `PageSource { space, path, on_navigate }` and `CommentAgent` bound strictly to the active space, and receives `Save` commits back into the space's `Replica`.

## Code Style

Keep parsing and indexing in `crates/osom-topics` and `crates/margin-core/src/comments.rs` pure (zero filesystem or network I/O), operating on slices of file paths and `&str` contents:

```rust
/// Build the topic tree from a space's file listing.
pub fn topics(files: &[SpaceFile]) -> Vec<Topic> {
    let mut roots: Vec<Topic> = Vec::new();
    for file in files {
        let parts: Vec<&str> = file.name.split('/').collect();
        if parts.len() < 2 || is_hidden_root(parts[0]) || parts.iter().any(|p| p.starts_with('.')) {
            continue;
        }
        let folder = folder_at(&mut roots, &parts[..parts.len() - 1], "");
        // ...
    }
    sort(&mut roots);
    roots
}
```

## Testing Strategy

| Layer | Location | What it verifies |
|---|---|---|
| Topic & facet parser unit tests | `crates/osom-topics/src/tests.rs` | Folder/sub-topic/people-registry tree, hidden `_osom/` roots, frontmatter title/channel parsing, Questions Bank open/answered split, Procedures + Logs detection, shopping list rollups, space-wide backlink index & wikilink resolution |
| Markdown comment sidecar unit tests | `crates/margin-core/src/comments.rs` (`#[cfg(test)]`) | Frontmatter `id:` extraction & lazy insertion, `_osom/comments/<page-id>.md` rename immunity, Markdown round-trip (threads, anchors, messages, proposals, multiline text, special characters), anchor re-attach after external edits, legacy `_margin/*.json` fallback read, deterministic deduplication after merge |
| Link preview & navigation tests | `crates/margin-projection/src/link_preview.rs`, `crates/margin-editor/src/tests.rs` | Relative links, `[[wikilinks]]`, `#heading` jumps, path traversal blocking, clicking `#link-card-open` invokes in-app `on_navigate` callback, backlink counts |

## Boundaries

- **Always:**
  - Keep `crates/osom-topics` and `crates/margin-core/src/comments.rs` 100% pure (no I/O, no UI dependencies).
  - Preserve literal Markdown in topic pages — never reformat or rewrite untouched lines when toggling a checkbox or appending a log/question entry.
  - Keep `_osom/` and `.*` hidden from the topic tree.
  - Scope every wiki link, backlink, and comment lookup strictly to the active space (zero cross-space resolution).
- **Ask first:**
  - Changing the Markdown comment sidecar syntax once live files exist in `_osom/comments/`.
  - Relocating [SPEC.md](../SPEC.md) next to the editor source (wait for explicit housekeeping task).
- **Never:**
  - Put editor editing-surface rules here — those belong in [SPEC.md](../SPEC.md).
  - Resolve a wiki link or backlink across space boundaries in V1.
  - Delete or overwrite user-written prose inside `README.md`, `questions.md`, or procedures when updating structured rows.

## Success Criteria

1. **Topic tree & facets:** Given a space replica like `z-osom-tka`, `topics()` builds the exact topic/sub-topic/people tree, hides `_osom/` and `_template.md`, parses frontmatter + H1 titles, and hides empty facets.
2. **In-app wiki navigation:** Clicking a relative `[link](other.md)` or `[[wikilink]]` in either `MarkdownView` or the editor's `#link-card-open` opens the target page inside the active space and supports navigating Back; path traversal (`../`) outside the space is rejected.
3. **Space-wide backlinks:** Editing or adding a link from Page A to Page B updates Page B's backlink count and backlink list without network calls.
4. **Markdown comment sidecars:** Creating, replying, proposing edits, applying, and resolving comment threads round-trips cleanly through `_osom/comments/<page-id>.md` (keyed by page frontmatter `id`), survives page renames/moves without losing threads, re-attaches anchors after external edits, and reads legacy `_margin/comments/*.json` without data loss.
5. **Telegram + UI parity:** Creating a topic or adding a doc/question/procedure via either the UI or an agent commit produces the exact same on-disk schema and renders identically after sync.

## Out of Scope (Deferred to V2)

- **Cross-space people linking / federation** — V1 keeps each space's `people/` completely isolated; linking a private person note in `personal` to a shared profile in `family` without leaking is deferred to V2.
- **Interactive "Run Procedure" wizard** — V1 edits procedures as live Markdown checklists + `log.md`; a guided step-by-step run runner that auto-generates run logs is deferred to V2.
- **Graph visualization of wiki links** — V1 provides clickable links + backlink lists; visual node graphs are deferred to V2.
- **Fuzzy anchor re-attachment (diff-match-patch)** — V1 uses exact quote + prefix/suffix scoring; fuzzy matching for heavily rewritten sentences is deferred to V2.

## Open Questions

None — all V1 decisions are resolved.
