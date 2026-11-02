#[cfg(not(test))]
use crate::media_decode::decode_media;
#[cfg(test)]
use crate::media_decode::{decode_media, validate_dimensions};
use crate::{
    database::Database,
    error::AppError,
    media_store::{hash, read_source, Store},
    models::{Asset, AssetInput, MediaCleanupStatus},
};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use uuid::Uuid;

const ASSET_COLUMNS:&str="id, character_id, original_filename, mime_type, byte_size, width, height, artist_name, source_url, notes, created_at";
fn row_asset(r: &rusqlite::Row<'_>) -> rusqlite::Result<Asset> {
    Ok(Asset {
        id: r.get(0)?,
        character_id: r.get(1)?,
        original_filename: r.get(2)?,
        mime_type: r.get(3)?,
        byte_size: r.get(4)?,
        width: r.get(5)?,
        height: r.get(6)?,
        artist_name: r.get(7)?,
        source_url: r.get(8)?,
        notes: r.get(9)?,
        created_at: r.get(10)?,
    })
}
fn character_exists(c: &Connection, id: &str) -> Result<(), AppError> {
    if !c.query_row(
        "SELECT EXISTS(SELECT 1 FROM characters WHERE id=?1)",
        [id],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(AppError::NotFound);
    }
    Ok(())
}
fn load_asset(c: &Connection, character: &str, id: &str) -> Result<Asset, AppError> {
    c.query_row(
        &format!("SELECT {ASSET_COLUMNS} FROM assets WHERE character_id=?1 AND id=?2"),
        params![character, id],
        row_asset,
    )
    .optional()?
    .ok_or(AppError::NotFound)
}
pub fn list_assets(db: &Database, character: &str) -> Result<Vec<Asset>, AppError> {
    let _guard = db.lock_media()?;
    let c = db.open()?;
    character_exists(&c, character)?;
    let mut s = c.prepare(&format!(
        "SELECT {ASSET_COLUMNS} FROM assets WHERE character_id=?1 ORDER BY created_at DESC,id"
    ))?;
    let rows = s
        .query_map([character], row_asset)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn import_selected(
    db: &Database,
    character: &str,
    source: Option<&Path>,
) -> Result<Option<Asset>, AppError> {
    let Some(source) = source else {
        return Ok(None);
    };
    let _guard = db.lock_media()?;
    let mut c = db.open()?;
    character_exists(&c, character)?;
    let bytes = read_source(source)?;
    let decoded = decode_media(&bytes)?;
    let original_filename = source
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.contains(['\\', '/']) && !s.chars().any(char::is_control))
        .filter(|s| s.chars().count() <= 255)
        .ok_or(AppError::InvalidMedia)?
        .to_owned();
    let store = db.media_store()?;
    let id = Uuid::new_v4().to_string();
    let files = [
        FileEntry::new(&id, "original", &bytes),
        FileEntry::new(&id, "preview", &decoded.preview),
    ];
    {
        let tx = c.transaction()?;
        character_exists(&tx, character)?;
        for f in &files {
            tx.execute("INSERT INTO media_files (name,staging_name,asset_id,kind,sha256,byte_size,state) VALUES (?1,?2,?3,?4,?5,?6,'pendingImport')",params![f.name,f.staging,id,f.kind,f.sha,f.size])?;
        }
        tx.commit()?;
    }
    let publish = (|| -> Result<Asset, AppError> {
        store.publish(&files[0].name, &files[0].staging, &bytes)?;
        store.publish(&files[1].name, &files[1].staging, &decoded.preview)?;
        // Readback verifies both durable files before making the business record visible.
        for f in &files {
            store.read(&f.name, &f.sha, f.size)?;
        }
        let tx = c.transaction()?;
        character_exists(&tx, character)?;
        tx.execute("INSERT INTO assets (id,character_id,original_filename,stored_relative_path,thumbnail_relative_path,media_type,mime_type,byte_size,width,height,sha256,created_at) VALUES (?1,?2,?3,?4,?5,'image',?6,?7,?8,?9,?10,strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
            params![id,character,original_filename,format!("media/{}",files[0].name),format!("media/{}",files[1].name),decoded.mime,bytes.len() as u64,decoded.width,decoded.height,files[0].sha])?;
        tx.execute(
            "UPDATE media_files SET state='ready' WHERE asset_id=?1",
            [&id],
        )?;
        touch(&tx, character)?;
        let asset = load_asset(&tx, character, &id)?;
        tx.commit()?;
        Ok(asset)
    })();
    match publish {
        Ok(asset) => Ok(Some(asset)),
        Err(error) => {
            // Never clean an ambiguously committed import. SQLite commit errors may still be durable.
            let visible = c.query_row(
                "SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1)",
                [&id],
                |r| r.get::<_, bool>(0),
            )?;
            if visible {
                return Err(error);
            }
            if cleanup_asset(&c, &store, &id).is_err() {
                return Err(AppError::CleanupPending { applied: false });
            }
            Err(error)
        }
    }
}

pub fn read_asset_preview(db: &Database, character: &str, id: &str) -> Result<Vec<u8>, AppError> {
    let _guard = db.lock_media()?;
    let c = db.open()?;
    let asset = load_asset(&c, character, id)?;
    let f=c.query_row("SELECT name,staging_name,kind,sha256,byte_size FROM media_files WHERE asset_id=?1 AND kind='preview' AND state='ready'",[&asset.id],row_file).optional()?.ok_or(AppError::InvalidMedia)?;
    if f.size > 4 * 1024 * 1024 {
        return Err(AppError::MediaTooLarge);
    }
    let bytes = db.media_store()?.read(&f.name, &f.sha, f.size)?;
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(AppError::InvalidMedia);
    }
    Ok(bytes)
}
pub fn update_asset(
    db: &Database,
    character: &str,
    id: &str,
    input: AssetInput,
) -> Result<Asset, AppError> {
    let input = normalize(input)?;
    let _guard = db.lock_media()?;
    let mut c = db.open()?;
    let tx = c.transaction()?;
    load_asset(&tx, character, id)?;
    tx.execute(
        "UPDATE assets SET artist_name=?1,source_url=?2,notes=?3 WHERE character_id=?4 AND id=?5",
        params![
            input.artist_name,
            input.source_url,
            input.notes,
            character,
            id
        ],
    )?;
    touch(&tx, character)?;
    let asset = load_asset(&tx, character, id)?;
    tx.commit()?;
    Ok(asset)
}
pub fn delete_asset(db: &Database, character: &str, id: &str) -> Result<(), AppError> {
    let _guard = db.lock_media()?;
    let mut c = db.open()?;
    let tx = c.transaction()?;
    load_asset(&tx, character, id)?;
    mark_asset(&tx, id)?;
    tx.execute(
        "DELETE FROM assets WHERE character_id=?1 AND id=?2",
        params![character, id],
    )?;
    touch(&tx, character)?;
    tx.commit()?;
    cleanup_ids(db, &c, &[id.to_owned()]).map_err(|_| AppError::CleanupPending { applied: true })
}
fn touch(c: &Connection, character: &str) -> Result<(), AppError> {
    c.execute(
        "UPDATE characters SET updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",
        [character],
    )?;
    Ok(())
}
fn normalize(input: AssetInput) -> Result<AssetInput, AppError> {
    fn field(s: Option<String>, max: usize) -> Result<Option<String>, AppError> {
        let s = s.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
        if s.as_ref()
            .is_some_and(|s| s.chars().count() > max || s.contains('\0'))
        {
            return Err(AppError::InvalidInput(
                "Media metadata exceeds its supported limits",
            ));
        }
        Ok(s)
    }
    let artist_name = field(input.artist_name, 200)?;
    let notes = field(input.notes, 10000)?;
    let source_url = field(input.source_url, 2048)?;
    if let Some(source) = &source_url {
        let url = url::Url::parse(source).map_err(|_| {
            AppError::InvalidInput("Source must be an HTTP(S) URL without credentials")
        })?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || source.chars().any(|c| c.is_control() || c.is_whitespace())
        {
            return Err(AppError::InvalidInput(
                "Source must be an HTTP(S) URL without credentials",
            ));
        }
    }
    Ok(AssetInput {
        artist_name,
        source_url,
        notes,
    })
}

struct FileEntry {
    name: String,
    staging: String,
    kind: String,
    sha: String,
    size: u64,
}
impl FileEntry {
    fn new(_id: &str, kind: &str, bytes: &[u8]) -> Self {
        Self {
            name: format!(
                "{}.{}",
                Uuid::new_v4(),
                if kind == "preview" { "png" } else { "bin" }
            ),
            staging: format!("{}.part", Uuid::new_v4()),
            kind: kind.into(),
            sha: hash(bytes),
            size: bytes.len() as u64,
        }
    }
}
fn row_file(r: &rusqlite::Row<'_>) -> rusqlite::Result<FileEntry> {
    Ok(FileEntry {
        name: r.get(0)?,
        staging: r.get(1)?,
        kind: r.get(2)?,
        sha: r.get(3)?,
        size: r.get(4)?,
    })
}

/// Missing credentials for any legacy asset are a safety failure, not permission to guess files.
pub fn mark_asset(c: &Connection, id: &str) -> Result<(), AppError> {
    let (original,preview,sha,size):(String,Option<String>,String,u64)=c.query_row("SELECT stored_relative_path,thumbnail_relative_path,sha256,byte_size FROM assets WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
    let mut s=c.prepare("SELECT name,staging_name,kind,sha256,byte_size FROM media_files WHERE asset_id=?1 AND state='ready'")?;
    let files = s
        .query_map([id], row_file)?
        .collect::<Result<Vec<_>, _>>()?;
    if files.len() != 2
        || !files.iter().any(|f| {
            f.kind == "original"
                && format!("media/{}", f.name) == original
                && f.sha == sha
                && f.size == size
        })
        || !files
            .iter()
            .any(|f| f.kind == "preview" && Some(format!("media/{}", f.name)) == preview)
    {
        return Err(AppError::CleanupPending { applied: false });
    }
    c.execute(
        "UPDATE media_files SET state='pendingDelete' WHERE asset_id=?1",
        [id],
    )?;
    Ok(())
}
pub fn cleanup_ids(db: &Database, c: &Connection, ids: &[String]) -> Result<(), AppError> {
    if ids.is_empty() {
        return Ok(());
    }
    let store = db.media_store()?;
    let mut failed = false;
    for id in ids {
        if cleanup_asset(c, &store, id).is_err() {
            failed = true;
        }
    }
    if failed {
        Err(AppError::CleanupPending { applied: false })
    } else {
        Ok(())
    }
}
fn cleanup_asset(c: &Connection, store: &Store, id: &str) -> Result<(), AppError> {
    // Referenced media is never deleted, even if the ledger is inconsistent.
    if c.query_row(
        "SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1)",
        [id],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(AppError::CleanupPending { applied: false });
    }
    let mut s=c.prepare("SELECT name,staging_name,kind,sha256,byte_size FROM media_files WHERE asset_id=?1 AND state!='ready'")?;
    let files = s
        .query_map([id], row_file)?
        .collect::<Result<Vec<_>, _>>()?;
    let mut failed = false;
    for f in files {
        if store.cleanup(&f.name, &f.staging, &f.sha, f.size).is_ok() {
            if c.execute(
                "DELETE FROM media_files WHERE name=?1 AND state!='ready'",
                [&f.name],
            )
            .is_err()
            {
                failed = true;
            }
        } else {
            failed = true;
        }
    }
    if failed {
        Err(AppError::CleanupPending { applied: false })
    } else {
        Ok(())
    }
}
pub fn media_cleanup_status(db: &Database) -> Result<MediaCleanupStatus, AppError> {
    let _guard = db.lock_media()?;
    pending(&db.open()?)
}
fn pending(c: &Connection) -> Result<MediaCleanupStatus, AppError> {
    Ok(MediaCleanupStatus {
        pending: c.query_row(
            "SELECT count(*) FROM media_files WHERE state!='ready'",
            [],
            |r| r.get(0),
        )?,
    })
}
pub fn retry_media_cleanup(db: &Database) -> Result<MediaCleanupStatus, AppError> {
    let _guard = db.lock_media()?;
    let c = db.open()?;
    let mut s = c.prepare("SELECT DISTINCT asset_id FROM media_files WHERE state!='ready'")?;
    let ids = s
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    // Failures intentionally retain exact credentials; status is retryable, not a silent success.
    let _ = cleanup_ids(db, &c, &ids);
    pending(&c)
}

#[cfg(test)]
#[path = "media_tests.rs"]
mod tests;
