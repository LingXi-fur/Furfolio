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
        GetLengthSid, GetSecurityDescriptorControl, GetTokenInformation, InitializeAcl,
        InitializeSecurityDescriptor, IsValidAcl, IsValidSecurityDescriptor, IsValidSid,
        SetSecurityDescriptorControl, SetSecurityDescriptorDacl, SetSecurityDescriptorGroup,
        SetSecurityDescriptorOwner, TokenUser, ACL, DACL_SECURITY_INFORMATION,
        OWNER_SECURITY_INFORMATION, SECURITY_DESCRIPTOR, SE_DACL_PRESENT, SE_DACL_PROTECTED,
        TOKEN_QUERY, TOKEN_USER,
    },
    Storage::FileSystem::{
        CreateFileW, FileBasicInfo, FileDispositionInfo, FileIdInfo, FileStandardInfo,
        GetDiskFreeSpaceExW, GetDriveTypeW, GetFileInformationByHandleEx,
        GetVolumeInformationByHandleW, LockFileEx, SetFileInformationByHandle, FILE_BASIC_INFO,
        FILE_DISPOSITION_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_ID_INFO, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_STANDARD_INFO,
        LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, OPEN_EXISTING,
    },
    System::{
        Memory::{LocalAlloc, LMEM_FIXED, LMEM_ZEROINIT},
        Threading::{GetCurrentProcess, OpenProcessToken},
        IO::OVERLAPPED,
    },
};

#[cfg(test)]
use windows_sys::Win32::Security::{
    GetAce, ACCESS_ALLOWED_ACE, ACE_HEADER, GROUP_SECURITY_INFORMATION,
};

const READ_CONTROL: u32 = 0x0002_0000;
const DELETE: u32 = 0x0001_0000;
const FILE_SHARE_DELETE: u32 = 0x0000_0004;
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
    identity_raw(raw(file))
}
fn identity_raw(handle: HANDLE) -> io::Result<Identity> {
    let mut value = MaybeUninit::<FILE_ID_INFO>::uninit();
    // SAFETY: callers pair FileIdInfo with its documented fixed-size struct; the
    // handle is borrowed live and storage is aligned, writable and exactly sized.
    if unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            value.as_mut_ptr().cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let info = unsafe { value.assume_init() };
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
    fn zeroed(length: usize) -> io::Result<Self> {
        // SAFETY: allocation size is bounded by the token/ACL callers; LocalFree owns it.
        let handle = unsafe { LocalAlloc(LMEM_FIXED | LMEM_ZEROINIT, length) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(handle))
    }
    fn from_bytes(bytes: Vec<u8>) -> io::Result<Self> {
        let memory = Self::zeroed(bytes.len())?;
        // SAFETY: allocation owns bytes.len() writable bytes, distinct from the source.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), memory.0.cast(), bytes.len()) };
        Ok(memory)
    }
}
fn sid_string(sid: *mut c_void) -> io::Result<String> {
    // SAFETY: callers supply bounded SID storage from an OS query or owned allocation.
    if sid.is_null() || unsafe { IsValidSid(sid) } == 0 {
        return Err(unsafe_entry());
    }
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
fn current_sid_bytes() -> io::Result<Vec<u8>> {
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
    let capacity = length;
    let mut buffer = vec![0usize; (capacity as usize).div_ceil(size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle() as HANDLE,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            capacity,
            &mut length,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if length > capacity || (length as usize) < size_of::<TOKEN_USER>() {
        return Err(unsafe_entry());
    }
    // SAFETY: the aligned buffer contains a complete TOKEN_USER from the OS.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let start = buffer.as_ptr() as usize;
    let offset = (user.User.Sid as usize)
        .checked_sub(start)
        .ok_or_else(unsafe_entry)?;
    // Bound the SID header and subauthorities before passing it to Win32 validation.
    let bytes =
        unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), length as usize) };
    let header = bytes.get(offset..).ok_or_else(unsafe_entry)?;
    if header.len() < 8 || header[0] != 1 || header[1] > 15 {
        return Err(unsafe_entry());
    }
    let sid_length = 8 + usize::from(header[1]) * 4;
    let sid = header.get(..sid_length).ok_or_else(unsafe_entry)?;
    // SAFETY: the full SID is inside the still-live, aligned token query buffer.
    if unsafe { IsValidSid(user.User.Sid) } == 0
        || unsafe { GetLengthSid(user.User.Sid) } as usize != sid_length
    {
        return Err(unsafe_entry());
    }
    Ok(sid.to_vec())
}
fn current_sid_string_impl() -> io::Result<String> {
    let sid = LocalMemory::from_bytes(current_sid_bytes()?)?;
    sid_string(sid.0)
}
fn constant_sid(subauthorities: &[u32]) -> io::Result<LocalMemory> {
    if subauthorities.len() > 15 {
        return Err(unsafe_entry());
    }
    // Revision 1, six-byte SECURITY_NT_AUTHORITY (5), little-endian subauthorities.
    let mut bytes = vec![0u8; 8 + 4 * subauthorities.len()];
    bytes[0] = 1;
    bytes[1] = subauthorities.len() as u8;
    bytes[2..8].copy_from_slice(&[0, 0, 0, 0, 0, 5]);
    for (index, value) in subauthorities.iter().enumerate() {
        bytes[8 + 4 * index..12 + 4 * index].copy_from_slice(&value.to_le_bytes());
    }
    let sid = LocalMemory::from_bytes(bytes)?;
    // SAFETY: the allocation contains the entire SID assembled above.
    if unsafe { IsValidSid(sid.0) } == 0 {
        return Err(unsafe_entry());
    }
    Ok(sid)
}
/// Absolute descriptor and every allocation referenced by it stay alive through creation.
/// AddAccessAllowedAceEx copies SYSTEM/Administrators SIDs into the owned ACL.
struct PrivateSecurity {
    descriptor: Box<SECURITY_DESCRIPTOR>,
    _user: LocalMemory,
    _acl: LocalMemory,
}
impl PrivateSecurity {
    fn as_ptr(&self) -> *mut c_void {
        (&*self.descriptor as *const SECURITY_DESCRIPTOR)
            .cast_mut()
            .cast()
    }
}
fn descriptor() -> io::Result<PrivateSecurity> {
    const ALL: u32 = 0x001f_01ff;
    const ACL_SPACE: usize = size_of::<ACL>() + 3 * (8 + 68);
    let user = LocalMemory::from_bytes(current_sid_bytes()?)?;
    let system = constant_sid(&[18])?;
    let administrators = constant_sid(&[32, 544])?;
    let acl_memory = LocalMemory::zeroed(ACL_SPACE)?;
    let acl = acl_memory.0.cast::<ACL>();
    let mut descriptor = Box::new(SECURITY_DESCRIPTOR {
        Revision: 0,
        Sbz1: 0,
        Control: 0,
        Owner: null_mut(),
        Group: null_mut(),
        Sacl: null_mut(),
        Dacl: null_mut(),
    });
    let sd = (&mut *descriptor as *mut SECURITY_DESCRIPTOR).cast();
    // SAFETY: SDK-defined absolute SD and aligned ACL allocations have their actual
    // sizes. Owner/group SID and ACL are retained in PrivateSecurity, not packed or
    // freed while NtCreateFile uses them. No self-relative offsets are fabricated.
    unsafe {
        if InitializeSecurityDescriptor(sd, 1) == 0 || InitializeAcl(acl, ACL_SPACE as u32, 2) == 0
        {
            return Err(io::Error::last_os_error());
        }
        for sid in [&user, &system, &administrators] {
            if AddAccessAllowedAceEx(acl, 2, 0, ALL, sid.0) == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        if SetSecurityDescriptorOwner(sd, user.0, 0) == 0
            || SetSecurityDescriptorGroup(sd, user.0, 0) == 0
            || SetSecurityDescriptorDacl(sd, 1, acl, 0) == 0
            || SetSecurityDescriptorControl(sd, SE_DACL_PROTECTED, SE_DACL_PROTECTED) == 0
        {
            return Err(io::Error::last_os_error());
        }
        if IsValidAcl(acl) == 0 || IsValidSecurityDescriptor(sd) == 0 {
            return Err(unsafe_entry());
        }
    }
    Ok(PrivateSecurity {
        descriptor,
        _user: user,
        _acl: acl_memory,
    })
}
fn trusted(sid: &str, user: &str) -> bool {
    sid == user || sid == "S-1-5-18" || sid == "S-1-5-32-544"
}
/// ACL strictness levels shared by the walker and the source-database check.
enum Level {
    /// Application-private object: owner is the user and the ACE set carries no
    /// untrusted write-surface grants at all.
    Private,
    /// User-chosen ancestor chain: untrusted subjects may exist but must not
    /// delete or replace the chain; TrustedInstaller is tolerated as owner.
    Ancestor,
    /// A source database file we did not create (SQLite's own default DACL):
    /// owner must be the user and untrusted subjects must hold no write surface.
    /// This mirrors the Unix uid==euid && group/other-not-writable rule.
    OwnedSource,
}
fn verify_acl(file: &File, ancestor: bool) -> io::Result<()> {
    verify_acl_level(
        file,
        if ancestor {
            Level::Ancestor
        } else {
            Level::Private
        },
    )
}
fn verify_acl_level(file: &File, level: Level) -> io::Result<()> {
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
    let user = current_sid_string_impl()?;
    let owner = sid_string(owner)?;
    // TrustedInstaller owns normal Windows volume roots. It is trusted only for ancestors,
    // never as the owner of an application-private root or a newly created object.
    let installer = "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464";
    match level {
        Level::Private | Level::OwnedSource => {
            if owner != user {
                return Err(unsafe_entry());
            }
        }
        Level::Ancestor => {
            if !trusted(&owner, &user) && owner != installer {
                return Err(unsafe_entry());
            }
        }
    }
    let mut control = 0u16;
    let mut revision = 0;
    if unsafe { GetSecurityDescriptorControl(sd, &mut control, &mut revision) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if control & SE_DACL_PRESENT == 0 {
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
            // TrustedInstaller is an ancestor-only trusted system-service principal.
            // Its generic write/all and delete-child grants are accepted here; explicit
            // DELETE/WRITE_DAC/WRITE_OWNER grants still fail this ancestor check.
            // Final private objects never gain this additional trusted principal.
            let exempt = matches!(level, Level::Ancestor) && subject == installer;
            // Untrusted subjects must never hold a write surface over this object:
            // write data/attributes/EA, delete, or change owner/DACL. The ancestor
            // level additionally tolerates nothing stronger, keeping the chain
            // un-replaceable; the owned-source level matches the Unix "group and
            // other must not be writable" rule for a file we did not create.
            let dangerous = match level {
                Level::Ancestor => 0x500d_0040,
                Level::Private => 0x500d_0156,
                Level::OwnedSource => 0x000d_0116,
            };
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
/// Ancestor trust rules for a user-chosen folder that is not application-private.
pub(crate) fn verify_ancestor(file: &File) -> io::Result<()> {
    verify_acl(file, true)
}
/// A source-database file is private enough when the current user owns it and
/// no untrusted subject holds any write surface over it. SQLite creates this
/// file with the token default DACL, not our private descriptor, so the strict
/// three-ACE private standard does not apply to it.
pub(crate) fn is_private(file: &File) -> io::Result<bool> {
    Ok(verify_acl_level(file, Level::OwnedSource).is_ok())
}
/// Verify SQLite's win32 VFS uses the exact OS handle identity captured earlier.
pub(crate) fn verify_vfs_connection(
    file: &File,
    connection: &rusqlite::Connection,
) -> Result<(), crate::error::AppError> {
    verify_sqlite_vfs_handle(file, connection)
}
/// Test-only audit of the security descriptor returned by the filesystem.
#[cfg(test)]
pub(crate) struct SecuritySummary {
    pub owner: String,
    pub group: String,
    pub dacl_present: bool,
    pub dacl_protected: bool,
    pub aces: Vec<(u8, u8, u32, String)>,
}
#[cfg(test)]
pub(crate) fn security_summary(file: &File) -> io::Result<SecuritySummary> {
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
    if sd.is_null() || owner.is_null() || group.is_null() || acl.is_null() {
        return Err(unsafe_entry());
    }
    // SAFETY: GetSecurityInfo owns a complete descriptor and its non-NULL ACL.
    if unsafe { IsValidSecurityDescriptor(sd) } == 0 || unsafe { IsValidAcl(acl) } == 0 {
        return Err(unsafe_entry());
    }
    let mut control = 0;
    let mut revision = 0;
    if unsafe { GetSecurityDescriptorControl(sd, &mut control, &mut revision) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut aces = Vec::new();
    for index in 0..u32::from(unsafe { (*acl).AceCount }) {
        let mut ace = null_mut();
        // SAFETY: index stays inside the validated ACL; pointer stays inside sd.
        if unsafe { GetAce(acl, index, &mut ace) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let header = unsafe { &*ace.cast::<ACE_HEADER>() };
        if header.AceType != 0 || usize::from(header.AceSize) < size_of::<ACCESS_ALLOWED_ACE>() {
            return Err(unsafe_entry());
        }
        let allowed = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
        let sid = std::ptr::addr_of!(allowed.SidStart).cast_mut().cast();
        aces.push((
            header.AceType,
            header.AceFlags,
            allowed.Mask,
            sid_string(sid)?,
        ));
    }
    Ok(SecuritySummary {
        owner: sid_string(owner)?,
        group: sid_string(group)?,
        dacl_present: control & SE_DACL_PRESENT != 0,
        dacl_protected: control & SE_DACL_PROTECTED != 0,
        aces,
    })
}
#[cfg(test)]
pub(crate) fn current_sid_string() -> io::Result<String> {
    current_sid_string_impl()
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
        security: security
            .as_ref()
            .map_or(null_mut(), PrivateSecurity::as_ptr),
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
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
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
                    // The runner diagnostics proved the held READ_DIR ancestor handle
                    // can create the final component directly (nt_create_private: ok):
                    // READ_DIR already carries READ_CONTROL plus FILE_ADD_FILE and
                    // FILE_ADD_SUBDIRECTORY, the only rights NtCreateFile needs here.
                    // ReOpenFile itself denied this exact call on the runner, so it is
                    // gone. The created object gets WRITE_DIR rights through its own
                    // private descriptor below.
                    let created = open_at(&file, name, WRITE_DIR, FILE_OPEN_IF, true, true)?;
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
    // The lock file is created private and verified below; the existing parent chain
    // (for example the OS temporary directory that contains a legacy test library) is
    // only traversed, so it needs the ancestor rules, not the private-object rules.
    let root = Directory::walk(path.parent().ok_or_else(unsafe_entry)?, false, false)?;
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

/// Open one existing directory by validated component walk without private rules.
pub(crate) fn open_directory(path: &Path) -> io::Result<Directory> {
    Directory::walk(path, false, false)
}

/// Validated drive letter and component names of an absolute local NTFS path.
pub(crate) fn path_parts(path: &Path) -> io::Result<(u8, Vec<String>)> {
    path_components(path)
}

/// Open the validated drive root of the destination volume for identity checks.
pub(crate) fn drive_root(drive: u8) -> io::Result<Directory> {
    Directory::from_drive(drive)
}
impl Directory {
    fn from_drive(drive: u8) -> io::Result<Self> {
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
                READ_DIR,
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
        let file = File::from(unsafe { OwnedHandle::from_raw_handle(handle) });
        volume(&file, drive)?;
        if !file_state(&file)?.directory {
            return Err(unsafe_entry());
        }
        verify_acl(&file, true)?;
        Ok(Self {
            file,
            ancestors: Vec::new(),
        })
    }
}

/// Open one existing child directory under held ancestor rules.
pub(crate) fn open_child_directory(parent: &Directory, name: &str) -> io::Result<Directory> {
    let file = open_at(parent.file(), name, READ_DIR, FILE_OPEN, true, false)?;
    verify_acl(&file, true)?;
    let mut ancestors = parent.ancestors.clone();
    ancestors.push(Arc::new(parent.file.try_clone()?));
    Ok(Directory { file, ancestors })
}

/// Open one existing child directory with FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY,
/// the access FileRenameInformation requires of the destination parent handle.
pub(crate) fn open_writable_directory(parent: &Directory, name: &str) -> io::Result<Directory> {
    let file = open_at(parent.file(), name, WRITE_DIR, FILE_OPEN, true, false)?;
    verify_acl(&file, true)?;
    let mut ancestors = parent.ancestors.clone();
    ancestors.push(Arc::new(parent.file.try_clone()?));
    Ok(Directory { file, ancestors })
}

/// Open one existing child directory with DELETE, which FileDispositionInfo
/// needs on the very handle that marks the directory for deletion.
pub(crate) fn open_deletable_directory(parent: &Directory, name: &str) -> io::Result<Directory> {
    let file = open_at(
        parent.file(),
        name,
        READ_DIR | DELETE,
        FILE_OPEN,
        true,
        false,
    )?;
    verify_acl(&file, true)?;
    let mut ancestors = parent.ancestors.clone();
    ancestors.push(Arc::new(parent.file.try_clone()?));
    Ok(Directory { file, ancestors })
}

/// Create one new private child directory whose held handle can rename or delete it.
/// DELETE rides on the creating handle: reopening by name would collide with the
/// still-open creator handle's share mode, and the identity must be the one verified.
pub(crate) fn create_private_directory(parent: &Directory, name: &str) -> io::Result<Directory> {
    let file = open_at(
        parent.file(),
        name,
        WRITE_DIR | DELETE,
        FILE_CREATE,
        true,
        true,
    )?;
    verify_private(&file)?;
    let mut ancestors = parent.ancestors.clone();
    ancestors.push(Arc::new(parent.file.try_clone()?));
    Ok(Directory { file, ancestors })
}

/// Mark this verified, held directory for deletion at last handle close.
pub(crate) fn mark_directory_deleted(directory: &Directory) -> io::Result<()> {
    mark_deleted(directory.file(), true)
}

/// Mark the verified object behind this handle for deletion at last close.
pub(crate) fn mark_deleted(file: &File, directory: bool) -> io::Result<()> {
    let state = file_state(file)?;
    if state.directory != directory || (!directory && state.links != 1) {
        return Err(unsafe_entry());
    }
    let info = FILE_DISPOSITION_INFO { DeleteFile: 1 };
    // SAFETY: fixed information class with its documented struct; handle stays live.
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

/// Available bytes on the fixed NTFS volume of this drive letter.
pub(crate) fn free_space(drive: u8) -> io::Result<u64> {
    let root = [u16::from(drive), b':' as u16, b'\\' as u16, 0];
    let mut available = 0u64;
    // SAFETY: root is NUL-terminated; outputs are one aligned u64 each.
    if unsafe {
        GetDiskFreeSpaceExW(
            root.as_ptr(),
            (&mut available as *mut u64).cast(),
            null_mut(),
            null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(available)
}

/// Identity of the actual OS handle SQLite's win32 VFS uses for this connection.
pub(crate) fn verify_sqlite_vfs_handle(
    file: &File,
    connection: &rusqlite::Connection,
) -> Result<(), crate::error::AppError> {
    let expected = identity(file).map_err(unsafe_entry_conv)?;
    let mut handle: HANDLE = null_mut();
    // SAFETY: connection owns a live SQLite handle, main is static, the win32
    // VFS writes one borrowed HANDLE and closes nothing. Never owned here.
    let result = unsafe {
        rusqlite::ffi::sqlite3_file_control(
            connection.handle(),
            c"main".as_ptr(),
            rusqlite::ffi::SQLITE_FCNTL_WIN32_GET_HANDLE,
            (&mut handle as *mut HANDLE).cast(),
        )
    };
    if result != rusqlite::ffi::SQLITE_OK || handle.is_null() {
        return Err(unsafe_entry().into());
    }
    let actual = identity_raw(handle).map_err(|_| unsafe_entry())?;
    if actual != expected {
        return Err(unsafe_entry().into());
    }
    Ok(())
}
fn unsafe_entry_conv(_: io::Error) -> crate::error::AppError {
    unsafe_entry().into()
}
#[cfg(test)]
mod security_tests {
    use super::*;
    use windows_sys::Win32::Security::{
        Authorization::SetNamedSecurityInfoW, GetSecurityDescriptorDacl,
        GetSecurityDescriptorGroup, GetSecurityDescriptorOwner, CONTAINER_INHERIT_ACE,
        INHERIT_ONLY_ACE, OBJECT_INHERIT_ACE, PROTECTED_DACL_SECURITY_INFORMATION,
    };

    fn assert_private_aces(aces: &[(u8, u8, u32, String)], user: &str) {
        assert_eq!(aces.len(), 3);
        let mut subjects = Vec::new();
        for (kind, flags, mask, subject) in aces {
            assert_eq!(*kind, 0, "only explicit ALLOW ACEs are expected");
            assert_eq!(*flags, 0, "no inherited or inheritable ACEs");
            assert_eq!(*mask, 0x001f_01ff);
            subjects.push(subject.as_str());
        }
        subjects.sort();
        let mut expected = vec![user, "S-1-5-18", "S-1-5-32-544"];
        expected.sort();
        assert_eq!(subjects, expected);
    }

    #[test]
    fn windows_binary_sids_validate_without_creating_files() {
        let bytes = current_sid_bytes().expect("query current token SID without filesystem IO");
        let user = LocalMemory::from_bytes(bytes.clone()).unwrap();
        assert_eq!(bytes[0], 1);
        assert_eq!(bytes.len(), 8 + usize::from(bytes[1]) * 4);
        assert_ne!(unsafe { IsValidSid(user.0) }, 0);
        assert_eq!(unsafe { GetLengthSid(user.0) } as usize, bytes.len());
        assert_eq!(
            sid_string(user.0).unwrap(),
            current_sid_string_impl().unwrap()
        );
        for (subauthorities, expected) in
            [(&[18][..], "S-1-5-18"), (&[32, 544][..], "S-1-5-32-544")]
        {
            let sid = constant_sid(subauthorities).unwrap();
            assert_ne!(unsafe { IsValidSid(sid.0) }, 0);
            let length = unsafe { GetLengthSid(sid.0) } as usize;
            assert_eq!(length, 8 + 4 * subauthorities.len());
            let bytes = unsafe { std::slice::from_raw_parts(sid.0.cast::<u8>(), length) };
            assert_eq!(bytes[0], 1);
            assert_eq!(usize::from(bytes[1]), subauthorities.len());
            assert_eq!(&bytes[2..8], &[0, 0, 0, 0, 0, 5]);
            for (index, value) in subauthorities.iter().enumerate() {
                assert_eq!(&bytes[8 + 4 * index..12 + 4 * index], &value.to_le_bytes());
            }
            assert_eq!(sid_string(sid.0).unwrap(), expected);
        }
    }

    #[test]
    fn windows_absolute_private_descriptor_validates_without_creating_files() {
        // Move the wrapper: all referenced allocations must remain stable and live.
        let security = Box::new(descriptor().expect("construct pure in-memory private SD"));
        let sd = security.as_ptr();
        assert_ne!(unsafe { IsValidSecurityDescriptor(sd) }, 0);
        let mut owner = null_mut();
        let mut group = null_mut();
        let mut defaulted = 1;
        assert_ne!(
            unsafe { GetSecurityDescriptorOwner(sd, &mut owner, &mut defaulted) },
            0
        );
        assert_eq!(defaulted, 0);
        assert_ne!(
            unsafe { GetSecurityDescriptorGroup(sd, &mut group, &mut defaulted) },
            0
        );
        assert_eq!(defaulted, 0);
        let user = current_sid_string_impl().unwrap();
        assert_eq!(sid_string(owner).unwrap(), user);
        assert_eq!(sid_string(group).unwrap(), user);
        let mut control = 0;
        let mut revision = 0;
        assert_ne!(
            unsafe { GetSecurityDescriptorControl(sd, &mut control, &mut revision) },
            0
        );
        assert_eq!(revision, 1);
        assert_ne!(control & SE_DACL_PRESENT, 0);
        assert_ne!(control & SE_DACL_PROTECTED, 0);
        let mut present = 0;
        let mut acl = null_mut();
        assert_ne!(
            unsafe { GetSecurityDescriptorDacl(sd, &mut present, &mut acl, &mut defaulted) },
            0
        );
        assert_ne!(present, 0);
        assert_eq!(defaulted, 0);
        assert!(!acl.is_null());
        assert_ne!(unsafe { IsValidAcl(acl) }, 0);
        let mut aces = Vec::new();
        for index in 0..u32::from(unsafe { (*acl).AceCount }) {
            let mut ace = null_mut();
            assert_ne!(unsafe { GetAce(acl, index, &mut ace) }, 0);
            let allowed = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
            let sid = std::ptr::addr_of!(allowed.SidStart).cast_mut().cast();
            aces.push((
                allowed.Header.AceType,
                allowed.Header.AceFlags,
                allowed.Mask,
                sid_string(sid).unwrap(),
            ));
        }
        assert_private_aces(&aces, &user);
    }

    struct SyntheticParent(std::path::PathBuf);
    impl Drop for SyntheticParent {
        fn drop(&mut self) {
            // Exact synthetic objects only; never recurse into unknown children.
            for name in ["child/nested.bin", "direct.bin"] {
                if let Err(error) = std::fs::remove_file(self.0.join(name)) {
                    if error.kind() != io::ErrorKind::NotFound && !std::thread::panicking() {
                        panic!("remove known synthetic file: {error}");
                    }
                }
            }
            for path in [self.0.join("child"), self.0.clone()] {
                if let Err(error) = std::fs::remove_dir(path) {
                    if error.kind() != io::ErrorKind::NotFound && !std::thread::panicking() {
                        panic!("remove known synthetic directory: {error}");
                    }
                }
            }
        }
    }

    #[test]
    fn windows_protected_creation_blocks_dangerous_inherit_only_parent_ace() {
        let root = SyntheticParent(
            std::env::temp_dir().join(format!("furfolio-inherit-{}", uuid::Uuid::new_v4())),
        );
        let parent = Directory::open(&root.0, true).expect("create private synthetic parent");
        let user = LocalMemory::from_bytes(current_sid_bytes().unwrap()).unwrap();
        let system = constant_sid(&[18]).unwrap();
        let admin = constant_sid(&[32, 544]).unwrap();
        let authenticated_users = constant_sid(&[11]).unwrap();
        let memory = LocalMemory::zeroed(size_of::<ACL>() + 4 * (8 + 68)).unwrap();
        let acl = memory.0.cast::<ACL>();
        unsafe {
            assert_ne!(
                InitializeAcl(acl, (size_of::<ACL>() + 4 * (8 + 68)) as u32, 2),
                0
            );
            for sid in [&user, &system, &admin] {
                assert_ne!(AddAccessAllowedAceEx(acl, 2, 0, 0x001f_01ff, sid.0), 0);
            }
            assert_ne!(
                AddAccessAllowedAceEx(
                    acl,
                    2,
                    OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE | INHERIT_ONLY_ACE,
                    0x001f_01ff,
                    authenticated_users.0
                ),
                0
            );
        }
        // Only this newly created synthetic parent's DACL is changed, never an ancestor.
        // The runner denied every ReOpenFile call, and NtCreateFile rejects a nameless
        // relative open, so set the DACL by path: SetNamedSecurityInfoW opens the
        // WRITE_DAC handle itself, and this path is a synthetic directory this test
        // created inside the OS temporary directory.
        let mut path: Vec<u16> = root.0.as_os_str().encode_wide().collect();
        path.push(0);
        let result = unsafe {
            SetNamedSecurityInfoW(
                path.as_mut_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                acl,
                null(),
            )
        };
        assert_eq!(
            result, 0,
            "install dangerous inherit-only ACE on synthetic parent"
        );
        let audit = security_summary(&parent.file).unwrap();
        assert_eq!(audit.aces.len(), 4);
        assert!(audit
            .aces
            .iter()
            .any(|(_, flags, mask, subject)| *flags == 0x0b
                && *mask == 0x001f_01ff
                && subject == "S-1-5-11"));
        verify_private(&parent.file)
            .expect("inherit-only ACE does not grant access to parent itself");
        let child = parent
            .child("child", true)
            .expect("protected child directory creation must succeed");
        let direct = parent
            .open_file("direct.bin", true)
            .expect("protected direct file creation must succeed");
        let nested = child
            .open_file("nested.bin", true)
            .expect("protected nested file creation must succeed");
        let user = current_sid_string_impl().unwrap();
        for file in [&child.file, &direct, &nested] {
            let audit = security_summary(file).unwrap();
            assert_eq!(audit.owner, user);
            assert_eq!(audit.group, user);
            assert!(audit.dacl_present && audit.dacl_protected);
            assert_private_aces(&audit.aces, &user);
        }
        // All held handles close before the exact-object cleanup guard.
    }
}
