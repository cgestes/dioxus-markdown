//! The `margin` app's own tests: what a launch opens, and the personal vault.
use super::*;
use margin_editor::testing::*;

#[test]
fn the_gallery_shows_the_components_then_markdown_as_the_editor_reads_it() {
    let doc = open(VirtualDom::new(|| rsx! { div { id: "gallery", {gallery()} } }));
    let shown = text(&doc, "gallery");
    assert!(shown.contains("Buttons") && shown.contains("Icons"), "the design system's sections");
    assert!(shown.contains("Field notes") && !shown.contains("# Field notes"), "the sample's title, read as a heading");
    assert!(shown.contains("ten seconds") && !shown.contains("**ten"), "its bold text, read as Markdown");
}

#[cfg(feature = "vault")]
#[test]
fn personal_editor_persists_input_and_keeps_diagnostics_on_device() -> anyhow::Result<()> {
    use margin_vault::{
        Vault,
        keys::{Identity, Recovery},
    };
    let tmp = tempfile::tempdir()?;
    let pass = tmp.path().join("password");
    std::fs::write(&pass, b"private editor test password")?;
    let path = tmp.path().join("vault");
    let vault = Vault::create(
        &path,
        Identity::from_recovery(&Recovery::new()),
        Some(&pass),
    )?;
    let starter = personal::open_note(&vault, None)?;
    assert!(!vault.shared(&starter)?);
    let field_notes = Note::from_doc(vault.load(&starter)?.0);
    assert_eq!(field_notes.markdown(), crate::SAMPLE);
    let threads: Vec<margin_editor::CommentThread> =
        serde_json::from_slice(&vault.private_value(&starter)?.unwrap())?;
    assert_eq!(threads.len(), 2);
    for thread in threads {
        let range = field_notes.resolve_anchor(thread.cursors.as_ref().unwrap()).unwrap();
        assert_eq!(&crate::SAMPLE[range], thread.file.anchor.exact);
    }
    let room = vault.new_note("# Diary\n\nHello", true)?;
    let (note, seq) = vault.load(&room)?;
    let session = personal::Session {
        vault: vault.clone(),
        room: room.clone(),
        doc: note,
        seq,
        threads: vec![],
    };
    // The session reaches the editor as a store, as the vault desktop passes it.
    fn open_session() -> Element {
        let store = use_hook(|| {
            Store::new(personal::VaultStore { session: consume_context::<personal::Session>() })
        });
        notebook_view("", NotebookOptions { store: Some(store), ..Default::default() })
    }
    let dom = VirtualDom::new(open_session).with_root_context(session);
    let mut doc = open(dom);
    click(&mut doc, "note-body");
    select(&doc, "note-body", [5, 5]);
    press(&mut doc, Key::Character("!".into()), false);
    assert_eq!(field(&doc, id(&doc, "note-body")), "Hello!");
    let reopened = Vault::open(&path, Some(&pass))?;
    assert_eq!(
        reopened.load(&room)?.0.get_text("markdown").to_string(),
        "# Diary\n\nHello!"
    );
    press(&mut doc, Key::Named(NamedKey::F8), false);
    click(&mut doc, "save-inspection");
    assert!(reopened.attachments(&room)?.is_empty());
    let files = reopened.attachments(&reopened.inbox())?;
    assert_eq!(files.len(), 1);
    let report = tmp.path().join("export.json");
    reopened.extract(&reopened.inbox(), &files[0].0, &report)?;
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(report)?)?;
    assert_eq!(json["current_markdown"], "# Diary\n\nHello!");
    drop(doc);

    // Launch the real app again, then switch notes through its menu.
    let other = reopened.new_note("# Elsewhere\n\nDifferent text", false)?;
    let launch = personal::open_note(&reopened, None)?;
    assert_eq!(launch, starter);
    personal::DEVICE
        .set((reopened.clone(), launch))
        .map_err(|_| anyhow::anyhow!("personal app already open"))?;
    let mut app = open(VirtualDom::new(crate::app));
    assert_eq!(value(&app, "note-title"), "Field notes");
    assert_eq!(value(&app, "cell-0-1-0"), "North lock");
    assert!(has(&app, "comment-thread-1"));
    click(&mut app, "notes-menu");
    click(&mut app, &format!("vault-room-{other}"));
    assert_eq!(field(&app, id(&app, "note-body")), "Different text");
    assert_eq!(personal::open_note(&reopened, None)?, other);
    click(&mut app, "notes-menu");
    click(&mut app, &format!("vault-room-{room}"));
    assert_eq!(field(&app, id(&app, "note-body")), "Hello!");
    assert_eq!(personal::open_note(&reopened, None)?, room);
    Ok(())
}

/// The default build has no vault: `margin` opens Field notes in memory, with
/// its table and sample discussions, and the status bar says nothing is saved.
#[cfg(not(feature = "vault"))]
#[test]
fn default_build_opens_field_notes_in_memory_and_says_it_is_not_saved() {
    let doc = open(VirtualDom::new(crate::app));
    assert!(value(&doc, "note-title").ends_with("Field notes"));
    assert!(value(&doc, "cell-0-1-0").contains("North lock"));
    id(&doc, "comments-pane");
    let status = text(&doc, "save-status");
    assert!(status.contains("Not saved") && status.contains("--features vault"), "{status}");
}

/// `--sync-file PATH`: a path with no file starts as Field notes, every
/// change is written to the file, and reopening the path shows it.
#[test]
fn a_sync_file_keeps_the_note_between_openings() {
    use margin_note::file::NoteFile;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes").join("field.sync");
    let editor = |path: &std::path::Path| {
        open(VirtualDom::new_with_props(crate::SyncFileEditor, crate::SyncFileEditorProps { path: path.to_owned(), inspect: true }))
    };
    let mut doc = editor(&path);
    assert!(value(&doc, "note-title").ends_with("Field notes"));
    assert_eq!(NoteFile::open(&path, "").unwrap().note().markdown(), SAMPLE, "the file is written as the note opens");

    click(&mut doc, "note-title");
    commit_text(&mut doc, "My ");
    let edited = NoteFile::open(&path, "").unwrap().note().markdown();
    assert!(edited != SAMPLE && edited.contains("My "), "the edit reached the file: {}", &edited[..40]);

    let reopened = editor(&path);
    assert!(value(&reopened, "note-title").contains("My "), "{}", value(&reopened, "note-title"));
}
