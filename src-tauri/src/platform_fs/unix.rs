use std::{
    ffi::CString,
    fs::{File, Metadata},
    io,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::MetadataExt,
    },
    path::{Component, Path},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Identity(pub u64, pub u64);
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileState {
    pub identity: Identity,
    pub size: u64,
    pub links: u64,
    pub directory: bool,
    stamp: (i64, i64, i64, i64),
}
pub(crate) fn unsafe_entry() -> io::Error {
    io::Error::other("Managed media identity could not be verified")
}
pub(crate) fn identity(file: &File) -> io::Result<Identity> {
    let m = file.metadata()?;
    Ok(Identity(m.dev(), m.ino()))
}
pub(crate) fn file_state(file: &File) -> io::Result<FileState> {
    let m = file.metadata()?;
    if !m.is_dir() && !m.is_file() {
        return Err(unsafe_entry());
    }
    Ok(FileState {
        identity: Identity(m.dev(), m.ino()),
        size: m.len(),
        links: m.nlink(),
        directory: m.is_dir(),
        stamp: (m.mtime(), m.mtime_nsec(), m.ctime(), m.ctime_nsec()),
    })
}
fn private(m: &Metadata) -> bool {
    // SAFETY: geteuid has no pointer arguments or preconditions.
    m.uid() == unsafe { libc::geteuid() } && m.mode() & 0o022 == 0
}
pub(crate) fn verify_private(file: &File) -> io::Result<()> {
    if !private(&file.metadata()?) {
        return Err(unsafe_entry());
    }
    Ok(())
}
fn name(s: &str) -> io::Result<CString> {
    if s.is_empty() || s == "." || s == ".." || s.contains('/') {
        return Err(unsafe_entry());
    }
    CString::new(s).map_err(|_| unsafe_entry())
}
fn open_at(parent: &File, s: &str, flags: i32, mode: libc::mode_t) -> io::Result<File> {
    let s = name(s)?;
    // SAFETY: parent is a live borrowed directory; name remains a live CString. The
    // new descriptor is wrapped exactly once, and no-follow/nonblock are unconditional.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            s.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
            mode as libc::c_uint,
        )
    };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}
pub(crate) struct Directory {
    file: File,
}
impl Directory {
    pub fn open(path: &Path, create: bool) -> io::Result<Self> {
        if !path.is_absolute() {
            return Err(unsafe_entry());
        }
        let mut file = File::open("/")?;
        for c in path.components() {
            match c {
                Component::RootDir => {}
                Component::Normal(s) => {
                    let s = s.to_str().ok_or_else(unsafe_entry)?;
                    if create {
                        mkdir(&file, s)?;
                    }
                    file = open_at(&file, s, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
                }
                _ => return Err(unsafe_entry()),
            }
        }
        verify_private(&file)?;
        Ok(Self { file })
    }
    pub fn file(&self) -> &File {
        &self.file
    }
    pub fn into_std_file(self) -> File {
        self.file
    }
    pub fn open_mutable(&self, name: &str) -> io::Result<File> {
        self.open_file(name, false)
    }
    pub fn child(&self, s: &str, create: bool) -> io::Result<Self> {
        if create {
            mkdir(&self.file, s)?;
        }
        let file = open_at(&self.file, s, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        verify_private(&file)?;
        Ok(Self { file })
    }
    pub fn open_file(&self, s: &str, create: bool) -> io::Result<File> {
        open_at(
            &self.file,
            s,
            if create {
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL
            } else {
                libc::O_RDONLY
            },
            0o600,
        )
    }
    pub fn exists(&self, s: &str) -> io::Result<bool> {
        let s = name(s)?;
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: live descriptor, CString, and writable stat storage; final links are not followed.
        if unsafe {
            libc::fstatat(
                self.file.as_raw_fd(),
                s.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } == 0
        {
            return Ok(true);
        }
        let e = io::Error::last_os_error();
        if e.kind() == io::ErrorKind::NotFound {
            Ok(false)
        } else {
            Err(e)
        }
    }
    pub fn sync(&self) -> io::Result<()> {
        self.file.sync_all()
    }
    pub fn rename(&self, from: &str, to: &str, _file: &File) -> io::Result<()> {
        let from = name(from)?;
        let to = name(to)?;
        #[cfg(target_os = "macos")]
        let r = unsafe {
            libc::renameatx_np(
                self.file.as_raw_fd(),
                from.as_ptr(),
                self.file.as_raw_fd(),
                to.as_ptr(),
                libc::RENAME_EXCL,
            )
        };
        #[cfg(target_os = "linux")]
        let r = unsafe {
            libc::renameat2(
                self.file.as_raw_fd(),
                from.as_ptr(),
                self.file.as_raw_fd(),
                to.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if r != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    pub fn remove_file(&self, s: &str, file: &File) -> io::Result<()> {
        let named = self.open_file(s, false)?;
        if identity(&named)? != identity(file)? {
            return Err(unsafe_entry());
        }
        let s = name(s)?;
        // SAFETY: held private parent and validated single component; never recursive.
        if unsafe { libc::unlinkat(self.file.as_raw_fd(), s.as_ptr(), 0) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
fn mkdir(parent: &File, s: &str) -> io::Result<()> {
    let s = name(s)?;
    // SAFETY: validated live single component; mode only affects newly created children.
    if unsafe { libc::mkdirat(parent.as_raw_fd(), s.as_ptr(), 0o700) } != 0 {
        let e = io::Error::last_os_error();
        if e.kind() != io::ErrorKind::AlreadyExists {
            return Err(e);
        }
    }
    Ok(())
}
pub(crate) struct LibraryLock {
    _root: Option<Directory>,
    _file: File,
}
pub(crate) fn lock_library(path: &Path) -> Result<LibraryLock, crate::error::AppError> {
    #[cfg(test)]
    if path.file_name() != Some(std::ffi::OsStr::new("furfolio.sqlite3")) {
        use std::os::unix::fs::OpenOptionsExt;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(path.with_extension("media-lock"))?;
        if !file.metadata()?.is_file() {
            return Err(unsafe_entry().into());
        }
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(crate::error::AppError::MediaBusy);
        }
        return Ok(LibraryLock {
            _root: None,
            _file: file,
        });
    }
    let root = Directory::open(path.parent().ok_or_else(unsafe_entry)?, false)?;
    let s = path
        .with_extension("media-lock")
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(unsafe_entry)?
        .to_owned();
    let file = open_at(root.file(), &s, libc::O_RDWR | libc::O_CREAT, 0o600)?;
    if !file.metadata()?.is_file() {
        return Err(unsafe_entry().into());
    }
    // SAFETY: live descriptor; nonblocking lock lasts until this owned file closes.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(crate::error::AppError::MediaBusy);
    }
    Ok(LibraryLock {
        _root: Some(root),
        _file: file,
    })
}
pub(crate) fn open_source(path: &Path) -> io::Result<(File, Option<Directory>)> {
    use std::os::unix::fs::OpenOptionsExt;
    Ok((
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)?,
        None,
    ))
}
