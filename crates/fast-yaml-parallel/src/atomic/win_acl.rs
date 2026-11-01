//! Windows DACL copy for [`AtomicFile`](super::AtomicFile): the only `unsafe` in this crate.
//!
//! The replacement file is created next to the target, so it already carries the DACL entries
//! inherited from the directory. [`copy_dacl`] gives it the descriptor the old file had instead,
//! through handles only, and does nothing when that would change nothing.

#![warn(
    clippy::undocumented_unsafe_blocks,
    clippy::multiple_unsafe_ops_per_block
)]

use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::mem::offset_of;
use std::os::windows::io::{AsRawHandle, HandleOrInvalid, OwnedHandle};
use std::ptr;
use std::slice;

use windows_sys::Win32::Foundation::{
    ERROR_INVALID_FUNCTION, ERROR_NOT_SUPPORTED, ERROR_SUCCESS, HLOCAL, LocalFree, WIN32_ERROR,
};
use windows_sys::Win32::Security::Authorization::{
    GetSecurityInfo, SE_FILE_OBJECT, SetSecurityInfo,
};
use windows_sys::Win32::Security::{
    ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation, DACL_SECURITY_INFORMATION, GetAce,
    GetAclInformation, GetSecurityDescriptorControl, INHERITED_ACE, OBJECT_SECURITY_INFORMATION,
    PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SE_DACL_PRESENT, SE_DACL_PROTECTED,
    UNPROTECTED_DACL_SECURITY_INFORMATION,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, READ_CONTROL, ReOpenFile, WRITE_DAC,
};

/// Gives the open file `to` the DACL of the open file `from`.
///
/// `from` must have been opened with `READ_CONTROL`. Both are used through their handles, so
/// the DACL lands on the file `to` was created as, whatever its name has become. In order, the
/// first rule that applies wins:
///
/// 1. The source has no DACL (everyone has full access): a NULL DACL is applied, protected so
///    that nothing is inherited on top of it.
/// 2. The source DACL is unprotected and holds only inherited entries: nothing is copied, since
///    the replacement inherited the same entries from the same directory. This is the common
///    case, and it never reads the parent descriptor. The owner is not copied, so an inherited
///    entry for the creator-owner resolves to the account that writes, which differs from the
///    old owner when someone else formats the file.
/// 3. Otherwise the DACL is applied with the protection flag of the source. Applying an
///    unprotected DACL makes Windows read the parent descriptor, so a parent that denies
///    `READ_CONTROL` fails the write.
///
/// A file system that cannot return or store security descriptors is skipped; every other
/// failure is returned.
///
/// The replacement keeps the rights the old file had to be deleted: it receives the DACL of a
/// file in the same directory, so renaming it over the old name needs exactly the delete rights
/// that replacing the old file needed. A commit that fails here would have failed before.
pub(super) fn copy_dacl(from: &File, to: &File) -> io::Result<()> {
    let Some(source) = read_dacl(from)? else {
        return Ok(());
    };
    let flags = match source.kind() {
        DaclKind::Null => PROTECTED_DACL_SECURITY_INFORMATION,
        DaclKind::Inherited => return Ok(()),
        DaclKind::Explicit if source.protected => PROTECTED_DACL_SECURITY_INFORMATION,
        DaclKind::Explicit => UNPROTECTED_DACL_SECURITY_INFORMATION,
    };
    let handle = reopen_for_dac(to)?;
    set_dacl(&handle, source.dacl, flags)
}

enum DaclKind {
    Null,
    Inherited,
    Explicit,
}

/// The DACL of a file, valid while the owning descriptor lives.
struct SourceDacl {
    dacl: *const ACL,
    protected: bool,
    _descriptor: LocalAlloc,
}

impl SourceDacl {
    fn kind(&self) -> DaclKind {
        if self.dacl.is_null() {
            DaclKind::Null
        } else if self.protected || has_explicit_ace(self.dacl) {
            DaclKind::Explicit
        } else {
            DaclKind::Inherited
        }
    }
}

/// A security descriptor allocated by the system, freed with `LocalFree`.
struct LocalAlloc(PSECURITY_DESCRIPTOR);

impl Drop for LocalAlloc {
    fn drop(&mut self) {
        // SAFETY: the pointer came from `GetSecurityInfo`, which documents `LocalFree` as the
        // way to release it, and it is freed once, here.
        unsafe { LocalFree(self.0 as HLOCAL) };
    }
}

const fn unsupported(code: WIN32_ERROR) -> bool {
    code == ERROR_NOT_SUPPORTED || code == ERROR_INVALID_FUNCTION
}

fn os_error(code: WIN32_ERROR) -> io::Error {
    io::Error::from_raw_os_error(code.cast_signed())
}

/// Reads the DACL of `file`; `None` when the file system does not support descriptors.
fn read_dacl(file: &File) -> io::Result<Option<SourceDacl>> {
    let mut dacl: *mut ACL = ptr::null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
    // SAFETY: the handle is open for the lifetime of `file`; the out-pointers refer to live
    // locals, and the unused owner, group and SACL outputs are null as the API allows.
    let code = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            &raw mut dacl,
            ptr::null_mut(),
            &raw mut descriptor,
        )
    };
    if unsupported(code) {
        return Ok(None);
    }
    if code != ERROR_SUCCESS {
        return Err(os_error(code));
    }
    let descriptor = LocalAlloc(descriptor);
    let control = control_bits(descriptor.0)?;
    let present = control & SE_DACL_PRESENT != 0;
    Ok(Some(SourceDacl {
        dacl: if present { dacl } else { ptr::null() },
        protected: control & SE_DACL_PROTECTED != 0,
        _descriptor: descriptor,
    }))
}

fn control_bits(descriptor: PSECURITY_DESCRIPTOR) -> io::Result<u16> {
    let mut control = 0u16;
    let mut revision = 0u32;
    // SAFETY: `descriptor` is a valid self-relative descriptor returned by `GetSecurityInfo`
    // and the out-pointers refer to live locals.
    let ok =
        unsafe { GetSecurityDescriptorControl(descriptor, &raw mut control, &raw mut revision) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(control)
}

/// Whether any entry of `dacl` was set on the file itself rather than inherited.
fn has_explicit_ace(dacl: *const ACL) -> bool {
    let mut info = ACL_SIZE_INFORMATION {
        AceCount: 0,
        AclBytesInUse: 0,
        AclBytesFree: 0,
    };
    let size = u32::try_from(size_of::<ACL_SIZE_INFORMATION>()).unwrap_or(u32::MAX);
    // SAFETY: `dacl` is a valid non-null ACL kept alive by its descriptor, and `info` is a live
    // buffer of exactly `size` bytes.
    let ok = unsafe {
        GetAclInformation(
            dacl,
            ptr::from_mut(&mut info).cast::<c_void>(),
            size,
            AclSizeInformation,
        )
    };
    // An unreadable ACL is treated as explicit so that it is copied rather than dropped
    let bytes = usize::try_from(info.AclBytesInUse).unwrap_or(0);
    ok == 0 || (0..info.AceCount).any(|index| !is_inherited(dacl, bytes, index))
}

fn is_inherited(dacl: *const ACL, bytes: usize, index: u32) -> bool {
    let mut ace: *mut c_void = ptr::null_mut();
    // SAFETY: `dacl` is a valid ACL, `index` is below its entry count, and `ace` is a live
    // out-pointer.
    let ok = unsafe { GetAce(dacl, index, &raw mut ace) };
    if ok == 0 || ace.is_null() {
        return false;
    }
    let start = dacl as usize;
    let end = start.saturating_add(bytes);
    let at = ace as usize;
    if at < start || at.saturating_add(size_of::<ACE_HEADER>()) > end {
        return false;
    }
    // SAFETY: `ace` lies inside the `bytes` of the ACL, which its descriptor keeps alive, and
    // every entry starts with an `ACE_HEADER`; a byte slice has no alignment requirement.
    let header =
        unsafe { slice::from_raw_parts(ace.cast::<u8>().cast_const(), size_of::<ACE_HEADER>()) };
    header
        .get(offset_of!(ACE_HEADER, AceFlags))
        .is_some_and(|flags| u32::from(*flags) & INHERITED_ACE != 0)
}

/// A second handle on `file` that may change its DACL.
///
/// The share mode lets the original handle, and the later rename, coexist with it.
fn reopen_for_dac(file: &File) -> io::Result<OwnedHandle> {
    // SAFETY: the handle is open for the lifetime of `file`, and the call has no other inputs.
    let raw = unsafe {
        ReOpenFile(
            file.as_raw_handle(),
            WRITE_DAC | READ_CONTROL,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            0,
        )
    };
    let error = io::Error::last_os_error();
    // SAFETY: `raw` is the result of `ReOpenFile`: a new handle we own, or `INVALID_HANDLE_VALUE`.
    let handle = unsafe { HandleOrInvalid::from_raw_handle(raw) };
    OwnedHandle::try_from(handle).map_err(|_| error)
}

fn set_dacl(
    handle: &OwnedHandle,
    dacl: *const ACL,
    flags: OBJECT_SECURITY_INFORMATION,
) -> io::Result<()> {
    // SAFETY: `handle` is open with `WRITE_DAC`, `dacl` is null or a valid ACL kept alive by
    // the caller, and the unused owner, group and SACL are null.
    let code = unsafe {
        SetSecurityInfo(
            handle.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | flags,
            ptr::null_mut(),
            ptr::null_mut(),
            dacl,
            ptr::null(),
        )
    };
    match code {
        ERROR_SUCCESS => Ok(()),
        code if unsupported(code) => Ok(()),
        code => Err(os_error(code)),
    }
}
