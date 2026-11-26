# Anchored comments: the real backend

Status: implemented (2026-11-25). It builds on the anchored AI comments on `main` (`crates/margin-editor/src/comments/mod.rs`, fake backend `mock_llm_reply`) and on the Topics window (`docs/topics-design.md`), which already edits SilverBullet pages and talks to ZeroClaw.

**What shipped**

- **Server:** `scribe` exists on the agent's host. It was created with `zeroclaw agents create scribe`, then configured through the daemon (`PATCH /api/config`), because the CLI refuses agent changes while the daemon owns the config. It has `model_provider = "anthropic.default"`, `risk_profile = "readonly"` (a new `[risk_profiles.readonly] level = "readonly"`), `runtime_profile = "unbounded"`, `workspace.access = { luc = "read" }` and `read_memory_from = ["luc"]`. Its workspace `AGENTS.md` and `IDENTITY.md` hold the reply contract. The config backup is `config.toml.bak-000506-before-scribe`.
- **Choosing the backend:** every agent turn goes through one object-safe trait, `margin_core::agent::AgentChat` (`chat(ask, on, decide)`, streaming `ChatEvent`s and ending with exactly one `Done` or `Error`). `ZeroClaw` implements it, and so does `agent::Canned`, the stand-in that streams a fixed reply. The editor's `comments::CommentAgent` is an alias plus an `Arc<dyn AgentChat>`: a page opened from Topics while paired gets ZeroClaw's comment agent (alias `comment_agent` in `~/.config/margin/topics.json`, default `scribe`); unpaired, an agent that fails with "not paired"; every other notebook the local stand-in (`CommentAgent::mock`, the former `mock_llm_reply`). Mock and real replies take the same path (`worker::turn`), so the tests exercise the real code.
- **Where the code is:**
  - Core (`crates/margin-core/src/comments.rs`): anchors, comment file and merge, the turn builder (`turn`: preamble with the contract and the page on the first turn, the quote on every turn), `visible`/`edit` parsers, `Proposal::fits`.
  - Editor (`crates/margin-editor/src/comments/mod.rs`): a `CommentThread` wraps the file's `Thread` with this session's Loro cursors, the reply being written and failed replies. `post` sends your message and streams the answer into `Discussion::stream` (the comment state in `Writing::discussion`) (one bubble re-renders per chunk). `save_comments` merges and retries on 412; `use_comment_file` loads on open (absent = no file yet) and shows an error banner when the file cannot be read or written.
  - Saving: conversation changes (messages, resolve, delete) save the comment file at once. Anchors and applied/undone proposal states describe the saved page, so they are written when the page is saved (`page::Writing::save_page` → `commit_comments`).
  - A proposal applies only while the text under the thread is what it was made for (`Proposal::fits`, checked both ways, for Apply and Undo); otherwise the thread offers *Ask again*. Staleness is derived, not stored.
- **Checks:**
  - Offline unit tests cover the core, a stale proposal, and a comment file round trip after the page was edited elsewhere.
  - `osom comment-test <page> "<quote>" "<request>"` runs one real turn on a throwaway session.
  - An opt-in live editor test (`live_comment_streams_a_reply_and_saves_the_thread`) streamed a real reply and saved, then removed, the comment file.

## 1. Goal

Select text in a page from the space and ask for something ("is this date right?", "shorten", "add the garage address"). An agent that knows the space answers in the comment thread and may propose a replacement for the selection. You review it, then **Apply** or **Undo** it as today. Threads stay attached to their text across sessions, across edits by other people, and across edits the agent makes elsewhere. The demo note keeps the fake backend.

## 2. Decisions

| Question | Decision | Why |
| --- | --- | --- |
| Who answers | A second ZeroClaw agent, **`scribe`**: `readonly` risk profile, read access to `luc`'s workspace and memory | Luc runs at `level = "full"` and commits and pushes every change. In a comment session it could rewrite the file underneath the editor. Autonomy in ZeroClaw is per agent, not per session, so only a separate agent guarantees "suggest, don't touch". `readonly` allows only `file_read`, `file_list`, `memory_search`, `web_search` and `time`. |
| Who edits the page | **Margin**: one undoable Loro edit, saved with `If-Match` | This keeps today's review/Apply/Undo flow. Edits are never written behind the open editor, and a concurrent change by luc surfaces as a conflict instead of a silent overwrite. |
| Transport | `/ws/chat?agent=scribe&session_id=margin-comment-<thread id>`, through `hub::ZeroClaw::chat` | Streaming, history and approvals already work for the topic chat. One session per thread means follow-ups ("shorter", "another option") keep their context. |
| Reply format | Markdown answer, then at most one fenced `margin-edit` block | It streams naturally and is easy to strip while streaming. Markdown is never JSON-escaped. |
| Where threads live | A sidecar file per page in the space: `_margin/comments/<page>.json` | Plain files, so the agent, SilverBullet and git can all see them. Anchors don't depend on Loro history, so they survive edits made outside Margin. |
| Anchors | Loro cursors while the page is open; a text-quote selector (`exact` + `prefix` + `suffix` + last offset) when saved | Luc edits files on disk and through git, with no Loro history. A quote plus its context re-attaches reliably, as in the W3C Web Annotation `TextQuoteSelector`. |

The topic chat stays with **luc**, which acts on the space (reminders, filing, commits). Comments go to **scribe**, which suggests edits only.

## 3. Flow

```
You (editor)          Margin                           SilverBullet            ZeroClaw · scribe
 select + ⌘⇧M ──────► new thread (ULID), anchor ──────► PUT _margin/…json
 "is this right?"     user message                     (If-Match)
                      ─────────────────────────────────────────────────────────► /ws/chat turn
                                                                                 file_read car/log.md
                      ◄───────────── chunks (text up to ```margin-edit) ─────────── memory_search
                      ◄───────────── done: answer + margin-edit block ───────────
                      parse → EditProposal{old, new}
                      PUT _margin/…json ───────────────►
 Apply ─────────────► one Loro edit (undoable), proposal = applied
 Save ──────────────► PUT car/maintenance.md If-Match ─► 200, or 412 if luc changed it
```

## 4. Wire protocol

### Request (the user message of a turn)

The first turn of a thread carries the page. Later turns carry only what changed.

````markdown
Comment on `car/maintenance.md` (“Car — maintenance schedule”), section “Schedule”,
table cell: row “Cabin filter”, column “Next due”.

Selected text:
> 2027-04

Request: Is this right given the last service?

<page>
…the page's Markdown at the time of the request…
</page>
````

On a follow-up, the request repeats `Selected text:` only if the text under the anchor changed since the previous turn, adding `(changed since your last reply)`. The page is not resent.

### Reply contract

The contract is defined in scribe's `AGENTS.md`, and the first turn restates it in one line:

- Answer briefly in Markdown, using the space for facts. Read other pages if useful, and cite them as `car/log.md`.
- To propose a change, end with exactly one block that replaces **only the selected text**:

  ````
  ```margin-edit
  Shorten text
  ---
  <replacement Markdown>
  ```
  ````

  The first line is a label of three words at most. Inside a table cell the replacement is a single line.
- Never edit files. Its profile enforces this anyway.

### Parsing (pure, in core)

- `visible(streamed: &str) -> &str`: the text before the last line-initial ```` ```margin-edit ````. It also holds back any trailing partial fence (```` `` ````, ```` ```marg ````), so the fence never flashes on screen.
- `proposal(full: &str) -> Option<(label, replacement)>`: parses the last block. A malformed block leaves the reply as text only.
- `EditProposal.old` is the quote the agent saw. On **Apply**, if the text under the anchor is no longer `old`, the proposal is marked **stale** and offers *Ask again* instead of overwriting.

## 5. Sidecar file

`_margin/comments/car/maintenance.md.json`. SilverBullet refuses dot-paths, so the folder name is `_margin`. The topic model must hide `_`-prefixed roots, so it doesn't show up as a topic.

```json
{
  "version": 1,
  "page": "car/maintenance.md",
  "threads": [{
    "id": "01JDQ8Z6V9K3M2N4P5R6S7T8U9",
    "session": "margin-comment-01JDQ8Z6V9K3M2N4P5R6S7T8U9",
    "created": "2026-11-24T22:41:07Z",
    "resolved": false,
    "updated": "2026-11-24T22:41:30Z",
    "anchor": { "exact": "2027-04", "prefix": "| Cabin filter | yearly | 2026-04-07 / 155,681 | ", "suffix": " |", "start": 1834 },
    "messages": [
      { "id": "01JDQ8Z6…", "author": "you", "at": "2026-11-24T22:41:07Z", "body": "Is this right given the last service?" },
      { "id": "01JDQ8Z7…", "author": "scribe", "ai": true, "at": "2026-11-24T22:41:30Z",
        "body": "Yes: yearly, last done 2026-04-07 (`car/log.md`).",
        "proposal": null }
    ]
  }]
}
```

- **Write:** after each finished message, resolve, apply or delete, never per chunk. Each write is a `PUT` with `If-Match`.
- **On 412, merge and retry:** union threads and messages by id, since messages are append-only. For `resolved` and a proposal's `state`, the later `updated` wins.
- **Re-attach on open:**
  1. Find every occurrence of `exact`.
  2. Score each by how much of `prefix`/`suffix` matches.
  3. Break ties by distance to `start`.
  4. With no match, the thread is **detached**. It stays listed with its quote, and selecting text re-attaches it.
  5. Fuzzy matching (diff-match-patch) can come later.
- The agent can read these files, and luc could write them. That opens agent-authored review threads later (§8).

## 6. Code

| Place | What |
| --- | --- |
| `crates/margin-core/src/comments.rs` (core, pure, tested) | Thread, message and proposal model (serde); sidecar merge; `TextQuote` capture and resolve on `&str`; request builder; `visible` and `proposal` parsers |
| `crates/osom-hub/src/lib.rs` | Reuse `ZeroClaw::chat` and `Space::read/write`; add `abort(session)` (`POST /api/sessions/{id}/abort`) for a **Stop** button |
| `crates/margin-editor/src/comments/mod.rs` (from `main`) | Replace the two `mock_llm_reply` call sites with `CommentAgent::reply`, run on a worker thread like the topic chat. Stream into a pending AI message. Load the sidecar when a vault page opens and save it on change. |
| `hub::AgentChat` (as built) | `trait AgentChat { fn chat(&self, ask: &Ask, on: &mut dyn FnMut(ChatEvent) -> bool, decide: &mut dyn FnMut() -> bool) }`, events `Chunk / Tool / Approval / Done(full) / Error`, exactly one of the last two. Implemented by `ZeroClaw` and `Canned` (the stand-in). Chosen per notebook through `NotebookOptions::comments`: vault page + paired → scribe; otherwise the stand-in. `on` returning false stops the turn. |

Server side, one config block plus scribe's own workspace files:

```toml
[agents.scribe]
model_provider = "anthropic.default"   # or a faster model for inline edits
risk_profile = "readonly"
runtime_profile = "unbounded"
channels = []                           # gateway only

[agents.scribe.workspace]
access = { luc = "read" }               # read luc's pages
read_memory_from = ["luc"]              # and luc's memory

[risk_profiles.readonly]
level = "readonly"
```

Then add scribe's `AGENTS.md` with the reply contract. Check the exact `access` spelling with `zeroclaw config` before relying on it.

## 7. Build order

1. **Core model:** `crates/margin-core/src/comments.rs` with anchors, parsers and sidecar merge, plus unit tests (quote re-attach after edits, fence hidden while streaming, a malformed block, merge after a 412).
2. **`CommentAgent` with `MockAgent`:** the fake backend becomes pluggable. No UX change.
3. **Persistence:** sidecar load and save for vault pages, detached threads, a stale-proposal state.
4. **Scribe:** server config and `AGENTS.md`, then `ScribeAgent` with streaming, **Stop** and tool names ("reading car/log.md…").
5. **Later:** agent-authored threads (§8), fuzzy re-attach, autosave after **Apply**.

## 8. Later: the agent starts threads

Because threads are files, the review can also start from another channel. For example, "luc, review `wedding/README.md`" sent on Telegram, or luc's question bank anchoring an open question to the sentence it is about. The agent writes threads, with proposals, into the sidecar, and they appear the next time the page opens. Margin needs no change beyond rendering `author: luc`.

## 9. Open questions

- **Separate agent:** a separate `scribe` agent (recommended), or luc with a "never edit" instruction and no hard guarantee?
- **Model:** scribe could use a faster or cheaper model, since inline edits are short.
- **Git:** should comment files be committed with the space? Luc stages only the files it changed, so today they would stay untracked. The options are to add `_margin/` to `.gitignore`, or to let luc commit them.
- **Resolved threads:** keep them in the file (history), or prune after N days?
- **SilverBullet:** should comments be visible there too? A Space Lua widget could render the sidecar.
