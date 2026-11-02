use super::*;
use crate::characters;
use crate::models::{AssetInput, CharacterInput};
use image::{DynamicImage, ImageFormat};
use std::{fs, path::PathBuf};
use uuid::Uuid;

struct Fixture {
    root: PathBuf,
    db: Database,
    character: String,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("furfolio-media-test-{}", Uuid::new_v4()));
        let db = Database::initialize(&root).unwrap();
        let character = characters::create_character(
            &db,
            CharacterInput {
                name: "Nova".into(),
                species: None,
                pronouns: None,
                description: None,
                notes: None,
                tags: vec![],
                colors: vec![],
            },
        )
        .unwrap()
        .id;
        Self {
            root,
            db,
            character,
        }
    }
    fn source(&self, format: ImageFormat) -> PathBuf {
        let path = self.root.join("user-original.wrong-extension");
        let mut cursor = std::io::Cursor::new(Vec::new());
        DynamicImage::new_rgb8(16, 12)
            .write_to(&mut cursor, format)
            .unwrap();
        fs::write(&path, cursor.into_inner()).unwrap();
        path
    }
    fn import(&self) -> Asset {
        import_selected(
            &self.db,
            &self.character,
            Some(&self.source(ImageFormat::Png)),
        )
        .unwrap()
        .unwrap()
    }
    fn ledger(&self) -> Vec<(String, String)> {
        let connection = self.db.open().unwrap();
        let mut s = connection
            .prepare("SELECT name, staging_name FROM media_files ORDER BY name")
            .unwrap();
        s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn imports_static_formats_by_content_without_changing_source_and_reopens() {
    for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::WebP] {
        let f = Fixture::new();
        let source = f.source(format);
        let before = fs::read(&source).unwrap();
        let asset = import_selected(&f.db, &f.character, Some(&source))
            .unwrap()
            .unwrap();
        assert_eq!((asset.width, asset.height), (16, 12));
        assert_eq!(asset.original_filename, "user-original.wrong-extension");
        assert_eq!(fs::read(&source).unwrap(), before);
        let db = Database::initialize(&f.root).unwrap();
        let assets = list_assets(&db, &f.character).unwrap();
        assert_eq!(assets.len(), 1);
        let stored: String = db
            .open()
            .unwrap()
            .query_row(
                "SELECT name FROM media_files WHERE asset_id=?1 AND kind='original'",
                [&asset.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(fs::read(f.root.join("media").join(stored)).unwrap(), before);
        let preview = read_asset_preview(&db, &f.character, &asset.id).unwrap();
        assert!(preview.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(preview.len() <= 4 * 1024 * 1024);
        assert_eq!(f.ledger().len(), 2);
    }
}

#[test]
fn cancellation_is_null_and_non_regular_or_oversized_sources_are_rejected() {
    let f = Fixture::new();
    assert!(import_selected(&f.db, &f.character, None)
        .unwrap()
        .is_none());
    assert!(import_selected(&f.db, &f.character, Some(&f.root)).is_err());
    let path = f.root.join("large.png");
    let file = fs::File::create(&path).unwrap();
    file.set_len(20 * 1024 * 1024 + 1).unwrap();
    assert!(matches!(
        import_selected(&f.db, &f.character, Some(&path)),
        Err(AppError::MediaTooLarge)
    ));
    assert!(f.ledger().is_empty());
}

#[test]
fn rejects_corruption_unsupported_animations_dimensions_and_pixel_bombs() {
    assert!(decode_media(b"<svg/>").is_err());
    assert!(decode_media(b"\xff\xd8\xff").is_err());
    let f = Fixture::new();
    let mut png = fs::read(f.source(ImageFormat::Png)).unwrap();
    png.truncate(png.len() / 2);
    assert!(decode_media(&png).is_err());
    let mut png = fs::read(f.source(ImageFormat::Png)).unwrap();
    // An animation control chunk is rejected before any frame allocation, including malformed APNG.
    png.splice(33..33, [0, 0, 0, 0, b'a', b'c', b'T', b'L', 0, 0, 0, 0]);
    assert!(matches!(decode_media(&png), Err(AppError::InvalidMedia)));
    assert!(validate_dimensions(12001, 1, 12001 * 4).is_err());
    assert!(validate_dimensions(6000, 4001, 6000 * 4001 * 4).is_err());
    assert!(validate_dimensions(12000, 2000, 128 * 1024 * 1024 + 1).is_err());
    assert!(validate_dimensions(12000, 2000, 12000 * 2000 * 4).is_ok());
    let mut webp = b"RIFF\x0c\x00\x00\x00WEBPANIM\x00\x00\x00\x00".to_vec();
    assert!(decode_media(&webp).is_err());
    webp[12..16].copy_from_slice(b"ANMF");
    assert!(decode_media(&webp).is_err());
}

#[test]
fn metadata_is_bounded_and_cross_character_access_is_denied() {
    let f = Fixture::new();
    let asset = f.import();
    let updated = update_asset(
        &f.db,
        &f.character,
        &asset.id,
        AssetInput {
            artist_name: Some("  Artist  ".into()),
            source_url: Some(" https://example.com/ref ".into()),
            notes: Some(" Notes ".into()),
        },
    )
    .unwrap();
    assert_eq!(updated.artist_name.as_deref(), Some("Artist"));
    assert_eq!(updated.notes.as_deref(), Some("Notes"));
    for input in [
        AssetInput {
            artist_name: Some("a".repeat(201)),
            ..Default::default()
        },
        AssetInput {
            source_url: Some("https://user:secret@example.com".into()),
            ..Default::default()
        },
        AssetInput {
            source_url: Some("file:///outside".into()),
            ..Default::default()
        },
        AssetInput {
            source_url: Some(format!("https://example.com/{}", "a".repeat(2048))),
            ..Default::default()
        },
        AssetInput {
            notes: Some("a".repeat(10001)),
            ..Default::default()
        },
    ] {
        assert!(matches!(
            update_asset(&f.db, &f.character, &asset.id, input),
            Err(AppError::InvalidInput(_))
        ));
    }
    assert!(matches!(
        read_asset_preview(&f.db, "other", &asset.id),
        Err(AppError::NotFound)
    ));
    assert!(matches!(
        delete_asset(&f.db, "other", &asset.id),
        Err(AppError::NotFound)
    ));
    assert!(matches!(
        update_asset(&f.db, "other", &asset.id, AssetInput::default()),
        Err(AppError::NotFound)
    ));
    let json = serde_json::to_string(&updated).unwrap();
    assert!(!json.contains(&f.root.to_string_lossy().to_string()));
    assert!(!json.contains("sha256"));
    assert!(!json.contains("relativePath"));
}

#[test]
fn import_database_failure_rolls_back_files_and_does_not_claim_applied() {
    let f = Fixture::new();
    f.db.open().unwrap().execute_batch(
        "CREATE TRIGGER reject_import BEFORE INSERT ON assets BEGIN SELECT RAISE(ABORT,'forced'); END;").unwrap();
    let error =
        import_selected(&f.db, &f.character, Some(&f.source(ImageFormat::Png))).unwrap_err();
    assert_ne!(crate::error::CommandError::from(error).applied, Some(true));
    assert!(list_assets(&f.db, &f.character).unwrap().is_empty());
    assert!(f.ledger().is_empty());
    assert_eq!(fs::read_dir(f.root.join("media")).unwrap().count(), 0);
}

#[test]
fn failed_delete_transaction_keeps_assets_and_ready_ledger() {
    let f = Fixture::new();
    let asset = f.import();
    f.db.open().unwrap().execute_batch(
        "CREATE TRIGGER reject_delete BEFORE DELETE ON assets BEGIN SELECT RAISE(ABORT,'forced'); END;").unwrap();
    assert!(delete_asset(&f.db, &f.character, &asset.id).is_err());
    assert_eq!(list_assets(&f.db, &f.character).unwrap().len(), 1);
    let pending = media_cleanup_status(&f.db).unwrap().pending;
    assert_eq!(pending, 0);
    assert!(read_asset_preview(&f.db, &f.character, &asset.id).is_ok());
}

#[test]
fn committed_delete_reports_pending_on_mismatch_and_can_retry_after_repair() {
    let f = Fixture::new();
    let asset = f.import();
    let (name, _) = f.ledger()[0].clone();
    let path = f.root.join("media").join(name);
    let original = fs::read(&path).unwrap();
    fs::write(&path, b"not-owned-content").unwrap();
    let error = delete_asset(&f.db, &f.character, &asset.id).unwrap_err();
    let error = crate::error::CommandError::from(error);
    assert_eq!(error.code, "cleanupPending");
    assert_eq!(error.applied, Some(true));
    assert!(!error
        .message
        .contains(&f.root.to_string_lossy().to_string()));
    assert!(list_assets(&f.db, &f.character).unwrap().is_empty());
    assert!(media_cleanup_status(&f.db).unwrap().pending > 0);
    fs::write(&path, original).unwrap();
    assert_eq!(retry_media_cleanup(&f.db).unwrap().pending, 0);
}

#[test]
fn restart_only_recovers_exact_pending_ledger_entries_never_scans_unknown_files() {
    let f = Fixture::new();
    let asset = f.import();
    let c = f.db.open().unwrap();
    c.execute(
        "UPDATE media_files SET state='pendingDelete' WHERE asset_id=?1",
        [&asset.id],
    )
    .unwrap();
    c.execute("DELETE FROM assets WHERE id=?1", [&asset.id])
        .unwrap();
    drop(c);
    fs::write(f.root.join("media/unrelated.txt"), b"sentinel").unwrap();
    let db = Database::initialize(&f.root).unwrap();
    assert_eq!(media_cleanup_status(&db).unwrap().pending, 0);
    assert_eq!(
        fs::read(f.root.join("media/unrelated.txt")).unwrap(),
        b"sentinel"
    );
}

#[test]
fn pending_import_is_recovered_but_in_flight_work_is_not_raced() {
    let f = Fixture::new();
    let asset = f.import();
    let c = f.db.open().unwrap();
    c.execute("DELETE FROM assets WHERE id=?1", [&asset.id])
        .unwrap();
    c.execute(
        "UPDATE media_files SET state='pendingImport' WHERE asset_id=?1",
        [&asset.id],
    )
    .unwrap();
    drop(c);
    let guard = f.db.lock_media().unwrap();
    assert!(matches!(
        retry_media_cleanup(&f.db),
        Err(AppError::MediaBusy)
    ));
    drop(guard);
    assert_eq!(retry_media_cleanup(&f.db).unwrap().pending, 0);
}

#[test]
fn character_cascade_preserves_cleanup_credentials_until_safe_delete() {
    let f = Fixture::new();
    f.import();
    characters::delete_character(&f.db, &f.character).unwrap();
    assert!(f.ledger().is_empty());
    assert_eq!(fs::read_dir(f.root.join("media")).unwrap().count(), 0);
}

#[test]
fn ready_files_are_never_recovered_and_partial_staging_keeps_its_credentials() {
    let f = Fixture::new();
    let asset = f.import();
    assert_eq!(retry_media_cleanup(&f.db).unwrap().pending, 0);
    assert_eq!(list_assets(&f.db, &f.character).unwrap().len(), 1);
    let (name, staging) = f.ledger()[0].clone();
    let c = f.db.open().unwrap();
    c.execute("DELETE FROM assets WHERE id=?1", [&asset.id])
        .unwrap();
    c.execute(
        "UPDATE media_files SET state='pendingImport' WHERE asset_id=?1",
        [&asset.id],
    )
    .unwrap();
    drop(c);
    fs::rename(
        f.root.join("media").join(&name),
        f.root.join("media").join(&staging),
    )
    .unwrap();
    let complete = fs::read(f.root.join("media").join(&staging)).unwrap();
    fs::write(f.root.join("media").join(&staging), b"partial").unwrap();
    assert!(retry_media_cleanup(&f.db).unwrap().pending > 0);
    assert_eq!(
        fs::read(f.root.join("media").join(&staging)).unwrap(),
        b"partial"
    );
    assert!(!f.ledger().is_empty());
    fs::write(f.root.join("media").join(&staging), complete).unwrap();
    assert_eq!(retry_media_cleanup(&f.db).unwrap().pending, 0);
}

#[test]
fn publishing_never_overwrites_and_recovery_checks_both_names() {
    let f = Fixture::new();
    let store = f.db.media_store().unwrap();
    let name = format!("{}.bin", Uuid::new_v4());
    let staging = format!("{}.part", Uuid::new_v4());
    fs::write(f.root.join("media").join(&name), b"outside-sentinel").unwrap();
    assert!(store.publish(&name, &staging, b"owned").is_err());
    assert_eq!(
        fs::read(f.root.join("media").join(&name)).unwrap(),
        b"outside-sentinel"
    );
    assert!(store.cleanup(&name, &staging, &hash(b"owned"), 5).is_err());
    assert_eq!(
        fs::read(f.root.join("media").join(&name)).unwrap(),
        b"outside-sentinel"
    );
    assert!(store.cleanup(".", &staging, &hash(b"owned"), 5).is_err());
    assert!(store.cleanup("/", &staging, &hash(b"owned"), 5).is_err());
}

#[test]
fn pending_character_delete_has_applied_and_preserves_unrelated_files() {
    let f = Fixture::new();
    f.import();
    let (name, _) = f.ledger()[0].clone();
    fs::write(f.root.join("media").join(name), b"replacement").unwrap();
    let error = characters::delete_character(&f.db, &f.character).unwrap_err();
    assert!(matches!(error, AppError::CleanupPending { applied: true }));
    assert!(characters::get_character(&f.db, &f.character).is_err());
    assert!(media_cleanup_status(&f.db).unwrap().pending > 0);
}

#[test]
fn selection_after_character_deleted_and_second_process_lock_are_safe() {
    let f = Fixture::new();
    let source = f.source(ImageFormat::Png);
    characters::delete_character(&f.db, &f.character).unwrap();
    assert!(matches!(
        import_selected(&f.db, &f.character, Some(&source)),
        Err(AppError::NotFound)
    ));
    assert!(f.ledger().is_empty());
    let second = Database::initialize(&f.root).unwrap();
    let guard = f.db.lock_media().unwrap();
    assert!(matches!(
        retry_media_cleanup(&second),
        Err(AppError::MediaBusy)
    ));
    drop(guard);
    assert_eq!(retry_media_cleanup(&second).unwrap().pending, 0);
}

#[cfg(unix)]
#[test]
fn symlink_source_and_hardlinked_managed_file_fail_closed() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let source = f.source(ImageFormat::Png);
    let link = f.root.join("source-link.png");
    symlink(&source, &link).unwrap();
    assert!(import_selected(&f.db, &f.character, Some(&link)).is_err());
    let asset = f.import();
    let (name, _) = f.ledger()[0].clone();
    let managed = f.root.join("media").join(name);
    let sentinel = f.root.join("hardlink-outside");
    fs::hard_link(&managed, &sentinel).unwrap();
    let bytes = fs::read(&sentinel).unwrap();
    assert!(matches!(
        delete_asset(&f.db, &f.character, &asset.id),
        Err(AppError::CleanupPending { applied: true })
    ));
    assert_eq!(fs::read(&sentinel).unwrap(), bytes);
    assert!(managed.exists());
}

#[cfg(unix)]
#[test]
fn symlink_files_roots_and_traversal_never_delete_an_outside_sentinel() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let asset = f.import();
    let sentinel = f.root.join("outside.png");
    fs::write(&sentinel, b"sentinel").unwrap();
    let (name, _) = f.ledger()[0].clone();
    let managed = f.root.join("media").join(&name);
    fs::remove_file(&managed).unwrap();
    symlink(&sentinel, &managed).unwrap();
    assert!(matches!(
        delete_asset(&f.db, &f.character, &asset.id),
        Err(AppError::CleanupPending { applied: true })
    ));
    assert_eq!(fs::read(&sentinel).unwrap(), b"sentinel");
    assert!(fs::symlink_metadata(&managed)
        .unwrap()
        .file_type()
        .is_symlink());
    let old = f.root.join("media-old");
    fs::rename(f.root.join("media"), &old).unwrap();
    symlink(&f.root, f.root.join("media")).unwrap();
    assert!(retry_media_cleanup(&f.db).unwrap().pending > 0);
    assert_eq!(fs::read(&sentinel).unwrap(), b"sentinel");
    f.db.open()
        .unwrap()
        .execute(
            "UPDATE media_files SET name='../outside.png' WHERE name=?1",
            [name],
        )
        .unwrap();
    assert!(retry_media_cleanup(&f.db).unwrap().pending > 0);
    assert_eq!(fs::read(&sentinel).unwrap(), b"sentinel");
    assert!(import_selected(&f.db, &f.character, Some(&managed)).is_err());
}

#[test]
fn unsafe_permissions_on_existing_media_root_are_rejected_without_chmod() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let f = Fixture::new();
        let source = f.source(ImageFormat::Png);
        f.db.media_store().unwrap();
        fs::set_permissions(f.root.join("media"), fs::Permissions::from_mode(0o777)).unwrap();
        assert!(import_selected(&f.db, &f.character, Some(&source)).is_err());
        assert_eq!(
            fs::metadata(f.root.join("media"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o777
        );
        assert!(f.ledger().is_empty());
    }
}

#[cfg(unix)]
#[test]
fn new_media_files_and_directories_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    f.import();
    assert_eq!(
        fs::metadata(f.root.join("media"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    for (name, _) in f.ledger() {
        assert_eq!(
            fs::metadata(f.root.join("media").join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
