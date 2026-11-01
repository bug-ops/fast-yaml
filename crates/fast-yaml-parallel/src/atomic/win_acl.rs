//! Windows DACL copy for [`AtomicFile`](super::AtomicFile): the only `unsafe` in this crate.
//!
//! The replacement file is created next to the target, so it already carries the DACL entries
//! inherited from the directory. [`copy_dacl`] gives it the descriptor the old file had instead,
//! and does nothing when that would change nothing.

#![warn(
    clippy::undocumented_unsafe_blocks,
    clippy::multiple_unsafe_ops_per_block
)]

use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::ptr;

use windows_sys::Win32::Foundation::{
    ERROR_INVALID_FUNCTION, ERROR_NOT_SUPPORTED, ERROR_SUCCESS, HLOCAL, LocalFree, WIN32_ERROR,
};
use windows_sys::Win32::Security::Authorization::{
    GetSecurityInfo, SE_FILE_OBJECT, SetNamedSecurityInfoW,
};
use windows_sys::Win32::Security::{
    ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation, DACL_SECURITY_INFORMATION, GetAce,
    GetAclInformation, GetSecurityDescriptorControl, INHERITED_ACE, OBJECT_SECURITY_INFORMATION,
    PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SE_DACL_PRESENT, SE_DACL_PROTECTED,
    UNPROTECTED_DACL_SECURITY_INFORMATION,
};

/// Gives the file at `to` the DACL of the open file `from`.
///
/// `from` must have been opened with `READ_CONTROL`. In order, the first rule that applies wins:
///
/// 1. The source has no DACL (everyone has full access): a NULL DACL is applied, protected so
///    that nothing is inherited on top of it.
/// 2. The source DACL is unprotected and holds only inherited entries: nothing is copied, since
///    the replacement inherited the same entries from the same directory. This is the common
///    case, and it never reads the parent descriptor.
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
pub(super) fn copy_dacl(from: &File, to: &Path) -> io::Result<()> {
    let Some(source) = read_dacl(from)? else {
        return Ok(());
    };
    let flags = match source.kind() {
        DaclKind::Null => PROTECTED_DACL_SECURITY_INFORMATION,
        DaclKind::Inherited => return Ok(()),
        DaclKind::Explicit if source.protected => PROTECTED_DACL_SECURITY_INFORMATION,
        DaclKind::Explicit => UNPROTECTED_DACL_SECURITY_INFORMATION,
    };
    set_dacl(to, source.dacl, flags)
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
    // every entry starts with an `ACE_HEADER`; it is read unaligned in case the entry is not
    // 4-byte aligned.
    let header = unsafe { ptr::read_unaligned(ace.cast::<ACE_HEADER>()) };
    u32::from(header.AceFlags) & INHERITED_ACE != 0
}

fn set_dacl(path: &Path, dacl: *const ACL, flags: OBJECT_SECURITY_INFORMATION) -> io::Result<()> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call, `dacl` is null or
    // a valid ACL kept alive by the caller, and the unused owner, group and SACL are null.
    let code = unsafe {
        SetNamedSecurityInfoW(
            wide.as_ptr(),
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
