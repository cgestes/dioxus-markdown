// Standalone upstream reproducer. Its only nonstandard dependency is loro.
#![cfg(not(target_arch = "wasm32"))]

use loro::{ExportMode, ID, IdSpan, LoroDoc, LoroError, cursor::Side};
use std::{collections::BTreeMap, error::Error};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn shallow(doc: &LoroDoc) -> Result<LoroDoc> {
    let snapshot = doc.export(ExportMode::shallow_snapshot(&doc.oplog_frontiers()))?;
    Ok(LoroDoc::from_snapshot(&snapshot)?)
}

// The JS experiment performs these reads as authenticated HTTP requests. This
// reproducer uses a full peer as the supplier, so it can also run in upstream Loro.
fn import_parents(replica: &LoroDoc, archive: &LoroDoc) -> Result<Vec<IdSpan>> {
    let mut fetched = Vec::new();
    while let Some(pending) = replica.export_pending_updates()? {
        let metadata = LoroDoc::decode_import_blob_meta(&pending, true)?;
        let known = replica.oplog_vv();
        let mut ends: BTreeMap<_, _> = metadata
            .partial_start_vv
            .iter()
            .map(|(&p, &n)| (p, n))
            .collect();
        for dep in metadata.start_frontiers.iter() {
            let end = ends.entry(dep.peer).or_default();
            *end = (*end).max(dep.counter + 1);
        }
        let spans: Vec<_> = ends
            .into_iter()
            .filter_map(|(peer, end)| {
                let start = known.get(&peer).copied().unwrap_or(0);
                (start < end).then(|| IdSpan::new(peer, start, end))
            })
            .collect();
        assert!(!spans.is_empty(), "could not locate a pending dependency");
        let raw = archive.export(ExportMode::updates_in_range(spans.as_slice()))?;
        replica.import(&raw)?;
        assert_ne!(
            replica.oplog_vv(),
            known,
            "this fixture must make progress on each range read"
        );
        fetched.extend(spans);
    }
    Ok(fetched)
}

#[test]
fn a_cached_root_merges_a_multi_writer_fork_after_fetching_original_operations() -> Result<()> {
    let root = LoroDoc::new();
    root.set_peer_id(11)?;
    root.get_text("markdown").insert(0, "east bridge")?;
    let base = root.oplog_vv();
    let replica = shallow(&root)?;
    let left = root.fork();
    left.set_peer_id(21)?;
    left.get_text("markdown").insert(0, "the ")?;
    let right = root.fork();
    right.set_peer_id(22)?;
    right.get_map("meta").insert("day", "Tuesday")?;
    left.import(&right.export(ExportMode::updates(&base))?)?;
    let parent = left.oplog_vv();
    let leaf = left.fork();
    leaf.set_peer_id(41)?;
    leaf.get_text("markdown").insert(4, "old ")?;
    let last = leaf.export(ExportMode::updates(&parent))?;
    assert!(replica.import(&last)?.pending.is_some());
    let ids: Vec<_> = LoroDoc::decode_import_blob_meta(&last, true)?
        .start_frontiers
        .iter()
        .map(|id| id.peer)
        .collect();
    assert!(ids.contains(&21) && ids.contains(&22));
    assert_eq!(import_parents(&replica, &left)?.len(), 2);
    assert_eq!(replica.oplog_vv(), leaf.oplog_vv());
    assert_eq!(replica.get_deep_value(), leaf.get_deep_value());
    assert!(replica.is_shallow());
    Ok(())
}

#[test]
fn an_earlier_shallow_root_recovers_a_deleted_anchor_and_both_offline_writers() -> Result<()> {
    let main = LoroDoc::new();
    main.set_peer_id(11)?;
    let text = main.get_text("markdown");
    text.insert(0, "old draft at the bridge")?;
    let list = main.get_movable_list("tasks");
    list.push("north")?;
    list.push("south")?;
    let cursor = text.get_cursor(5, Side::Middle).unwrap();
    let base = main.oplog_vv();
    let older = shallow(&main)?;
    let offline = main.fork();
    offline.set_peer_id(41)?;
    offline.get_text("markdown").insert(0, "Keep ")?;
    offline.get_movable_list("tasks").mov(1, 0)?;
    let incoming = offline.export(ExportMode::updates(&base))?;

    text.delete(0, 10)?;
    main.commit();
    text.insert(0, "Meet ")?;
    let current = shallow(&main)?;
    current.set_peer_id(91)?;
    let current_base = current.oplog_vv();
    current.get_text("markdown").insert(0, "Local · ")?;
    let local = current.export(ExportMode::updates(&current_base))?;
    assert!(matches!(
        current.import(&incoming),
        Err(LoroError::ImportUpdatesThatDependsOnOutdatedVersion)
    ));
    assert!(current.export_pending_updates()?.is_none());
    assert!(current.get_cursor_pos(&cursor).is_err());

    older.import(&incoming)?;
    assert!(older.import(&local)?.pending.is_some());
    assert!(!import_parents(&older, &main)?.is_empty());
    main.import_batch(&[local, incoming])?;
    assert!(older.is_shallow());
    assert_eq!(older.get_deep_value(), main.get_deep_value());
    assert_eq!(older.oplog_vv(), main.oplog_vv());
    assert_eq!(older.oplog_frontiers(), main.oplog_frontiers());
    assert_eq!(
        older.get_cursor_pos(&cursor)?.current,
        main.get_cursor_pos(&cursor)?.current
    );
    for peer in [41, 91] {
        let actual = older.get_change(ID::new(peer, 0)).unwrap();
        let expected = main.get_change(ID::new(peer, 0)).unwrap();
        assert_eq!(
            (actual.id, actual.len, actual.deps),
            (expected.id, expected.len, expected.deps)
        );
    }
    Ok(())
}
