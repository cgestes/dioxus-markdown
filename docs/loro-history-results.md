# Recovering an offline Loro edit from encrypted history

An earlier **shallow** checkpoint can recover an edit that the current shallow checkpoint rejects. In the tested fork, the older checkpoint was a common ancestor. A fresh replica opened it, replayed the operations needed to reach both writers, and recovered the same Markdown, rich-text marks, moved list items, version vector, frontiers and comment positions as an always-full replica. Both offline changes kept their original IDs. The recovered document still had the earlier shallow root. [Standalone Rust reproducer](../tests/loro_history.rs) · [JavaScript tests](../mockup/test/loro-history-recovery.test.mjs)

A second case found a failure that comparing text and versions would miss. A comment pointed inside the deleted words “old draft.” The latest shallow replica could merge the new edits, yet could no longer resolve that cursor. Rebuilding from a checkpoint where those words still existed, then replaying the deletion, recovered its reference offset and side. [Upstream cursor lookup](https://github.com/loro-dev/loro/blob/934563cfb4117373f237bc8b282533eacb17207d/crates/loro-internal/src/loro.rs#L4630-L4685)

## How the device recovers

1. Keep the original encrypted incoming record on disk before import. Loro parks ordinary missing dependencies. It can reject an edit based before a shallow root and leave its pending queue empty.
2. Read the outstanding update’s external frontier and partial starting counters. Compare with integrated writer counters and request the first missing operation. The archive returns the entire independently encrypted record covering that counter. The device verifies its authentication tag, writer, range and embedded Loro metadata before import. Repeat using the new pending state. This worked for a multi-writer batch and disjoint ranges with a gap inside one writer’s history.
3. If an edit or required cursor needs older history, open a fresh replica from an authenticated earlier checkpoint. Restore the local branch, rejected input, pending updates and intervening ranges. The adapter checks that every required cursor resolves and that the replacement includes all local edits. The benchmark also compares each cursor’s offset and side with the full-history reference. A test inserted a local edit during an outstanding download and recovered it. Another killed the client process during its second request and resumed using the first verified record from disk.
4. If the retained roots or ranges are insufficient, try a full checkpoint. If required history is absent from every source, return a recovery error and keep the existing document and encrypted input available for retry. [Recovery adapter](../mockup/experiments/loro-history/recover.mjs) · [Upstream shallow-root guarantees](https://github.com/loro-dev/loro/blob/934563cfb4117373f237bc8b282533eacb17207d/docs/shallow-snapshot-concurrency.md)

## Measured transfers

The two strategies start with the same cached shallow checkpoint, offline local edit and incoming encrypted record. Each must reach the same recorded reference state. The fixture has 512 KiB of deterministic text written and deleted before the older checkpoint. The “visible text” control keeps it live. The archive runs in a separate loopback HTTP process with ciphertext and public span headers. It receives no document key or comment cursor.

| Cached-device recovery | Range path | Range bytes / HTTP requests | Full bytes / HTTP requests | Loopback ms, range / full | Peak client MiB, range / full |
| --- | --- | ---: | ---: | ---: | ---: |
| Edit after the current root | Current | 493 / 3 | 525,503 / 1 | 79 / 41 | 83.2 / 74.9 |
| Offline fork before the current root | Earlier shallow | 1,303 / 6 | 525,516 / 1 | 76 / 47 | 85.6 / 75.0 |
| Deleted comment anchor | Earlier shallow | 1,302 / 6 | 525,503 / 1 | 114 / 56 | 85.7 / 74.8 |
| Offline fork before both roots | Full | 526,609 / 6 | 525,496 / 1 | 85 / 42 | 87.4 / 74.9 |
| Post-root edit, text still visible | Current | 491 / 3 | 1,049,790 / 1 | 84 / 52 | 87.8 / 82.0 |

The first three rows show when keeping an older shallow checkpoint and its intervening encrypted records saves bandwidth. The fourth shows the cost of guessing wrong. The device eventually needs the full checkpoint and pays for the earlier attempts. On loopback the single full download was faster in all five cases in this run.

The constrained-link run added a 40 ms round trip per request plus the time to transfer each response body at 10 Mbit/s. The post-root repair took **175 ms versus 518 ms** and the pre-root repair took **320 ms versus 517 ms**, each compared with fetching a full checkpoint. These are simulated link conditions.

| Cold open through first committed local edit | Shallow bytes | Full bytes | Loopback ms, shallow / full |
| --- | ---: | ---: | ---: |
| 512 KiB already deleted | 516 | 525,134 | 51 / 57 |
| 512 KiB still visible | 524,851 | 1,049,435 | 50 / 68 |

Measurements are medians of five fresh Node v26.13.1 processes per strategy on an Apple M1 Pro running macOS arm64. Byte counts are encrypted response bodies. They exclude HTTP headers, the cached checkpoint and journal during recovery, and any production authentication framing. Peak memory is the client process’s lifetime high-water RSS, including Node and WASM. Recovery timing includes authenticated reads and cache writes. Cold timing starts before the initial request and includes the first text edit. The benchmark checks parity with the reference on every run. [Benchmark and fixture](../mockup/experiments/loro-history/run.mjs)

## What belongs in Loro

This prototype works through public APIs with Loro 1.18.0 in JavaScript. The standalone Rust tests also passed against Margin’s pinned 1.18.0 and upstream commit `934563c` with Rust crate version 1.19.1. Loro already has internal lookups for an operation’s containing block and a synchronous KV reader. A transparent asynchronous fetch during checkout or cursor lookup remains an upstream design question. [ChangeStore](https://github.com/loro-dev/loro/blob/934563cfb4117373f237bc8b282533eacb17207d/crates/loro-internal/src/oplog/change_store.rs#L1389-L1445) · [KV interface](https://github.com/loro-dev/loro/blob/934563cfb4117373f237bc8b282533eacb17207d/crates/loro-internal/src/kv_store.rs#L10-L35)

For Margin, this would require an authenticated archive of earlier checkpoints and addressable delta records, a retention policy covering both, and an editor handoff when the document instance changes. A trusted device can open a full encrypted snapshot and export a range today. A keyless relay can serve independently sealed spans from its index. A snapshot sealed as a single AES-GCM record needs its entire ciphertext for authentication. The research archive keeps shallow checkpoints separate from the snapshots that authorize relay compaction. [Loro encrypted span index](https://github.com/loro-dev/protocol/blob/main/protocol-e2ee.md#server-behavior-indexing-and-dedup) · [Brief for upstream work](loro-history-brief.md)

## Reproduce

```sh
npm ci --prefix mockup
node --test mockup/test/loro-history-recovery.test.mjs
cargo test --locked --features native --test loro_history
npm --prefix mockup run experiment:history
npm --prefix mockup run experiment:history -- --constrained
```

Add `--json` to the benchmark arguments for individual timings. All fixtures are synthetic. The runner uses disposable document keys and removes its temporary archive and device directories.
