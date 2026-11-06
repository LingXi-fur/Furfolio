//! Real Windows success-path contracts. No unsupported assertions or emulated OS results.
use crate::{
    backup, characters,
    database::Database,
    media,
    media_store::hash,
    models::{Asset, CharacterInput},
    platform_fs,
};
use image::{DynamicImage, ImageFormat};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::{fs, io::Cursor, os::windows::fs::OpenOptionsExt, path::PathBuf};
use uuid::Uuid;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE,
};

struct Fixture {
    root: PathBuf,
    library: PathBuf,
    destination: PathBuf,
    db: Database,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("furfolio-windows-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let library = root.join("角色资料库 Unicode space");
        let destination = root.join("备份 destination");
        fs::create_dir(&destination).unwrap();
        let db = Database::initialize(&library).expect("fixture initialize");
        Self {
            root,
            library,
            destination,
            db,
        }
    }
    fn character(&self) -> String {
        characters::create_character(
            &self.db,
            CharacterInput {
                name: "合成 Nova".into(),
                species: None,
                pronouns: None,
                description: None,
                notes: Some("synthetic private note".into()),
                tags: vec![],
                colors: vec![],
            },
        )
        .unwrap()
        .id
    }
    fn import(&self, character: &str, format: ImageFormat) -> (Asset, Vec<u8>) {
        let source = self.root.join("合成 原图.wrong-extension");
        let mut cursor = Cursor::new(Vec::new());
        DynamicImage::new_rgb8(16, 12)
            .write_to(&mut cursor, format)
            .unwrap();
        let bytes = cursor.into_inner();
        fs::write(&source, &bytes).unwrap();
        let asset = media::import_selected(&self.db, character, Some(&source))
            .expect("Windows must import a supported synthetic image")
            .unwrap();
        assert_eq!(fs::read(source).unwrap(), bytes);
        (asset, bytes)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let result = fs::remove_dir_all(&self.root);
        if !std::thread::panicking() {
            result.expect("Windows fixture cleanup must expose leaked handles");
        }
    }
}

#[test]
fn windows_media_formats_preview_delete_and_reopen_succeed() {
    for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::WebP] {
        let f = Fixture::new();
        let character = f.character();
        let (asset, original) = f.import(&character, format);
        assert_eq!((asset.width, asset.height), (16, 12));
        let stored: String =
            f.db.open()
                .unwrap()
                .query_row(
                    "SELECT name FROM media_files WHERE asset_id=?1 AND kind='original'",
                    [&asset.id],
                    |row| row.get(0),
                )
                .unwrap();
        assert_eq!(
            fs::read(f.library.join("media").join(stored)).unwrap(),
            original
        );
        let db = Database::initialize(&f.library).unwrap();
        assert_eq!(media::list_assets(&db, &character).unwrap().len(), 1);
        assert!(media::read_asset_preview(&db, &character, &asset.id)
            .unwrap()
            .starts_with(b"\x89PNG\r\n\x1a\n"));
        media::delete_asset(&db, &character, &asset.id).unwrap();
        assert!(media::list_assets(&db, &character).unwrap().is_empty());
        assert_eq!(media::media_cleanup_status(&db).unwrap().pending, 0);
        assert_eq!(fs::read_dir(f.library.join("media")).unwrap().count(), 0);
    }
}

#[test]
fn windows_initialization_recovers_only_registered_pending_media() {
    let f = Fixture::new();
    let character = f.character();
    let (asset, _) = f.import(&character, ImageFormat::Png);
    let connection = f.db.open().unwrap();
    connection
        .execute("DELETE FROM assets WHERE id=?1", [&asset.id])
        .unwrap();
    connection
        .execute(
            "UPDATE media_files SET state='pendingImport' WHERE asset_id=?1",
            [&asset.id],
        )
        .unwrap();
    drop(connection);
    fs::write(f.library.join("media/unknown.txt"), b"unknown sentinel").unwrap();
    let db = Database::initialize(&f.library).unwrap();
    assert_eq!(media::media_cleanup_status(&db).unwrap().pending, 0);
    assert_eq!(
        fs::read(f.library.join("media/unknown.txt")).unwrap(),
        b"unknown sentinel"
    );
    assert_eq!(fs::read_dir(f.library.join("media")).unwrap().count(), 1);
}

fn verify_backup(f: &Fixture, expected_assets: u64) {
    let result = backup::export_selected(&f.db, Some(&f.destination))
        .expect("Windows must visibly publish a verified backup, not permanent uncertain")
        .unwrap();
    assert_eq!(result.asset_count, expected_assets);
    let output = f.destination.join(&result.directory_name);
    assert_eq!(fs::read_dir(&f.destination).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&output).unwrap().count(), 3);
    let manifest_bytes = fs::read(output.join("manifest.json")).unwrap();
    assert!(!String::from_utf8_lossy(&manifest_bytes).contains("synthetic private note"));
    let manifest: Value = serde_json::from_slice(&manifest_bytes).unwrap();
    let mut total = 0;
    for entry in std::iter::once(&manifest["database"]).chain(manifest["media"].as_array().unwrap())
    {
        let bytes = fs::read(output.join(entry["path"].as_str().unwrap())).unwrap();
        assert_eq!(entry["size"].as_u64(), Some(bytes.len() as u64));
        assert_eq!(entry["sha256"].as_str(), Some(hash(&bytes).as_str()));
        total += bytes.len() as u64;
    }
    assert_eq!(total, result.total_bytes);
    assert_eq!(manifest["totalBytes"].as_u64(), Some(total));
    let exported = Connection::open_with_flags(
        output.join("library.sqlite3"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        exported
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(
        exported
            .query_row("SELECT count(*) FROM assets", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        expected_assets
    );
    assert_eq!(
        exported
            .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, u64>(
                0
            ))
            .unwrap(),
        0
    );
    assert!(!output.join("library.sqlite3-wal").exists());
    assert!(!output.join("library.sqlite3-shm").exists());
}

#[test]
fn windows_empty_backup_succeeds_without_durability_uncertain() {
    verify_backup(&Fixture::new(), 0);
}

#[test]
fn windows_wal_and_media_backup_is_independent_and_preserves_source() {
    let f = Fixture::new();
    let connection = f.db.open().unwrap();
    connection
        .pragma_update(None, "wal_autocheckpoint", 0)
        .unwrap();
    let character = f.character();
    let (asset, _) = f.import(&character, ImageFormat::Png);
    assert!(
        f.library
            .join("furfolio.sqlite3-wal")
            .metadata()
            .unwrap()
            .len()
            > 0
    );
    let before = media::read_asset_preview(&f.db, &character, &asset.id).unwrap();
    verify_backup(&f, 1);
    assert_eq!(
        media::read_asset_preview(&f.db, &character, &asset.id).unwrap(),
        before
    );
    assert_eq!(media::list_assets(&f.db, &character).unwrap().len(), 1);
    drop(connection);
}

#[test]
fn windows_hardlinked_source_is_rejected_without_touching_either_name() {
    let f = Fixture::new();
    let character = f.character();
    let source = f.root.join("linked.png");
    let alias = f.root.join("alias.png");
    let mut cursor = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(8, 8)
        .write_to(&mut cursor, ImageFormat::Png)
        .unwrap();
    fs::write(&source, cursor.get_ref()).unwrap();
    fs::hard_link(&source, &alias).unwrap();
    assert!(media::import_selected(&f.db, &character, Some(&source)).is_err());
    assert_eq!(fs::read(source).unwrap(), *cursor.get_ref());
    assert_eq!(fs::read(alias).unwrap(), *cursor.get_ref());
    assert!(media::list_assets(&f.db, &character).unwrap().is_empty());
}

#[test]
fn windows_created_objects_have_user_owner_protected_three_ace_dacl_no_inheritance() {
    let f = Fixture::new();
    let directory = platform_fs::Directory::open(&f.root.join("audit-root"), true).unwrap();
    let mut file = directory.open_file("audit.bin", true).unwrap();
    std::io::Write::write_all(&mut file, b"audit").unwrap();
    file.sync_all().unwrap();
    let child = directory.child("audit-child", true).unwrap();
    drop(child);
    for object in [
        f.root.join("audit-root"),
        f.root.join("audit-root/audit.bin"),
        f.root.join("audit-root/audit-child"),
    ] {
        let handle = if object.is_dir() {
            fs::OpenOptions::new()
                .read(true)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
                .open(&object)
                .unwrap()
        } else {
            fs::File::open(&object).unwrap()
        };
        let audit = platform_fs::security_summary(&handle).unwrap();
        let user = platform_fs::current_sid_string().unwrap();
        assert_eq!(audit.owner, user);
        assert_eq!(audit.group, user);
        assert!(audit.dacl_present);
        assert!(audit.dacl_protected);
        assert_eq!(audit.aces.len(), 3);
        let mut subjects = Vec::new();
        for (kind, flags, mask, subject) in audit.aces {
            assert_eq!(kind, 0, "only ACCESS_ALLOWED_ACE is expected");
            assert_eq!(flags, 0, "ACE must be explicit and non-inheriting");
            assert_eq!(mask, 0x001f_01ff, "only FILE_ALL_ACCESS is expected");
            subjects.push(subject);
        }
        subjects.sort();
        let mut expected = vec![user, "S-1-5-18".to_owned(), "S-1-5-32-544".to_owned()];
        expected.sort();
        assert_eq!(subjects, expected);
    }
}

#[test]
fn windows_held_parent_cannot_be_renamed_and_created_file_has_one_full_id() {
    let f = Fixture::new();
    let private = f.root.join("private-parent");
    let directory = platform_fs::Directory::open(&private, true).unwrap();
    let mut file = directory.open_file("owned.bin", true).unwrap();
    std::io::Write::write_all(&mut file, b"owned").unwrap();
    file.sync_all().unwrap();
    platform_fs::verify_private(&file).unwrap();
    let state = platform_fs::file_state(&file).unwrap();
    assert_eq!(state.links, 1);
    assert_eq!(state.size, 5);
    assert!(!state.directory);
    assert!(fs::rename(&private, f.root.join("replaced-parent")).is_err());
    drop(file);
    let reopened = directory.open_file("owned.bin", false).unwrap();
    assert_eq!(platform_fs::file_state(&reopened).unwrap(), state);
}

#[test]
fn windows_lock_is_nonblocking_and_process_death_releases_fixed_range() {
    const CHILD_ENV: &str = "FURFOLIO_SYNTHETIC_LOCK_CHILD";
    if let Some(root) = std::env::var_os(CHILD_ENV) {
        let root = PathBuf::from(root);
        let _lock = platform_fs::lock_library(&root.join("furfolio.sqlite3")).unwrap();
        fs::write(root.join("child-ready"), b"ready").unwrap();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
    let f = Fixture::new();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "windows_tests::windows_lock_is_nonblocking_and_process_death_releases_fixed_range",
            "--nocapture",
        ])
        .env(CHILD_ENV, &f.library)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let started = std::time::Instant::now();
    while !f.library.join("child-ready").exists()
        && started.elapsed() < std::time::Duration::from_secs(10)
    {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("lock child exited before ready: {status}");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    if !f.library.join("child-ready").exists() {
        child.kill().unwrap();
        child.wait().unwrap();
        panic!("lock child failed to become ready");
    }
    let busy = platform_fs::lock_library(&f.library.join("furfolio.sqlite3"));
    assert!(matches!(busy, Err(crate::error::AppError::MediaBusy)));
    child.kill().unwrap();
    child.wait().unwrap();
    let _lock = platform_fs::lock_library(&f.library.join("furfolio.sqlite3")).unwrap();
}

#[test]
fn windows_concurrent_production_initialization_serializes_migration_and_recovery() {
    let f = Fixture::new();
    let path = f.root.join("concurrent library");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let workers: Vec<_> = (0..2)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                Database::initialize(&path)
            })
        })
        .collect();
    barrier.wait();
    for worker in workers {
        worker.join().unwrap().unwrap();
    }
    let db = Database::initialize(&path).unwrap();
    assert_eq!(media::media_cleanup_status(&db).unwrap().pending, 0);
}

#[test]
fn windows_spike_parent_relative_nonreplace_rename_and_full_identity() {
    let f = Fixture::new();
    let source = f.destination.join("stage.bin");
    fs::write(&source, b"owned payload").unwrap();
    let parent = fs::OpenOptions::new()
        .read(true)
        .access_mode(FILE_GENERIC_READ | FILE_GENERIC_WRITE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(&f.destination)
        .unwrap();
    let file = fs::OpenOptions::new()
        .read(true)
        // GENERIC_READ | DELETE; rename requires DELETE on the captured source handle.
        .access_mode(0x8000_0000 | 0x0001_0000)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .open(&source)
        .unwrap();
    let before = platform_fs::identity(&file).unwrap();
    fs::write(f.destination.join("collision.bin"), b"outside sentinel").unwrap();
    assert!(platform_fs::rename_exclusive(&file, &parent, "collision.bin").is_err());
    assert_eq!(
        fs::read(f.destination.join("collision.bin")).unwrap(),
        b"outside sentinel"
    );
    assert_eq!(fs::read(&source).unwrap(), b"owned payload");
    platform_fs::rename_exclusive(&file, &parent, "published.bin").unwrap();
    assert!(!source.exists());
    assert_eq!(
        fs::read(f.destination.join("published.bin")).unwrap(),
        b"owned payload"
    );
    let named = fs::File::open(f.destination.join("published.bin")).unwrap();
    assert_eq!(platform_fs::identity(&named).unwrap(), before);
    assert_eq!(platform_fs::identity(&file).unwrap(), before);
}

#[test]
fn windows_deep_path_library_and_backup_stay_verbatim_safe() {
    // A user-visible path deeper than MAX_PATH-260 must still initialize, import,
    // and export: the platform walk is component-wise on verbatim \\?\ handles.
    let mut root = std::env::temp_dir();
    root.push(format!("furfolio-deep-{}", Uuid::new_v4()));
    let long_segment = "深深的角色资料库目录-".repeat(6);
    let mut library = root.clone();
    for _ in 0..8 {
        library.push(&long_segment);
    }
    let mut destination = root.clone();
    destination.push("备份 目标");
    fs::create_dir_all(&destination).unwrap();
    let db = Database::initialize(&library).unwrap();
    let character = characters::create_character(
        &db,
        CharacterInput {
            name: "合成 Deep".into(),
            species: None,
            pronouns: None,
            description: None,
            notes: Some("deep-path note".into()),
            tags: vec![],
            colors: vec![],
        },
    )
    .unwrap()
    .id;
    // Cancel semantics on a deep path: no picker selection is not an error.
    media::import_selected(&db, &character, None)
        .unwrap()
        .unwrap_none();
    // A real import under the deep path also works: the source lives on the normal
    // visible path while the library itself sits deep.
    let source = root.join("合成 原图.png");
    let mut cursor = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(16, 12)
        .write_to(&mut cursor, ImageFormat::Png)
        .unwrap();
    fs::write(&source, cursor.get_ref()).unwrap();
    media::import_selected(&db, &character, Some(&source))
        .expect("deep-path library must import a synthetic image")
        .unwrap();
    let result = backup::export_selected(&db, Some(&destination))
        .expect("deep-path export must publish")
        .unwrap();
    let output = destination.join(&result.directory_name);
    assert!(output.join("library.sqlite3").is_file());
    assert!(output.join("manifest.json").is_file());
    fs::remove_dir_all(&root).unwrap();
}
