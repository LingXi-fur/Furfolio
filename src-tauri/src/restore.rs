//! Offline, bounded, native-only whole-library restore. The picked directory is
//! untrusted: the manifest is parsed within a size budget, every file is hash-verified
//! against it before it is materialized, and the live library must be empty.
use crate::{
    database::Database,
    error::{AppError, RestoreError},
    models::RestoreResult,
};
use std::path::Path;

#[cfg(any(target_os = "macos", windows))]
use {
    serde::Deserialize,
    std::{
        collections::{BTreeMap, BTreeSet},
        io::{Read, Write},
    },
};

// Mirrors backup::BackupOptions::default(); schema matches database::LATEST_SCHEMA_VERSION.
#[cfg(any(target_os = "macos", windows))]
const FORMAT: &str = "furfolio-library-backup";
#[cfg(any(target_os = "macos", windows))]
const MANIFEST_VERSION: u32 = 1;
#[cfg(any(target_os = "macos", windows))]
const SCHEMA_VERSION: u32 = 2;
#[cfg(any(target_os = "macos", windows))]
const DATABASE_LIMIT: u64 = 64 * 1024 * 1024;
#[cfg(any(target_os = "macos", windows))]
const CHARACTER_LIMIT: u64 = 10_000;
#[cfg(any(target_os = "macos", windows))]
const ASSET_LIMIT: u64 = 1_000;
#[cfg(any(target_os = "macos", windows))]
const MEDIA_LIMIT: u64 = 2_000;
#[cfg(any(target_os = "macos", windows))]
const PAYLOAD_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
#[cfg(any(target_os = "macos", windows))]
const MANIFEST_LIMIT: u64 = 2 * 1024 * 1024;

#[cfg(any(target_os = "macos", windows))]
fn failure(error: RestoreError) -> AppError {
    AppError::Restore(error)
}
#[cfg(any(target_os = "macos", windows))]
fn invalid() -> AppError {
    failure(RestoreError::Integrity)
}

#[cfg(any(target_os = "macos", windows))]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestoreManifest {
    format: String,
    version: u32,
    schema_version: u32,
    character_count: u64,
    asset_count: u64,
    total_bytes: u64,
    database: RestoreDatabaseEntry,
    media: Vec<RestoreMediaEntry>,
}
#[cfg(any(target_os = "macos", windows))]
#[derive(Deserialize)]
struct RestoreDatabaseEntry {
    path: String,
    size: u64,
    sha256: String,
}
#[cfg(any(target_os = "macos", windows))]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestoreMediaEntry {
    path: String,
    asset_id: String,
    kind: String,
    size: u64,
    sha256: String,
}

pub(crate) fn restore_with_picker(
    db: &Database,
    picker: impl FnOnce() -> Result<Option<std::path::PathBuf>, AppError>,
) -> Result<Option<RestoreResult>, AppError> {
    if cfg!(not(any(target_os = "macos", windows))) {
        return Err(AppError::Restore(RestoreError::Unsupported));
    }
    let _export = db
        .lock_export()
        .map_err(|_| AppError::Restore(RestoreError::Busy))?;
    let selected = picker().map_err(|_| AppError::Restore(RestoreError::Destination))?;
    restore_from_directory(db, selected.as_deref())
}

pub(crate) fn restore_from_directory(
    db: &Database,
    source: Option<&Path>,
) -> Result<Option<RestoreResult>, AppError> {
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (db, source);
        return Err(AppError::Restore(RestoreError::Unsupported));
    }
    #[cfg(any(target_os = "macos", windows))]
    {
        let Some(source) = source else {
            return Ok(None);
        };
        let _media = db.lock_media().map_err(|e| match e {
            AppError::MediaBusy => failure(RestoreError::Busy),
            _ => invalid(),
        })?;
        ensure_empty(db)?;

        let manifest_bytes = read_source_bounded(&source.join("manifest.json"), MANIFEST_LIMIT)?;
        let manifest: RestoreManifest =
            serde_json::from_slice(&manifest_bytes).map_err(|_| invalid())?;
        validate_manifest(&manifest)?;

        // Hash-verify the standalone database snapshot before trusting any of it.
        let database_bytes =
            read_source_bounded(&source.join(&manifest.database.path), DATABASE_LIMIT)?;
        if database_bytes.len() as u64 != manifest.database.size
            || crate::media_store::hash(&database_bytes) != manifest.database.sha256
        {
            return Err(invalid());
        }

        // Cross-check the manifest's media set against the database's own ledger rows.
        let connection = rusqlite::Connection::open_with_flags(
            source.join(&manifest.database.path),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .map_err(|_| invalid())?;
        validate_source_database(&connection, &manifest)?;

        materialize(db, source, &manifest, &database_bytes)
    }
}

#[cfg(any(target_os = "macos", windows))]
fn read_source_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, AppError> {
    let (mut file, _parent) = crate::platform_fs::open_source(path).map_err(|_| invalid())?;
    let before = crate::platform_fs::file_state(&file).map_err(|_| invalid())?;
    if before.directory || before.links != 1 || before.size > limit {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    if bytes.len() as u64 > limit
        || crate::platform_fs::file_state(&file).map_err(|_| invalid())? != before
    {
        return Err(invalid());
    }
    Ok(bytes)
}

#[cfg(any(target_os = "macos", windows))]
fn ensure_empty(db: &Database) -> Result<(), AppError> {
    let connection = db.open().map_err(|_| invalid())?;
    let count: u64 = connection
        .query_row(
            "SELECT (SELECT count(*) FROM characters) \
                   + (SELECT count(*) FROM assets) \
                   + (SELECT count(*) FROM media_files)",
            [],
            |row| row.get(0),
        )
        .map_err(|_| invalid())?;
    if count != 0 {
        return Err(failure(RestoreError::NotEmpty));
    }
    Ok(())
}

#[cfg(any(target_os = "macos", windows))]
fn validate_manifest(manifest: &RestoreManifest) -> Result<(), AppError> {
    if manifest.format != FORMAT
        || manifest.version != MANIFEST_VERSION
        || manifest.schema_version != SCHEMA_VERSION
    {
        return Err(invalid());
    }
    if manifest.database.path != "library.sqlite3"
        || manifest.database.size > DATABASE_LIMIT
        || !crate::backup::digest(&manifest.database.sha256)
    {
        return Err(invalid());
    }
    if manifest.character_count > CHARACTER_LIMIT
        || manifest.asset_count > ASSET_LIMIT
        || manifest.media.len() as u64 > MEDIA_LIMIT
        || manifest.total_bytes > PAYLOAD_LIMIT
    {
        return Err(failure(RestoreError::Limit));
    }
    let mut paths = BTreeSet::new();
    for entry in &manifest.media {
        let Some(name) = entry.path.strip_prefix("media/") else {
            return Err(invalid());
        };
        if !crate::backup::uuid(&entry.asset_id)
            || !crate::backup::digest(&entry.sha256)
            || entry.size > crate::media_store::MAX_SOURCE
            || !paths.insert(entry.path.as_str())
        {
            return Err(invalid());
        }
        match entry.kind.as_str() {
            "original" if crate::backup::managed(name, ".bin") => {}
            "preview" if crate::backup::managed(name, ".png") => {}
            _ => return Err(invalid()),
        }
    }
    Ok(())
}

#[cfg(any(target_os = "macos", windows))]
fn validate_source_database(
    connection: &rusqlite::Connection,
    manifest: &RestoreManifest,
) -> Result<(), AppError> {
    if connection
        .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
        .map_err(|_| invalid())?
        != "ok"
    {
        return Err(invalid());
    }
    let mut foreign = connection
        .prepare("PRAGMA foreign_key_check")
        .map_err(|_| invalid())?;
    if foreign
        .query([])
        .map_err(|_| invalid())?
        .next()
        .map_err(|_| invalid())?
        .is_some()
    {
        return Err(invalid());
    }
    if !Database::backup_schema_matches(connection).map_err(|_| invalid())? {
        return Err(invalid());
    }
    let (characters, assets, media_files): (u64, u64, u64) = connection
        .query_row(
            "SELECT (SELECT count(*) FROM characters), \
                   (SELECT count(*) FROM assets), \
                   (SELECT count(*) FROM media_files)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|_| invalid())?;
    if characters != manifest.character_count
        || assets != manifest.asset_count
        || media_files != manifest.media.len() as u64
        || media_files != assets.checked_mul(2).ok_or_else(invalid)?
    {
        return Err(invalid());
    }
    if connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM media_files WHERE state!='ready')",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| invalid())?
    {
        return Err(invalid());
    }
    // Every ledger row must have a matching manifest entry with identical size/hash, so a
    // tampered (unhashed) manifest cannot drop or retarget a file the database references.
    let mut by_path = BTreeMap::new();
    for entry in &manifest.media {
        if by_path.insert(entry.path.as_str(), entry).is_some() {
            return Err(invalid());
        }
    }
    let mut statement = connection
        .prepare("SELECT name, sha256, byte_size FROM media_files ORDER BY name")
        .map_err(|_| invalid())?;
    let mut rows = statement.query([]).map_err(|_| invalid())?;
    let mut seen = 0u64;
    while let Some(row) = rows.next().map_err(|_| invalid())? {
        let name: String = row.get(0).map_err(|_| invalid())?;
        let sha256: String = row.get(1).map_err(|_| invalid())?;
        let size: u64 = row.get(2).map_err(|_| invalid())?;
        let Some(entry) = by_path.get(format!("media/{name}").as_str()) else {
            return Err(invalid());
        };
        if entry.sha256 != sha256 || entry.size != size {
            return Err(invalid());
        }
        seen += 1;
    }
    if seen != manifest.media.len() as u64 {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(any(target_os = "macos", windows))]
fn materialize(
    db: &Database,
    source: &Path,
    manifest: &RestoreManifest,
    database_bytes: &[u8],
) -> Result<Option<RestoreResult>, AppError> {
    let root =
        crate::media_store::private_directory(db.library_root().map_err(|_| invalid())?, false)?;

    // Replace the disposable empty database with the verified snapshot bytes. The
    // existing file must carry a delete-capable handle before it is unlinked.
    let existing = root
        .open_mutable("furfolio.sqlite3")
        .map_err(|_| invalid())?;
    root.remove_file("furfolio.sqlite3", &existing)
        .map_err(|_| invalid())?;
    drop(existing);
    {
        let mut output = root
            .open_file("furfolio.sqlite3", true)
            .map_err(|_| invalid())?;
        output.write_all(database_bytes).map_err(|_| invalid())?;
        output.sync_all().map_err(|_| invalid())?;
    }
    root.sync().map_err(|_| invalid())?;

    let media = root.child("media", true).map_err(|_| invalid())?;
    for entry in &manifest.media {
        let name = entry.path.strip_prefix("media/").ok_or_else(invalid)?;
        let bytes = read_source_bounded(&source.join(&entry.path), crate::media_store::MAX_SOURCE)?;
        if bytes.len() as u64 != entry.size || crate::media_store::hash(&bytes) != entry.sha256 {
            return Err(invalid());
        }
        let mut output = media.open_file(name, true).map_err(|_| invalid())?;
        output.write_all(&bytes).map_err(|_| invalid())?;
        output.sync_all().map_err(|_| invalid())?;
    }
    media.sync().map_err(|_| invalid())?;

    // Smoke-check the materialized library opens and is intact before reporting success.
    let connection = db.open().map_err(|_| invalid())?;
    if connection
        .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
        .map_err(|_| invalid())?
        != "ok"
    {
        return Err(invalid());
    }
    Ok(Some(RestoreResult {
        character_count: manifest.character_count,
        asset_count: manifest.asset_count,
        total_bytes: manifest.total_bytes,
    }))
}

#[cfg(all(test, any(target_os = "macos", windows)))]
#[path = "restore_tests.rs"]
mod tests;
