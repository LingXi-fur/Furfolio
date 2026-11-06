//! Offline, bounded, native-only whole-library export.
use crate::{
    database::Database,
    error::{AppError, BackupError},
    models::BackupExportResult,
};
use std::path::Path;
#[cfg(any(target_os = "macos", windows))]
use std::time::{Duration, Instant};
#[cfg(any(target_os = "macos", windows))]
use {
    rusqlite::{
        backup::{Backup, StepResult},
        Connection, MAIN_DB,
    },
    serde::Serialize,
    std::{collections::BTreeSet, io::Write},
};

#[cfg(all(test, any(target_os = "macos", windows)))]
type PhaseHook = std::sync::Arc<dyn Fn(&str) + Send + Sync>;

#[cfg(not(any(target_os = "macos", windows)))]
#[derive(Clone, Default)]
pub(crate) struct BackupOptions {}
#[cfg(any(target_os = "macos", windows))]
#[derive(Clone)]
pub(crate) struct BackupOptions {
    pub database_limit: u64,
    pub character_limit: u64,
    pub asset_limit: u64,
    pub media_limit: u64,
    pub payload_limit: u64,
    pub manifest_limit: u64,
    pub timeout: Duration,
    pub snapshot_timeout: Duration,
    started: Instant,
    #[cfg(test)]
    pub fault: Option<Fault>,
    #[cfg(test)]
    pub hook: Option<PhaseHook>,
    #[cfg(test)]
    pub fixed_uuid: Option<uuid::Uuid>,
    #[cfg(test)]
    pub unsupported: bool,
    #[cfg(test)]
    pub backup_busy_steps: u64,
    #[cfg(test)]
    pub backup_more_steps: u64,
}
#[cfg(any(target_os = "macos", windows))]
impl Default for BackupOptions {
    fn default() -> Self {
        Self {
            database_limit: 64 * 1024 * 1024,
            character_limit: 10_000,
            asset_limit: 1_000,
            media_limit: 2_000,
            payload_limit: 2 * 1024 * 1024 * 1024,
            manifest_limit: 2 * 1024 * 1024,
            timeout: Duration::from_secs(600),
            snapshot_timeout: Duration::from_secs(30),
            started: Instant::now(),
            #[cfg(test)]
            fault: None,
            #[cfg(test)]
            hook: None,
            #[cfg(test)]
            fixed_uuid: None,
            #[cfg(test)]
            unsupported: false,
            #[cfg(test)]
            backup_busy_steps: 0,
            #[cfg(test)]
            backup_more_steps: 0,
        }
    }
}
#[cfg(all(test, any(target_os = "macos", windows)))]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fault {
    NoSpace,
    Write,
    Sync,
    Rename,
    ParentSync,
    Cleanup,
    MidWrite,
    MediaSync,
    CleanupIdentity,
}

#[cfg(any(target_os = "macos", windows))]
impl BackupOptions {
    pub(crate) fn check(&self) -> Result<(), AppError> {
        if self.started.elapsed() >= self.timeout {
            Err(failure(BackupError::Timeout))
        } else {
            Ok(())
        }
    }
    fn phase(&self, _phase: &str) {
        #[cfg(test)]
        if let Some(hook) = &self.hook {
            hook(_phase);
        }
    }
    pub(crate) fn output_write(&self) -> Result<(), AppError> {
        #[cfg(test)]
        match self.fault {
            Some(Fault::NoSpace) => return Err(failure(BackupError::NoSpace)),
            Some(Fault::Write | Fault::Cleanup) => return Err(failure(BackupError::Failed)),
            _ => {}
        }
        Ok(())
    }
    pub(crate) fn file_sync(&self) -> Result<(), AppError> {
        #[cfg(test)]
        if self.fault == Some(Fault::Sync) {
            return Err(failure(BackupError::Failed));
        }
        Ok(())
    }
    pub(crate) fn before_rename(&self) -> Result<(), AppError> {
        #[cfg(test)]
        if self.fault == Some(Fault::Rename) {
            return Err(failure(BackupError::Failed));
        }
        Ok(())
    }
    pub(crate) fn parent_sync(&self) -> Result<(), AppError> {
        #[cfg(test)]
        if self.fault == Some(Fault::ParentSync) {
            return Err(failure(BackupError::Failed));
        }
        Ok(())
    }
    pub(crate) fn cleanup(&self) -> Result<(), AppError> {
        #[cfg(test)]
        if self.fault == Some(Fault::Cleanup) {
            return Err(failure(BackupError::Failed));
        }
        Ok(())
    }
}
fn failure(error: BackupError) -> AppError {
    AppError::Backup(error)
}
#[cfg(any(target_os = "macos", windows))]
pub(crate) fn output_error(error: std::io::Error) -> AppError {
    #[cfg(target_os = "macos")]
    if error.raw_os_error() == Some(libc::ENOSPC) {
        return failure(BackupError::NoSpace);
    }
    // Windows reports disk-full as one OS error code (0x70 = ERROR_DISK_FULL).
    #[cfg(windows)]
    if error.raw_os_error() == Some(112) {
        return failure(BackupError::NoSpace);
    }
    // TODO(runner-diagnostic): temporary error surfacing for the Windows
    // backup RED loop; removed once the cause is proven.
    #[cfg(all(test, windows))]
    eprintln!("backup output_error: {error:?}");
    failure(BackupError::Failed)
}
#[cfg(any(target_os = "macos", windows))]
fn sql_error(error: rusqlite::Error) -> AppError {
    if matches!(
        error.sqlite_error_code(),
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
    ) {
        failure(BackupError::Busy)
    } else {
        failure(BackupError::Integrity)
    }
}

pub(crate) fn export_with_picker(
    db: &Database,
    picker: impl FnOnce() -> Result<Option<std::path::PathBuf>, AppError>,
) -> Result<Option<BackupExportResult>, AppError> {
    let options = BackupOptions::default();
    if cfg!(not(any(target_os = "macos", windows))) {
        return Err(failure(BackupError::Unsupported));
    }
    let _busy = db.lock_export()?;
    // The independent guard spans the dialog; no media mutex or flock is held here.
    let selected = picker().map_err(|_| failure(BackupError::Destination))?;
    export_with_options(db, selected.as_deref(), options)
}

#[cfg(all(test, any(target_os = "macos", windows)))]
pub(crate) fn export_selected(
    db: &Database,
    selected: Option<&Path>,
) -> Result<Option<BackupExportResult>, AppError> {
    export_with_options(db, selected, BackupOptions::default())
}

pub(crate) fn export_with_options(
    db: &Database,
    selected: Option<&Path>,
    options: BackupOptions,
) -> Result<Option<BackupExportResult>, AppError> {
    #[cfg(all(test, any(target_os = "macos", windows)))]
    if options.unsupported {
        return Err(failure(BackupError::Unsupported));
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (db, selected, options);
        return Err(failure(BackupError::Unsupported));
    }
    #[cfg(any(target_os = "macos", windows))]
    {
        let Some(selected) = selected else {
            return Ok(None);
        };
        options.check()?;
        let _media = db.lock_media().map_err(|e| match e {
            AppError::MediaBusy => failure(BackupError::Busy),
            // TODO(runner-diagnostic): temporary error surfacing for the Windows
            // backup RED loop; reverts to Integrity once the cause is proven.
            #[cfg(all(test, windows))]
            other => {
                let detail = format!("lock:{other:?}");
                failure(BackupError::CleanupPending(
                    detail.chars().filter(|c| c.is_ascii()).take(120).collect(),
                ))
            }
            #[cfg(not(all(test, windows)))]
            _ => failure(BackupError::Integrity),
        })?;
        let parent = crate::backup_store::Parent::open(selected, db.library_root()?)?;
        let snapshot = snapshot(db, &options)?;
        let (character_count, asset_count, media) = validate_snapshot(&snapshot, &options)?;
        let logical_before = logical_digest(&snapshot, &options)?;
        // Normalize only the already-validated in-memory image using SQLite itself.
        // VACUUM may discard free pages; exporting every historical page is not promised.
        snapshot.execute_batch("VACUUM").map_err(sql_error)?;
        snapshot
            .pragma_update(None, "journal_mode", "DELETE")
            .map_err(sql_error)?;
        options.check()?;
        let (after_characters, after_assets, after_media) = validate_snapshot(&snapshot, &options)?;
        if logical_digest(&snapshot, &options)? != logical_before
            || after_characters != character_count
            || after_assets != asset_count
            || after_media != media
        {
            return Err(failure(BackupError::Integrity));
        }
        let size = database_size(&snapshot, &options)?.0;
        let serialized = snapshot.serialize(MAIN_DB).map_err(sql_error)?;
        options.check()?;
        let db_size = u64::try_from(serialized.len()).map_err(|_| failure(BackupError::Limit))?;
        if db_size != size || db_size > options.database_limit {
            return Err(failure(BackupError::Limit));
        }
        let total = media.iter().try_fold(db_size, |n, f| {
            n.checked_add(f.size)
                .ok_or_else(|| failure(BackupError::Limit))
        })?;
        if total > options.payload_limit {
            return Err(failure(BackupError::Limit));
        }
        let created = chrono::Utc::now();
        let manifest = Manifest {
            format: "furfolio-library-backup",
            version: 1,
            app_version: env!("CARGO_PKG_VERSION"),
            schema_version: 2,
            created_at: created.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            character_count,
            asset_count,
            total_bytes: total,
            database: DatabaseEntry {
                path: "library.sqlite3",
                size: db_size,
                sha256: crate::media_store::hash(&serialized),
            },
            media,
        };
        let mut writer = BoundedManifest {
            bytes: Vec::new(),
            limit: options.manifest_limit,
            options: &options,
            exceeded: false,
        };
        options.phase("manifestEncoding");
        let encoded = serde_json::to_writer(&mut writer, &manifest);
        if writer.exceeded {
            return Err(failure(BackupError::Limit));
        }
        options.check()?;
        encoded.map_err(|_| failure(BackupError::Failed))?;
        let manifest_bytes = writer.bytes;
        options.check()?;
        parent.check_space(total, manifest_bytes.len() as u64)?;
        let uuid = {
            #[cfg(test)]
            {
                options.fixed_uuid.unwrap_or_else(uuid::Uuid::new_v4)
            }
            #[cfg(not(test))]
            {
                uuid::Uuid::new_v4()
            }
        };
        let mut stage = parent.create(uuid, &created.format("%Y%m%dT%H%M%SZ").to_string())?;
        let result = (|| {
            stage.create_media()?;
            stage.write_bytes("library.sqlite3", &serialized, &options)?;
            // Open only ledger-authorized source media; no scan, repair, or mkdir.
            if !manifest.media.is_empty() {
                let store = db
                    .existing_media_store()
                    .map_err(|_| failure(BackupError::Integrity))?;
                for entry in &manifest.media {
                    options.check()?;
                    let name = entry
                        .path
                        .strip_prefix("media/")
                        .ok_or_else(|| failure(BackupError::Integrity))?;
                    let mut file = stage.create_file(name, true, entry.size, &entry.sha256)?;
                    options.output_write()?;
                    store.copy_verified(name, &entry.sha256, entry.size, &mut file, || {
                        options.check()
                    })?;
                    options.file_sync()?;
                    #[cfg(test)]
                    if options.fault == Some(Fault::MediaSync) {
                        return Err(failure(BackupError::Failed));
                    }
                    file.sync_all().map_err(output_error)?;
                }
            }
            stage.write_bytes("manifest.json", &manifest_bytes, &options)?;
            stage.sync_and_verify(&manifest_bytes, &options)?;
            options.phase("beforePublish");
            stage.publish(&manifest_bytes, &options)?;
            Ok(Some(BackupExportResult {
                directory_name: stage.final_name.clone(),
                character_count,
                asset_count,
                total_bytes: total,
            }))
        })();
        match result {
            Ok(result) => Ok(result),
            Err(error @ AppError::Backup(BackupError::DurabilityUncertain(_))) => Err(error),
            Err(error) => {
                if stage.cleanup(&options).is_ok() {
                    Err(error)
                } else {
                    Err(failure(BackupError::CleanupPending(stage.partial.clone())))
                }
            }
        }
    }
}

#[cfg(any(target_os = "macos", windows))]
fn database_size(c: &Connection, options: &BackupOptions) -> Result<(u64, u64, u64), AppError> {
    options.check()?;
    let page_size: u64 = c
        .pragma_query_value(None, "page_size", |r| r.get(0))
        .map_err(sql_error)?;
    let page_count: u64 = c
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .map_err(sql_error)?;
    let size = page_size
        .checked_mul(page_count)
        .ok_or_else(|| failure(BackupError::Limit))?;
    if !(512..=65536).contains(&page_size) || !page_size.is_power_of_two() || page_count == 0 {
        return Err(failure(BackupError::Integrity));
    }
    if size > options.database_limit {
        return Err(failure(BackupError::Limit));
    }
    Ok((size, page_size, page_count))
}
#[cfg(any(target_os = "macos", windows))]
fn snapshot(db: &Database, options: &BackupOptions) -> Result<Connection, AppError> {
    let start = Instant::now();
    let check = || {
        options.check()?;
        if start.elapsed() >= options.snapshot_timeout {
            Err(failure(BackupError::Timeout))
        } else {
            Ok(())
        }
    };
    check()?;
    let (source, identity) = db.open_backup_source().map_err(|e| match e {
        AppError::Database(e) => sql_error(e),
        // TODO(runner-diagnostic): temporary error surfacing for the Windows
        // backup RED loop; reverts to Integrity once the cause is proven.
        #[cfg(all(test, windows))]
        other => {
            let detail = format!("source:{other:?}");
            failure(BackupError::CleanupPending(
                detail.chars().filter(|c| c.is_ascii()).take(120).collect(),
            ))
        }
        #[cfg(not(all(test, windows)))]
        _ => failure(BackupError::Integrity),
    })?;
    source.execute_batch("BEGIN DEFERRED").map_err(sql_error)?;
    // BEGIN alone does not pin a WAL snapshot: a real read is required before the hook.
    source
        .query_row("SELECT count(*) FROM sqlite_schema", [], |r| {
            r.get::<_, u64>(0)
        })
        .map_err(sql_error)?;
    options.phase("snapshotPinned");
    check()?;
    let (_, page_size, pages) = database_size(&source, options)?;
    if !Database::backup_schema_matches(&source).map_err(|_| failure(BackupError::Integrity))? {
        return Err(failure(BackupError::Integrity));
    }
    check()?;
    if source
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM media_files WHERE state!='ready')",
            [],
            |r| r.get::<_, bool>(0),
        )
        .map_err(sql_error)?
    {
        return Err(failure(BackupError::Pending));
    }
    let mut target = Connection::open_in_memory().map_err(sql_error)?;
    target
        .pragma_update(None, "page_size", page_size)
        .map_err(sql_error)?;
    let max_calls = pages
        .div_ceil(64)
        .checked_add(100)
        .ok_or_else(|| failure(BackupError::Limit))?;
    {
        let backup = Backup::new(&source, &mut target).map_err(sql_error)?;
        let mut calls = 0u64;
        let mut busy = 0u64;
        loop {
            check()?;
            calls += 1;
            if calls > max_calls {
                return Err(failure(BackupError::Timeout));
            }
            #[cfg(test)]
            let injected = if calls <= options.backup_busy_steps {
                Some(if calls.is_multiple_of(2) {
                    StepResult::Locked
                } else {
                    StepResult::Busy
                })
            } else if calls - options.backup_busy_steps <= options.backup_more_steps {
                Some(StepResult::More)
            } else {
                None
            };
            #[cfg(not(test))]
            let injected = None;
            let result = if let Some(result) = injected {
                result
            } else {
                backup.step(64).map_err(sql_error)?
            };
            let progress = backup.progress();
            if progress.pagecount < 0
                || progress.remaining < 0
                || progress.remaining > progress.pagecount
                || progress.pagecount as u64 > pages
            {
                return Err(failure(BackupError::Integrity));
            }
            check()?;
            match result {
                StepResult::Done => {
                    if progress.remaining != 0 || progress.pagecount as u64 != pages {
                        return Err(failure(BackupError::Integrity));
                    }
                    break;
                }
                StepResult::More => {}
                StepResult::Busy | StepResult::Locked => {
                    busy += 1;
                    if busy > 100 {
                        return Err(failure(BackupError::Timeout));
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                _ => return Err(failure(BackupError::Failed)),
            }
        }
    }
    identity
        .verify(&source)
        // TODO(runner-diagnostic): temporary error surfacing for the Windows
        // backup RED loop; reverts to Integrity once the cause is proven.
        .map_err(|e| {
            #[cfg(all(test, windows))]
            {
                let detail = format!("verify:{e:?}");
                failure(BackupError::CleanupPending(
                    detail.chars().filter(|c| c.is_ascii()).take(120).collect(),
                ))
            }
            #[cfg(not(all(test, windows)))]
            {
                let _ = &e;
                failure(BackupError::Integrity)
            }
        })?;
    source.execute_batch("ROLLBACK").map_err(sql_error)?;
    drop(source);
    check()?;
    database_size(&target, options)?;
    Ok(target)
}

#[cfg(any(target_os = "macos", windows))]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    format: &'static str,
    version: u32,
    app_version: &'static str,
    schema_version: u32,
    created_at: String,
    character_count: u64,
    asset_count: u64,
    total_bytes: u64,
    database: DatabaseEntry,
    media: Vec<MediaEntry>,
}
#[cfg(any(target_os = "macos", windows))]
#[derive(Serialize)]
struct DatabaseEntry {
    path: &'static str,
    size: u64,
    sha256: String,
}
#[cfg(any(target_os = "macos", windows))]
#[derive(Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct MediaEntry {
    path: String,
    asset_id: String,
    kind: String,
    size: u64,
    sha256: String,
}
#[cfg(any(target_os = "macos", windows))]
struct BoundedManifest<'a> {
    bytes: Vec<u8>,
    limit: u64,
    options: &'a BackupOptions,
    exceeded: bool,
}
#[cfg(any(target_os = "macos", windows))]
impl Write for BoundedManifest<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.options
            .check()
            .map_err(|_| std::io::Error::other("Backup deadline"))?;
        if (self.bytes.len() as u64)
            .checked_add(bytes.len() as u64)
            .is_none_or(|s| s > self.limit)
        {
            self.exceeded = true;
            return Err(std::io::Error::other("Backup manifest limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[cfg(any(target_os = "macos", windows))]
fn uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|u| u.get_version_num() == 4 && u.to_string() == value)
}
#[cfg(any(target_os = "macos", windows))]
fn managed(value: &str, extension: &str) -> bool {
    value.strip_suffix(extension).is_some_and(uuid)
}
#[cfg(any(target_os = "macos", windows))]
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(any(target_os = "macos", windows))]
fn validate_snapshot(
    c: &Connection,
    options: &BackupOptions,
) -> Result<(u64, u64, Vec<MediaEntry>), AppError> {
    options.check()?;
    let invalid = || failure(BackupError::Integrity);
    if c.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
        .map_err(sql_error)?
        != "ok"
    {
        return Err(invalid());
    }
    let mut fk = c.prepare("PRAGMA foreign_key_check").map_err(sql_error)?;
    if fk
        .query([])
        .map_err(sql_error)?
        .next()
        .map_err(sql_error)?
        .is_some()
    {
        return Err(invalid());
    }
    let versions: (u64, u64, u64) = c
        .query_row(
            "SELECT count(*),min(version),max(version) FROM schema_migrations",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(sql_error)?;
    if versions != (2, 1, 2) {
        return Err(invalid());
    }
    if !crate::database::Database::backup_schema_matches(c).map_err(|_| invalid())? {
        return Err(invalid());
    }
    // Bounded counts precede allocations and per-row string reads. No source enumeration.
    let character_count: u64 = c
        .query_row("SELECT count(*) FROM characters", [], |r| r.get(0))
        .map_err(sql_error)?;
    let asset_count: u64 = c
        .query_row("SELECT count(*) FROM assets", [], |r| r.get(0))
        .map_err(sql_error)?;
    let media_count: u64 = c
        .query_row("SELECT count(*) FROM media_files", [], |r| r.get(0))
        .map_err(sql_error)?;
    if character_count > options.character_limit
        || asset_count > options.asset_limit
        || media_count > options.media_limit
    {
        return Err(failure(BackupError::Limit));
    }
    if media_count != asset_count.checked_mul(2).ok_or_else(invalid)? {
        return Err(invalid());
    }
    if c.query_row(
        "SELECT EXISTS(SELECT 1 FROM media_files WHERE state!='ready')",
        [],
        |r| r.get::<_, bool>(0),
    )
    .map_err(sql_error)?
    {
        return Err(failure(BackupError::Pending));
    }
    if c.query_row("SELECT EXISTS(SELECT 1 FROM characters c LEFT JOIN assets a ON a.id=c.avatar_asset_id WHERE c.avatar_asset_id IS NOT NULL AND (a.id IS NULL OR a.character_id!=c.id))", [], |r| r.get::<_,bool>(0)).map_err(sql_error)? { return Err(invalid()); }
    let mut characters = c.prepare("SELECT id FROM characters").map_err(sql_error)?;
    let mut rows = characters.query([]).map_err(sql_error)?;
    while let Some(row) = rows.next().map_err(sql_error)? {
        options.check()?;
        if !uuid(&row.get::<_, String>(0).map_err(sql_error)?) {
            return Err(invalid());
        }
    }
    let mut statement = c.prepare("SELECT f.name,f.staging_name,f.asset_id,f.kind,f.sha256,f.byte_size,a.character_id,a.stored_relative_path,a.thumbnail_relative_path,a.sha256,a.byte_size FROM media_files f LEFT JOIN assets a ON a.id=f.asset_id ORDER BY f.name").map_err(sql_error)?;
    let mut rows = statement.query([]).map_err(sql_error)?;
    let mut media = Vec::new();
    let mut names = BTreeSet::new();
    let mut kinds = BTreeSet::new();
    let mut payload = 0u64;
    while let Some(row) = rows.next().map_err(sql_error)? {
        options.check()?;
        if media.len() as u64 >= options.media_limit {
            return Err(failure(BackupError::Limit));
        }
        let name: String = row.get(0).map_err(sql_error)?;
        let staging: String = row.get(1).map_err(sql_error)?;
        let asset_id: String = row.get(2).map_err(sql_error)?;
        let kind: String = row.get(3).map_err(sql_error)?;
        let sha256: String = row.get(4).map_err(sql_error)?;
        let size: u64 = row.get(5).map_err(sql_error)?;
        let character: Option<String> = row.get(6).map_err(sql_error)?;
        let original: Option<String> = row.get(7).map_err(sql_error)?;
        let preview: Option<String> = row.get(8).map_err(sql_error)?;
        let original_hash: Option<String> = row.get(9).map_err(sql_error)?;
        let original_size: Option<u64> = row.get(10).map_err(sql_error)?;
        let path = format!("media/{name}");
        if !uuid(&asset_id)
            || !character.as_deref().is_some_and(uuid)
            || !managed(&staging, ".part")
            || !digest(&sha256)
            || size > crate::media_store::MAX_SOURCE
            || !names.insert(name.clone())
            || !names.insert(staging)
            || !kinds.insert((asset_id.clone(), kind.clone()))
        {
            return Err(invalid());
        }
        match kind.as_str() {
            "original"
                if managed(&name, ".bin")
                    && original.as_deref() == Some(&path)
                    && original_hash.as_deref() == Some(&sha256)
                    && original_size == Some(size) => {}
            "preview" if managed(&name, ".png") && preview.as_deref() == Some(&path) => {}
            _ => return Err(invalid()),
        }
        payload = payload
            .checked_add(size)
            .ok_or_else(|| failure(BackupError::Limit))?;
        if payload > options.payload_limit {
            return Err(failure(BackupError::Limit));
        }
        media.push(MediaEntry {
            path,
            asset_id,
            kind,
            size,
            sha256,
        });
    }
    if media.len() as u64 != media_count || kinds.len() as u64 != media_count {
        return Err(invalid());
    }
    let mut assets = c.prepare("SELECT id FROM assets").map_err(sql_error)?;
    let mut rows = assets.query([]).map_err(sql_error)?;
    while let Some(row) = rows.next().map_err(sql_error)? {
        options.check()?;
        let id: String = row.get(0).map_err(sql_error)?;
        if !uuid(&id)
            || !kinds.contains(&(id.clone(), "original".into()))
            || !kinds.contains(&(id, "preview".into()))
        {
            return Err(invalid());
        }
    }
    options.check()?;
    Ok((character_count, asset_count, media))
}

#[cfg(any(target_os = "macos", windows))]
fn logical_digest(c: &Connection, options: &BackupOptions) -> Result<Vec<u8>, AppError> {
    use rusqlite::types::ValueRef;
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    // Literal queries cover every business column, not rowids or physical page layout.
    for sql in [
        "SELECT * FROM schema_migrations ORDER BY version",
        "SELECT * FROM characters ORDER BY id",
        "SELECT * FROM assets ORDER BY id",
        "SELECT * FROM tags ORDER BY id",
        "SELECT * FROM character_tags ORDER BY character_id,tag_id",
        "SELECT * FROM character_colors ORDER BY id",
        "SELECT * FROM settings ORDER BY key",
        "SELECT * FROM media_files ORDER BY name",
    ] {
        options.check()?;
        digest.update(sql.as_bytes());
        let mut statement = c.prepare(sql).map_err(sql_error)?;
        let columns = statement.column_count();
        let mut rows = statement.query([]).map_err(sql_error)?;
        while let Some(row) = rows.next().map_err(sql_error)? {
            options.check()?;
            digest.update([0xfe]);
            for i in 0..columns {
                match row.get_ref(i).map_err(sql_error)? {
                    ValueRef::Null => digest.update([0]),
                    ValueRef::Integer(value) => {
                        digest.update([1]);
                        digest.update(value.to_le_bytes());
                    }
                    ValueRef::Real(value) => {
                        digest.update([2]);
                        digest.update(value.to_bits().to_le_bytes());
                    }
                    ValueRef::Text(value) | ValueRef::Blob(value) => {
                        digest.update([
                            if matches!(row.get_ref(i).map_err(sql_error)?, ValueRef::Text(_)) {
                                3
                            } else {
                                4
                            },
                        ]);
                        digest.update((value.len() as u64).to_le_bytes());
                        for chunk in value.chunks(64 * 1024) {
                            options.check()?;
                            digest.update(chunk);
                        }
                    }
                }
            }
        }
    }
    Ok(digest.finalize().to_vec())
}

#[cfg(all(test, any(target_os = "macos", windows)))]
#[path = "backup_tests.rs"]
mod tests;
