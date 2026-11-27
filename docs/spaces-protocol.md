# Spaces: offline-first topics on ZeroClaw and git

Status: phase 1 server implemented in the fork (branch `margin-spaces`), 2026-11-25. It replaces SilverBullet's `/.fs` and the margin-hub plan in [topics-design.md](topics-design.md). The server side goes in the ZeroClaw fork (`cgestes/zeroclaw`, branch `margin-spaces`).

Goals:
- **Offline first.**
- **Nothing is ever lost.**
- **Many kinds of client:** Margin on Mac, web and phone, a CLI, scripts, bots and agents.

## 1. What we start from

- **The vault** is `procedures/` in luc's workspace. All of `~/.zeroclaw` is one git repo, pushed to a private GitHub repo.
- **Three committers race on it with no lock:**
  - the `zeroclaw-autosync` cron job;
  - `commit-notes.timer`;
  - luc's own "commit + push".

  Two of them run `pull --rebase --autostash` over files that are being edited.
- **Loss already happened:** commit `c503f78` says "README wiped by disk-full". ZeroClaw's `file_edit` writes in place (`tokio::fs::write`).
- **The upstream gateway** reads, lists, moves and deletes workspace files. It has no way to write a file and no change feed.

## 2. Decisions

1. **Git is the source of truth and the history.** Every edit ends up in a commit.
2. **A client edit is stored as its own commit before anything is merged.** Its parent is the commit the client started from. The exact text the client sent is therefore always in history, whatever the merge does.
3. **Offline first.** A device keeps its own copy of each space and an outbox. An edit leaves the outbox only when the server answers with the commit that contains it.
4. **Merges never drop content.** On overlapping edits both sides are kept, and the overlap is reported. A modified file survives a concurrent delete.
5. **One committer, under one lock,** is the only thing that runs git on the repo. It commits the agents' file edits too.
6. **Live editing uses short Loro sessions** (layer 2, §7). A session's changes become one commit, and its Loro history is then dropped.
7. **One protocol for every client:** HTTPS, JSON and SSE with the device token, plus a WebSocket for live sessions.

## 3. Agents, spaces and topics

**An agent is a trust boundary.** It has its own:
- workspace and memory;
- risk profile (tools and commands);
- model;
- channels: who can talk to it.

One agent already runs many concurrent sessions over one workspace, and the committer handles that concurrency whatever the number of agents. The reason to split agents is privacy. An agent can reach everything in its workspace and memory from any conversation, so we use **one agent per audience**:

| Agent | Who talks to it | Space it owns | Grants |
|---|---|---|---|
| `luc` | only you | `me` | `read_write` on `family` |
| `family` (new) | groups with Nikki | `family` | none on `me`; no shell or code tools |
| `scribe` | Margin comments | none | `read` |
| `work` (if Slack is for work) | Slack | `work` | none |

**A space** is a folder of topics in its owner agent's workspace, declared as `[spaces.<id>] agent = "luc", path = "procedures"`.

**A topic** is a folder with a `README.md`. Its front matter can set `title`, `agent` (who answers the topic chat) and `channels`. Until it does, clients fall back to the README's "Channel → folder" table.

**Devices:**
- v1: every paired device sees every space.
- v2: device tokens limited to some spaces.

## 4. Git layer: protocol

All calls use `Authorization: Bearer <device token>`. `{s}` is a space id, and paths are relative to the space.

| Call | Result |
|---|---|
| `GET /api/spaces` | `[{id, agent, title, head}]` |
| `GET /api/spaces/{s}/tree` | `{head, files: [{path, blob, size}]}` |
| `GET /api/spaces/{s}/file?path=<p>&at=<commit>` | raw bytes, with `ETag: "<blob>"` and `X-Commit: <commit>`. Without `at`, the file at HEAD. |
| `GET /api/spaces/{s}/changes?since=<commit>` | `{head, full, changes: [{path, blob or null}]}`. This is the net difference between `since` and HEAD. When `since` is unknown, `full` is true and the list holds every file. |
| `POST /api/spaces/{s}/commits` | send an edit (below) |
| `GET /api/spaces/{s}/history?path=<p>&limit=<n>` | commits that touched the file: `[{commit, author, time, message}]` |
| `GET /api/spaces/{s}/events` | SSE. A `head` event comes first, then one `commit` event per commit that touched the space: `{head, paths, author}`. |

Reads first commit any agent edits waiting in the space, so a client always sees what the agents wrote. Commit bodies may be up to 32 MiB; the gateway's 64 KB control-plane limit doesn't apply here.

**Sending an edit:**

```json
POST /api/spaces/home/commits
{
  "change_id": "01JD…",               // client-generated, unique; makes retries safe
  "base": "<commit the client started from>",
  "author": "Margin (MacBook)",
  "message": "Edit car/README.md",
  "changes": [
    {"path": "car/README.md", "content": "…"},               // text, UTF-8
    {"path": "car/photo.jpg", "content_b64": "…"},
    {"path": "old.md", "delete": true}
  ]
}
→ 200 {"commit": "<the client's own commit>", "head": "<new HEAD>",
       "merged": true, "conflicts": [{"path": "car/README.md", "kept": "both"}]}
```

`conflicts` lists the files where an overlapping edit was merged. Margin flags those spots with an anchored comment.

**Server steps, all under the lock:**

1. If `change_id` was seen before, answer as the first time did.
2. Commit any uncommitted edits in the working tree (the agents' edits) as their own commits, so they're never overwritten.
3. Build the client commit `C`: the `base` tree plus `changes`, with parent `base`. Record it under `refs/spaces/changes/<change_id>`. The client's text is now durable.
4. If `base` is HEAD, HEAD moves to `C`. Otherwise the server merges `C` into HEAD with the §5 rules and creates a merge commit with parents `HEAD` and `C`.
5. It updates the working tree for the files that changed (§6) and moves HEAD with a compare-and-swap.
6. It publishes the event, answers the client, and schedules a push to GitHub.

**Client rules:**
- Edits go to the device's copy and to the outbox at once, written durably.
- The outbox is sent in order. Unsent edits to the same file are combined, keeping the earliest base.
- An entry leaves the outbox only on `200`. A retry reuses its `change_id`.
- After the outbox is flushed, the client applies `changes?since=` to every file with no pending entry.
- The client follows `events` to know when to sync. Browsers read SSE through `fetch()`.

## 5. Merge rules

These apply per file, between the base, HEAD and the incoming version.

| Case | Result |
|---|---|
| Only one side changed the file | that side |
| Both changed it, text file (`.md`, `.txt`, …), no overlap | ordinary 3-way merge |
| Both changed it, text, overlapping | `git merge-file --union`: both sides' lines are kept, and the file is listed in `conflicts` |
| Both changed it, any other file (JSON, binary) | HEAD's version stays at the path, and the incoming one is saved as `<name>.conflict-<author>-<date>.<ext>` |
| One side deleted it, the other changed it | the changed version is kept; an edit beats a delete |
| Both created it with different content | merged as text with an empty base, or as a conflict copy |

Margin's comment threads (`.margin/comments/*.json`) are JSON. Margin merges their conflict copies with `Sidecar::merge`, then deletes the copies.

## 6. Working tree and the agents

Agents keep editing files on disk with their tools and shell.

- **Snapshot first.** Before any client commit or merge, the committer commits the dirty files (step 2 of §4).
  - Authorship comes from the fork's file tools, which record the last agent and session to write each path.
  - Anything else counts as `external`.
- **Writing a file into the working tree:**
  1. Write a temporary file and `fsync` it.
  2. Swap it in with `renameat2(RENAME_EXCHANGE)` on Linux (`renamex_np(RENAME_SWAP)` on macOS).
  3. Read the file that was swapped out. If it isn't the blob the server expected, an agent or a shell wrote in the meantime. Commit that content as an `external` edit and merge again.

  A file is never truncated in place, and free disk space is checked first.
- **`file_edit`** becomes atomic, using the same temp-file-and-rename as `file_write`.
- **The periodic committer** replaces four things:
  - the autosync cron job;
  - `commit-notes.timer`;
  - luc's "commit + push" (luc gets a `commit_note` tool for its message);
  - `command_hook` used for git.

  It commits after a short quiet period, and at least every 30 minutes.
- **The lock** is an in-process mutex per repo, plus `flock` on `.git/zeroclaw-commit.lock` so scripts and `zeroclaw git -- <args>` wait their turn.
- **GitHub is a mirror.** The server pushes after commits, retrying with backoff, and never force-pushes. Commits pushed by laptops are fetched and merged under the §5 rules, as if they were client commits whose base is the merge base.

## 7. Layer 2: live Loro sessions

**A session** is one document being edited live: devices that have it open, plus agents.
- It starts from a commit `B`.
- Its initial Loro state is deterministic: one insert of the text at `B`, made by a peer id derived from `B` and the path. Any device or server can rebuild it exactly from git.
- Devices exchange Loro changes through the server over `wss://<host>/ws/spaces`. The server stores each change durably before it confirms.
- An agent's edit to the file on disk is diffed against the session's text and applied as Loro changes by that agent.
- A client using the git layer with base `B` has its edit applied as changes made at the initial state.

**Closing a session:** when no device has the document open and it's been quiet for a minute or two, or an agent closes it, or 30 minutes pass, the server commits the text as a client commit (§4) with every participant as co-author. Then it drops the Loro history.

**A device that comes back after its session closed** rebuilds the initial state from `B`, applies its own changes, and sends the resulting text as an ordinary git-layer edit with base `B`. The §5 merge handles it.

Devices that don't have the document open simply catch up through git.

## 8. Why nothing is lost

| Case | What keeps the edit |
|---|---|
| Editing offline for days | the durable outbox, kept until the server answers with a commit |
| A retry after a network failure | `change_id`: the server answers as the first time and never duplicates |
| Two devices and an agent edit the same file | each edit is its own commit first; the merge keeps both sides and flags the overlap |
| An edit on one device, a delete on another | an edit beats a delete; the deleted version stays in history |
| An agent's or a shell edit while a client commit lands | snapshot first, then swap and verify on write |
| Disk full | atomic swap, never truncate; free-space check |
| Crash in the middle of a commit | the client commit is under its ref before the merge; HEAD moves only by compare-and-swap |
| Two committers | there's one, under a lock |
| Commits pushed from a laptop | merged as client commits, never rebased over the live tree |
| The server lost | GitHub mirror, plus every device's own copy |

## 9. Work

**Phase 1, fork:**
1. `spaces` in `zeroclaw-runtime`: git operations, merge rules, lock, committer, safe working-tree writes.
2. Atomic `file_edit`.
3. `/api/spaces` routes in the gateway.
4. Config `[spaces.<id>]`.

**Phase 1, Margin:**
1. A copy of each space kept on the device.
2. The outbox and sync.
3. `hub::Space` over `/api/spaces`.
4. Topics read from the device's copy.
5. Removing SilverBullet.

**Phase 1, server cutover:**
1. Deploy the fork.
2. Declare the `home` space on `procedures/`.
3. Remove the autosync cron job and `commit-notes.timer`.
4. Update AGENTS.md.
5. Stop SilverBullet.

**Phase 2:**
1. Loro sessions.
2. Agents per audience and space-limited device tokens.

## 10. Open questions

1. The agent split in §3: is Slack for work? Which topics go into `family`?
2. One repo for all of `~/.zeroclaw`, or one repo per space? Per space matches the access boundaries.
