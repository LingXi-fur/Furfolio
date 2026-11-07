use super::*;
use crate::{characters, media, models::CharacterInput};
use image::{DynamicImage, ImageFormat};
use rusqlite::Connection;
use std::fs;
use uuid::Uuid;

/// Exports a synthetic one-character, one-image library and returns its backup directory.
struct Source {
    root: std::path::PathBuf,
    backup: std::path::PathBuf,
    character_count: u64,
    asset_count: u64,
}
impl Source {
    fn new() -> Self {
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("furfolio-restore-source-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let library = root.join("library");
        let destination = root.join("destination");
        fs::create_dir(&destination).unwrap();
        let db = Database::initialize(&library).unwrap();
        let character = characters::create_character(
            &db,
            CharacterInput {
                name: "Nova".into(),
                species: Some("Wolf".into()),
                pronouns: None,
                description: None,
                notes: Some("restore round-trip notes".into()),
                tags: vec![],
                colors: vec![],
            },
        )
        .unwrap()
        .id;
        let path = root.join("synthetic.png");
        let mut cursor = std::io::Cursor::new(Vec::new());
        DynamicImage::new_rgb8(16, 12)
            .write_to(&mut cursor, ImageFormat::Png)
            .unwrap();
        fs::write(&path, cursor.into_inner()).unwrap();
        media::import_selected(&db, &character, Some(&path))
            .unwrap()
            .unwrap();
        let result = crate::backup::export_selected(&db, Some(&destination))
            .unwrap()
            .unwrap();
        Self {
            root,
            backup: destination.join(&result.directory_name),
            character_count: result.character_count,
            asset_count: result.asset_count,
        }
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Target {
    root: std::path::PathBuf,
    db: Database,
}
impl Target {
    fn empty() -> Self {
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("furfolio-restore-target-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let db = Database::initialize(&root).unwrap();
        Self { root, db }
    }
}
impl Drop for Target {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn media_entries(source: &Source) -> Vec<String> {
    fs::read_dir(source.backup.join("media"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect()
}

#[test]
fn restore_round_trips_characters_and_media_into_an_empty_library() {
    let source = Source::new();
    let target = Target::empty();
    let result = restore_from_directory(&target.db, Some(&source.backup))
        .unwrap()
        .unwrap();
    assert_eq!(result.character_count, source.character_count);
    assert_eq!(result.asset_count, source.asset_count);

    let connection = target.db.open().unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM characters", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        source.character_count
    );
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM media_files", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        source.asset_count * 2
    );
    assert_eq!(
        connection.query_row(
            "SELECT name FROM characters WHERE id=(SELECT id FROM characters)",
            [],
            |r| r.get::<_, String>(0)
        ),
        Ok("Nova".into())
    );

    // Every backup media file is byte-identical in the restored library.
    for name in media_entries(&source) {
        let expected = fs::read(source.backup.join("media").join(&name)).unwrap();
        let actual = fs::read(target.root.join("media").join(&name)).unwrap();
        assert_eq!(actual, expected);
    }
}

#[test]
fn restore_is_null_when_no_directory_is_selected() {
    let target = Target::empty();
    assert!(restore_from_directory(&target.db, None).unwrap().is_none());
}

#[test]
fn restore_requires_an_empty_library() {
    let source = Source::new();
    let target = Target::empty();
    characters::create_character(
        &target.db,
        CharacterInput {
            name: "existing".into(),
            species: None,
            pronouns: None,
            description: None,
            notes: None,
            tags: vec![],
            colors: vec![],
        },
    )
    .unwrap();
    let error = restore_from_directory(&target.db, Some(&source.backup)).unwrap_err();
    assert!(matches!(error, AppError::Restore(RestoreError::NotEmpty)));
}

#[test]
fn restore_rejects_a_missing_media_file() {
    let source = Source::new();
    let target = Target::empty();
    let first = media_entries(&source)
        .into_iter()
        .next()
        .expect("exported media");
    fs::remove_file(source.backup.join("media").join(&first)).unwrap();
    let error = restore_from_directory(&target.db, Some(&source.backup)).unwrap_err();
    assert!(matches!(error, AppError::Restore(RestoreError::Integrity)));
}

#[test]
fn restore_rejects_a_corrupted_media_hash() {
    let source = Source::new();
    let target = Target::empty();
    let first = media_entries(&source)
        .into_iter()
        .next()
        .expect("exported media");
    fs::write(source.backup.join("media").join(&first), b"corrupted").unwrap();
    let error = restore_from_directory(&target.db, Some(&source.backup)).unwrap_err();
    assert!(matches!(error, AppError::Restore(RestoreError::Integrity)));
}

#[test]
fn restore_rejects_a_tampered_manifest_format() {
    let source = Source::new();
    let target = Target::empty();
    let manifest_path = source.backup.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["format"] = serde_json::json!("evil-format");
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let error = restore_from_directory(&target.db, Some(&source.backup)).unwrap_err();
    assert!(matches!(error, AppError::Restore(RestoreError::Integrity)));
}

#[test]
fn restore_rejects_a_manifest_that_drops_a_media_entry() {
    let source = Source::new();
    let target = Target::empty();
    let manifest_path = source.backup.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["media"] = serde_json::json!([]);
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    // The database still references its media; the manifest no longer lists it.
    let error = restore_from_directory(&target.db, Some(&source.backup)).unwrap_err();
    assert!(matches!(error, AppError::Restore(RestoreError::Integrity)));
}

#[test]
fn restored_database_opens_read_only_without_source_sidecars() {
    let source = Source::new();
    let target = Target::empty();
    restore_from_directory(&target.db, Some(&source.backup)).unwrap();
    // The restored snapshot is standalone: no WAL/SHM sidecars were copied from the source.
    assert!(!target.root.join("furfolio.sqlite3-wal").exists());
    let read_only = Connection::open_with_flags(
        target.root.join("furfolio.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        read_only
            .query_row("SELECT count(*) FROM schema_migrations", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        2
    );
}
