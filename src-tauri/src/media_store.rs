//! Private capability-rooted media IO. No canonicalize/check-then-open path authorization.
use crate::error::AppError;
use cap_std::fs::Dir;
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
    std::io::Error::other("Managed media identity could not be verified")
}

pub struct Store {
    dir: Dir,
}
impl Store {
    pub fn open(root: &Path) -> Result<Self, AppError> {
        let parent = private_directory(root, false)?;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let fd = parent.as_raw_fd();
            mkdir_at(fd, "media")?;
            let directory = open_at(fd, "media", libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
            verify_private_directory(&directory)?;
            Ok(Self {
                dir: Dir::from_std_file(directory),
            })
        }
        #[cfg(not(unix))]
        {
            let _ = parent;
            Err(unsafe_entry().into())
        }
    }
    pub fn read(&self, name: &str, expected: &str, size: u64) -> Result<Vec<u8>, AppError> {
        valid_name(name)?;
        let file = self.open_file(name, false)?;
        let bytes = read_regular(file, size)?;
        if bytes.len() as u64 != size || hash(&bytes) != expected {
            return Err(unsafe_entry().into());
        }
        Ok(bytes)
    }
    pub fn publish(&self, name: &str, staging: &str, bytes: &[u8]) -> Result<(), AppError> {
        valid_name(name)?;
        valid_name(staging)?;
        let mut file = self.open_file(staging, true)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        self.sync()?;
        self.rename_exclusive(staging, name)?;
        self.sync()?;
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
        // Never unlink a path merely because it has a UUID. Both names are ledger credentials.
        // Quarantine with no-overwrite rename, then re-verify the captured entry before unlink.
        if self.exists(name)? {
            self.read(name, expected, size)?;
            if self.exists(staging)? {
                return Err(unsafe_entry().into());
            }
            self.rename_exclusive(name, staging)?;
            self.sync()?;
        }
        if self.exists(staging)? {
            self.read(staging, expected, size)?;
            #[cfg(unix)]
            {
                use std::os::fd::AsRawFd;
                let name = c_name(staging)?;
                // unlinkat never follows the final component. The private directory is held open.
                // SAFETY: fd is a live directory handle; CString is NUL-terminated and valid
                // until the call returns; flags=0 forbids directory removal and never follows links.
                if unsafe { libc::unlinkat(self.dir.as_raw_fd(), name.as_ptr(), 0) } != 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
            }
            #[cfg(not(unix))]
            {
                return Err(unsafe_entry().into());
            }
            self.sync()?;
        }
        Ok(())
    }
    fn sync(&self) -> Result<(), AppError> {
        self.dir.try_clone()?.into_std_file().sync_all()?;
        Ok(())
    }
    fn exists(&self, name: &str) -> Result<bool, AppError> {
        match self.dir.symlink_metadata(name) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
    fn open_file(&self, name: &str, create: bool) -> Result<File, AppError> {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let flags = if create {
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL
            } else {
                libc::O_RDONLY
            };
            Ok(open_at(
                self.dir.as_raw_fd(),
                name,
                flags | libc::O_NONBLOCK,
                0o600,
            )?)
        }
        #[cfg(not(unix))]
        {
            let _ = (name, create);
            Err(unsafe_entry().into())
        }
    }
    fn rename_exclusive(&self, from: &str, to: &str) -> Result<(), AppError> {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let fd = self.dir.as_raw_fd();
            let from = c_name(from)?;
            let to = c_name(to)?;
            // SAFETY: both names are validated single components and remain live CStrings;
            // fd is the same private open directory for both sides. EXCL never overwrites.
            #[cfg(target_os = "macos")]
            let result = unsafe {
                libc::renameatx_np(fd, from.as_ptr(), fd, to.as_ptr(), libc::RENAME_EXCL)
            };
            // SAFETY: same live directory and validated CString invariants as renameatx_np.
            #[cfg(target_os = "linux")]
            let result = unsafe {
                libc::renameat2(fd, from.as_ptr(), fd, to.as_ptr(), libc::RENAME_NOREPLACE)
            };
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            return Err(unsafe_entry().into());
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            if result != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(())
        }
        #[cfg(not(unix))]
        {
            let _ = (from, to);
            Err(unsafe_entry().into())
        }
    }
}

pub fn lock_library(path: &Path) -> Result<File, AppError> {
    #[cfg(unix)]
    {
        use std::{os::fd::AsRawFd, os::unix::fs::OpenOptionsExt};
        let lock_path = path.with_extension("media-lock");
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(lock_path)?;
        if !file.metadata()?.is_file() {
            return Err(unsafe_entry().into());
        }
        // A second app instance must not recover another process's pending import.
        // SAFETY: file owns this live fd; flock takes no pointers, and nonblocking failure
        // is propagated without proceeding to recovery. Closing the guard releases the lock.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(AppError::MediaBusy);
        }
        Ok(file)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(unsafe_entry().into())
    }
}

pub fn read_source(path: &Path) -> Result<Vec<u8>, AppError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)?;
        read_regular(file, MAX_SOURCE)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(unsafe_entry().into())
    }
}
fn read_regular(file: File, limit: u64) -> Result<Vec<u8>, AppError> {
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(AppError::InvalidMedia);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() != 1 {
            return Err(unsafe_entry().into());
        }
    }
    if meta.len() > limit {
        return Err(AppError::MediaTooLarge);
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(AppError::MediaTooLarge);
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

/// Walk every component with openat(O_NOFOLLOW|O_DIRECTORY); new directories are 0700.
pub fn private_directory(path: &Path, create: bool) -> Result<Dir, AppError> {
    #[cfg(unix)]
    {
        use std::{os::fd::AsRawFd, path::Component};
        if !path.is_absolute() {
            return Err(unsafe_entry().into());
        }
        let mut current = File::open("/")?;
        for component in path.components() {
            match component {
                Component::RootDir => {}
                Component::Normal(name) => {
                    let name = name.to_str().ok_or_else(unsafe_entry)?;
                    if create {
                        mkdir_at(current.as_raw_fd(), name)?;
                    }
                    current = open_at(
                        current.as_raw_fd(),
                        name,
                        libc::O_RDONLY | libc::O_DIRECTORY,
                        0,
                    )?;
                }
                _ => return Err(unsafe_entry().into()),
            }
        }
        verify_private_directory(&current)?;
        Ok(Dir::from_std_file(current))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, create);
        Err(unsafe_entry().into())
    }
}
#[cfg(unix)]
fn verify_private_directory(file: &File) -> Result<(), AppError> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    // Same-uid hostile processes can already alter the database; group/world writers cannot
    // be allowed to race our final-component identity checks. Never chmod an existing library.
    // SAFETY: geteuid has no pointer arguments or preconditions.
    if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o022 != 0 {
        return Err(unsafe_entry().into());
    }
    Ok(())
}

#[cfg(unix)]
fn c_name(name: &str) -> Result<std::ffi::CString, std::io::Error> {
    std::ffi::CString::new(name).map_err(|_| unsafe_entry())
}
#[cfg(unix)]
fn open_at(
    fd: std::os::fd::RawFd,
    name: &str,
    flags: i32,
    mode: libc::mode_t,
) -> Result<File, std::io::Error> {
    use std::os::fd::FromRawFd;
    let name = c_name(name)?;
    // SAFETY: fd is a borrowed live directory handle, the CString lives through the call,
    // flags always forbid symlinks; mode is promoted to c_uint for the variadic ABI.
    let fd = unsafe {
        libc::openat(
            fd,
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            mode as libc::c_uint,
        )
    };
    if fd < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        // SAFETY: a successful openat returns a new owned fd, wrapped exactly once.
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}
#[cfg(unix)]
fn mkdir_at(fd: std::os::fd::RawFd, name: &str) -> Result<(), std::io::Error> {
    let name = c_name(name)?;
    // SAFETY: fd is a live directory and the CString remains valid; permissions apply only
    // when mkdirat creates a new entry. Existing entries are subsequently opened no-follow.
    if unsafe { libc::mkdirat(fd, name.as_ptr(), 0o700) } != 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(error);
        }
    }
    Ok(())
}
