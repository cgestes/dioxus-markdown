# Loro is the document model in every build

Status: accepted, 2026-11-26.

## Context

Margin and Osom edit Markdown notes and pages. Most of the time one person is typing, and Osom reads and writes pages as files. A plain `String` with its own undo stack would cover that single-writer case. It would also drop about 100 crates from every build, and `loro-internal` is the last dependency to finish in a clean build.

But you are not the only writer. Agents (Claude, the ZeroClaw agent) edit the same notes and pages, and paired devices edit the same vault notes.

## Decision

`Note` (`crates/margin-note/src/note.rs`) stays a Loro document in every build of Margin and Osom. There is no plain-text alternative.

What Loro gives the editor:

- **Merging, not rejecting.** Your edits and an agent's edits to the same note can merge as two writers, the way paired devices already merge in the vault.
- **Undo and redo** that restore the selection, and stay correct while another writer's changes arrive.
- **Comment anchors** (Loro cursors) that follow the text through edits made by anyone.
- **Checkbox toggles** stored as a mark on the existing character, so two writers toggling at once never duplicate it.
- **One format** shared with the encrypted vault's sync and the browser study (Loro in JavaScript).

## Consequences

- `margin-note` and Loro are always compiled. Build speed comes from the crate layering instead (see the crate map in the README).
- The encrypted vault is a separate choice from the document model. It is storage and device sync, built only with `margin`'s `vault` feature. The editor (`margin-editor`) asks the app where a note lives through its `NoteStore` interface.
- Next step this enables: Osom can bring an agent's change to an open page in as a second writer and merge it, instead of rejecting the save when the page's etag has changed.
