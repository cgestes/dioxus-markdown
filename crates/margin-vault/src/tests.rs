use super::*;
use std::{io::Cursor, time::Duration};
use tokio::{net::TcpListener, sync::watch};

fn device(dir: &Path, id: Identity) -> Vault {
    fs::create_dir_all(dir).unwrap();
    Vault::with_identity(dir, id).unwrap()
}

async fn start(
    path: &Path,
    owner: &str,
    n: usize,
) -> (String, tokio::task::JoinHandle<Result<()>>) {
    let server = relay::Relay::open(path, owner, n).unwrap();
    let socket = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", socket.local_addr().unwrap());
    (url, tokio::spawn(server.serve(socket)))
}

fn append(v: &Vault, room: &str, s: &str) {
    let (d, _) = v.load(room).unwrap();
    let vv = d.oplog_vv();
    let t = d.get_text("markdown");
    t.insert(t.len_unicode(), s).unwrap();
    v.append_update(room, &d.export(ExportMode::updates(&vv)).unwrap())
        .unwrap();
}

#[test]
fn desktop_first_launch_is_atomic_and_keeps_a_cleared_starter_empty() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let id = Identity::from_recovery(&keys::Recovery::new());
    let a = device(tmp.path(), id.clone());
    let b = device(tmp.path(), id);
    let ready = std::sync::Barrier::new(2);
    let launch = |v: &Vault| {
        ready.wait();
        v.resume_note(None, || Ok((Note::new("first note"), b"private starter discussion".to_vec())))
    };
    let (first, second) = std::thread::scope(|scope| {
        let first = scope.spawn(|| launch(&a));
        (launch(&b), first.join().unwrap())
    });
    let room = first?;
    assert_eq!(room, second?);
    assert_eq!(a.rooms()?.len(), 2);
    assert_ne!(room, a.inbox());
    assert!(a.pending()?.is_none());
    let (doc, _) = a.load(&room)?;
    let vv = doc.oplog_vv();
    let text = doc.get_text("markdown");
    assert_eq!(text.to_string(), "first note");
    text.delete(0, text.len_unicode())?;
    a.append_update(&room, &doc.export(ExportMode::updates(&vv))?)?;
    a.set_private_value(&room, b"private starter discussion", b"[]")?;
    assert_eq!(b.resume_note(None, || panic!("reseeded"))?, room);
    assert!(b.load(&room)?.0.get_text("markdown").is_empty());
    assert_eq!(b.private_value(&room)?.unwrap(), b"[]");
    Ok(())
}

#[test]
fn desktop_upgrade_preserves_the_inbox_and_existing_notes() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let v = device(tmp.path(), Identity::from_recovery(&keys::Recovery::new()));
    let inbox = v.inbox();
    append(&v, &inbox, "my old inbox");
    let existing_only = || panic!("replaced an existing note");
    assert_eq!(v.resume_note(None, existing_only)?, inbox);
    assert_eq!(v.rooms()?.len(), 1);
    assert_eq!(
        v.load(&inbox)?.0.get_text("markdown").to_string(),
        "my old inbox"
    );

    let empty = v.new_note("", false)?;
    v.conn()?
        .execute("DELETE FROM settings WHERE k='desktop-note'", [])?;
    assert_eq!(v.resume_note(None, existing_only)?, empty);
    assert!(v.resume_note(Some("missing"), existing_only).is_err());
    assert_eq!(v.resume_note(None, existing_only)?, empty);
    assert_eq!(v.resume_note(Some(&inbox), existing_only)?, inbox);
    assert_eq!(v.resume_note(None, existing_only)?, inbox);
    assert_eq!(v.rooms()?.len(), 2);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn offline_writers_checkpoint_race_restart_and_file_restore() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let r = keys::Recovery::new();
    let a = device(&tmp.path().join("a"), Identity::from_recovery(&r));
    let b = device(&tmp.path().join("b"), Identity::from_pair(r.pair())?);
    let c = device(&tmp.path().join("c"), Identity::from_pair(r.pair())?);
    let server = tmp.path().join("relay.db");
    let (url, task) = start(&server, &a.id.paired.vault, 500).await;
    let local = a.new_note("DO-NOT-SYNC-capture", false)?;
    a.attach(&local, "private.wav", Cursor::new(b"PRIVATE-RECORDING"))?;
    a.set_private_value(&local, b"[]", b"DO-NOT-SYNC-prompt")?;
    let room = a.new_note("# Encrypted field diary 🎧\n\n- [ ] rain\n", true)?;
    sync::once(&a, &url).await?;
    sync::once(&b, &url).await?;
    append(&a, &room, "north lock\n");
    let file: Vec<u8> = (0..record::FILE_CHUNK * 2 + 53)
        .map(|i| (i % 251) as u8)
        .collect();
    let blob = a.attach(&room, "private-listening-recording.wav", Cursor::new(&file))?;
    sync::once(&a, &url).await?;
    assert!(a.checkpoint(&room)?);
    // The snapshot has already been produced when the disconnected writer's edit arrives.
    append(&b, &room, "footbridge\n");
    sync::once(&b, &url).await?;
    sync::once(&a, &url).await?;
    let db = database(&server)?;
    let active: i64 = db.query_row(
        "SELECT count(*) FROM events WHERE room=? AND peer IS NOT NULL AND retired IS NULL",
        [&room],
        |r| r.get(0),
    )?;
    assert_eq!(active, 1);
    assert!(
        db.query_row(
            "SELECT count(*) FROM events WHERE retired IS NOT NULL",
            [],
            |r| r.get::<_, i64>(0)
        )? > 0
    );
    drop(db);
    task.abort();
    let _ = task.await;
    let (url, task) = start(&server, &a.id.paired.vault, 500).await;
    sync::once(&c, &url).await?;
    sync::once(&b, &url).await?;
    let expected = a.load(&room)?.0;
    for replica in [&b, &c] {
        let d = replica.load(&room)?.0;
        assert_eq!(d.oplog_vv(), expected.oplog_vv());
        assert_eq!(d.get_deep_value(), expected.get_deep_value());
        let text = d.get_text("markdown").to_string();
        assert!(text.contains("north lock") && text.contains("footbridge"));
        assert!(replica.shared(&local).is_err());
        assert_eq!(
            replica.attachments(&room)?,
            vec![(blob.clone(), "private-listening-recording.wav".into())]
        );
    }
    let out = tmp.path().join("restored");
    c.extract(&room, &blob, &out)?;
    assert_eq!(fs::read(&out)?, file);
    let cc = c.conn()?;
    cc.execute("DELETE FROM records WHERE room=? AND part=2", [&blob])?;
    drop(cc);
    let bad = tmp.path().join("incomplete");
    assert!(c.extract(&room, &blob, &bad).is_err() && !bad.exists());
    for dir in [
        tmp.path().to_owned(),
        tmp.path().join("a"),
        tmp.path().join("b"),
        tmp.path().join("c"),
    ] {
        for path in fs::read_dir(&dir)? {
            let p = path?.path();
            if !p.is_file() || p == out {
                continue;
            }
            let bytes = fs::read(p)?;
            for marker in [
                "Encrypted field diary",
                "private-listening-recording.wav",
                "DO-NOT-SYNC",
                "PRIVATE-RECORDING",
            ] {
                assert!(!bytes.windows(marker.len()).any(|w| w == marker.as_bytes()));
            }
        }
    }
    task.abort();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_broadcast_automatic_checkpoint_and_revocation() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let r = keys::Recovery::new();
    let a = device(&tmp.path().join("a"), Identity::from_recovery(&r));
    let b = device(&tmp.path().join("b"), Identity::from_pair(r.pair())?);
    let server = tmp.path().join("relay");
    let (url, task) = start(&server, &a.id.paired.vault, 2).await;
    let room = a.new_note("hello", true)?;
    sync::once(&a, &url).await?;
    let (stop, rx) = watch::channel(false);
    let (ready, mut ready_rx) = tokio::sync::mpsc::unbounded_channel();
    let follower = tokio::spawn(sync::follow(b.clone(), url.clone(), rx, move |s| {
        if s == "Synced" {
            let _ = ready.send(());
        }
    }));
    tokio::time::timeout(Duration::from_secs(10), ready_rx.recv()).await?;
    append(&a, &room, " broadcast");
    sync::once(&a, &url).await?;
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if b.load(&room).unwrap().0.get_text("markdown").to_string() == "hello broadcast" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await
        }
    })
    .await?;
    let db = database(&server)?;
    assert!(
        db.query_row("SELECT count(*) FROM events WHERE snapshot=1", [], |r| r
            .get::<_, i64>(0))?
            > 0
    );
    stop.send(true)?;
    follower.await?;
    relay::Relay::open(&server, &a.id.paired.vault, 2)?.revoke(&b.id.paired.grant.device)?;
    assert!(sync::once(&b, &url).await.is_err());
    let mut forged = r.pair();
    forged.grant.device = keys::public(&keys::random());
    assert!(Identity::from_pair(forged).is_err());
    task.abort();
    Ok(())
}

#[test]
fn recovery_open_promotion_and_tamper_detection() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let dir = tmp.path().join("vault");
    let pass = tmp.path().join("pass");
    fs::write(&pass, "isolated test only password")?;
    let r = keys::Recovery::new();
    let a = Vault::create(&dir, Identity::from_recovery(&r), Some(&pass))?;
    let local = a.new_note("sensitive original", false)?;
    let (d, _) = a.load(&local)?;
    let vv = d.oplog_vv();
    d.get_text("markdown")
        .update("share this summary", Default::default())?;
    a.append_update(&local, &d.export(ExportMode::updates(&vv))?)?;
    let published = a.publish(&local)?;
    let (public_doc, _) = a.load(&published)?;
    assert_eq!(
        public_doc.get_text("markdown").to_string(),
        "share this summary"
    );
    assert!(!public_doc.oplog_vv().includes_vv(&vv));
    let wire = a.pending()?.unwrap().data;
    assert!(a.decode(&local, true, &wire).is_err());
    let mut damaged = wire.clone();
    *damaged.last_mut().unwrap() ^= 1;
    assert!(a.decode(&published, true, &damaged).is_err());
    drop(a);
    assert!(Vault::open(&dir, Some(&pass))?.load(&local).is_ok());
    fs::write(&pass, "a different test password")?;
    assert!(Vault::open(&dir, Some(&pass)).is_err());
    assert!(sync::validate_url("ws://example.org").is_err());
    Ok(())
}

#[tokio::test]
async fn rotated_devices_recover_offline_edits_without_sharing_their_private_history() -> Result<()>
{
    let tmp = tempfile::tempdir()?;
    let mut recovery = keys::Recovery::new();
    let a_dir = tmp.path().join("a");
    let b_dir = tmp.path().join("b");
    let a = device(&a_dir, Identity::from_recovery(&recovery));
    let b = device(&b_dir, Identity::from_pair(recovery.pair())?);
    let (url, task) = start(&tmp.path().join("relay"), &a.id.paired.vault, 100).await;
    let shared = a.new_note("base", true)?;
    sync::once(&a, &url).await?;
    sync::once(&b, &url).await?;
    let private = b.new_note("private start", false)?;
    append(&b, &private, " with history");
    let original = b.load(&private)?.0;
    assert!(b.checkpoint(&private)?);
    assert_eq!(b.load(&private)?.0.oplog_vv(), original.oplog_vv());
    append(&b, &shared, " offline");
    recovery.keys.insert(2, keys::random());
    let a2 = device(&a_dir, Identity::from_recovery(&recovery));
    sync::once(&a2, &url).await?;
    assert!(sync::once(&b, &url).await.is_err());
    let mut next = Identity::from_pair(recovery.pair())?;
    next.local = b.id.local;
    let b2 = device(&b_dir, next);
    sync::once(&b2, &url).await?;
    sync::once(&a2, &url).await?;
    assert_eq!(
        a2.load(&shared)?.0.get_text("markdown").to_string(),
        "base offline"
    );
    assert_eq!(
        b2.load(&private)?.0.get_deep_value(),
        original.get_deep_value()
    );
    assert!(a2.shared(&private).is_err());
    task.abort();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interrupted_document_and_multi_window_replay_use_the_durable_outbox() -> Result<()> {
    use futures_util::SinkExt;
    use loro_protocol::{BatchId, CrdtType, MAX_MESSAGE_SIZE, ProtocolMessage};
    use rand::{RngCore, SeedableRng};
    use tokio_tungstenite::{connect_async, tungstenite::Message};

    let tmp = tempfile::tempdir()?;
    let recovery = keys::Recovery::new();
    let a = device(&tmp.path().join("a"), Identity::from_recovery(&recovery));
    let b = device(&tmp.path().join("b"), Identity::from_pair(recovery.pair())?);
    let path = tmp.path().join("relay");
    let (url, task) = start(&path, &a.id.paired.vault, 100).await;
    let mut bytes = vec![0u8; 410_000];
    rand::rngs::StdRng::seed_from_u64(31).fill_bytes(&mut bytes);
    let markdown = b64(&bytes);
    let room = a.new_note(&markdown, true)?;
    let first = a.pending()?.unwrap();
    assert!(first.data.len() > MAX_MESSAGE_SIZE);

    let (mut ws, _) = connect_async(&url).await?;
    let p = &a.id.paired;
    stream::json(
        &mut ws,
        &relay::Request::Hello {
            vault: p.vault.clone(),
            grant: p.grant.clone(),
            token: b64(&p.token),
            streaming: true,
        },
    )
    .await?;
    let reply: relay::Reply = serde_json::from_slice(&stream::frame(&mut ws).await?.into_data())?;
    assert!(matches!(reply, relay::Reply::Ready { streaming: true }));
    let mut packet = a.outbound(&first)?;
    packet.data.clear();
    stream::json(&mut ws, &relay::Request::Push { packet }).await?;
    let payload_len = first.data.len() + 4; // count plus three-byte record length for this fixture
    let batch_id = BatchId([7; 8]);
    for msg in [
        ProtocolMessage::DocUpdateFragmentHeader {
            crdt: CrdtType::Elo,
            room_id: room.clone(),
            batch_id,
            fragment_count: 3,
            total_size_bytes: payload_len as u64,
        },
        ProtocolMessage::DocUpdateFragment {
            crdt: CrdtType::Elo,
            room_id: room.clone(),
            batch_id,
            index: 0,
            fragment: vec![1],
        },
    ] {
        ws.send(Message::Binary(loro_protocol::encode(&msg).unwrap().into()))
            .await?;
    }
    ws.close(None).await?;
    let db = database(&path)?;
    assert_eq!(
        db.query_row("SELECT count(*) FROM events", [], |r| r.get::<_, i64>(0))?,
        0
    );
    assert_eq!(a.pending()?.unwrap().seq, first.seq);

    // Cross two receiver windows and confirm that their cursors resume in order.
    let short = a.new_note("ordered", true)?;
    for _ in 0..35 {
        append(&a, &short, ".");
    }
    sync::once(&a, &url).await?;
    sync::once(&b, &url).await?;
    assert_eq!(b.load(&room)?.0.get_text("markdown").to_string(), markdown);
    assert_eq!(b.load(&short)?.0.oplog_vv(), a.load(&short)?.0.oplog_vv());
    assert_eq!(b.cursor()?, a.cursor()?);
    assert!(a.pending()?.is_none());
    task.abort();
    Ok(())
}

/// Every note the vault creates records the layout version, as `Note` does.
#[test]
fn notes_the_vault_creates_record_their_layout_version() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let v = device(tmp.path(), Identity::from_recovery(&keys::Recovery::new()));
    let format = |room: &str| -> Result<_> {
        Ok(v.load(room)?.0.get_map("margin").get("format").and_then(|f| f.into_value().ok()))
    };
    let first = v.resume_note(None, || Ok((Note::new("first"), Vec::new())))?;
    let other = v.new_note("other", false)?;
    assert_eq!(format(&first)?, Some(loro::LoroValue::I64(margin_note::note::FORMAT)));
    assert_eq!(format(&other)?, Some(loro::LoroValue::I64(margin_note::note::FORMAT)));
    Ok(())
}
