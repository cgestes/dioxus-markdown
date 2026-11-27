# Spec: Spaces & Isolation (`spaces-and-isolation`) — V1

> Module id: `spaces-and-isolation` (see [capability-map.md](capability-map.md)).
> Status: approved.

## Objective

Provide strictly isolated, offline-first **Spaces** (`Personal`, `Family`, `Project A`, `Climbing`, …) as the foundational storage, sync, and privacy boundary for **osom**.

### Why
Today's prototype mounts a single `procedures/` folder inside one monolithic `~/.zeroclaw` repository under a single agent (`luc`). As soon as a user has both private topics (`Personal`: friends, journal, private notes on people) and shared-channel topics (`Family`: car, house, dog, pregnancy; or `Project A`), a single agent and repo cannot guarantee that private data never leaks into shared Telegram/Slack groups, nor keep binary user files from bloating git history.

### User Stories (V1)
1. **Strict zero cross-space isolation:** As the owner, I can declare multiple spaces (e.g., `personal`, `family`, `climbing`) on my ZeroClaw gateway, each with its own isolated git repository, its own user-files subfolder, and its own isolated agent(s), with **zero** cross-space file or memory access in V1.
2. **Offline-first multi-space desktop sync:** As a macOS desktop user, I can read, edit, create, and delete Markdown and config files in any space while offline; changes are immediately durable on disk under `~/.config/osom/spaces/` and automatically sync when the gateway is reachable over Tailscale.
3. **Zero data loss with agent-assisted conflict resolution:** When I edit a file while an agent (or another device) edits the same file, every client edit is stored as its own git commit on its `base` first so nothing is lost. Non-overlapping text edits 3-way merge cleanly; when overlapping conflicts occur, both sides are preserved and the client/agent reconciles (rebases) the conflict cleanly rather than dropping content. Non-text JSON sidecars keep HEAD + a conflict copy merged on load. An edit always beats a concurrent delete.
4. **Markdown & config in git; user files in per-space folders:** Git stores only Markdown and config/metadata files. User binary files (PDFs, photos, chat exports, audio, etc.) live in a dedicated local folder with one subfolder per space (`<files-root>/<space-id>/`) where users drop files and agents/osom read them when needed — with no Git LFS complexity in V1.
5. **Live change notifications:** When an agent or another device commits to a space, my open desktop client learns promptly via SSE (`/api/spaces/{id}/events`) and updates its local replica without overwriting any file that still has unsent outbox edits.

## Tech Stack

- **Client (this repo — `osom` / `margin-editor-core`):** Rust (edition 2024), `ureq` 3.5 (HTTP + TLS via `native-tls`), `serde` / `serde_json`, `libc` (`flock`), unique `change_id` generation.
- **Server (`~/src/osom/zeroclaw`, branch `margin-spaces`):** Rust, `zeroclaw-spaces` (git CLI ≥ 2.43), `zeroclaw-gateway` (`axum` REST + SSE), `zeroclaw-runtime` (periodic committer & strict agent workspace boundaries), `zeroclaw-tools` (atomic `file_write` / `file_edit` + write attribution).
- **Transport & Auth:** HTTPS over Tailscale (`tailscale serve`), `Authorization: Bearer <device-token>` (Keychain in release builds, `0600` local token file under `~/.config/osom/` in dev builds, `ZEROCLAW_TOKEN` env override).

## Commands

### Client (Cargo workspace)
```sh
# Type-check both desktop binaries (0 warnings required)
cargo check --locked -p osom -p margin

# Run unit tests for spaces replica, outbox, and hub client
cargo test --locked -p osom-hub

# Run full workspace test suite
cargo test --locked --workspace
```

### Server (`~/src/osom/zeroclaw`)
```sh
# Run zeroclaw-spaces engine tests (merge rules, locking, conflict reporting)
cargo test --manifest-path ~/src/osom/zeroclaw/Cargo.toml -p zeroclaw-spaces

# Run gateway /api/spaces route tests
cargo test --manifest-path ~/src/osom/zeroclaw/Cargo.toml -p zeroclaw-gateway spaces
```

## Project Structure

### Client (this repository)
```
spec/SPEC-spaces-and-isolation.md     → This specification
docs/spaces-protocol.md               → Wire protocol & merge rules reference
crates/osom-hub/src/spaces.rs         → Offline-first Replica, durable outbox, Remote trait, multi-space store
crates/osom-hub/src/lib.rs            → Gateway HTTP/SSE client (SpaceApi, SpaceRegistry, Space, Endpoints, token store)
crates/osom/src/live.rs               → Live multi-space loading & SSE subscription bridge (consumer)
```

### Server (`~/src/osom/zeroclaw`)
```
crates/zeroclaw-spaces/src/lib.rs       → Space engine, commit/merge/history API, lock coordination
crates/zeroclaw-spaces/src/git.rs       → Git command wrappers, temp index, CAS ref updates
crates/zeroclaw-spaces/src/merge.rs     → 3-way text merge, union merge, conflict copies, edit-beats-delete
crates/zeroclaw-spaces/src/worktree.rs  → Atomic fsync + rename-swap writes and unseen-edit recovery
crates/zeroclaw-spaces/tests/spaces.rs  → Integration tests for git engine, concurrency, and merge rules
crates/zeroclaw-gateway/src/api_spaces.rs → /api/spaces REST + SSE routes
crates/zeroclaw-runtime/src/spaces_sync.rs → Periodic committer, GitHub mirror push/fetch, agent workspace isolation
```

## Interface & Protocol Contract

This module owns the boundary consumed by `topics-and-wiki`, `agents-and-chat`, `channels-and-routing`, `agenda-and-reminders`, `connectors-and-import`, and `app-shell`.

### 1. Space Configuration & Strict Isolation Boundary (Server)
Each space is declared in ZeroClaw config with its own git repo and its own user-files subfolder:
```toml
[spaces.personal]
title = "Personal"
agent = "luc"
repo = "spaces/personal"          # isolated git repository (Markdown & config only)
files_dir = "files/personal"      # isolated drop folder for user files (not in git)
path = "."                        # relative subpath within repo (default ".")

[spaces.family]
title = "Family"
agent = "family"
repo = "spaces/family"
files_dir = "files/family"
path = "."
```
- **Strict zero cross-space isolation (V1):**
  - Every space has its own git repository root, its own `files_dir` subfolder, and its own isolated agent(s) (including any read-only review agent for that space).
  - No agent in Space A may read, list, search, or write files, `files_dir`, or memory belonging to Space B.
  - Even when two spaces mention the same person (e.g., a family member in both `family` and `personal`), their files and agent memories remain completely separate.
- **One committer under one lock:** Every git mutation acquires an in-process mutex plus `flock(<git-dir>/zeroclaw-spaces.lock)`. No external autosync cron or `pull --rebase` races the working tree.

### 2. Git vs. Local User-Files Split
- **In git (synced via `/api/spaces`):** Markdown documents (`.md`), space/topic configuration files (`.toml`, `.yaml`, `.json` metadata), and comment sidecars (`_margin/comments/*.json`).
- **Outside git (`files_dir`):** User-dropped binary and large files (PDFs, photos, audio, chat exports, Apple Notes exports, etc.) live in a local directory with one subfolder per space (`<files-root>/<space-id>/`). Agents belonging to that space have read access to their space's `files_dir` only, and the desktop app can reference/open files within that space's subfolder without committing them to git.

### 3. HTTP / SSE Wire Contract (`/api/spaces`)
All routes require `Authorization: Bearer <device-token>`. Paths are relative to the space root; `..`, absolute paths, NUL bytes, and `.git` segments are rejected with `400`.

| Method & Route | Purpose |
|---|---|
| `GET /api/spaces` | List all spaces visible to this device: `[{ id, agent, title, head, error? }]` |
| `GET /api/spaces/{s}/tree` | Full file listing at HEAD: `{ head, files: [{ path, blob, size }] }` |
| `GET /api/spaces/{s}/file?path=<p>&at=<commit>` | Raw file bytes, headers `ETag: "<blob>"` and `X-Commit: <commit>` |
| `GET /api/spaces/{s}/changes?since=<commit>` | Net diff from `since` to HEAD: `{ head, full, changes: [{ path, blob: string \| null }] }` |
| `POST /api/spaces/{s}/commits` | Idempotent commit (`change_id`, `base`, `author`, `message`, `changes[]`) → `{ commit, head, merged, conflicts[] }` |
| `GET /api/spaces/{s}/history?path=<p>&limit=<n>` | Recent commits touching `path`: `[{ commit, author, time, message }]` |
| `GET /api/spaces/{s}/events` | SSE stream: initial `head` event, then `commit` events `{ head, paths, author }` |

### 4. Conflict & Rebase Semantics
1. **Durable client commit first:** On `POST /commits`, the server snapshots any dirty working-tree edits, then records the client's exact edit as commit `C` with parent `base` under `refs/spaces/changes/<change_id>`. Nothing sent by a client is ever lost.
2. **Clean merges:**
   - If `base == HEAD`, HEAD fast-forwards to `C`.
   - If `base != HEAD` and edits don't overlap, a 3-way merge creates a merge commit (`HEAD`, `C`).
   - An edit always beats a concurrent delete.
3. **Overlapping conflicts → preserve + client-agent rebase:**
   - **Markdown / text files:** Server preserves both sides (union merge) and returns `conflicts: [{ path, kind: "overlap", base, head, client_commit }]` so the exact three versions (`base`, `head`, `client_commit`) are addressable via `GET /file?path=<p>&at=<sha>`. The client agent automatically rebases/reconciles the overlapping edit against `HEAD` (keeping the original edit and concurrent changes intact) and commits the reconciled result — with every raw commit preserved in git history.
   - **Structured JSON sidecars (e.g., `_margin/comments/*.json`):** Server keeps `HEAD` and writes `<name>.conflict-<author>-<date>.json`; the client merges conflict copies deterministically with `Sidecar::merge` and deletes the copies.

### 5. Client Multi-Space Registry & Replica Contract (`crates/osom-hub`)
- **Config & local storage path:** `~/.config/osom/` (`~/.config/osom/config.json` or `topics.json`, `~/.config/osom/token-<host>` in dev builds, and replicas under `~/.config/osom/spaces/<encoded-gateway>/<encoded-space-id>/`).
- **Replica layout:** Each space directory holds `files/`, `outbox/`, `rejected/`, `state.json`, and `lock`.
- **Offline discovery:** The client caches the last known space list from `GET /api/spaces` under `~/.config/osom/spaces/<encoded-gateway>/index.json` so the app boots and opens every synced space offline with zero network calls.
- **Outbox durability:**
  - `Replica::write` and `Replica::delete` update `files/` and write a durable outbox entry (`fsync` + atomic rename) before returning.
  - Unsent edits to the same path coalesce while keeping the earliest `base`.
  - Once `sent = true` is persisted prior to HTTP POST, an entry is never rewritten — retries reuse the exact `change_id` and payload.
  - Only HTTP `200` removes an outbox entry. Permanent rejections (`400`, `413`, `422`) move to `rejected/` with their error reason so they never block later entries or disappear silently.
  - Pulls never overwrite any path that still has a pending outbox entry.
- **SSE catch-up:** A background subscriber listens to `/api/spaces/{id}/events` per active space (reconnecting with exponential backoff) and triggers `Replica::sync` when `event.head != local.head`.

## Code Style

Follow the existing pure-core + thin-I/O Rust style in `crates/osom-hub/src/spaces.rs` and `zeroclaw-spaces`:
- Domain types and merge/validation logic stay synchronous, deterministic, and testable without network or UI (`Remote` trait in `crates/osom-hub/src/spaces.rs`).
- Return `Result<T, String>` (client) or typed `thiserror::Error` (server); never panic on I/O, corrupt JSON, or network failure.
- Atomic file writes always use temp-file + `sync_all` + rename.

```rust
/// The server, seen from the device. Tests supply an in-memory fake.
pub trait Remote: Send + Sync {
    fn changes(&self, since: Option<&str>) -> Result<Changes>;
    fn file(&self, path: &str, at: &str) -> Result<Vec<u8>>;
    fn commit(&self, entry: &Outgoing) -> std::result::Result<Committed, SendError>;
}
```

## Testing Strategy

| Layer | Location | What it verifies |
|---|---|---|
| Client replica unit tests | `crates/osom-hub/src/spaces.rs` (`#[cfg(test)]`) | First sync, incremental pull, outbox coalescing, crash/restart durability, `sent` immutability, refused entries → `rejected/`, path traversal rejection, multi-space offline registry cache |
| Client hub/SSE & rebase unit tests | `crates/osom-hub/src/lib.rs` (`#[cfg(test)]`) | `~/.config/osom/` path resolution, multi-space discovery parsing, SSE event parsing & reconnect trigger, conflict-rebase handoff with `base`/`head`/`client_commit` |
| Server engine integration tests | `zeroclaw-spaces/tests/spaces.rs` | Snapshot-before-merge, idempotent `change_id`, 3-way merge, overlapping conflict report with SHAs, JSON conflict copies, edit-beats-delete, concurrent working-tree write recovery |
| Isolation boundary tests | `zeroclaw-runtime` / `zeroclaw-gateway` | Agent A (`family`) is denied reading/listing/searching git files, `files_dir`, or memory belonging to Space B (`personal`) — and vice versa |

## Boundaries

- **Always:**
  - Keep `cargo check --locked -p osom -p margin` at **0 warnings** and all tests passing before committing.
  - Validate every path component (`Component::Normal` only) on both client and server; reject traversal, absolute paths, and `.git`.
  - Write durably (temp file + `fsync` + rename) for all replica files, outbox entries, state files, and server working-tree swaps.
  - Keep the client `Replica` and `Remote` trait completely independent of Dioxus/UI so tests run headlessly and fast.
- **Ask first:**
  - Changing the `/api/spaces` wire JSON schema in a backwards-incompatible way.
  - Adding new Cargo dependencies to either repo.
  - Migrating or modifying live repositories under `~/src/osom/z-osom-tka` or the running ZeroClaw server config.
- **Never:**
  - Silently drop or overwrite unsent local edits or unseen agent working-tree edits.
  - Run `git pull --rebase` or `git push --force` on any space repository.
  - Commit bearer tokens, Keychain secrets, or personal data from `~/src/osom/z-osom-tka` into this repository.
  - Allow any agent to read or write another space's git repo, `files_dir`, or memory in V1.

## Success Criteria

1. **Multi-space discovery & offline boot:** With two spaces (`personal`, `family`) declared on ZeroClaw, the client lists both via `GET /api/spaces`, syncs independent local replicas under `~/.config/osom/spaces/`, and reopens both offline with zero network calls.
2. **Strict zero cross-space isolation verified by test:** An agent bound to `family` attempting to read a git file, `files_dir` file, or memory from `personal` is denied — and `personal`'s agent attempting to read `family` is likewise denied.
3. **Zero-loss merge + conflict handoff verified by test:** Automated tests pass for all merge cases (one-sided edit, non-overlapping 3-way merge, overlapping conflict returning addressable commit SHAs for client-agent rebase, JSON conflict copy + `Sidecar::merge`, edit-beats-delete, simultaneous creation), retry idempotency (`change_id`), and unseen working-tree edit recovery.
4. **Per-space user-files folder verified by test:** Dropping a file into `<files-root>/family/` makes it readable by `family`'s agent and inaccessible to `personal`'s agent, without entering git history.
5. **Live SSE catch-up:** A commit on the server emits an SSE `commit` event on `/api/spaces/{s}/events`, and a connected client updates its local replica automatically without clobbering any path with a pending outbox entry.

## Out of Scope (Deferred to V2)

- **Automated `z-osom-tka` split migration** — Existing `z-osom-tka` stays untouched as `personal` for now (keeping its current topics, including family topics, until split manually or via V2 promotion).
- **Interactive conflict-approval UX** — V1 auto-rebases overlapping text conflicts automatically (full history stays in git); manual diff-approval UI before committing is deferred to V2.
- **Git LFS for binary/large files** — V1 keeps only Markdown & config in git and stores user files in a per-space local drop folder; Git LFS syncing is deferred to V2.
- **Topic-to-space promotion** — Moving a topic (e.g., `personal/modular`) into its own standalone space with automated migration is deferred to V2.
- **Multi-user membership & space-scoped device tokens** — V1 is single-user across your own devices; per-user accounts and tokens restricted to a subset of spaces are deferred to V2.
- **Cross-space access grants** — V1 enforces strict zero cross-space access; explicit grants (e.g., `luc` reading `family`) are deferred to V2.
- **Live collaborative Loro sessions (Spaces Layer 2 over `/ws/spaces`)** — Deferred to V2.
- **End-to-end encryption (E2EE) & `src/vault/` unification** — No E2EE in V1; Loro's broader role outside editor undo/editing is deferred.
- **Web & mobile (iOS/Android) clients** — V1 targets macOS desktop (`Dioxus Native`) only.
- **Multi-gateway federation** — V1 connects to one self-hosted ZeroClaw gateway over Tailscale.

## Open Questions

None — all V1 decisions are resolved.
