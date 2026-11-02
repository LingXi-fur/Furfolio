//! Narrow Win32 ABI spikes, deliberately not yet wired into production media IO.
use std::{
    fs::File,
    io,
    mem::{offset_of, size_of, MaybeUninit},
    os::windows::io::AsRawHandle,
};
use windows_sys::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{
        FileIdInfo, FileRenameInfo, GetFileInformationByHandleEx, SetFileInformationByHandle,
        FILE_ID_INFO, FILE_RENAME_INFO,
    },
};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Identity {
    volume: u64,
    file: [u8; 16],
}

pub(crate) fn identity(file: &File) -> io::Result<Identity> {
    let mut info = MaybeUninit::<FILE_ID_INFO>::uninit();
    // SAFETY: file owns a live handle and info points to correctly sized writable storage.
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle() as HANDLE,
            FileIdInfo,
            info.as_mut_ptr().cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the successful API call initialized the complete FILE_ID_INFO structure.
    let info = unsafe { info.assume_init() };
    Ok(Identity {
        volume: info.VolumeSerialNumber,
        file: info.FileId.Identifier,
    })
}

/// Rename an already-open entry into a held parent, never replacing an existing object.
/// The runner verifies RootDirectory semantics before this is used by managed IO.
pub(crate) fn rename_exclusive(file: &File, parent: &File, name: &str) -> io::Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.ends_with([' ', '.'])
        || name
            .chars()
            .any(|c| c.is_control() || "\\/:<>\"|?*".contains(c))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid component",
        ));
    }
    let name: Vec<u16> = name.encode_utf16().collect();
    let bytes = name
        .len()
        .checked_mul(size_of::<u16>())
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| io::Error::other("Rename component too long"))?;
    let length = offset_of!(FILE_RENAME_INFO, FileName)
        .checked_add(bytes as usize)
        .ok_or_else(|| io::Error::other("Rename buffer too long"))?;
    let length = length.max(size_of::<FILE_RENAME_INFO>());
    let api_length =
        u32::try_from(length).map_err(|_| io::Error::other("Rename buffer too long"))?;
    // usize storage gives FILE_RENAME_INFO its required pointer alignment on Windows x64.
    let mut storage = vec![0usize; length.div_ceil(size_of::<usize>())];
    let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    // SAFETY: aligned zeroed storage covers both the fixed header and the UTF-16 tail;
    // byte and API lengths were checked above, handles stay borrowed throughout the call.
    let result = unsafe {
        (*info).Anonymous.ReplaceIfExists = 0;
        (*info).RootDirectory = parent.as_raw_handle() as HANDLE;
        (*info).FileNameLength = bytes;
        std::ptr::copy_nonoverlapping(name.as_ptr(), (*info).FileName.as_mut_ptr(), name.len());
        SetFileInformationByHandle(
            file.as_raw_handle() as HANDLE,
            FileRenameInfo,
            info.cast(),
            api_length,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
