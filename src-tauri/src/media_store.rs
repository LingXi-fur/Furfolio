//! Private capability-rooted media IO. Platform primitives stay in platform_fs.
use crate::{
    error::AppError,
    platform_fs::{self, Directory},
};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

pub const MAX_SOURCE: u64 = 20 * 1024 * 1024;
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn unsafe_entry() -> std::io::Error {
    platform_fs::unsafe_entry()
}
pub struct Store {
    dir: Directory,
}
impl Store {
    #[cfg(any(target_os = "macos", windows))]
    pub(crate) fn open_existing(root: &Path) -> Result<Self, AppError> {
        Ok(Self {
            dir: Directory::open(root, false)?.child("media", false)?,
        })
    }
    #[cfg(any(target_os = "macos", windows))]
    pub(crate) fn copy_verified(
        &self,
        name: &str,
        expected: &str,
        size: u64,
        output: &mut impl Write,
        mut check: impl FnMut() -> Result<(), AppError>,
    ) -> Result<(), AppError> {
        let invalid = || AppError::Backup(crate::error::BackupError::Integrity);
        valid_name(name).map_err(|_| invalid())?;
        if size > MAX_SOURCE {
            return Err(invalid());
        }
        let mut file = self.dir.open_file(name, false).map_err(|_| invalid())?;
        let before = platform_fs::file_state(&file).map_err(|_| invalid())?;
        if before.directory || before.links != 1 || before.size != size {
            return Err(invalid());
        }
        let mut buffer = [0u8; 64 * 1024];
        let mut remaining = size;
        let mut digest = Sha256::new();
        while remaining > 0 {
            check()?;
            let length =
                usize::try_from(remaining.min(buffer.len() as u64)).map_err(|_| invalid())?;
            let read = file.read(&mut buffer[..length]).map_err(|_| invalid())?;
            if read == 0 {
                return Err(invalid());
            }
            digest.update(&buffer[..read]);
            output
                .write_all(&buffer[..read])
                .map_err(crate::backup::output_error)?;
            remaining -= read as u64;
        }
        check()?;
        if file.read(&mut buffer[..1]).map_err(|_| invalid())? != 0
            || platform_fs::file_state(&file).map_err(|_| invalid())? != before
            || format!("{:x}", digest.finalize()) != expected
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn open(root: &Path) -> Result<Self, AppError> {
        Ok(Self {
            dir: Directory::open(root, false)?.child("media", true)?,
        })
    }
    pub fn read(&self, name: &str, expected: &str, size: u64) -> Result<Vec<u8>, AppError> {
        valid_name(name)?;
        let mut file = self.dir.open_file(name, false)?;
        checked_bytes(&mut file, expected, size)
    }
    pub fn publish(&self, name: &str, staging: &str, bytes: &[u8]) -> Result<(), AppError> {
        valid_name(name)?;
        valid_name(staging)?;
        let mut file = self.dir.open_file(staging, true)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        self.dir.sync()?;
        self.dir.rename(staging, name, &file)?;
        self.dir.sync()?;
        Ok(())
    }
    pub fn cleanup(
        &self,
        name: &str,
        staging: &str,
        expected: &str,
        size: u64,
    ) -> Result<(), AppError> {
        valid_name(name)?;
        valid_name(staging)?;
        if self.dir.exists(name)? {
            let mut file = self.dir.open_mutable(name)?;
            checked_bytes(&mut file, expected, size)?;
            if self.dir.exists(staging)? {
                return Err(unsafe_entry().into());
            }
            self.dir.rename(name, staging, &file)?;
            self.dir.sync()?;
        }
        if self.dir.exists(staging)? {
            let mut file = self.dir.open_mutable(staging)?;
            checked_bytes(&mut file, expected, size)?;
            self.dir.remove_file(staging, &file)?;
            drop(file);
            self.dir.sync()?;
        }
        Ok(())
    }
}
fn checked_bytes(file: &mut File, expected: &str, size: u64) -> Result<Vec<u8>, AppError> {
    let bytes = read_regular(file, size)?;
    if bytes.len() as u64 != size || hash(&bytes) != expected {
        return Err(unsafe_entry().into());
    }
    Ok(bytes)
}
fn read_regular(file: &mut File, limit: u64) -> Result<Vec<u8>, AppError> {
    let before = platform_fs::file_state(file)?;
    if before.directory {
        return Err(AppError::InvalidMedia);
    }
    if before.links != 1 {
        return Err(unsafe_entry().into());
    }
    if before.size > limit {
        return Err(AppError::MediaTooLarge);
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(AppError::MediaTooLarge);
    }
    if platform_fs::file_state(file)? != before {
        return Err(unsafe_entry().into());
    }
    Ok(bytes)
}
fn valid_name(name: &str) -> Result<(), AppError> {
    let (stem, extension) = name.rsplit_once('.').ok_or_else(unsafe_entry)?;
    if !matches!(extension, "bin" | "png" | "part")
        || uuid::Uuid::parse_str(stem)
            .map(|u| u.to_string() != stem)
            .unwrap_or(true)
    {
        return Err(unsafe_entry().into());
    }
    Ok(())
}
pub fn private_directory(path: &Path, create: bool) -> Result<Directory, AppError> {
    Ok(Directory::open(path, create)?)
}
pub(crate) use platform_fs::{lock_library, LibraryLock};
pub fn read_source(path: &Path) -> Result<Vec<u8>, AppError> {
    let (mut file, _parent) = platform_fs::open_source(path)?;
    read_regular(&mut file, MAX_SOURCE)
}

/// Capture a no-follow source descriptor and verify SQLite's actual VFS identity.
#[cfg(any(target_os = "macos", windows))]
pub(crate) struct SourceDatabase {
    parent: Directory,
    file: File,
}
#[cfg(any(target_os = "macos", windows))]
impl SourceDatabase {
    pub(crate) fn open(root: &Path) -> Result<Self, AppError> {
        let parent = Directory::open(root, false)?;
        let file = parent.open_file("furfolio.sqlite3", false)?;
        let state = platform_fs::file_state(&file)?;
        if state.directory || state.links != 1 || !platform_fs::is_private(&file)? {
            return Err(unsafe_entry().into());
        }
        Ok(Self { parent, file })
    }
    pub(crate) fn verify(&self, connection: &rusqlite::Connection) -> Result<(), AppError> {
        let named = self.parent.open_file("furfolio.sqlite3", false)?;
        let state = platform_fs::file_state(&named)?;
        if state.directory
            || state.links != 1
            || platform_fs::identity(&self.file)? != state.identity
        {
            return Err(unsafe_entry().into());
        }
        platform_fs::verify_vfs_connection(&self.file, connection)
    }
}
