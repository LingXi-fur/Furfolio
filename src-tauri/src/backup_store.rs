//! Destination capabilities and no-replace publication. Platform primitives stay
//! in platform_fs. No ambient path is used after opening the parent; cleanup
//! never recurses.
#[cfg(target_os = "macos")]
mod macos {
    use crate::{
        backup::{output_error, BackupOptions},
        error::{AppError, BackupError},
    };
    use sha2::{Digest, Sha256};
    use std::{
        ffi::CString,
        fs::{File, Metadata},
        io::{Read, Write},
        os::{
            fd::{AsRawFd, FromRawFd, RawFd},
            unix::{ffi::OsStrExt, fs::MetadataExt},
        },
        path::{Component, Path},
    };

    fn error() -> AppError {
        AppError::Backup(BackupError::Destination)
    }
    fn integrity() -> AppError {
        AppError::Backup(BackupError::Integrity)
    }
    fn c_name(name: &str) -> Result<CString, AppError> {
        if name.is_empty() || name.contains('/') || name == "." || name == ".." {
            return Err(error());
        }
        CString::new(name).map_err(|_| error())
    }
    fn open_at(
        parent: RawFd,
        name: &std::ffi::CStr,
        flags: i32,
        mode: libc::mode_t,
    ) -> std::io::Result<File> {
        // SAFETY: parent is borrowed live fd, name is a live C string, result owns a fresh fd.
        let fd = unsafe {
            libc::openat(
                parent,
                name.as_ptr(),
                flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
                mode as libc::c_uint,
            )
        };
        if fd < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }
    fn identity(meta: &Metadata) -> (u64, u64) {
        (meta.dev(), meta.ino())
    }
    fn private(meta: &Metadata) -> bool {
        // SAFETY: geteuid has no pointer arguments.
        meta.uid() == unsafe { libc::geteuid() } && meta.mode() & 0o022 == 0
    }
    fn same_at(
        parent: &File,
        name: &str,
        expected: &Metadata,
        directory: bool,
    ) -> Result<(), AppError> {
        let name = c_name(name)?;
        let opened = open_at(
            parent.as_raw_fd(),
            &name,
            libc::O_RDONLY | if directory { libc::O_DIRECTORY } else { 0 },
            0,
        )
        .map_err(|_| integrity())?;
        let meta = opened.metadata().map_err(|_| integrity())?;
        if identity(&meta) != identity(expected)
            || (directory && !meta.is_dir())
            || (!directory && (!meta.is_file() || meta.nlink() != 1))
        {
            return Err(integrity());
        }
        Ok(())
    }
    pub(crate) struct Parent {
        file: File,
    }
    impl Parent {
        pub fn open(path: &Path, source: &Path) -> Result<Self, AppError> {
            if !path.is_absolute() {
                return Err(error());
            }
            let source = crate::media_store::private_directory(source, false)
                .map_err(|_| error())?
                .into_std_file();
            let excluded = identity(&source.metadata().map_err(|_| error())?);
            let mut file = File::open("/").map_err(|_| error())?;
            if identity(&file.metadata().map_err(|_| error())?) == excluded {
                return Err(error());
            }
            for component in path.components() {
                match component {
                    Component::RootDir => {}
                    Component::Normal(name) => {
                        let name = CString::new(name.as_bytes()).map_err(|_| error())?;
                        file = open_at(
                            file.as_raw_fd(),
                            &name,
                            libc::O_RDONLY | libc::O_DIRECTORY,
                            0,
                        )
                        .map_err(|_| error())?;
                        if identity(&file.metadata().map_err(|_| error())?) == excluded {
                            return Err(error());
                        }
                    }
                    _ => return Err(error()),
                }
            }
            let meta = file.metadata().map_err(|_| error())?;
            if !meta.is_dir() || !private(&meta) {
                return Err(error());
            }
            Ok(Self { file })
        }
        pub fn check_space(&self, payload: u64, manifest_limit: u64) -> Result<(), AppError> {
            let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
            // SAFETY: stat points to valid writable storage and fd is a live directory.
            if unsafe { libc::fstatvfs(self.file.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
                return Err(output_error(std::io::Error::last_os_error()));
            }
            let stat = unsafe { stat.assume_init() };
            let available = u128::from(stat.f_bavail) * u128::from(stat.f_frsize);
            let required = u128::from(payload) + u128::from(manifest_limit) + 1024 * 1024;
            if available < required {
                return Err(AppError::Backup(BackupError::NoSpace));
            }
            Ok(())
        }
        pub fn create(self, uuid: uuid::Uuid, timestamp: &str) -> Result<Staging, AppError> {
            let partial = format!(".furfolio-backup-{uuid}.partial");
            let final_name = format!("furfolio-backup-{timestamp}-{uuid}");
            let name = c_name(&partial)?;
            // SAFETY: parent is held open and name is a validated single component.
            if unsafe { libc::mkdirat(self.file.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
                let e = std::io::Error::last_os_error();
                return Err(if e.kind() == std::io::ErrorKind::AlreadyExists {
                    error()
                } else {
                    output_error(e)
                });
            }
            // If ownership cannot be established, do not delete an entry based on its name alone.
            let directory = open_at(
                self.file.as_raw_fd(),
                &name,
                libc::O_RDONLY | libc::O_DIRECTORY,
                0,
            )
            .map_err(|_| AppError::Backup(BackupError::CleanupPending(partial.clone())))?;
            let meta = directory
                .metadata()
                .map_err(|_| AppError::Backup(BackupError::CleanupPending(partial.clone())))?;
            if !private(&meta) || meta.mode() & 0o777 != 0o700 {
                return Err(AppError::Backup(BackupError::CleanupPending(partial)));
            }
            Ok(Staging {
                parent: self,
                directory,
                media: None,
                metadata: meta,
                partial,
                final_name,
                files: Vec::new(),
                published: false,
            })
        }
    }
    struct Created {
        name: String,
        media: bool,
        metadata: Metadata,
        size: u64,
        hash: String,
    }
    pub(crate) struct Staging {
        parent: Parent,
        directory: File,
        media: Option<(File, Metadata)>,
        metadata: Metadata,
        pub partial: String,
        pub final_name: String,
        files: Vec<Created>,
        published: bool,
    }
    impl Staging {
        pub fn create_media(&mut self) -> Result<(), AppError> {
            let name = c_name("media")?;
            // SAFETY: stage fd and CString are live, mkdir creates only this single child.
            if unsafe { libc::mkdirat(self.directory.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
                return Err(output_error(std::io::Error::last_os_error()));
            }
            let file = open_at(
                self.directory.as_raw_fd(),
                &name,
                libc::O_RDONLY | libc::O_DIRECTORY,
                0,
            )
            .map_err(output_error)?;
            let meta = file.metadata().map_err(output_error)?;
            self.media = Some((file, meta));
            Ok(())
        }
        fn directory_for(&self, media: bool) -> Result<&File, AppError> {
            if media {
                self.media.as_ref().map(|(f, _)| f).ok_or_else(integrity)
            } else {
                Ok(&self.directory)
            }
        }
        pub fn create_file(
            &mut self,
            name: &str,
            media: bool,
            size: u64,
            hash: &str,
        ) -> Result<File, AppError> {
            let c = c_name(name)?;
            let file = open_at(
                self.directory_for(media)?.as_raw_fd(),
                &c,
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
                0o600,
            )
            .map_err(output_error)?;
            let metadata = file.metadata().map_err(output_error)?;
            self.files.push(Created {
                name: name.into(),
                media,
                metadata,
                size,
                hash: hash.into(),
            });
            Ok(file)
        }
        pub fn write_bytes(
            &mut self,
            name: &str,
            bytes: &[u8],
            options: &BackupOptions,
        ) -> Result<(), AppError> {
            let digest = crate::media_store::hash(bytes);
            let mut file = self.create_file(name, false, bytes.len() as u64, &digest)?;
            options.output_write()?;
            for (index, chunk) in bytes.chunks(64 * 1024).enumerate() {
                options.check()?;
                file.write_all(chunk).map_err(output_error)?;
                #[cfg(test)]
                if index == 0 && options.fault == Some(crate::backup::Fault::MidWrite) {
                    return Err(AppError::Backup(BackupError::Failed));
                }
                #[cfg(not(test))]
                let _ = index;
            }
            options.file_sync()?;
            file.sync_all().map_err(output_error)
        }
        pub fn sync_and_verify(
            &self,
            manifest: &[u8],
            options: &BackupOptions,
        ) -> Result<(), AppError> {
            options.file_sync()?;
            self.media
                .as_ref()
                .ok_or_else(integrity)?
                .0
                .sync_all()
                .map_err(output_error)?;
            self.directory.sync_all().map_err(output_error)?;
            same_at(&self.parent.file, &self.partial, &self.metadata, true)?;
            let media = self.media.as_ref().ok_or_else(integrity)?;
            same_at(&self.directory, "media", &media.1, true)?;
            self.verify_set(&self.directory, false)?;
            self.verify_set(&media.0, true)?;
            for entry in &self.files {
                options.check()?;
                let directory = self.directory_for(entry.media)?;
                same_at(directory, &entry.name, &entry.metadata, false)?;
                let mut file = open_at(
                    directory.as_raw_fd(),
                    &c_name(&entry.name)?,
                    libc::O_RDONLY,
                    0,
                )
                .map_err(output_error)?;
                let before = file.metadata().map_err(output_error)?;
                if before.len() != entry.size || !private(&before) || before.mode() & 0o777 != 0o600
                {
                    return Err(integrity());
                }
                let mut remaining = entry.size;
                let mut digest = Sha256::new();
                let mut buffer = [0u8; 64 * 1024];
                let mut manifest_offset = 0usize;
                while remaining > 0 {
                    options.check()?;
                    let length = remaining.min(buffer.len() as u64) as usize;
                    let read = file.read(&mut buffer[..length]).map_err(output_error)?;
                    if read == 0 {
                        return Err(integrity());
                    }
                    if entry.name == "manifest.json" && !entry.media {
                        let end = manifest_offset.checked_add(read).ok_or_else(integrity)?;
                        if manifest.get(manifest_offset..end) != Some(&buffer[..read]) {
                            return Err(integrity());
                        }
                        manifest_offset = end;
                    }
                    digest.update(&buffer[..read]);
                    remaining -= read as u64;
                }
                if file.read(&mut buffer[..1]).map_err(output_error)? != 0
                    || format!("{:x}", digest.finalize()) != entry.hash
                {
                    return Err(integrity());
                }
                let after = file.metadata().map_err(output_error)?;
                if (
                    before.dev(),
                    before.ino(),
                    before.len(),
                    before.mtime(),
                    before.mtime_nsec(),
                    before.ctime(),
                    before.ctime_nsec(),
                    before.nlink(),
                ) != (
                    after.dev(),
                    after.ino(),
                    after.len(),
                    after.mtime(),
                    after.mtime_nsec(),
                    after.ctime(),
                    after.ctime_nsec(),
                    after.nlink(),
                ) {
                    return Err(integrity());
                }
            }
            Ok(())
        }
        fn verify_set(&self, directory: &File, media: bool) -> Result<(), AppError> {
            let dir = cap_std::fs::Dir::from_std_file(directory.try_clone().map_err(output_error)?);
            let mut count = 0usize;
            for entry in dir.entries().map_err(output_error)? {
                let name = entry.map_err(output_error)?.file_name();
                count = count.checked_add(1).ok_or_else(integrity)?;
                if !(self
                    .files
                    .iter()
                    .any(|e| e.media == media && name == std::ffi::OsStr::new(&e.name))
                    || (!media && name == "media"))
                {
                    return Err(integrity());
                }
                if count > self.files.len() + 1 {
                    return Err(integrity());
                }
            }
            let expected =
                self.files.iter().filter(|e| e.media == media).count() + usize::from(!media);
            if count != expected {
                return Err(integrity());
            }
            Ok(())
        }
        pub fn publish(
            &mut self,
            manifest: &[u8],
            options: &BackupOptions,
        ) -> Result<(), AppError> {
            options.check()?;
            // A hostile window exists between readback and rename: re-verify every
            // captured identity and byte before the directory ever becomes final.
            self.sync_and_verify(manifest, options)?;
            same_at(&self.parent.file, &self.partial, &self.metadata, true)?;
            options.before_rename()?;
            let from = c_name(&self.partial)?;
            let to = c_name(&self.final_name)?;
            // SAFETY: both validated names and parent fd remain live; EXCL forbids *any* overwrite.
            if unsafe {
                libc::renameatx_np(
                    self.parent.file.as_raw_fd(),
                    from.as_ptr(),
                    self.parent.file.as_raw_fd(),
                    to.as_ptr(),
                    libc::RENAME_EXCL,
                )
            } != 0
            {
                let e = std::io::Error::last_os_error();
                return Err(if e.kind() == std::io::ErrorKind::AlreadyExists {
                    error()
                } else {
                    output_error(e)
                });
            }
            self.published = true;
            if options
                .parent_sync()
                .and_then(|()| self.parent.file.sync_all().map_err(output_error))
                .is_err()
            {
                return Err(AppError::Backup(BackupError::DurabilityUncertain(
                    self.final_name.clone(),
                )));
            }
            Ok(())
        }
        pub fn cleanup(&mut self, options: &BackupOptions) -> Result<(), AppError> {
            if self.published {
                return Ok(());
            }
            options.cleanup()?;
            same_at(&self.parent.file, &self.partial, &self.metadata, true)?;
            for entry in self.files.iter().rev() {
                let dir = self.directory_for(entry.media)?;
                same_at(dir, &entry.name, &entry.metadata, false)?;
                Self::unlink(dir, &entry.name, false)?;
            }
            if let Some((_, meta)) = &self.media {
                same_at(&self.directory, "media", meta, true)?;
                Self::unlink(&self.directory, "media", true)?;
            }
            same_at(&self.parent.file, &self.partial, &self.metadata, true)?;
            Self::unlink(&self.parent.file, &self.partial, true)?;
            self.parent.file.sync_all().map_err(output_error)?;
            Ok(())
        }
        fn unlink(parent: &File, name: &str, directory: bool) -> Result<(), AppError> {
            let name = c_name(name)?;
            // SAFETY: live parent and validated single component; no symlink following, no recursion.
            if unsafe {
                libc::unlinkat(
                    parent.as_raw_fd(),
                    name.as_ptr(),
                    if directory { libc::AT_REMOVEDIR } else { 0 },
                )
            } != 0
            {
                return Err(output_error(std::io::Error::last_os_error()));
            }
            Ok(())
        }
    }
}
#[cfg(target_os = "macos")]
pub(crate) use macos::*;

#[cfg(windows)]
mod windows {
    use crate::{
        backup::{output_error, BackupOptions},
        error::{AppError, BackupError},
        platform_fs,
    };
    use sha2::{Digest, Sha256};
    use std::{fs::File, io::Read, io::Write, path::Path};

    fn error() -> AppError {
        AppError::Backup(BackupError::Destination)
    }
    fn integrity() -> AppError {
        AppError::Backup(BackupError::Integrity)
    }
    fn pending(name: &str) -> AppError {
        AppError::Backup(BackupError::CleanupPending(name.to_owned()))
    }

    /// One created backup file: ledger entry plus the identity read at creation.
    struct Created {
        name: String,
        media: bool,
        identity: platform_fs::Identity,
        size: u64,
        hash: String,
    }
    #[cfg(all(test, windows))]
    impl std::fmt::Debug for Created {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Created")
                .field("name", &self.name)
                .field("media", &self.media)
                .field("identity", &self.identity)
                .field("size", &self.size)
                .finish()
        }
    }

    pub(crate) struct Parent {
        directory: platform_fs::Directory,
        drive: u8,
    }
    impl Parent {
        pub fn open(path: &Path, source: &Path) -> Result<Self, AppError> {
            let (drive, names) = platform_fs::path_parts(path).map_err(|_| error())?;
            if names.is_empty() {
                return Err(error());
            }
            // The source library and its managed media directory identities are excluded
            // so a destination can never alias either, whatever its position in the path.
            let excluded = {
                let source = platform_fs::open_directory(source).map_err(|_| error())?;
                let mut identities =
                    vec![platform_fs::identity(source.file()).map_err(|_| error())?];
                if let Ok(media) = platform_fs::open_child_directory(&source, "media") {
                    identities.push(platform_fs::identity(media.file()).map_err(|_| error())?);
                }
                identities
            };
            let root = platform_fs::drive_root(drive).map_err(|_| error())?;
            if excluded.contains(&platform_fs::identity(root.file()).map_err(|_| error())?) {
                return Err(error());
            }
            let mut current = root;
            for name in &names {
                let final_component = std::ptr::eq(name, names.last().expect("non-empty"));
                current = if final_component {
                    platform_fs::open_writable_directory(&current, name)
                } else {
                    platform_fs::open_child_directory(&current, name)
                }
                .map_err(|_| error())?;
                if excluded.contains(&platform_fs::identity(current.file()).map_err(|_| error())?) {
                    return Err(error());
                }
            }
            // The destination is a user-chosen folder: ancestor ACL rules (no dangerous
            // untrusted grants) apply, but a normal folder is not application-private.
            platform_fs::verify_ancestor(current.file()).map_err(|_| error())?;
            Ok(Self {
                directory: current,
                drive,
            })
        }
        pub fn check_space(&self, payload: u64, manifest_limit: u64) -> Result<(), AppError> {
            let available = platform_fs::free_space(self.drive).map_err(output_error)?;
            let required = u128::from(payload) + u128::from(manifest_limit) + 1024 * 1024;
            if u128::from(available) < required {
                return Err(AppError::Backup(BackupError::NoSpace));
            }
            Ok(())
        }
        pub fn create(self, uuid: uuid::Uuid, timestamp: &str) -> Result<Staging, AppError> {
            let partial = format!(".furfolio-backup-{uuid}.partial");
            let final_name = format!("furfolio-backup-{timestamp}-{uuid}");
            // A name collision means a previous backup already occupies this UUID slot;
            // every other failure leaves a possible orphan that must not be name-deleted.
            let directory = match platform_fs::create_private_directory(&self.directory, &partial) {
                Ok(directory) => directory,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Err(error()),
                Err(_) => return Err(pending(&partial)),
            };
            Ok(Staging {
                parent: self,
                directory,
                media: None,
                partial,
                final_name,
                files: Vec::new(),
                published: false,
                media_created: false,
            })
        }
    }

    pub(crate) struct Staging {
        parent: Parent,
        // Declared before the stage root so any drop path releases the media
        // handle first: Windows deletion resolves only at last close, and an
        // empty stage must never linger behind its still-open child.
        media: Option<platform_fs::Directory>,
        directory: platform_fs::Directory,
        pub partial: String,
        pub final_name: String,
        files: Vec<Created>,
        published: bool,
        // Whether the media subdirectory was created, surviving the publish-time
        // release of its handle: cleanup must still mark it deleted.
        media_created: bool,
    }
    impl Staging {
        pub fn create_media(&mut self) -> Result<(), AppError> {
            let directory = platform_fs::create_private_directory(&self.directory, "media")
                .map_err(output_error)?;
            self.media = Some(directory);
            self.media_created = true;
            Ok(())
        }
        fn directory_for(&self, media: bool) -> Result<&platform_fs::Directory, AppError> {
            if media {
                self.media.as_ref().ok_or_else(integrity)
            } else {
                Ok(&self.directory)
            }
        }
        /// The named child still exists and keeps the exact identity captured earlier.
        fn same(
            parent: &platform_fs::Directory,
            name: &str,
            expected: &platform_fs::Identity,
        ) -> Result<(), AppError> {
            let opened =
                platform_fs::open_child_directory(parent, name).map_err(|_| integrity())?;
            if platform_fs::identity(opened.file()).map_err(|_| integrity())? != *expected {
                return Err(integrity());
            }
            Ok(())
        }
        pub fn create_file(
            &mut self,
            name: &str,
            media: bool,
            size: u64,
            hash: &str,
        ) -> Result<File, AppError> {
            let file = self
                .directory_for(media)?
                .open_file(name, true)
                .map_err(output_error)?;
            let identity = platform_fs::identity(&file).map_err(output_error)?;
            self.files.push(Created {
                name: name.into(),
                media,
                identity,
                size,
                hash: hash.into(),
            });
            Ok(file)
        }
        pub fn write_bytes(
            &mut self,
            name: &str,
            bytes: &[u8],
            options: &BackupOptions,
        ) -> Result<(), AppError> {
            let digest = crate::media_store::hash(bytes);
            let mut file = self.create_file(name, false, bytes.len() as u64, &digest)?;
            options.output_write()?;
            for (index, chunk) in bytes.chunks(64 * 1024).enumerate() {
                options.check()?;
                file.write_all(chunk).map_err(output_error)?;
                #[cfg(test)]
                if index == 0 && options.fault == Some(crate::backup::Fault::MidWrite) {
                    return Err(AppError::Backup(BackupError::Failed));
                }
                #[cfg(not(test))]
                let _ = index;
            }
            options.file_sync()?;
            file.sync_all().map_err(output_error)
        }
        pub fn sync_and_verify(
            &self,
            manifest: &[u8],
            options: &BackupOptions,
        ) -> Result<(), AppError> {
            options.file_sync()?;
            let media = self.media.as_ref().ok_or_else(integrity)?;
            let stage_identity =
                platform_fs::identity(self.directory.file()).map_err(|_| integrity())?;
            Self::same(&self.parent.directory, &self.partial, &stage_identity)?;
            let media_identity = platform_fs::identity(media.file()).map_err(|_| integrity())?;
            Self::same(&self.directory, "media", &media_identity)?;
            self.verify_set(&self.directory, false)?;
            self.verify_set(media, true)?;
            for entry in &self.files {
                options.check()?;
                let directory = self.directory_for(entry.media)?;
                let mut file = directory
                    .open_file(&entry.name, false)
                    .map_err(output_error)?;
                let before = platform_fs::file_state(&file).map_err(output_error)?;
                if before.directory || before.links != 1 || before.identity != entry.identity {
                    return Err(integrity());
                }
                let mut remaining = entry.size;
                let mut digest = Sha256::new();
                let mut buffer = [0u8; 64 * 1024];
                let mut manifest_offset = 0usize;
                while remaining > 0 {
                    options.check()?;
                    let length = remaining.min(buffer.len() as u64) as usize;
                    let read = file.read(&mut buffer[..length]).map_err(output_error)?;
                    if read == 0 {
                        return Err(integrity());
                    }
                    if entry.name == "manifest.json" && !entry.media {
                        let end = manifest_offset.checked_add(read).ok_or_else(integrity)?;
                        if manifest.get(manifest_offset..end) != Some(&buffer[..read]) {
                            return Err(integrity());
                        }
                        manifest_offset = end;
                    }
                    digest.update(&buffer[..read]);
                    remaining -= read as u64;
                }
                if file.read(&mut buffer[..1]).map_err(output_error)? != 0
                    || format!("{:x}", digest.finalize()) != entry.hash
                    || platform_fs::file_state(&file).map_err(output_error)? != before
                {
                    return Err(integrity());
                }
            }
            Ok(())
        }
        /// Every staged directory holds exactly the ledgered files, nothing else.
        fn verify_set(
            &self,
            directory: &platform_fs::Directory,
            media: bool,
        ) -> Result<(), AppError> {
            let mut expected: std::collections::BTreeSet<String> = self
                .files
                .iter()
                .filter(|e| e.media == media)
                .map(|e| e.name.clone())
                .collect();
            if !media {
                // The stage root always contains the media subdirectory itself.
                expected.insert("media".to_owned());
            }
            let dir = cap_std::fs::Dir::from_std_file(
                directory.file().try_clone().map_err(output_error)?,
            );
            let mut count = 0usize;
            for entry in dir.entries().map_err(output_error)? {
                let name = entry.map_err(output_error)?.file_name();
                count = count.checked_add(1).ok_or_else(integrity)?;
                let name = name.to_str().ok_or_else(integrity)?;
                if !expected.contains(name) {
                    return Err(integrity());
                }
                if count > expected.len() {
                    return Err(integrity());
                }
            }
            if count != expected.len() {
                return Err(integrity());
            }
            Ok(())
        }
        pub fn publish(
            &mut self,
            manifest: &[u8],
            options: &BackupOptions,
        ) -> Result<(), AppError> {
            options.check()?;
            // A hostile window exists between readback and rename: re-verify every
            // captured identity and byte before the directory ever becomes final.
            self.sync_and_verify(manifest, options)?;
            options.before_rename()?;
            // Windows refuses to rename a directory while any file inside it (or a
            // subdirectory) still has an open handle. The verified media handle's
            // work is done; release it before the rename. Failure paths after this
            // point keep the stage name in the ledger for manual cleanup.
            self.media = None;
            // The held stage handle carries DELETE, which FileRenameInformation requires;
            // its identity was verified against the parent at creation and re-verified above.
            // NT rename with ReplaceIfExists=false rejects any existing target, which is
            // this platform's rename-exclusive; no overwrite fallback exists.
            platform_fs::rename_exclusive(
                self.directory.file(),
                self.parent.directory.file(),
                &self.final_name,
            )
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    error()
                } else {
                    output_error(e)
                }
            })?;
            self.published = true;
            // Windows has no directory FlushFileBuffers; verified content was flushed on
            // its file handles before publication. A failure after the rename is reported
            // as uncertain exactly like the Unix flow: the final name is published and
            // must never be deleted or re-staged by cleanup.
            if options.parent_sync().is_err() {
                return Err(AppError::Backup(BackupError::DurabilityUncertain(
                    self.final_name.clone(),
                )));
            }
            Ok(())
        }
        pub fn cleanup(&mut self, options: &BackupOptions) -> Result<(), AppError> {
            if self.published {
                return Ok(());
            }
            options.cleanup()?;
            let stage_identity =
                platform_fs::identity(self.directory.file()).map_err(|_| pending(&self.partial))?;
            Self::same(&self.parent.directory, &self.partial, &stage_identity)
                .map_err(|_| pending(&self.partial))?;
            // Ledgered files first (reverse order), then the media directory, then the
            // stage itself: only ledger-verified identities are ever marked for deletion.
            for entry in self.files.iter().rev() {
                options.check()?;
                let directory = self.directory_for(entry.media)?;
                let file = directory.open_mutable(&entry.name).map_err(|e| {
                    #[cfg(all(test, windows))]
                    eprintln!("cleanup open_mutable {}: {e:?}", entry.name);
                    #[cfg(not(all(test, windows)))]
                    let _ = &e;
                    pending(&self.partial)
                })?;
                let state = platform_fs::file_state(&file).map_err(|_| pending(&self.partial))?;
                if state.identity != entry.identity || state.size != entry.size || state.links != 1
                {
                    #[cfg(all(test, windows))]
                    eprintln!(
                        "cleanup state mismatch {}: recorded={:?} actual={:?}",
                        entry.name, entry, state
                    );
                    return Err(pending(&self.partial));
                }
                directory.remove_file(&entry.name, &file).map_err(|e| {
                    #[cfg(all(test, windows))]
                    eprintln!("cleanup remove_file {}: {e:?}", entry.name);
                    #[cfg(not(all(test, windows)))]
                    let _ = &e;
                    output_error(e)
                })?;
                drop(file);
            }
            // Mark the now-empty media directory for deletion and release its handle,
            // then mark the stage: Windows deletion is POSIX-style delayed and only
            // resolves once every handle has closed, and each directory must be
            // empty-and-unheld to disappear at last close. After a failed publish
            // the media handle was already released ahead of the rename attempt,
            // so reopen it by its ledgered name only for the explicit mark.
            if let Some(media) = self.media.as_ref() {
                platform_fs::mark_directory_deleted(media).map_err(output_error)?;
            } else if self.media_created {
                let media = platform_fs::open_deletable_directory(&self.directory, "media")
                    .map_err(|_| pending(&self.partial))?;
                platform_fs::mark_directory_deleted(&media).map_err(output_error)?;
            }
            self.media = None;
            platform_fs::mark_directory_deleted(&self.directory).map_err(output_error)?;
            Ok(())
        }
    }
}
#[cfg(windows)]
pub(crate) use windows::*;
