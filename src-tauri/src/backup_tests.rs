use super::*;
use crate::{characters, error::CommandError, media, media_store::hash, models::CharacterInput};
use image::{DynamicImage, ImageFormat};
use rusqlite::{params, Connection, OpenFlags};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Barrier},
    thread,
};
use uuid::Uuid;

struct Fixture {
    root: PathBuf,
    source: PathBuf,
    destination: PathBuf,
    db: Database,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("furfolio-backup-test-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let source = root.join("library");
        let destination = root.join("destination");
        fs::create_dir(&destination).unwrap();
        let db = Database::initialize(&source).unwrap();
        Self {
            root,
            source,
            destination,
            db,
        }
    }
    fn character(&self, name: &str) -> String {
        characters::create_character(
            &self.db,
            CharacterInput {
                name: name.into(),
                species: None,
                pronouns: None,
                description: None,
                notes: Some("synthetic private notes — not for manifest".into()),
                tags: vec![],
                colors: vec![],
            },
        )
        .unwrap()
        .id
    }
    fn import(&self, character: &str) -> crate::models::Asset {
        let path = self.root.join("synthetic.png");
        let mut cursor = std::io::Cursor::new(Vec::new());
        DynamicImage::new_rgb8(16, 12)
            .write_to(&mut cursor, ImageFormat::Png)
            .unwrap();
        fs::write(&path, cursor.into_inner()).unwrap();
        media::import_selected(&self.db, character, Some(&path))
            .unwrap()
            .unwrap()
    }
    fn export(&self) -> crate::models::BackupExportResult {
        export_selected(&self.db, Some(&self.destination))
            .unwrap()
            .unwrap()
    }
    fn count_destination(&self) -> usize {
        fs::read_dir(&self.destination).unwrap().count()
    }
    fn error(&self, options: BackupOptions) -> CommandError {
        export_with_options(&self.db, Some(&self.destination), options)
            .unwrap_err()
            .into()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn validate_name(name: &str) {
    let prefix = "furfolio-backup-";
    assert!(name.starts_with(prefix));
    let tail = &name[prefix.len()..];
    assert_eq!(tail.len(), 53);
    assert!(chrono::NaiveDateTime::parse_from_str(&tail[..16], "%Y%m%dT%H%M%SZ").is_ok());
    assert_eq!(&tail[16..17], "-");
    let uuid = Uuid::parse_str(&tail[17..]).unwrap();
    assert_eq!(uuid.get_version_num(), 4);
    assert_eq!(uuid.to_string(), &tail[17..]);
}
fn verify_export(f: &Fixture, result: &crate::models::BackupExportResult) -> (Connection, Value) {
    validate_name(&result.directory_name);
    let output = f.destination.join(&result.directory_name);
    assert_eq!(fs::read_dir(&output).unwrap().count(), 3);
    let manifest_bytes = fs::read(output.join("manifest.json")).unwrap();
    assert!(!String::from_utf8_lossy(&manifest_bytes).contains("synthetic private notes"));
    assert!(!String::from_utf8_lossy(&manifest_bytes).contains(f.source.to_str().unwrap()));
    let manifest: Value = serde_json::from_slice(&manifest_bytes).unwrap();
    let mut total = 0;
    for entry in std::iter::once(&manifest["database"]).chain(manifest["media"].as_array().unwrap())
    {
        let path = entry["path"].as_str().unwrap();
        let bytes = fs::read(output.join(path)).unwrap();
        assert_eq!(entry["size"].as_u64().unwrap(), bytes.len() as u64);
        assert_eq!(entry["sha256"].as_str().unwrap(), hash(&bytes));
        total += bytes.len() as u64;
    }
    assert_eq!(total, result.total_bytes);
    assert_eq!(manifest["totalBytes"].as_u64(), Some(total));
    let c = Connection::open_with_flags(
        output.join("library.sqlite3"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        c.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(
        c.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, u64>(
            0
        ))
        .unwrap(),
        0
    );
    assert_eq!(
        c.query_row("SELECT count(*) FROM characters", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        result.character_count
    );
    assert!(!output.join("library.sqlite3-wal").exists());
    assert!(!output.join("library.sqlite3-shm").exists());
    (c, manifest)
}

#[test]
fn empty_export_is_independent_and_camel_case_payload_excludes_manifest() {
    let f = Fixture::new();
    let result = f.export();
    assert_eq!(result.character_count, 0);
    assert_eq!(result.asset_count, 0);
    let (c, manifest) = verify_export(&f, &result);
    assert_eq!(manifest["format"], "furfolio-library-backup");
    assert_eq!(manifest["version"], 1);
    assert_eq!(manifest["schemaVersion"], 2);
    let value = serde_json::to_value(&result).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 4);
    assert!(value.get("directoryName").is_some());
    drop(c);
}

#[test]
fn wal_commit_png_and_private_notes_export_without_modifying_source() {
    let f = Fixture::new();
    let c = f.db.open().unwrap();
    c.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
    let character = f.character("before");
    let asset = f.import(&character);
    c.execute(
        "UPDATE characters SET name='WAL visible' WHERE id=?1",
        [&character],
    )
    .unwrap();
    assert!(
        f.source
            .join("furfolio.sqlite3-wal")
            .metadata()
            .unwrap()
            .len()
            > 0
    );
    let source_png = fs::read(f.root.join("synthetic.png")).unwrap();
    let result = f.export();
    assert_eq!((result.character_count, result.asset_count), (1, 1));
    let (backup, manifest) = verify_export(&f, &result);
    assert_eq!(
        backup
            .query_row("SELECT name,notes FROM characters", [], |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?
            )))
            .unwrap(),
        (
            "WAL visible".into(),
            "synthetic private notes — not for manifest".into()
        )
    );
    assert_eq!(manifest["media"].as_array().unwrap().len(), 2);
    assert!(manifest["media"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["assetId"] == asset.id));
    assert_eq!(fs::read(f.root.join("synthetic.png")).unwrap(), source_png);
    assert_eq!(
        characters::get_character(&f.db, &character).unwrap().name,
        "WAL visible"
    );
}

#[test]
fn cancel_is_null_without_media_lock_or_destination_changes() {
    let f = Fixture::new();
    let _lock = f.db.lock_media().unwrap();
    assert!(export_selected(&f.db, None).unwrap().is_none());
    assert_eq!(f.count_destination(), 0);
}

#[test]
fn busy_gate_prevents_second_picker_but_picker_does_not_hold_media_lock() {
    let f = Fixture::new();
    let entered = Arc::new(Barrier::new(2));
    let leave = Arc::new(Barrier::new(2));
    let db = f.db.clone();
    let e = entered.clone();
    let l = leave.clone();
    let worker = thread::spawn(move || {
        export_with_picker(&db, || {
            e.wait();
            l.wait();
            Ok(None)
        })
    });
    entered.wait();
    let second = export_with_picker(&f.db, || panic!("duplicate dialog must never open"));
    assert_eq!(CommandError::from(second.unwrap_err()).code, "backupBusy");
    let media = f.db.lock_media().unwrap();
    drop(media);
    leave.wait();
    assert!(worker.join().unwrap().unwrap().is_none());
}

#[test]
fn snapshot_keeps_concurrent_character_edit_out_and_media_locked_until_publish() {
    let f = Fixture::new();
    let character = f.character("snapshot name");
    let db = f.db.clone();
    let root = f.source.clone();
    let options = BackupOptions {
        hook: Some(Arc::new(move |phase| {
            if phase == "snapshotPinned" {
                let c = db.open().unwrap();
                c.execute("UPDATE characters SET name='later name'", [])
                    .unwrap();
                assert!(db.lock_media().is_err());
                let second = Database::initialize_at(root.join("furfolio.sqlite3")).unwrap();
                assert!(second.lock_media().is_err());
            }
            if phase == "beforePublish" {
                assert!(db.lock_media().is_err());
            }
        })),
        ..Default::default()
    };
    let result = export_with_options(&f.db, Some(&f.destination), options)
        .unwrap()
        .unwrap();
    let (backup, _) = verify_export(&f, &result);
    assert_eq!(
        backup
            .query_row("SELECT name FROM characters", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "snapshot name"
    );
    assert_eq!(
        characters::get_character(&f.db, &character).unwrap().name,
        "later name"
    );
    assert!(f.db.lock_media().is_ok());
}

#[test]
fn media_lock_contention_maps_to_backup_busy() {
    let f = Fixture::new();
    let _guard = f.db.lock_media().unwrap();
    assert_eq!(f.error(Default::default()).code, "backupBusy");
    assert_eq!(f.count_destination(), 0);
}

#[test]
fn refuses_pending_orphan_missing_ledger_and_invalid_ready_state_without_repair() {
    for variant in ["pending", "orphan", "missing", "mismatch", "uuid", "avatar"] {
        let f = Fixture::new();
        let character = f.character("test");
        let asset = f.import(&character);
        let c = f.db.open().unwrap();
        match variant {
            "pending" => {
                c.execute(
                    "UPDATE media_files SET state='pendingImport' WHERE kind='preview'",
                    [],
                )
                .unwrap();
            }
            "orphan" => {
                c.execute(
                    "UPDATE media_files SET asset_id=?1 WHERE kind='preview'",
                    [Uuid::new_v4().to_string()],
                )
                .unwrap();
            }
            "missing" => {
                c.execute("DELETE FROM media_files WHERE kind='preview'", [])
                    .unwrap();
            }
            "mismatch" => {
                c.execute(
                    "UPDATE assets SET byte_size=byte_size+1 WHERE id=?1",
                    [&asset.id],
                )
                .unwrap();
            }
            "uuid" => {
                c.execute("UPDATE assets SET id='not-a-uuid' WHERE id=?1", [&asset.id])
                    .unwrap();
                c.execute("UPDATE media_files SET asset_id='not-a-uuid'", [])
                    .unwrap();
            }
            "avatar" => {
                let other = f.character("other");
                c.execute_batch("DROP TRIGGER validate_avatar_asset_update")
                    .unwrap();
                c.execute(
                    "UPDATE characters SET avatar_asset_id=?1 WHERE id=?2",
                    params![asset.id, other],
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let before: u64 = c
            .query_row("SELECT count(*) FROM media_files", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            f.error(Default::default()).code,
            if variant == "pending" {
                "backupPending"
            } else {
                "backupIntegrity"
            },
            "{variant}"
        );
        assert_eq!(f.count_destination(), 0);
        assert_eq!(
            c.query_row("SELECT count(*) FROM media_files", [], |r| r
                .get::<_, u64>(0))
                .unwrap(),
            before
        );
    }
}

#[test]
fn rejects_corrupt_symlink_hardlink_size_and_hash_media_without_final_output() {
    use std::os::unix::fs::symlink;
    for variant in ["symlink", "hardlink", "size", "hash", "missing"] {
        let f = Fixture::new();
        let character = f.character("test");
        f.import(&character);
        let name: String =
            f.db.open()
                .unwrap()
                .query_row(
                    "SELECT name FROM media_files WHERE kind='original'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
        let path = f.source.join("media").join(name);
        match variant {
            "symlink" => {
                fs::remove_file(&path).unwrap();
                symlink(f.root.join("synthetic.png"), &path).unwrap();
            }
            "hardlink" => {
                fs::hard_link(&path, f.root.join("hardlink")).unwrap();
            }
            "size" => {
                fs::OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .unwrap()
                    .set_len(1)
                    .unwrap();
            }
            "hash" => {
                let mut bytes = fs::read(&path).unwrap();
                bytes[0] ^= 1;
                fs::write(&path, bytes).unwrap();
            }
            "missing" => {
                fs::remove_file(&path).unwrap();
            }
            _ => unreachable!(),
        }
        assert_eq!(
            f.error(Default::default()).code,
            "backupIntegrity",
            "{variant}"
        );
        assert_eq!(f.count_destination(), 0);
    }
}

#[test]
fn rejects_source_descendant_symlink_and_writable_parent_without_chmod() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let f = Fixture::new();
    let child = f.source.join("child");
    fs::create_dir(&child).unwrap();
    for p in [&f.source, &child] {
        let error = export_selected(&f.db, Some(p)).unwrap_err();
        assert_eq!(CommandError::from(error).code, "backupDestination");
    }
    let link = f.root.join("link");
    symlink(&f.destination, &link).unwrap();
    assert_eq!(
        CommandError::from(export_selected(&f.db, Some(&link)).unwrap_err()).code,
        "backupDestination"
    );
    fs::set_permissions(&f.destination, fs::Permissions::from_mode(0o777)).unwrap();
    assert_eq!(f.error(Default::default()).code, "backupDestination");
    assert_eq!(
        fs::metadata(&f.destination).unwrap().permissions().mode() & 0o777,
        0o777
    );
}

#[test]
fn respects_all_size_count_and_soft_deadline_limits() {
    for kind in [
        "database",
        "characters",
        "assets",
        "media",
        "payload",
        "manifest",
        "timeout",
        "snapshot",
    ] {
        let f = Fixture::new();
        let character = f.character("test");
        f.import(&character);
        let mut o = BackupOptions::default();
        match kind {
            "database" => o.database_limit = 1,
            "characters" => o.character_limit = 0,
            "assets" => o.asset_limit = 0,
            "media" => o.media_limit = 0,
            "payload" => o.payload_limit = 1,
            "manifest" => o.manifest_limit = 1,
            "timeout" => o.timeout = Duration::ZERO,
            "snapshot" => o.snapshot_timeout = Duration::ZERO,
            _ => unreachable!(),
        }
        assert_eq!(
            f.error(o).code,
            if matches!(kind, "timeout" | "snapshot") {
                "backupTimeout"
            } else {
                "backupLimit"
            },
            "{kind}"
        );
        assert_eq!(f.count_destination(), 0);
    }
}

#[test]
fn injected_write_sync_rename_and_enospc_are_prepublication_and_clean() {
    for fault in [Fault::NoSpace, Fault::Write, Fault::Sync, Fault::Rename] {
        let f = Fixture::new();
        let error = f.error(BackupOptions {
            fault: Some(fault),
            ..Default::default()
        });
        assert_eq!(
            error.code,
            if fault == Fault::NoSpace {
                "backupNoSpace"
            } else {
                "backupFailed"
            }
        );
        assert_eq!(f.count_destination(), 0);
        let value = serde_json::to_value(error).unwrap();
        assert!(value.get("applied").is_none());
        assert!(value.get("published").is_none());
    }
}

#[test]
fn published_parent_sync_failure_keeps_valid_final_and_never_applied() {
    let f = Fixture::new();
    let error = f.error(BackupOptions {
        fault: Some(Fault::ParentSync),
        ..Default::default()
    });
    let value = serde_json::to_value(error).unwrap();
    assert_eq!(value["code"], "backupDurabilityUncertain");
    assert_eq!(value["published"], true);
    assert!(value.get("applied").is_none());
    let name = value["directoryName"].as_str().unwrap();
    validate_name(name);
    assert!(f.destination.join(name).join("library.sqlite3").is_file());
    assert_eq!(f.count_destination(), 1);
}

#[test]
fn cleanup_failure_reports_only_owned_partial_name() {
    let f = Fixture::new();
    let error = serde_json::to_value(f.error(BackupOptions {
        fault: Some(Fault::Cleanup),
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(error["code"], "backupCleanupPending");
    let name = error["directoryName"].as_str().unwrap();
    assert!(name.starts_with(".furfolio-backup-") && name.ends_with(".partial"));
    assert!(f.destination.join(name).is_dir());
    assert!(error.get("published").is_none());
}

#[test]
fn final_collisions_preserve_file_empty_and_nonempty_directory_sentinels() {
    for kind in ["file", "empty", "nonempty"] {
        let f = Fixture::new();
        let destination = f.destination.clone();
        let options = BackupOptions {
            hook: Some(Arc::new(move |phase| {
                if phase != "beforePublish" {
                    return;
                }
                let partial = fs::read_dir(&destination)
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap()
                    .path();
                let manifest: Value =
                    serde_json::from_slice(&fs::read(partial.join("manifest.json")).unwrap())
                        .unwrap();
                let created =
                    chrono::DateTime::parse_from_rfc3339(manifest["createdAt"].as_str().unwrap())
                        .unwrap();
                let partial_name = partial.file_name().unwrap().to_str().unwrap();
                let uuid = partial_name
                    .strip_prefix(".furfolio-backup-")
                    .unwrap()
                    .strip_suffix(".partial")
                    .unwrap();
                let final_path = destination.join(format!(
                    "furfolio-backup-{}-{uuid}",
                    created.format("%Y%m%dT%H%M%SZ")
                ));
                if kind == "file" {
                    fs::write(&final_path, b"do not overwrite").unwrap();
                } else {
                    fs::create_dir(&final_path).unwrap();
                    if kind == "nonempty" {
                        fs::write(final_path.join("sentinel"), b"do not overwrite").unwrap();
                    }
                }
            })),
            ..Default::default()
        };
        assert_eq!(f.error(options).code, "backupDestination");
        let existing = fs::read_dir(&f.destination)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(f.count_destination(), 1);
        if kind == "file" {
            assert_eq!(fs::read(existing).unwrap(), b"do not overwrite");
        } else if kind == "nonempty" {
            assert_eq!(
                fs::read(existing.join("sentinel")).unwrap(),
                b"do not overwrite"
            );
        } else {
            assert_eq!(fs::read_dir(existing).unwrap().count(), 0);
        }
    }
}

#[test]
fn partial_collision_preserves_existing_sentinel_without_cleanup() {
    let f = Fixture::new();
    let uuid = Uuid::new_v4();
    let partial = f
        .destination
        .join(format!(".furfolio-backup-{uuid}.partial"));
    fs::create_dir(&partial).unwrap();
    fs::write(partial.join("sentinel"), b"untouched").unwrap();
    assert_eq!(
        f.error(BackupOptions {
            fixed_uuid: Some(uuid),
            ..Default::default()
        })
        .code,
        "backupDestination"
    );
    assert_eq!(fs::read(partial.join("sentinel")).unwrap(), b"untouched");
}

#[test]
fn source_unknown_files_are_not_enumerated_or_exported() {
    let f = Fixture::new();
    let character = f.character("test");
    f.import(&character);
    let unknown = f.source.join("media").join("unknown-private-file");
    fs::write(&unknown, b"unmanaged").unwrap();
    let result = f.export();
    let output = f.destination.join(result.directory_name).join("media");
    assert_eq!(fs::read_dir(output).unwrap().count(), 2);
    assert_eq!(fs::read(unknown).unwrap(), b"unmanaged");
}

#[test]
fn unsupported_schema_version_missing_triggers_and_foreign_key_corruption_are_rejected() {
    for kind in ["version", "trigger", "foreignKey", "integrity"] {
        let f = Fixture::new();
        let id = f.character("test");
        f.import(&id);
        let c = f.db.open().unwrap();
        match kind {
            "version" => {
                c.execute("UPDATE schema_migrations SET version=3 WHERE version=2", [])
                    .unwrap();
            }
            "trigger" => {
                c.execute_batch("DROP TRIGGER validate_avatar_asset_update")
                    .unwrap();
            }
            "foreignKey" => {
                c.pragma_update(None, "foreign_keys", false).unwrap();
                c.execute(
                    "UPDATE assets SET character_id=?1",
                    [Uuid::new_v4().to_string()],
                )
                .unwrap();
            }
            "integrity" => {
                c.execute_batch(
                    "PRAGMA ignore_check_constraints=ON; UPDATE characters SET name=''",
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        assert_eq!(
            f.error(Default::default()).code,
            "backupIntegrity",
            "{kind}"
        );
        assert_eq!(f.count_destination(), 0);
    }
}

#[test]
fn unknown_staging_entry_is_not_recursively_deleted() {
    let f = Fixture::new();
    let destination = f.destination.clone();
    let options = BackupOptions {
        hook: Some(Arc::new(move |phase| {
            if phase == "beforePublish" {
                let stage = fs::read_dir(&destination)
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap()
                    .path();
                fs::write(stage.join("unknown-sentinel"), b"must survive").unwrap();
            }
        })),
        fault: Some(Fault::Rename),
        ..Default::default()
    };
    let error = serde_json::to_value(f.error(options)).unwrap();
    assert_eq!(error["code"], "backupCleanupPending");
    let stage = f.destination.join(error["directoryName"].as_str().unwrap());
    assert_eq!(
        fs::read(stage.join("unknown-sentinel")).unwrap(),
        b"must survive"
    );
}

#[test]
fn final_database_opens_immutable_readonly_without_sidecars_and_has_private_modes() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let result = f.export();
    let root = f.destination.join(result.directory_name);
    assert_eq!(
        fs::metadata(&root).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(root.join("media"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    for name in ["library.sqlite3", "manifest.json"] {
        assert_eq!(
            fs::metadata(root.join(name)).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let uri = format!(
        "file:{}?immutable=1",
        root.join("library.sqlite3").display()
    );
    let c = Connection::open_with_flags(
        uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .unwrap();
    assert_eq!(
        c.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert!(!root.join("library.sqlite3-wal").exists());
    assert!(!root.join("library.sqlite3-shm").exists());
}

#[test]
fn directory_injection_after_readback_is_rejected_and_preserved_for_inspection() {
    let f = Fixture::new();
    let destination = f.destination.clone();
    let options = BackupOptions {
        hook: Some(Arc::new(move |phase| {
            if phase == "beforePublish" {
                let stage = fs::read_dir(&destination)
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap()
                    .path();
                fs::write(stage.join("unexpected"), b"unknown ownership").unwrap();
            }
        })),
        ..Default::default()
    };
    let value = serde_json::to_value(f.error(options)).unwrap();
    assert_eq!(value["code"], "backupCleanupPending");
    let stage = f.destination.join(value["directoryName"].as_str().unwrap());
    assert_eq!(
        fs::read(stage.join("unexpected")).unwrap(),
        b"unknown ownership"
    );
    assert_eq!(f.count_destination(), 1);
}

#[test]
fn source_database_symlink_never_exports_another_valid_library() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let unrelated = Fixture::new();
    unrelated.character("unrelated private library");
    let source = f.source.join("furfolio.sqlite3");
    fs::remove_file(&source).unwrap();
    symlink(unrelated.source.join("furfolio.sqlite3"), &source).unwrap();
    assert_eq!(f.error(Default::default()).code, "backupIntegrity");
    assert_eq!(f.count_destination(), 0);
}

#[test]
fn recursive_business_view_is_rejected_before_any_business_read() {
    let f = Fixture::new();
    let c = f.db.open().unwrap();
    c.execute_batch("DROP TABLE media_files; CREATE VIEW media_files AS WITH RECURSIVE spin(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM spin WHERE x<10000000) SELECT printf('%d',x) AS state FROM spin").unwrap();
    let started = std::time::Instant::now();
    assert_eq!(f.error(Default::default()).code, "backupIntegrity");
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(f.count_destination(), 0);
}

#[test]
fn manifest_deadline_is_backup_timeout_not_generic_io_failure() {
    let f = Fixture::new();
    let options = BackupOptions {
        timeout: Duration::from_secs(2),
        hook: Some(Arc::new(|phase| {
            if phase == "manifestEncoding" {
                thread::sleep(Duration::from_millis(2100));
            }
        })),
        ..Default::default()
    };
    assert_eq!(f.error(options).code, "backupTimeout");
    assert_eq!(f.count_destination(), 0);
}

#[test]
fn final_readback_rejects_database_mutation_and_unknown_media_entry() {
    for kind in ["database", "media"] {
        let f = Fixture::new();
        let destination = f.destination.clone();
        let options = BackupOptions {
            hook: Some(Arc::new(move |phase| {
                if phase != "beforePublish" {
                    return;
                }
                let stage = fs::read_dir(&destination)
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap()
                    .path();
                if kind == "database" {
                    let path = stage.join("library.sqlite3");
                    let mut bytes = fs::read(&path).unwrap();
                    bytes[0] ^= 1;
                    fs::write(path, bytes).unwrap();
                } else {
                    fs::write(stage.join("media").join("unknown"), b"unmanaged").unwrap();
                }
            })),
            ..Default::default()
        };
        let error = serde_json::to_value(f.error(options)).unwrap();
        assert_eq!(
            error["code"],
            if kind == "database" {
                "backupIntegrity"
            } else {
                "backupCleanupPending"
            }
        );
        assert!(error.get("published").is_none());
        if kind == "database" {
            assert_eq!(f.count_destination(), 0);
        } else {
            assert!(f
                .destination
                .join(error["directoryName"].as_str().unwrap())
                .join("media/unknown")
                .is_file());
        }
    }
}

#[test]
fn backup_step_busy_locked_and_total_call_budget_are_bounded() {
    for (busy, more, succeeds) in [(100, 0, true), (101, 0, false), (0, 1000, false)] {
        let f = Fixture::new();
        let options = BackupOptions {
            backup_busy_steps: busy,
            backup_more_steps: more,
            ..Default::default()
        };
        let result = export_with_options(&f.db, Some(&f.destination), options);
        if succeeds {
            assert!(result.unwrap().is_some());
        } else {
            assert_eq!(
                CommandError::from(result.unwrap_err()).code,
                "backupTimeout"
            );
            assert_eq!(f.count_destination(), 0);
        }
    }
}

#[test]
fn source_stream_rejects_midcopy_length_metadata_and_link_changes() {
    use std::io::Write;
    use std::os::unix::fs::MetadataExt;
    for kind in ["length", "mtime", "nlink"] {
        let f = Fixture::new();
        let id = f.character("test");
        f.import(&id);
        let (name, sha, size): (String, String, u64) =
            f.db.open()
                .unwrap()
                .query_row(
                    "SELECT name,sha256,byte_size FROM media_files WHERE kind='original'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap();
        let path = f.source.join("media").join(&name);
        let before = fs::metadata(&path).unwrap();
        struct MutatingWriter {
            path: PathBuf,
            extra: PathBuf,
            kind: &'static str,
            changed: bool,
        }
        impl Write for MutatingWriter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if !self.changed {
                    self.changed = true;
                    match self.kind {
                        "length" => fs::OpenOptions::new()
                            .append(true)
                            .open(&self.path)?
                            .write_all(b"x")?,
                        "mtime" => {
                            let file = fs::OpenOptions::new().write(true).open(&self.path)?;
                            file.set_modified(std::time::UNIX_EPOCH + Duration::from_secs(10))?;
                        }
                        "nlink" => fs::hard_link(&self.path, &self.extra)?,
                        _ => unreachable!(),
                    }
                }
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut output = MutatingWriter {
            path: path.clone(),
            extra: f.root.join("extra-link"),
            kind,
            changed: false,
        };
        let result = f.db.existing_media_store().unwrap().copy_verified(
            &name,
            &sha,
            size,
            &mut output,
            || Ok(()),
        );
        assert_eq!(
            CommandError::from(result.unwrap_err()).code,
            "backupIntegrity",
            "{kind}"
        );
        assert_eq!(before.ino(), fs::metadata(&path).unwrap().ino());
    }
}

#[test]
fn database_real_page_limit_accepts_64mib_and_rejects_next_page() {
    const LIMIT: u64 = 64 * 1024 * 1024;
    let f = Fixture::new();
    let c = f.db.open().unwrap();
    c.execute(
        "INSERT INTO settings(key,value) VALUES('synthetic-big',zeroblob(?1))",
        [LIMIT - 1024 * 1024],
    )
    .unwrap();
    let page_size: u64 = c
        .pragma_query_value(None, "page_size", |r| r.get(0))
        .unwrap();
    let pages: u64 = c
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    // Grow a real free-page tail in whole-page increments, without forging SQLite bytes.
    let mut needed = (LIMIT / page_size).saturating_sub(pages);
    while needed > 0 {
        let amount = if needed > 16 {
            (needed - 8) * page_size
        } else {
            page_size / 2
        };
        let key = format!("padding-{needed}");
        c.execute(
            "INSERT INTO settings(key,value) VALUES(?1,zeroblob(?2))",
            params![key, amount],
        )
        .unwrap();
        let now: u64 = c
            .pragma_query_value(None, "page_count", |r| r.get(0))
            .unwrap();
        if now > LIMIT / page_size {
            break;
        }
        needed = LIMIT / page_size - now;
    }
    let actual: u64 = c
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    assert_eq!(
        actual * page_size,
        LIMIT,
        "fixture must hit the real page boundary exactly"
    );
    let result = f.export();
    assert!(result.total_bytes <= LIMIT);
    c.execute(
        "INSERT INTO settings(key,value) VALUES('over-limit',zeroblob(?1))",
        [page_size * 2],
    )
    .unwrap();
    assert_eq!(f.error(Default::default()).code, "backupLimit");
}

#[test]
fn midwrite_media_sync_and_replaced_cleanup_entry_are_safe() {
    for fault in [Fault::MidWrite, Fault::MediaSync, Fault::CleanupIdentity] {
        let f = Fixture::new();
        let character = f.character("synthetic");
        f.import(&character);
        let destination = f.destination.clone();
        let options = BackupOptions {
            fault: Some(fault),
            hook: Some(Arc::new(move |phase| {
                if fault == Fault::CleanupIdentity && phase == "beforePublish" {
                    let stage = fs::read_dir(&destination)
                        .unwrap()
                        .next()
                        .unwrap()
                        .unwrap()
                        .path();
                    let original = stage.join("library.sqlite3");
                    fs::rename(&original, stage.join("captured-original")).unwrap();
                    fs::write(original, b"replacement sentinel must survive").unwrap();
                }
            })),
            ..Default::default()
        };
        let value = serde_json::to_value(f.error(options)).unwrap();
        assert!(value.get("published").is_none());
        if fault == Fault::CleanupIdentity {
            assert_eq!(value["code"], "backupCleanupPending");
            let stage = f.destination.join(value["directoryName"].as_str().unwrap());
            assert_eq!(
                fs::read(stage.join("library.sqlite3")).unwrap(),
                b"replacement sentinel must survive"
            );
            assert!(stage.join("captured-original").is_file());
        } else {
            assert_eq!(value["code"], "backupFailed");
            assert_eq!(f.count_destination(), 0);
        }
    }
}

#[test]
fn unsupported_export_does_not_break_character_crud() {
    let f = Fixture::new();
    let id = f.character("test");
    assert_eq!(
        f.error(BackupOptions {
            unsupported: true,
            ..Default::default()
        })
        .code,
        "backupUnsupported"
    );
    assert_eq!(characters::get_character(&f.db, &id).unwrap().name, "test");
    characters::delete_character(&f.db, &id).unwrap();
    assert!(characters::list_characters(&f.db).unwrap().is_empty());
}
