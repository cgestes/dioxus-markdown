# Crate coherence: audit and plan

Status: in progress, 2026-11-26. Phase 1 is done; the decisions below are taken.

The question: does each crate provide one thing, contain only what serves it, and expose an interface that matches? Eight of the thirteen crates do. Four do not, and one has a duplicated concept. This document lists every finding, the layout to aim for, and the steps to get there.

## Verdict

| Crate | Coherent? | Main issue |
| --- | --- | --- |
| `margin-editor-core` (repo root) | No | Several unrelated things under an "editor core" name |
| `margin-editor` | No | Also serves as the app runtime; `NoteStore` is too wide |
| `margin-hub` | No | It is Osom's network layer under a Margin name |
| `margin-note` | No | `NoteSync` exposes a sync protocol nobody calls |
| `margin-projection` | Mostly | Two different "block" models |
| `margin-vault` | Mostly | Two methods named after the desktop app |
| `margin-ui` | Yes | Two helpers that belong to the editor |
| `osom-topics` | Yes | One 1,228-line file |
| `osom` | Yes | Everything nested under `osom::topics` |
| `margin`, `margin-web`, `margin-vault-cli`, `workspace-hack` | Yes | Leftovers only |

## Findings

Each finding has an id; the plan refers to them, and the last table maps every id to a step.

### `margin-editor-core`

- **C1. Name, doc and place.** The crate doc says "Markdown editing commands". The crate holds Markdown editing, the comment file, the agent protocol, two service interfaces and dates. Osom's model and the hub depend on an "editor core" for things that are not editing. It is the only crate at the repo root.
- **C2. Topics dates.** The editor uses `dates::now_ms` and `dates::stamp`. `chip`, `ago`, `long_date`, `day_month` and `parse_time` are called only by Osom: they format dates for the Topics window.
- **C3. Two `Ask` types, one module with two jobs.** `agent::Ask` is a message to an agent. `comments::Ask` is one comment turn as the agent receives it. `comments` (555 lines) holds both the comment file format and the prompt protocol for asking an agent about a comment.
- **C4. Projections in the core.** `preview` is the reading projection: it emits the CSS class names the stylesheet matches. `syntax` is the code highlighter: it brings the workspace's only optional feature (`syntax-highlighting`) and its heaviest optional dependency. Both are projections of Markdown, and `margin-projection` is the crate for those. The feature has to be switched on by another crate so that every build agrees, which is fragile.
- **C5. `table::heading_above`** finds the heading above an offset. It is a heading function in the table module.
- **C6. Dead code.** `Passage::source_at` is called only by its own tests. It served the removed facing-page preview, which the module doc still names.

### `margin-editor`

- **E1. The app runtime lives here.** Osom's window calls `margin_editor::launch`, `support::worker` (threads and agent turns) and `support::restart` (restart into a newer build). None of that is editing. The macOS launch code also reads two editor markers from the page (`data-range-keys`, ids starting with `cell-`).
- **E2. `support::page`** is the reverse: editor internals (`impl Writing { save_page }`) inside the public `support` module.
- **E3. `NoteStore` is the host interface, not a store.** Its eight methods cover four concerns: where the note lives (`note`, `use_persistence`, `page`), where comments are saved (`threads`, `save_discussion`), the host's window (`title`, `menu`), and where an inspector report goes (`attach_report`).
- **E4. `Note` leaks.** An app needs `margin-note` only to name `Note` in `NoteStore::use_persistence`. Osom depends on it for that one signature.
- **E5. Rich cannot be asked for.** `View` has `Live` and `Source`. Rich is switched on when `host` is set.
- **E6. A dev dependency on the hub** for one live test (`live_comment_streams_a_reply_and_saves_the_thread`).
- **E7. Stale doc.** `testing.rs` tells other crates to enable a `testing` feature that no longer exists.

### `margin-hub`

- **H1. It is Osom's.** It returns `osom-topics` types (`Session`, `Message`, `SpaceFile`). Only Osom uses it, apart from the editor's live test.
- **H2. Dead browser support.** About twenty `cfg(not(target_arch = "wasm32"))` gates and a set of browser stubs remain. No browser build includes the hub any more.
- **H3. A conflict is a magic string.** `Space::write` returns the text `CONFLICT` as its error; the `PageFiles` implementation compares strings to produce `WriteError::Conflict`.

### `margin-note`

- **N1. `NoteSync` has no caller.** It exposes a sync protocol (`ingest`, `export_spans`, `next_demand`, `rebase_onto_epoch`, `rebuild_from_checkpoint` and more). Nothing outside the crate calls it. `margin --sync-file` uses only `load_from_file`, `save_to_file` and `note`. The vault has its own sync. The protocol implements the recovery studied in `docs/loro-history-results.md`.
- **N2. A note's layout is written twice.** `Note::new` and `Vault::new_note` each build the Loro document, and the vault's adds a `format` marker the other lacks.

### `margin-projection`

- **P1. Two "block" models.** `document::blocks` gives the headings and tables that split the body into fields. `blocks::split_blocks` gives the top-level blocks the drag handles move. Same word, different things.
- **P2. `link_preview` says "space".** `space_md_path`, `space_cache_key` and `OpenTarget::SpacePage` use Osom's word for what the editor elsewhere calls a page on a server.

### `margin-vault`

- **V1. App names.** `open_desktop_note` and `field_notes_room` are named after the desktop app and its sample note.
- **V2. The relay server ships with the client.** `margin --features vault` compiles it. This is documented and stays as it is.

### `margin-ui`

- **U1. Editor helpers.** `typefaces::prose_width` exists so Rich mode can fit a styled run over a field. `notebook_fonts` is named after the notebook, and the gallery and Osom use it too.
- **U2. Two ways to import Dioxus.** The editor uses `margin_ui::prelude`, which picks the renderer. Osom and `margin` import `dioxus_native::prelude` directly.

### `osom-topics`, `osom`, `margin-vault-cli`

- **T1.** `osom-topics` is one file of 1,228 lines covering topics, routes, reminders, questions, sessions and transcripts.
- **T2.** It re-exports the core's heading functions as its own (`h1`, `page_title`, `split_heading`).
- **O1.** `osom` nests everything under `osom::topics`, a leftover from when Topics was a module of Margin.
- **K1.** `margin-vault-cli` keeps a `fn main() {}` for wasm and a comment about "the editor library", from when it lived in the root crate.

### Across crates

- **X1. Same name, different thing.** `Message` exists in the core's comments, in `osom-topics` and in `margin-ui`. `Thread` and `Turn` each exist in the core's comments and in `osom-topics`.
- **X2. Stale comments.** `src/comments.rs` (`topics::transcript`), `osom-topics` (`hub`), `osom/src/topics/live.rs` (`topics_view`), `osom/src/topics/view.rs` (`topics_demo`), `docs/design-system.md` (`ui/`).

## Target layout

| Crate | Provides | Uses |
| --- | --- | --- |
| `margin-core` (was the root crate) | The shared domain, no I/O and no UI: Markdown source edits (`callout`, `checkbox`, `edit`, `format`, `heading`, `table`), the comment file, the request an agent gets about a comment, the agent and page-file interfaces, the clock and ISO dates | none |
| `margin-ui` | The design system | none |
| `margin-note` | `Note`, a Loro document | core |
| `margin-projection` | Both projections of Markdown: editing and reading, with code highlighting | core |
| `osom-topics` | Osom's model, with its date formatting | core, projection |
| `margin-runtime` (new) | What an app runs around its UI: the window (`launch`), background work and agent turns (`worker`), restart into a newer build (`restart`) | core, ui |
| `margin-vault` | The encrypted vault | note |
| `osom-hub` (was `margin-hub`) | Osom's network access: the space and the agent | core, osom-topics |
| `margin-editor` | The notebook, its comments, Markdown to read, the test driver | core, note, projection, ui, runtime |
| `margin`, `margin-web` | The notebook apps | editor, ui, runtime, vault (optional) |
| `osom` | The Topics window | editor, ui, runtime, osom-topics, osom-hub |
| `margin-vault-cli` | The vault from the terminal | vault, note |

What changes in the dependency graph: the core loses its optional feature and the highlighter; `osom-topics` gains `margin-projection`; the editor loses the hub entirely; a new small crate sits between the design system and the editor.

## Plan

Every step is one commit and leaves the workspace building and green. Sizes: S is under an hour, M a few hours, L a day.

### Phase 1: clean up, no interface change (done)

| Step | Change | Findings | Size |
| --- | --- | --- | --- |
| 1.1 | Remove the hub's browser gates and stubs | H2 | S |
| 1.2 | Remove the wasm `main` and stale comment from `margin-vault-cli` | K1 | S |
| 1.3 | Remove `Passage::source_at` and anything only it needs | C6 | S |
| 1.4 | Correct the stale doc comments and the core's crate doc | X2, E7, C1 (doc) | S |

### Phase 2: move each thing to the crate that owns it

| Step | Change | Findings | Size |
| --- | --- | --- | --- |
| 2.1 | Move the Topics date forms to `osom-topics`. The core keeps the clock, civil dates, RFC 3339 and ISO dates. | C2 | S |
| 2.2 | Move `heading_above` to `heading` | C5 | S |
| 2.3 | Split `comments` into the file format and the agent request; rename the comment `Ask` (proposed: `comments::Request`) | C3 | M |
| 2.4 | `Space::write` returns `WriteError`; delete the `CONFLICT` comparison | H3 | S |
| 2.5 | `Note` gets the one constructor that writes a note's layout; `Vault::new_note` uses it. Decide whether the `format` marker is part of the layout. | N2 | S |
| 2.6 | Rename the vault's two app-named methods to what they do (resume the last note or start the first) | V1 | S |
| 2.7 | Move `prose_width` to the editor's platform text module; rename `notebook_fonts` to `fonts`; import Dioxus through `margin_ui::prelude` in Osom and `margin` | U1, U2 | S |
| 2.8 | Rename `document::Block` and `blocks()` to what they are (proposed: `Landmark`, `landmarks`); use "page" in `link_preview` | P1, P2 | M |
| 2.9 | Flatten `osom::topics` into `osom`; split `osom-topics` into modules by concept; drop its heading re-export | O1, T1, T2 | M |

### Phase 3: the editor's interface

| Step | Change | Findings | Size |
| --- | --- | --- | --- |
| 3.1 | Move `support::page` into `notebook`; the crate root keeps exporting `PageSource` | E2 | S |
| 3.2 | Split `NoteStore`. It keeps where the note and its discussion live (`note`, `use_persistence`, `threads`, `save_discussion`, `page`). The window title, the app's menu button, the embedded bar and the report destination move to one `Host` value in `NotebookOptions`. Re-export `Note` from the editor. | E3, E4 | M |
| 3.3 | `View` becomes `Rich`, `Live`, `Source`. Osom asks for `Rich`; `host` no longer implies it. | E5 | S |
| 3.4 | Move the live comment test to `osom`, which already has the hub and an `osom comment-test` command. The editor drops its dev dependency. | E6 | S |

### Phase 4: crate boundaries

These touch the most files. Do them when no other branch is in flight.

| Step | Change | Findings | Size |
| --- | --- | --- | --- |
| 4.1 | Move `preview` (as `reading`) and `syntax` into `margin-projection`. Highlighting is chosen by target (native yes, browser no) instead of a feature. `osom-topics` depends on `margin-projection` for `tasks`. The core drops `two-face` and the feature. | C4 | M |
| 4.2 | New crate `margin-runtime`: `launch`, `worker`, `restart`. The macOS key bridge moves with `launch`; its two editor markers become attributes the crate documents. The editor uses the crate for `worker` and `restart`; apps call `launch`. The editor drops `blitz-shell` and `dioxus-devtools`. | E1 | L |
| 4.3 | Rename `margin-hub` to `osom-hub` | H1 | S |
| 4.4 | Move the root crate to `crates/margin-core` and rename it. The root `Cargo.toml` becomes a workspace manifest only. 50 references in 27 files and 6 manifests change. | C1 | M |

### Phase 5: last

| Step | Change | Findings | Size |
| --- | --- | --- | --- |
| 5.1 | Remove `NoteSync`'s protocol and keep a small note file for `--sync-file`. It can be rebuilt if the vault needs it. | N1 | M |
| 5.2 | Optional renames for the shared names (`Message`, `Thread`, `Turn`) | X1 | M |

## Checks after every step

1. `cargo check --locked --workspace --all-targets`, with default features and with `--all-features`: no warnings.
2. `cargo check --target wasm32-unknown-unknown` for `margin-projection`, `margin-editor`, `margin-web`.
3. `cargo test --locked --workspace`, with default features and with `--all-features`.
4. `scripts/test-editor --headless`.
5. The two browser suites (`npm --prefix mockup run test:web`, `test:web:offline`) after any step that touches the editor, the design system or `margin-web`.
6. After any manifest change: the app builds and the test builds still resolve every shared crate to the same features (compare `cargo tree -e features` across `-p margin`, `-p osom`, `-p margin-editor`, `--workspace`), and `workspace-hack` is refreshed.
7. After phase 4: time a rebuild after an edit in each crate and compare with today (a core edit rebuilds all four binaries in about 4 s).
8. Update the README crate map and `docs/design-system.md` in the step that changes them.

One check fails today for a reason outside this plan: `test:web` stops at the phone column handle (`#col-menu-btn-0-2`), since commit `b4c2cc3` removed the handle shown after a tap. Until that is fixed or the test is changed, the gate for check 5 is "no new failure".

## Decisions

| # | Decision | Taken |
| --- | --- | --- |
| D1 | Rename and move the root crate to `crates/margin-core` (step 4.4)? | Yes: `margin-core`, as the last boundary step, a rename with no other change |
| D2 | `NoteSync`'s protocol: keep for the vault's recovery, or remove (step 5.1)? | Remove; rebuild it if it is needed |
| D3 | Name of the new runtime crate (step 4.2) | `margin-runtime` (default) |
| D4 | Rename the hub to `osom-hub` (step 4.3)? | Yes (default) |
| D5 | Highlighting chosen by target instead of a Cargo feature (step 4.1)? | Yes (default): no build can then disagree about it |
| D6 | Rename the shared names (step 5.2)? | Only the two `Ask` types (step 2.3); the rest stay (default) |

## What stays as it is

- The relay server in `margin-vault` (V2).
- `margin` and `margin-web` each add the gallery's Markdown section, about ten lines each.
- Components in `margin-ui` that only Osom uses today (`Tile`, `DateBox`, `NavItem`, `ChannelGlyph`): they are design-system components.

## Every finding and its step

| Finding | Step | Finding | Step | Finding | Step |
| --- | --- | --- | --- | --- | --- |
| C1 | 1.4, 4.4 | E3 | 3.2 | P1 | 2.8 |
| C2 | 2.1 | E4 | 3.2 | P2 | 2.8 |
| C3 | 2.3 | E5 | 3.3 | V1 | 2.6 |
| C4 | 4.1 | E6 | 3.4 | V2 | none (stays) |
| C5 | 2.2 | E7 | 1.4 | U1 | 2.7 |
| C6 | 1.3 | H1 | 4.3 | U2 | 2.7 |
| E1 | 4.2 | H2 | 1.1 | T1, T2 | 2.9 |
| E2 | 3.1 | H3 | 2.4 | O1 | 2.9 |
| N1 | 5.1 | N2 | 2.5 | K1 | 1.2 |
| X1 | 5.2 | X2 | 1.4 | | |
