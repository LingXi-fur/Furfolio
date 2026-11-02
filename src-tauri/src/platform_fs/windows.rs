//! Windows handle-relative primitives. All raw handles, NT ABI and ACL parsing stay here.
use std::{
    ffi::c_void,
    fs::File,
    io,
    mem::{offset_of, size_of, MaybeUninit},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Component, Path, Prefix},
    ptr::{null, null_mut},
    sync::Arc,
};
use windows_sys::Win32::{
    Foundation::{LocalFree, HANDLE},
    Security::{
        AddAccessAllowedAceEx,
        Authorization::{ConvertSidToStringSidW, GetSecurityInfo, SE_FILE_OBJECT},
        ConvertStringSidToSidW, GetLengthSid, GetSecurityDescriptorControl, GetTokenInformation,
        InitializeAcl, IsValidSid, SetSecurityDescriptorDacl, SetSecurityDescriptorGroup,
        SetSecurityDescriptorOwner, TokenUser, ACL, DACL_SECURITY_INFORMATION,
        GROUP_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, TOKEN_QUERY,
        TOKEN_USER,
    },
    Storage::FileSystem::{
        CreateFileW, FileBasicInfo, FileDispositionInfo, FileIdInfo, FileStandardInfo,
        GetDriveTypeW, GetFileInformationByHandleEx, GetVolumeInformationByHandleW, LockFileEx,
        ReOpenFile, SetFileInformationByHandle, FILE_BASIC_INFO, FILE_DISPOSITION_INFO,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FILE_STANDARD_INFO, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY,
        OPEN_EXISTING,
    },
    System::{
        Memory::{LocalAlloc, LMEM_FIXED},
        Threading::{GetCurrentProcess, OpenProcessToken},
        IO::OVERLAPPED,
    },
};

const READ_CONTROL: u32 = 0x0002_0000;
const DELETE: u32 = 0x0001_0000;
const SYNCHRONIZE: u32 = 0x0010_0000;
const READ_DIR: u32 = READ_CONTROL | SYNCHRONIZE | 0x81 | 0x20;
const WRITE_DIR: u32 = READ_DIR | 0x6;
const READ_FILE: u32 = READ_CONTROL | SYNCHRONIZE | 0x89;
const WRITE_FILE: u32 = READ_FILE | 0x116 | DELETE;
const REPARSE: u32 = 0x400;
const DIRECTORY: u32 = 0x10;
const FILE_OPEN: u32 = 1;
const FILE_CREATE: u32 = 2;
const FILE_OPEN_IF: u32 = 3;
const FILE_DIRECTORY_FILE: u32 = 1;
const FILE_NON_DIRECTORY_FILE: u32 = 0x40;
const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x20;
const FILE_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *mut u16,
}
#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root: HANDLE,
    name: *mut UnicodeString,
    attributes: u32,
    security: *mut c_void,
    qos: *mut c_void,
}
#[repr(C)]
struct IoStatus {
    status: usize,
    information: usize,
}
#[repr(C)]
struct RenameInfo {
    replace: u8,
    root: HANDLE,
    length: u32,
    name: [u16; 1],
}
#[link(name = "ntdll")]
extern "system" {
    fn NtCreateFile(
        handle: *mut HANDLE,
        access: u32,
        attributes: *const ObjectAttributes,
        status: *mut IoStatus,
        allocation: *const i64,
        file_attributes: u32,
        share: u32,
        disposition: u32,
        options: u32,
        ea: *const c_void,
        ea_length: u32,
    ) -> i32;
    fn NtSetInformationFile(
        handle: HANDLE,
        status: *mut IoStatus,
        information: *const c_void,
        length: u32,
        class: u32,
    ) -> i32;
    fn RtlNtStatusToDosError(status: i32) -> u32;
}
fn raw(file: &File) -> HANDLE {
    file.as_raw_handle() as HANDLE
}
pub(crate) fn unsafe_entry() -> io::Error {
    io::Error::other("Managed media identity could not be verified")
}
fn status_error(status: i32) -> io::Error {
    // SAFETY: NTSTATUS conversion is pure and accepts every status value.
    io::Error::from_raw_os_error(unsafe { RtlNtStatusToDosError(status) } as i32)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Identity {
    volume: u64,
    file: [u8; 16],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileState {
    pub identity: Identity,
    pub size: u64,
    pub links: u64,
    pub directory: bool,
    stamp: (i64, i64, i64, u32),
}
fn query<T>(file: &File, class: i32) -> io::Result<T> {
    let mut value = MaybeUninit::<T>::uninit();
    // SAFETY: callers pair the fixed Win32 information class with its documented T;
    // storage is aligned, writable and exactly sized; the file remains live.
    if unsafe {
        GetFileInformationByHandleEx(
            raw(file),
            class,
            value.as_mut_ptr().cast(),
            size_of::<T>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { value.assume_init() })
}
pub(crate) fn identity(file: &File) -> io::Result<Identity> {
    let info: FILE_ID_INFO = query(file, FileIdInfo)?;
    Ok(Identity {
        volume: info.VolumeSerialNumber,
        file: info.FileId.Identifier,
    })
}
pub(crate) fn file_state(file: &File) -> io::Result<FileState> {
    let basic: FILE_BASIC_INFO = query(file, FileBasicInfo)?;
    let standard: FILE_STANDARD_INFO = query(file, FileStandardInfo)?;
    if basic.FileAttributes & REPARSE != 0 || standard.DeletePending != 0 || standard.EndOfFile < 0
    {
        return Err(unsafe_entry());
    }
    let directory = standard.Directory != 0;
    if directory != (basic.FileAttributes & DIRECTORY != 0) {
        return Err(unsafe_entry());
    }
    Ok(FileState {
        identity: identity(file)?,
        size: standard.EndOfFile as u64,
        links: u64::from(standard.NumberOfLinks),
        directory,
        stamp: (
            basic.CreationTime,
            basic.LastWriteTime,
            basic.ChangeTime,
            basic.FileAttributes,
        ),
    })
}
struct LocalMemory(*mut c_void);
impl Drop for LocalMemory {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}
impl LocalMemory {
    fn from_bytes(bytes: Vec<u8>) -> io::Result<Self> {
        let handle = LocalAlloc(LMEM_FIXED, bytes.len());
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: handle owns exactly bytes.len() writable, initialized storage.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), handle.cast(), bytes.len()) };
        Ok(Self(handle))
    }
}
fn sid_string(sid: *mut c_void) -> io::Result<String> {
    let mut text = null_mut();
    // SAFETY: the SID comes from a successfully queried OS token/security descriptor.
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let _memory = LocalMemory(text.cast());
    let mut n = 0;
    // SID text is OS-produced and NUL-terminated; impose a defensive maximum anyway.
    while n < 256 && unsafe { *text.add(n) } != 0 {
        n += 1;
    }
    if n == 256 {
        return Err(unsafe_entry());
    }
    String::from_utf16(unsafe { std::slice::from_raw_parts(text, n) }).map_err(|_| unsafe_entry())
}
fn current_sid() -> io::Result<String> {
    let mut token = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: OpenProcessToken returned a newly owned handle, wrapped once.
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    let mut length = 0;
    unsafe {
        GetTokenInformation(
            token.as_raw_handle() as HANDLE,
            TokenUser,
            null_mut(),
            0,
            &mut length,
        );
    }
    if (length as usize) < size_of::<TOKEN_USER>() || length > 65536 {
        return Err(unsafe_entry());
    }
    let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle() as HANDLE,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &mut length,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    sid_string(user.User.Sid)
}
fn sid_bytes() -> io::Result<(Vec<u8>, String)> {
    let sid = current_sid()?;
    let text: Vec<u16> = sid.encode_utf16().chain(Some(0)).collect();
    let mut parsed = null_mut();
    if unsafe { ConvertStringSidToSidW(text.as_ptr(), &mut parsed) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let _memory = LocalMemory(parsed.cast());
    // SAFETY: IsValidSid accepts only a well-formed OS SID; GetLengthSid returns
    // its exact byte size, bounded here before copying into owned storage.
    if unsafe { IsValidSid(parsed) } == 0 {
        return Err(unsafe_entry());
    }
    let length = unsafe { GetLengthSid(parsed) } as usize;
    if length < 8 || length > 68 {
        return Err(unsafe_entry());
    }
    let bytes = unsafe { std::slice::from_raw_parts(parsed.cast::<u8>(), length) }.to_vec();
    Ok((bytes, sid))
}
/// Absolute SECURITY_DESCRIPTOR: owner=group=current user, protected DACL granting
/// FILE_ALL_ACCESS to the current user, SYSTEM and Administrators only. The ACL is
/// allocated behind the descriptor inside one self-contained LocalAlloc block whose
/// Dacl offset points just past the fixed header, so no separate lifetime is needed.
fn descriptor() -> io::Result<LocalMemory> {
    const ALL: u32 = 0x001f_01ff;
    const ADMINISTRATORS: &str = "S-1-5-32-544";
    const SYSTEM: &str = "S-1-5-18";
    fn parse(text: &str) -> io::Result<LocalMemory> {
        let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        let mut parsed = null_mut();
        if unsafe { ConvertStringSidToSidW(wide.as_ptr(), &mut parsed) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let memory = LocalMemory(parsed.cast());
        // SAFETY: the parser returns a well-formed OS SID or fails; bounded copy below.
        if unsafe { IsValidSid(parsed) } == 0 {
            return Err(unsafe_entry());
        }
        let length = unsafe { GetLengthSid(parsed) } as usize;
        if length < 8 || length > 68 {
            return Err(unsafe_entry());
        }
        Ok(memory)
    }
    let (user_bytes, _) = sid_bytes()?;
    let user = LocalMemory::from_bytes(user_bytes)?;
    let system = parse(SYSTEM)?;
    let administrators = parse(ADMINISTRATORS)?;
    const HEADER: usize = size_of::<u32>() * 6;
    const ACL_SPACE: usize = 8 + 3 * (8 + 68); // ACL header plus three maximal ACEs
    let allocation = LocalAlloc(LMEM_FIXED, HEADER + ACL_SPACE);
    if allocation.is_null() {
        return Err(io::Error::last_os_error());
    }
    let descriptor = LocalMemory(allocation);
    let acl = unsafe { descriptor.0.add(HEADER).cast::<ACL>() };
    // SAFETY: descriptor is a fresh zeroed absolute SECURITY_DESCRIPTOR of HEADER
    // bytes followed by ACL_SPACE writable bytes; every SID is a validated OS SID.
    unsafe {
        if InitializeAcl(acl, ACL_SPACE as u32, 2) == 0 {
            return Err(io::Error::last_os_error());
        }
        for sid in [&user, &system, &administrators] {
            if AddAccessAllowedAceEx(acl, 2, 0, ALL, sid.0.cast()) == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        let used = (*acl).AclSize as usize;
        let user_sid: PSECURITY_DESCRIPTOR = user.0.cast();
        if SetSecurityDescriptorOwner(descriptor.0.cast(), user_sid.cast(), 0) == 0
            || SetSecurityDescriptorGroup(descriptor.0.cast(), user_sid.cast(), 0) == 0
            || SetSecurityDescriptorDacl(descriptor.0.cast(), 1, acl, 0) == 0
        {
            return Err(io::Error::last_os_error());
        }
        // Pack the live ACL to the tail of the block, then point Dacl at it so the
        // whole security descriptor stays valid for as long as the allocation lives.
        let packed = descriptor.0.add(HEADER + used);
        std::ptr::copy_nonoverlapping(acl.cast::<u8>(), packed.cast(), used);
        let dacl_offset = (HEADER + used) as u32;
        std::ptr::copy_nonoverlapping(
            dacl_offset.to_le_bytes().as_ptr(),
            descriptor.0.add(3 * size_of::<u32>()).cast(),
            4,
        );
    }
    Ok(descriptor)
}
fn trusted(sid: &str, user: &str) -> bool {
    sid == user || sid == "S-1-5-18" || sid == "S-1-5-32-544"
}
fn verify_acl(file: &File, ancestor: bool) -> io::Result<()> {
    let mut owner = null_mut();
    let mut acl = null_mut::<ACL>();
    let mut sd = null_mut();
    let error = unsafe {
        GetSecurityInfo(
            raw(file),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut acl,
            null_mut(),
            &mut sd,
        )
    };
    if error != 0 {
        return Err(io::Error::from_raw_os_error(error as i32));
    }
    let _memory = LocalMemory(sd);
    if owner.is_null() || acl.is_null() {
        return Err(unsafe_entry());
    }
    let user = current_sid()?;
    let owner = sid_string(owner)?;
    // TrustedInstaller owns normal Windows volume roots. It is trusted only for ancestors,
    // never as the owner of an application-private root or a newly created object.
    let installer = "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464";
    if (!ancestor && owner != user) || (ancestor && !trusted(&owner, &user) && owner != installer) {
        return Err(unsafe_entry());
    }
    let mut control = 0u16;
    let mut revision = 0;
    if unsafe { GetSecurityDescriptorControl(sd, &mut control, &mut revision) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if control & 0x4 == 0 {
        return Err(unsafe_entry());
    } // SE_DACL_PRESENT; NULL already rejected.
    let header = unsafe { *acl };
    let length = usize::from(header.AclSize);
    if length < size_of::<ACL>() || header.AclRevision != 2 {
        return Err(unsafe_entry());
    }
    let bytes = unsafe { std::slice::from_raw_parts(acl.cast::<u8>(), length) };
    let mut offset = size_of::<ACL>();
    for _ in 0..header.AceCount {
        let h = bytes.get(offset..offset + 4).ok_or_else(unsafe_entry)?;
        let kind = h[0];
        let flags = h[1];
        let size = usize::from(u16::from_le_bytes([h[2], h[3]]));
        // Object/callback/conditional/unknown ACEs are not interpreted optimistically.
        if !matches!(kind, 0 | 1) || flags & !0x1f != 0 || size < 16 || size % 4 != 0 {
            return Err(unsafe_entry());
        }
        let ace = bytes.get(offset..offset + size).ok_or_else(unsafe_entry)?;
        let mask = u32::from_le_bytes(ace[4..8].try_into().map_err(|_| unsafe_entry())?);
        let sid = &ace[8..];
        if sid[0] != 1 || usize::from(sid[1]) * 4 + 8 > sid.len() {
            return Err(unsafe_entry());
        }
        let subject = sid_string(sid.as_ptr().cast_mut().cast())?;
        if kind == 0 && flags & 0x8 == 0 && !trusted(&subject, &user) {
            // TrustedInstaller's inherited ACEs are interpreted only as far as granting
            // this chain read/execute plus normal create-sibling; the exemption never
            // tolerates delete/replace of existing children, owner or DACL changes.
            let exempt = ancestor && subject == installer;
            // A volume root may allow creating unrelated children, not deleting this
            // chain, changing its owner/DACL, or deleting/replacing existing children.
            let dangerous = if ancestor { 0x500d_0040 } else { 0x500d_0156 };
            if mask & dangerous != 0 && !exempt {
                return Err(unsafe_entry());
            }
            if exempt && mask & dangerous & !0x5000_0040 != 0 {
                return Err(unsafe_entry());
            }
        }
        offset = offset.checked_add(size).ok_or_else(unsafe_entry)?;
    }
    Ok(())
}
pub(crate) fn verify_private(file: &File) -> io::Result<()> {
    verify_acl(file, false)
}
/// Test-only audit view: owner, group, DACL presence and its ACE count.
#[cfg(test)]
pub(crate) fn security_summary(file: &File) -> io::Result<(String, String, bool, usize)> {
    let mut owner = null_mut();
    let mut group = null_mut();
    let mut acl = null_mut::<ACL>();
    let mut sd = null_mut();
    let error = unsafe {
        GetSecurityInfo(
            raw(file),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            &mut group,
            &mut acl,
            null_mut(),
            &mut sd,
        )
    };
    if error != 0 {
        return Err(io::Error::from_raw_os_error(error as i32));
    }
    let _memory = LocalMemory(sd);
    let owner = sid_string(owner)?;
    let group = sid_string(group)?;
    Ok((
        owner,
        group,
        !acl.is_null(),
        acl.is_null()
            .then_some(0)
            .unwrap_or(unsafe { (*acl).AceCount } as usize),
    ))
}
#[cfg(test)]
pub(crate) fn current_sid_string() -> io::Result<String> {
    current_sid()
}
fn component(name: &str) -> io::Result<Vec<u16>> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.ends_with([' ', '.'])
        || name
            .chars()
            .any(|c| c.is_control() || "\\/:<>\"|?*".contains(c))
    {
        return Err(unsafe_entry());
    }
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || (stem.len() == 4
        && (stem.starts_with("COM") || stem.starts_with("LPT"))
        && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    {
        return Err(unsafe_entry());
    }
    let wide: Vec<u16> = name.encode_utf16().collect();
    if wide.len() > 255 {
        return Err(unsafe_entry());
    }
    Ok(wide)
}
fn open_at(
    parent: &File,
    name: &str,
    access: u32,
    disposition: u32,
    directory: bool,
    private: bool,
) -> io::Result<File> {
    let mut name = component(name)?;
    let length = u16::try_from(name.len() * 2).map_err(|_| unsafe_entry())?;
    let mut name = UnicodeString {
        length,
        maximum_length: length,
        buffer: name.as_mut_ptr(),
    };
    let security = if private { Some(descriptor()?) } else { None };
    // SAFETY note: the descriptor stays alive through the NtCreateFile call below.
    let attributes = ObjectAttributes {
        length: size_of::<ObjectAttributes>() as u32,
        root: raw(parent),
        name: &mut name,
        attributes: 0x40,
        security: security.as_ref().map_or(null_mut(), |s| s.0),
        qos: null_mut(),
    };
    let mut handle = null_mut();
    let mut status = IoStatus {
        status: 0,
        information: 0,
    };
    // SAFETY: only one validated component is passed relative to a held directory.
    // All buffers live through this synchronous call. Reparse opens never follow links;
    // successful handles are wrapped once and subsequently checked by handle.
    let result = unsafe {
        NtCreateFile(
            &mut handle,
            access,
            &attributes,
            &mut status,
            null(),
            0x80,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            disposition,
            FILE_OPEN_REPARSE_POINT
                | FILE_SYNCHRONOUS_IO_NONALERT
                | if directory {
                    FILE_DIRECTORY_FILE
                } else {
                    FILE_NON_DIRECTORY_FILE
                },
            null(),
            0,
        )
    };
    if result < 0 {
        return Err(status_error(result));
    }
    let file = File::from(unsafe { OwnedHandle::from_raw_handle(handle) });
    let state = file_state(&file)?;
    if state.directory != directory || (!directory && state.links != 1) {
        return Err(unsafe_entry());
    }
    Ok(file)
}
fn reopen(file: &File, access: u32) -> io::Result<File> {
    // ReOpenFile binds the existing object, never its former ambient pathname.
    let handle = unsafe {
        ReOpenFile(
            raw(file),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
        )
    };
    if handle as isize == -1 {
        return Err(io::Error::last_os_error());
    }
    let opened = File::from(unsafe { OwnedHandle::from_raw_handle(handle) });
    if identity(file)? != identity(&opened)? {
        return Err(unsafe_entry());
    }
    Ok(opened)
}
fn volume(file: &File, drive: u8) -> io::Result<()> {
    let root = [u16::from(drive), b':' as u16, b'\\' as u16, 0];
    if unsafe { GetDriveTypeW(root.as_ptr()) } != 3 {
        return Err(unsafe_entry());
    }
    let mut filesystem = [0u16; 32];
    if unsafe {
        GetVolumeInformationByHandleW(
            raw(file),
            null_mut(),
            0,
            null_mut(),
            null_mut(),
            null_mut(),
            filesystem.as_mut_ptr(),
            filesystem.len() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let n = filesystem
        .iter()
        .position(|c| *c == 0)
        .ok_or_else(unsafe_entry)?;
    if String::from_utf16_lossy(&filesystem[..n]) != "NTFS" {
        return Err(unsafe_entry());
    }
    Ok(())
}
fn path_components(path: &Path) -> io::Result<(u8, Vec<String>)> {
    let mut components = path.components();
    let drive = match components.next() {
        Some(Component::Prefix(p)) => match p.kind() {
            Prefix::Disk(d) | Prefix::VerbatimDisk(d) => d,
            _ => return Err(unsafe_entry()),
        },
        _ => return Err(unsafe_entry()),
    };
    if components.next() != Some(Component::RootDir) {
        return Err(unsafe_entry());
    }
    let mut names = Vec::new();
    for c in components {
        match c {
            Component::Normal(s) => {
                let s = s.to_str().ok_or_else(unsafe_entry)?;
                component(s)?;
                names.push(s.to_owned());
            }
            _ => return Err(unsafe_entry()),
        }
    }
    // components() normalizes interior '.': reject it lexically before authorization.
    let raw = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if raw
        .split(|c| *c == b'\\' as u16 || *c == b'/' as u16)
        .any(|s| s == [b'.' as u16] || s == [b'.' as u16, b'.' as u16])
    {
        return Err(unsafe_entry());
    }
    Ok((drive, names))
}
pub(crate) struct Directory {
    file: File,
    ancestors: Vec<Arc<File>>,
}
impl Directory {
    pub fn open(path: &Path, create: bool) -> io::Result<Self> {
        Self::walk(path, create, true)
    }
    fn walk(path: &Path, create: bool, private: bool) -> io::Result<Self> {
        let (drive, names) = path_components(path)?;
        let root = [
            b'\\' as u16,
            b'\\' as u16,
            b'?' as u16,
            b'\\' as u16,
            u16::from(drive),
            b':' as u16,
            b'\\' as u16,
            0,
        ];
        let handle = unsafe {
            CreateFileW(
                root.as_ptr(),
                if names.is_empty() && private {
                    WRITE_DIR
                } else {
                    READ_DIR
                },
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                null_mut(),
            )
        };
        if handle as isize == -1 {
            return Err(io::Error::last_os_error());
        }
        let mut file = File::from(unsafe { OwnedHandle::from_raw_handle(handle) });
        volume(&file, drive)?;
        if !file_state(&file)?.directory {
            return Err(unsafe_entry());
        }
        verify_acl(&file, true)?;
        let mut ancestors = Vec::new();
        for (index, name) in names.iter().enumerate() {
            let final_component = index + 1 == names.len();
            let access = if final_component && private {
                WRITE_DIR
            } else {
                READ_DIR
            };
            let next = match open_at(&file, name, access, FILE_OPEN, true, false) {
                Ok(f) => f,
                Err(e) if create && e.kind() == io::ErrorKind::NotFound => {
                    let writable = reopen(&file, WRITE_DIR)?;
                    let created = open_at(&writable, name, WRITE_DIR, FILE_OPEN_IF, true, true)?;
                    verify_private(&created)?;
                    created
                }
                Err(e) => return Err(e),
            };
            verify_acl(&next, !final_component || !private)?;
            ancestors.push(Arc::new(file));
            file = next;
        }
        if private {
            verify_private(&file)?;
        }
        Ok(Self { file, ancestors })
    }
    pub fn file(&self) -> &File {
        &self.file
    }
    pub fn child(&self, name: &str, create: bool) -> io::Result<Self> {
        let file = open_at(
            &self.file,
            name,
            WRITE_DIR,
            if create { FILE_OPEN_IF } else { FILE_OPEN },
            true,
            create,
        )?;
        verify_private(&file)?;
        let mut ancestors = self.ancestors.clone();
        ancestors.push(Arc::new(self.file.try_clone()?));
        Ok(Self { file, ancestors })
    }
    pub fn open_file(&self, name: &str, create: bool) -> io::Result<File> {
        let file = open_at(
            &self.file,
            name,
            if create { WRITE_FILE } else { READ_FILE },
            if create { FILE_CREATE } else { FILE_OPEN },
            false,
            create,
        )?;
        verify_private(&file)?;
        Ok(file)
    }
    pub fn open_mutable(&self, name: &str) -> io::Result<File> {
        let file = open_at(
            &self.file,
            name,
            READ_FILE | DELETE,
            FILE_OPEN,
            false,
            false,
        )?;
        verify_private(&file)?;
        Ok(file)
    }
    pub fn exists(&self, name: &str) -> io::Result<bool> {
        match open_at(&self.file, name, READ_FILE, FILE_OPEN, false, false) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }
    pub fn sync(&self) -> io::Result<()> {
        // Windows does not support directory FlushFileBuffers. Content is flushed by
        // file handles before atomic visible publication; this is not Unix fsync durability.
        Ok(())
    }
    pub fn rename(&self, _from: &str, to: &str, file: &File) -> io::Result<()> {
        rename_exclusive(file, &self.file, to)
    }
    pub fn remove_file(&self, _name: &str, file: &File) -> io::Result<()> {
        let state = file_state(file)?;
        if state.directory || state.links != 1 {
            return Err(unsafe_entry());
        }
        let info = FILE_DISPOSITION_INFO { DeleteFile: 1 };
        if unsafe {
            SetFileInformationByHandle(
                raw(file),
                FileDispositionInfo,
                (&info as *const FILE_DISPOSITION_INFO).cast(),
                size_of::<FILE_DISPOSITION_INFO>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
pub(crate) fn rename_exclusive(file: &File, parent: &File, name: &str) -> io::Result<()> {
    let name = component(name)?;
    let bytes = u32::try_from(name.len() * 2).map_err(|_| unsafe_entry())?;
    let length = (offset_of!(RenameInfo, name) + bytes as usize).max(size_of::<RenameInfo>());
    let mut storage = vec![0usize; length.div_ceil(size_of::<usize>())];
    let info = storage.as_mut_ptr().cast::<RenameInfo>();
    let mut status = IoStatus {
        status: 0,
        information: 0,
    };
    // SAFETY: NT FILE_RENAME_INFORMATION has BOOLEAN (not the Win32 DWORD union),
    // pointer-aligned RootDirectory, byte-counted UTF-16 tail, and no NUL in its length.
    // Only a held parent plus one validated component is accepted. ReplaceIfExists=false.
    let result = unsafe {
        (*info).replace = 0;
        (*info).root = raw(parent);
        (*info).length = bytes;
        std::ptr::copy_nonoverlapping(name.as_ptr(), (*info).name.as_mut_ptr(), name.len());
        NtSetInformationFile(raw(file), &mut status, info.cast(), length as u32, 10)
    };
    if result < 0 {
        Err(status_error(result))
    } else {
        Ok(())
    }
}
pub(crate) struct LibraryLock {
    _root: Directory,
    _file: File,
}
pub(crate) fn lock_library(path: &Path) -> Result<LibraryLock, crate::error::AppError> {
    let root = Directory::open(path.parent().ok_or_else(unsafe_entry)?, false)?;
    let name = path
        .with_extension("media-lock")
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(unsafe_entry)?
        .to_owned();
    let file = open_at(
        root.file(),
        &name,
        WRITE_FILE & !DELETE,
        FILE_OPEN_IF,
        false,
        true,
    )?;
    verify_private(&file)?;
    let mut overlapped = unsafe { std::mem::zeroed::<OVERLAPPED>() };
    // SAFETY: live synchronous file, zero offset, fixed one-byte range, no wait.
    if unsafe {
        LockFileEx(
            raw(&file),
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            0,
            1,
            0,
            &mut overlapped,
        )
    } == 0
    {
        let e = io::Error::last_os_error();
        return Err(if e.raw_os_error() == Some(33) {
            crate::error::AppError::MediaBusy
        } else {
            e.into()
        });
    }
    Ok(LibraryLock {
        _root: root,
        _file: file,
    })
}
pub(crate) fn open_source(path: &Path) -> io::Result<(File, Option<Directory>)> {
    let parent = path.parent().ok_or_else(unsafe_entry)?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(unsafe_entry)?;
    let directory = Directory::walk(parent, false, false)?;
    // Source ACL need not be application-private, but its ancestors and object type
    // cannot redirect the read; single-link and metadata checks are enforced on handle.
    let file = open_at(&directory.file, name, READ_FILE, FILE_OPEN, false, false)?;
    Ok((file, Some(directory)))
}
