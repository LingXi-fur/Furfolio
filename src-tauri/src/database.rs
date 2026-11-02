use crate::error::AppError;
use rusqlite::{params, Connection, OpenFlags, Transaction, TransactionBehavior};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

const LATEST_SCHEMA_VERSION: i64 = 2;
const MEDIA_SCHEMA: &str = r#"
CREATE TABLE media_files (
    name TEXT PRIMARY KEY NOT NULL,
    staging_name TEXT NOT NULL UNIQUE,
    asset_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('original', 'preview')),
    sha256 TEXT NOT NULL CHECK (length(sha256) = 64),
    byte_size INTEGER NOT NULL CHECK (byte_size >= 0 AND byte_size <= 20971520),
    state TEXT NOT NULL CHECK (state IN ('pendingImport', 'ready', 'pendingDelete')),
    UNIQUE(asset_id, kind)
);
CREATE INDEX media_files_pending_index ON media_files(state);
"#;

const INITIAL_SCHEMA: &str = r#"
CREATE TABLE characters (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    avatar_asset_id TEXT,
    species TEXT,
    pronouns TEXT,
    description TEXT,
    notes TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (avatar_asset_id) REFERENCES assets(id) ON DELETE SET NULL
);

CREATE TABLE assets (
    id TEXT PRIMARY KEY NOT NULL,
    character_id TEXT NOT NULL,
    original_filename TEXT NOT NULL,
    stored_relative_path TEXT NOT NULL UNIQUE CHECK (
        length(stored_relative_path) > 0 AND
        substr(stored_relative_path, 1, 1) NOT IN ('/', char(92)) AND
        NOT (
            length(stored_relative_path) >= 2 AND
            substr(stored_relative_path, 2, 1) = ':'
        ) AND
        instr('/' || replace(stored_relative_path, char(92), '/') || '/', '/../') = 0
    ),
    thumbnail_relative_path TEXT CHECK (
        thumbnail_relative_path IS NULL OR (
            length(thumbnail_relative_path) > 0 AND
            substr(thumbnail_relative_path, 1, 1) NOT IN ('/', char(92)) AND
            NOT (
                length(thumbnail_relative_path) >= 2 AND
                substr(thumbnail_relative_path, 2, 1) = ':'
            ) AND
            instr('/' || replace(thumbnail_relative_path, char(92), '/') || '/', '/../') = 0
        )
    ),
    media_type TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    byte_size INTEGER NOT NULL CHECK (byte_size >= 0),
    width INTEGER NOT NULL CHECK (width > 0),
    height INTEGER NOT NULL CHECK (height > 0),
    sha256 TEXT NOT NULL CHECK (length(sha256) = 64),
    artist_name TEXT,
    source_url TEXT,
    notes TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (character_id) REFERENCES characters(id) ON DELETE CASCADE
);

CREATE TABLE tags (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    normalized_name TEXT NOT NULL UNIQUE
);

CREATE TABLE character_tags (
    character_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    position INTEGER NOT NULL CHECK (position >= 0),
    PRIMARY KEY (character_id, tag_id),
    UNIQUE (character_id, position),
    FOREIGN KEY (character_id) REFERENCES characters(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

CREATE TABLE character_colors (
    id TEXT PRIMARY KEY NOT NULL,
    character_id TEXT NOT NULL,
    hex_value TEXT NOT NULL CHECK (
        length(hex_value) = 7 AND
        substr(hex_value, 1, 1) = '#'
    ),
    position INTEGER NOT NULL CHECK (position >= 0),
    UNIQUE (character_id, position),
    FOREIGN KEY (character_id) REFERENCES characters(id) ON DELETE CASCADE
);

CREATE TABLE settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);

CREATE TRIGGER validate_avatar_asset_insert
BEFORE INSERT ON characters
WHEN NEW.avatar_asset_id IS NOT NULL
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM assets
        WHERE id = NEW.avatar_asset_id AND character_id = NEW.id
    ) THEN RAISE(ABORT, 'avatar asset must belong to character') END;
END;

CREATE TRIGGER validate_avatar_asset_update
BEFORE UPDATE OF avatar_asset_id ON characters
WHEN NEW.avatar_asset_id IS NOT NULL
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM assets
        WHERE id = NEW.avatar_asset_id AND character_id = NEW.id
    ) THEN RAISE(ABORT, 'avatar asset must belong to character') END;
END;

CREATE TRIGGER validate_asset_character_update
BEFORE UPDATE OF character_id ON assets
WHEN EXISTS (
    SELECT 1 FROM characters
    WHERE avatar_asset_id = OLD.id AND id != NEW.character_id
)
BEGIN
    SELECT RAISE(ABORT, 'avatar asset must belong to character');
END;

CREATE INDEX assets_character_id_index ON assets(character_id);
CREATE INDEX character_tags_tag_id_index ON character_tags(tag_id);
CREATE INDEX character_colors_character_id_index ON character_colors(character_id);
CREATE INDEX characters_name_index ON characters(name COLLATE NOCASE);
"#;

#[derive(Debug, Clone)]
pub struct Database {
    path: PathBuf,
    media_mutex: Arc<Mutex<()>>,
    export_mutex: Arc<Mutex<()>>,
    media_supported: bool,
}

impl Database {
    pub fn initialize(app_data_dir: &Path) -> Result<Self, AppError> {
        Self::initialize_for_platform(app_data_dir, cfg!(any(unix, windows)))
    }

    fn initialize_for_platform(
        app_data_dir: &Path,
        media_supported: bool,
    ) -> Result<Self, AppError> {
        if media_supported {
            crate::media_store::private_directory(app_data_dir, true)?;
        } else {
            // Preserve v1's non-media library support. This does not authorize any media IO.
            fs::create_dir_all(app_data_dir)?;
        }
        let database = Self {
            path: app_data_dir.join("furfolio.sqlite3"),
            media_mutex: Arc::new(Mutex::new(())),
            export_mutex: Arc::new(Mutex::new(())),
            media_supported,
        };
        let _initialization = if media_supported {
            Some(lock_initialization(&database.path)?)
        } else {
            None
        };
        let mut connection = database.open()?;
        migrate(&mut connection)?;
        // Unsupported media platforms must not recover or touch any ledger files.
        if media_supported {
            crate::media::recover_locked(&database)?;
        }
        Ok(database)
    }

    #[cfg(test)]
    pub fn initialize_without_media_for_test(app_data_dir: &Path) -> Result<Self, AppError> {
        Self::initialize_for_platform(app_data_dir, false)
    }

    #[cfg(test)]
    pub fn initialize_at(path: PathBuf) -> Result<Self, AppError> {
        let database = Self {
            path,
            media_mutex: Arc::new(Mutex::new(())),
            export_mutex: Arc::new(Mutex::new(())),
            media_supported: cfg!(any(unix, windows)),
        };
        // This legacy test helper permits arbitrary SQLite filenames outside a private
        // library. Serialize only helper construction; production startup uses the held
        // capability-rooted cross-process media lock across migration and recovery.
        static TEST_INITIALIZATION: Mutex<()> = Mutex::new(());
        let _initialization = TEST_INITIALIZATION
            .lock()
            .map_err(|_| AppError::MediaBusy)?;
        let mut connection = database.open()?;
        migrate(&mut connection)?;
        Ok(database)
    }

    pub(crate) fn lock_export(&self) -> Result<MutexGuard<'_, ()>, AppError> {
        self.export_mutex
            .try_lock()
            .map_err(|_| AppError::Backup(crate::error::BackupError::Busy))
    }

    /// Never create, migrate, checkpoint, or configure WAL on the export source.
    #[cfg(target_os = "macos")]
    pub(crate) fn open_backup_source(
        &self,
    ) -> Result<(Connection, crate::media_store::SourceDatabase), AppError> {
        let source = crate::media_store::SourceDatabase::open(self.library_root()?)?;
        let connection = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        connection.busy_timeout(Duration::ZERO)?;
        connection.pragma_update(None, "query_only", true)?;
        source.verify(&connection)?;
        Ok((connection, source))
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn library_root(&self) -> Result<&Path, AppError> {
        self.path
            .parent()
            .ok_or(AppError::Backup(crate::error::BackupError::Integrity))
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn existing_media_store(&self) -> Result<crate::media_store::Store, AppError> {
        crate::media_store::Store::open_existing(self.library_root()?)
    }

    /// Exact expected tables, indexes, and triggers, without trusting a version stamp alone.
    #[cfg(target_os = "macos")]
    pub(crate) fn backup_schema_matches(connection: &Connection) -> Result<bool, AppError> {
        let mut reference = Connection::open_in_memory()?;
        migrate(&mut reference)?;
        let mut expected = reference
            .prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema ORDER BY type,name")?;
        let mut actual = connection
            .prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema ORDER BY type,name")?;
        let mut expected_rows = expected.query([])?;
        let mut actual_rows = actual.query([])?;
        // Iterate only the fixed known schema plus one row: never collect attacker-controlled
        // schema text or execute a business view before the schema has been authenticated.
        while let Some(expected) = expected_rows.next()? {
            let Some(actual) = actual_rows.next()? else {
                return Ok(false);
            };
            for column in 0..4 {
                if actual.get_ref(column)? != expected.get_ref(column)? {
                    return Ok(false);
                }
            }
        }
        Ok(actual_rows.next()?.is_none())
    }

    pub fn open(&self) -> Result<Connection, AppError> {
        let connection = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        configure_connection(&connection)?;
        Ok(connection)
    }

    /// Serialization across recovery and in-flight operations (Tauri commands run
    /// on a multithreaded pool). Lock before any staged import or cleanup pass.
    pub fn lock_media(&self) -> Result<MediaLock<'_>, AppError> {
        let guard = self
            .media_mutex
            .try_lock()
            .map_err(|_| AppError::MediaBusy)?;
        let file = if self.media_supported {
            Some(crate::media_store::lock_library(&self.path)?)
        } else {
            None
        };
        Ok(MediaLock {
            _guard: guard,
            _file: file,
        })
    }

    /// Without safe media IO, even orphaned ledger credentials forbid deletion. The
    /// check runs in the same SQLite transaction as character deletion, so another
    /// instance cannot race a media row into a cascade after authorization.
    pub fn ensure_character_delete_supported(
        &self,
        connection: &Connection,
    ) -> Result<(), AppError> {
        if !self.media_supported
            && connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM assets) OR EXISTS(SELECT 1 FROM media_files)",
                [],
                |row| row.get::<_, bool>(0),
            )?
        {
            return Err(std::io::Error::other(
                "Safe media deletion is unavailable on this platform",
            )
            .into());
        }
        Ok(())
    }

    pub fn media_store(&self) -> Result<crate::media_store::Store, AppError> {
        if !self.media_supported {
            return Err(
                std::io::Error::other("Safe media IO is unavailable on this platform").into(),
            );
        }
        crate::media_store::Store::open(
            self.path
                .parent()
                .ok_or_else(|| std::io::Error::other("Missing library parent"))?,
        )
    }
}

pub struct MediaLock<'a> {
    _guard: MutexGuard<'a, ()>,
    _file: Option<crate::media_store::LibraryLock>,
}

fn lock_initialization(path: &Path) -> Result<crate::media_store::LibraryLock, AppError> {
    let started = std::time::Instant::now();
    loop {
        match crate::media_store::lock_library(path) {
            Ok(lock) => return Ok(lock),
            Err(AppError::MediaBusy) if started.elapsed() < Duration::from_secs(5) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }
}

fn migrate(connection: &mut Connection) -> Result<(), AppError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL
        );",
    )?;

    let current_version = transaction.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get::<_, i64>(0),
    )?;

    if current_version > LATEST_SCHEMA_VERSION {
        return Err(AppError::UnsupportedDatabase(current_version));
    }

    if current_version < 1 {
        apply_migration(&transaction, 1, "initial_schema", INITIAL_SCHEMA)?;
    }
    // v2 is append-only; the v1 schema and its data are never rewritten.
    if current_version < 2 {
        apply_migration(&transaction, 2, "media_files_ledger", MEDIA_SCHEMA)?;
    }

    transaction.commit()?;
    Ok(())
}

fn configure_connection(connection: &Connection) -> Result<(), AppError> {
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    Ok(())
}

fn apply_migration(
    transaction: &Transaction<'_>,
    version: i64,
    name: &str,
    sql: &str,
) -> Result<(), AppError> {
    transaction.execute_batch(sql)?;
    record_migration(transaction, version, name)?;
    Ok(())
}

fn record_migration(
    transaction: &Transaction<'_>,
    version: i64,
    name: &str,
) -> Result<(), AppError> {
    transaction.execute(
        "INSERT INTO schema_migrations (version, name, applied_at)
         VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
        params![version, name],
    )?;
    Ok(())
}

#[cfg(all(test, unix))]
mod unix_production_lock_tests {
    use super::*;
    use std::os::unix::fs::DirBuilderExt;
    use std::process::{Child, Command, Stdio};
    use std::time::Instant;

    const CHILD_TEST: &str = "database::unix_production_lock_tests::production_lock_child";
    const DEADLINE: Duration = Duration::from_secs(15);

    struct SyntheticLibrary(PathBuf);
    impl SyntheticLibrary {
        fn new() -> Self {
            // Canonicalize only the OS temp directory: macOS /var is a symlink, while
            // production capability traversal deliberately rejects symlink components.
            let temp = fs::canonicalize(std::env::temp_dir()).unwrap();
            let root = temp.join(format!("furfolio-unix-lock-{}", uuid::Uuid::new_v4()));
            fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
            Self(root)
        }
        fn initialize(&self) -> Database {
            let database = Database::initialize(&self.0).expect("production initialize");
            assert_eq!(database.path, self.0.join("furfolio.sqlite3"));
            database
        }
    }
    impl Drop for SyntheticLibrary {
        fn drop(&mut self) {
            let result = fs::remove_dir_all(&self.0);
            if !std::thread::panicking() {
                result.expect("remove only this synthetic library");
            }
        }
    }

    struct ChildProcess(Child);
    impl ChildProcess {
        fn spawn(root: &Path, mode: &str, label: &str) -> Self {
            Self(
                Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", CHILD_TEST, "--nocapture", "--test-threads=1"])
                    .env("FURFOLIO_UNIX_LOCK_CHILD", mode)
                    .env("FURFOLIO_UNIX_LOCK_ROOT", root)
                    .env("FURFOLIO_UNIX_LOCK_LABEL", label)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .spawn()
                    .expect("spawn independent production-lock test process"),
            )
        }
        fn wait_for_marker(&mut self, marker: &Path) {
            let start = Instant::now();
            while !marker.exists() {
                assert!(
                    self.0.try_wait().unwrap().is_none(),
                    "child exited before ready"
                );
                assert!(start.elapsed() < DEADLINE, "child marker deadline");
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        fn wait_success(&mut self) {
            let start = Instant::now();
            loop {
                if let Some(status) = self.0.try_wait().unwrap() {
                    assert!(status.success(), "production-lock child failed: {status}");
                    return;
                }
                assert!(start.elapsed() < DEADLINE, "child exit deadline");
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        fn kill_and_reap(&mut self) {
            self.0.kill().expect("terminate synthetic lock holder");
            let start = Instant::now();
            loop {
                if let Some(status) = self.0.try_wait().unwrap() {
                    assert!(!status.success(), "holder must exit abnormally");
                    return;
                }
                assert!(
                    start.elapsed() < DEADLINE,
                    "terminated holder reap deadline"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
    impl Drop for ChildProcess {
        fn drop(&mut self) {
            // Never call unbounded Child::wait, including assertion/deadline unwinding.
            if matches!(self.0.try_wait(), Ok(Some(_))) {
                return;
            }
            let killed = self.0.kill();
            let start = Instant::now();
            let failure = loop {
                match self.0.try_wait() {
                    Ok(Some(_)) => return,
                    Err(error) => break format!("could not reap synthetic child: {error}"),
                    Ok(None) if start.elapsed() >= Duration::from_secs(2) => {
                        break format!("synthetic child cleanup deadline; kill result: {killed:?}");
                    }
                    Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                }
            };
            if std::thread::panicking() {
                eprintln!("{failure}");
            } else {
                panic!("{failure}");
            }
        }
    }

    fn wait_for_file(path: &Path) {
        let start = Instant::now();
        while !path.exists() {
            assert!(start.elapsed() < DEADLINE, "parent signal deadline");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn production_lock_child() {
        let Some(mode) = std::env::var_os("FURFOLIO_UNIX_LOCK_CHILD") else {
            return; // Entry point only; parent tests below exercise both child modes.
        };
        let root = PathBuf::from(std::env::var_os("FURFOLIO_UNIX_LOCK_ROOT").unwrap());
        let label = std::env::var("FURFOLIO_UNIX_LOCK_LABEL").unwrap();
        assert!(root.is_absolute());
        assert!(root
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("furfolio-unix-lock-"));
        assert!(matches!(
            label.as_str(),
            "first" | "second" | "holder" | "waiter"
        ));
        if mode == "initialize" {
            fs::write(root.join(format!("{label}.ready")), b"ready").unwrap();
            wait_for_file(&root.join("initialize.go"));
            fs::write(root.join(format!("{label}.started")), b"started").unwrap();
            let database = Database::initialize(&root).expect("child production initialize");
            assert_eq!(database.path, root.join("furfolio.sqlite3"));
            let connection = database.open().unwrap();
            let count: i64 = connection
                .query_row("SELECT count(*) FROM schema_migrations", [], |r| r.get(0))
                .unwrap();
            assert_eq!(count, LATEST_SCHEMA_VERSION);
        } else if mode == "hold" {
            let database = Database::initialize(&root).expect("holder production initialize");
            let _lock = database
                .lock_media()
                .expect("hold production cross-process lock");
            fs::write(root.join(format!("{label}.ready")), b"ready").unwrap();
            wait_for_file(&root.join("holder.release"));
        } else {
            panic!("unknown synthetic child mode");
        }
    }

    fn initialize_two_processes(root: &Path) {
        let mut first = ChildProcess::spawn(root, "initialize", "first");
        let mut second = ChildProcess::spawn(root, "initialize", "second");
        first.wait_for_marker(&root.join("first.ready"));
        second.wait_for_marker(&root.join("second.ready"));
        fs::write(root.join("initialize.go"), b"go").unwrap();
        first.wait_success();
        second.wait_success();
    }

    #[test]
    fn production_initialize_serializes_independent_processes_and_recovers() {
        let library = SyntheticLibrary::new();
        initialize_two_processes(&library.0); // Concurrent cold schema migration.
        let database = library.initialize();
        let connection = database.open().unwrap();
        connection
            .execute(
                "INSERT INTO media_files (name,staging_name,asset_id,kind,sha256,byte_size,state)
             VALUES (?1,?2,'synthetic-pending','original',?3,1,'pendingImport')",
                params![
                    format!("{}.bin", uuid::Uuid::new_v4()),
                    format!("{}.part", uuid::Uuid::new_v4()),
                    "a".repeat(64)
                ],
            )
            .unwrap();
        drop(connection);
        for marker in [
            "initialize.go",
            "first.ready",
            "second.ready",
            "first.started",
            "second.started",
        ] {
            fs::remove_file(library.0.join(marker)).unwrap();
        }
        initialize_two_processes(&library.0); // Same production lock covers recovery.
        let connection = database.open().unwrap();
        let pending: i64 = connection
            .query_row("SELECT count(*) FROM media_files", [], |r| r.get(0))
            .unwrap();
        assert_eq!(pending, 0, "registered pending media must be recovered");
        let lock = database
            .lock_media()
            .expect("initializers released production lock");
        drop(lock);
    }

    #[test]
    fn production_lock_is_nonblocking_but_initialize_waits_for_release() {
        let library = SyntheticLibrary::new();
        let database = library.initialize();
        let mut holder = ChildProcess::spawn(&library.0, "hold", "holder");
        holder.wait_for_marker(&library.0.join("holder.ready"));
        let start = Instant::now();
        assert!(matches!(database.lock_media(), Err(AppError::MediaBusy)));
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "daily lock must not wait"
        );
        let mut waiter = ChildProcess::spawn(&library.0, "initialize", "waiter");
        waiter.wait_for_marker(&library.0.join("waiter.ready"));
        fs::write(library.0.join("initialize.go"), b"go").unwrap();
        waiter.wait_for_marker(&library.0.join("waiter.started"));
        std::thread::sleep(Duration::from_millis(100));
        assert!(
            waiter.0.try_wait().unwrap().is_none(),
            "initializer must wait, not return busy"
        );
        fs::write(library.0.join("holder.release"), b"release").unwrap();
        holder.wait_success();
        waiter.wait_success();
        let lock = database
            .lock_media()
            .expect("lock released after normal child exit");
        drop(lock);
    }

    #[test]
    fn production_lock_test_child_drop_terminates_and_reaps_with_deadline() {
        let library = SyntheticLibrary::new();
        let database = library.initialize();
        let mut holder = ChildProcess::spawn(&library.0, "hold", "holder");
        holder.wait_for_marker(&library.0.join("holder.ready"));
        assert!(matches!(database.lock_media(), Err(AppError::MediaBusy)));
        let start = Instant::now();
        drop(holder); // Exercise cleanup without the explicit kill_and_reap path.
        assert!(start.elapsed() < Duration::from_secs(3));
        let lock = database
            .lock_media()
            .expect("Drop reaped the synthetic holder");
        drop(lock);
    }

    #[test]
    fn production_lock_is_released_after_holder_is_killed() {
        let library = SyntheticLibrary::new();
        let database = library.initialize();
        let mut holder = ChildProcess::spawn(&library.0, "hold", "holder");
        holder.wait_for_marker(&library.0.join("holder.ready"));
        assert!(matches!(database.lock_media(), Err(AppError::MediaBusy)));
        holder.kill_and_reap();
        let reopened = library.initialize();
        let lock = reopened
            .lock_media()
            .expect("abnormal process exit releases production lock");
        drop(lock);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::{Arc, Barrier};
    use std::thread;
    use uuid::Uuid;

    fn temporary_database_path() -> PathBuf {
        std::env::temp_dir().join(format!("furfolio-{}.sqlite3", Uuid::new_v4()))
    }

    fn remove_database(path: &Path) {
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(path.with_extension("sqlite3-shm"));
        let _ = fs::remove_file(path.with_extension("sqlite3-wal"));
    }

    #[test]
    fn media_v2_adds_a_non_cascading_ownership_ledger() {
        let path = temporary_database_path();
        let database = Database::initialize_at(path.clone()).expect("initialize database");
        let connection = database.open().expect("open database");
        let version: i64 = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, 2, "media ledger requires an additive v2 migration");
        let foreign_keys: i64 = connection
            .query_row(
                "SELECT count(*) FROM pragma_foreign_key_list('media_files')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            foreign_keys, 0,
            "cleanup credentials must survive character cascade"
        );
        connection.execute(
            "INSERT INTO media_files (name, staging_name, asset_id, kind, sha256, byte_size, state)
             VALUES (?1, ?2, 'asset', 'original', ?3, 1, 'pendingImport')",
            params![format!("{}.bin", Uuid::new_v4()), format!("{}.part", Uuid::new_v4()), "a".repeat(64)],
        ).unwrap();
        drop(connection);
        remove_database(&path);
    }

    #[test]
    fn initializes_and_reopens_the_schema() {
        let path = temporary_database_path();
        let database = Database::initialize_at(path.clone()).expect("initialize database");
        Database::initialize_at(path.clone()).expect("reopen database");

        let connection = database.open().expect("open database");
        let version: i64 = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("read schema version");
        assert_eq!(version, LATEST_SCHEMA_VERSION);

        remove_database(&path);
    }

    #[test]
    fn serializes_concurrent_initialization() {
        let path = temporary_database_path();
        let barrier = Arc::new(Barrier::new(3));
        let handles = (0..2)
            .map(|_| {
                let path = path.clone();
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    Database::initialize_at(path)
                })
            })
            .collect::<Vec<_>>();

        barrier.wait();
        for handle in handles {
            handle
                .join()
                .expect("initialization thread")
                .expect("initialize database concurrently");
        }

        let database = Database::initialize_at(path.clone()).expect("reopen database");
        let connection = database.open().expect("open database");
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("count migrations");
        assert_eq!(count, LATEST_SCHEMA_VERSION);

        remove_database(&path);
    }

    #[test]
    fn rejects_unsafe_managed_paths() {
        let path = temporary_database_path();
        let database = Database::initialize_at(path.clone()).expect("initialize database");
        let connection = database.open().expect("open database");
        connection
            .execute(
                "INSERT INTO characters (id, name, created_at, updated_at)
                 VALUES ('character', 'Nova', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("insert character");

        for unsafe_path in [
            "/absolute.png",
            r"\rooted.png",
            r"C:\absolute.png",
            r"\\server\share\image.png",
            "../escape.png",
            "media/../escape.png",
            r"media\..\escape.png",
        ] {
            let result = insert_test_asset(&connection, unsafe_path);
            assert!(result.is_err(), "path should be rejected: {unsafe_path}");
        }

        for safe_path in ["media/asset/original.png", r"media\asset\original.png"] {
            insert_test_asset(&connection, safe_path).expect("insert safe relative path");
        }

        remove_database(&path);
    }

    #[test]
    fn enforces_avatar_asset_ownership() {
        let path = temporary_database_path();
        let database = Database::initialize_at(path.clone()).expect("initialize database");
        let connection = database.open().expect("open database");
        for (id, name) in [("a", "Nova"), ("b", "Echo")] {
            connection
                .execute(
                    "INSERT INTO characters (id, name, created_at, updated_at)
                     VALUES (?1, ?2, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                    params![id, name],
                )
                .expect("insert character");
        }
        insert_test_asset_for_character(&connection, "asset-a", "a", "media/a.png")
            .expect("insert asset a");
        insert_test_asset_for_character(&connection, "asset-b", "b", "media/b.png")
            .expect("insert asset b");

        connection
            .execute(
                "UPDATE characters SET avatar_asset_id = 'asset-a' WHERE id = 'a'",
                [],
            )
            .expect("set owned avatar");
        assert!(connection
            .execute(
                "UPDATE characters SET avatar_asset_id = 'asset-b' WHERE id = 'a'",
                [],
            )
            .is_err());

        assert!(connection
            .execute(
                "UPDATE assets SET character_id = 'b' WHERE id = 'asset-a'",
                [],
            )
            .is_err());

        connection
            .execute("DELETE FROM assets WHERE id = 'asset-a'", [])
            .expect("delete avatar asset");
        let avatar: Option<String> = connection
            .query_row(
                "SELECT avatar_asset_id FROM characters WHERE id = 'a'",
                [],
                |row| row.get(0),
            )
            .expect("read avatar");
        assert_eq!(avatar, None);

        remove_database(&path);
    }

    fn insert_test_asset(connection: &Connection, stored_path: &str) -> rusqlite::Result<usize> {
        insert_test_asset_for_character(
            connection,
            &Uuid::new_v4().to_string(),
            "character",
            stored_path,
        )
    }

    fn insert_test_asset_for_character(
        connection: &Connection,
        id: &str,
        character_id: &str,
        stored_path: &str,
    ) -> rusqlite::Result<usize> {
        connection.execute(
            "INSERT INTO assets (
                id, character_id, original_filename, stored_relative_path, media_type,
                mime_type, byte_size, width, height, sha256, created_at
             ) VALUES (?1, ?2, 'image.png', ?3, 'image', 'image/png', 1, 1, 1,
                       ?4, '2026-01-01T00:00:00Z')",
            params![id, character_id, stored_path, "0".repeat(64)],
        )
    }

    #[test]
    fn rejects_a_newer_schema_version() {
        let path = temporary_database_path();
        let database = Database::initialize_at(path.clone()).expect("initialize database");
        let connection = database.open().expect("open database");
        connection
            .execute(
                "INSERT INTO schema_migrations (version, name, applied_at)
                 VALUES (99, 'future', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("insert future migration");
        drop(connection);

        let result = Database::initialize_at(path.clone());
        assert!(matches!(result, Err(AppError::UnsupportedDatabase(99))));

        remove_database(&path);
    }

    #[test]
    fn enables_foreign_keys_for_every_connection() {
        let path = temporary_database_path();
        let database = Database::initialize_at(path.clone()).expect("initialize database");
        let connection = database.open().expect("open database");
        let enabled: i64 = connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .expect("read foreign key setting");
        assert_eq!(enabled, 1);

        remove_database(&path);
    }
}
