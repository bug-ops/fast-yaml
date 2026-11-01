//! Secure atomic file replacement shared by the CLI and batch file processing.

use fast_yaml_core::fs::DisplayPath;
use std::fs::{self, Permissions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[cfg(windows)]
#[allow(unsafe_code)]
mod win_acl;

/// What [`write_atomic`] does with a target that has several hard links.
///
/// The policy only has an effect on Unix; on other platforms every target is replaced by rename
/// and both variants behave alike. There is deliberately no `Default`: each caller states
/// whether writing through other names of the file is acceptable.
///
/// # Examples
///
/// ```
/// use fast_yaml_parallel::{HardLinkPolicy, write_atomic};
///
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("a.yaml");
/// write_atomic(&path, b"a: 1\n", HardLinkPolicy::Refuse)?;
/// # Ok::<(), std::io::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardLinkPolicy {
    /// Write the content through the shared inode so every link sees it.
    WriteInPlace,
    /// Fail with an [`io::Error`] that carries [`HardLinkRefused`] and leave the file untouched.
    Refuse,
}

/// The payload of the [`io::ErrorKind::InvalidInput`] error returned for [`HardLinkPolicy::Refuse`].
///
/// Callers find it with [`io::Error::get_ref`] and `downcast_ref`, which lets them add their own
/// advice to the neutral message.
///
/// # Examples
///
/// ```
/// # #[cfg(unix)] {
/// use fast_yaml_parallel::{HardLinkPolicy, HardLinkRefused, write_atomic};
///
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("a.yaml");
/// std::fs::write(&path, "a: 1\n")?;
/// std::fs::hard_link(&path, dir.path().join("b.yaml"))?;
///
/// let err = write_atomic(&path, b"a: 2\n", HardLinkPolicy::Refuse).unwrap_err();
/// let refused = err.get_ref().and_then(|e| e.downcast_ref::<HardLinkRefused>()).unwrap();
/// assert_eq!(refused.links(), 2);
/// # }
/// # Ok::<(), std::io::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HardLinkRefused {
    links: u64,
}

impl HardLinkRefused {
    /// The number of hard links the refused file has.
    #[must_use]
    pub const fn links(&self) -> u64 {
        self.links
    }
}

impl std::fmt::Display for HardLinkRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "refused to write through {} hard links", self.links)
    }
}

impl std::error::Error for HardLinkRefused {}

/// Atomically replaces the contents of `path` with `content`.
///
/// The data is written to an unpredictably named temporary file created with `O_EXCL`
/// in the destination directory, flushed to disk, and then renamed over the target. This
/// avoids following planted `<name>.tmp` symlinks and never touches unrelated sibling files.
///
/// Behavior guarantees:
/// - An existing target keeps its permission bits (including the executable bit).
/// - On Unix, an existing target keeps its owner and group when the process may assign them;
///   when it may not (not root and not the owner), the replacement belongs to the caller.
/// - A symlink target is resolved and the real file is replaced; the link itself is preserved.
/// - A dangling symlink or a non-regular target (directory, FIFO, device) is refused.
/// - A missing target is created like a regular new file: mode `0o666` filtered by the
///   process umask on Unix.
/// - The data is `fsync`ed before the rename and, on Unix, the directory entry afterwards, so a
///   crash leaves either the old or the new content.
/// - On Unix, a read-only (`0o444`) target is still replaced when its directory is writable,
///   and stays read-only afterwards; on Windows the read-only attribute is cleared for the
///   rename and set again.
/// - If the owner or group cannot be restored, the replacement never becomes more readable
///   than the original: without the group, the group and other permission bits are dropped.
///
/// Limitations:
/// - With [`HardLinkPolicy::Refuse`], a Unix target with several hard links is refused with
///   [`HardLinkRefused`] before anything is written.
/// - With [`HardLinkPolicy::WriteInPlace`], a Unix target with several hard links that the caller
///   owns is **not** replaced atomically: the content is written in place so every link sees it. The file is opened
///   with `O_NOFOLLOW` and must still be the inode that was resolved; a crash or error
///   mid-write can leave it truncated or partial, and a read-only hard-linked file is refused.
///   A hard-linked target owned by someone else is replaced through the rename instead, which
///   breaks the link rather than writing into a file the caller may not control.
/// - On Unix, the extended attributes of the old file are copied to the replacement, through
///   file handles opened at `create` time (the old file is checked to be the resolved inode), after
///   the owner is restored and before the mode is applied. A file system without xattr support
///   is skipped silently, and so is an attribute that vanished or that the replacement already
///   carries with the same value (a new file inherits, for example, its `SELinux` label). Permission
///   denied is skipped for `com.apple.*` attributes, which the OS protects (for example
///   `com.apple.provenance`), and for `security.selinux`, which confined domains such as
///   containers may not set. Any other failure, including permission denied on other
///   `security.*`, `system.*` or `trusted.*` names, fails the write instead of dropping the
///   attribute silently. If the old file cannot be opened for reading, no attributes are copied.
///   POSIX ACLs on Linux ride on xattrs and are preserved that way.
/// - On macOS, the ACL of the old file is read through its resolved path and set on the
///   replacement right after the xattrs, replacing any entries the replacement inherited from
///   its directory; a file without an ACL leaves it with none. The path is checked against the
///   identity recorded at `create` time once the ACL is read: the ACL of a file that was swapped
///   in meanwhile is never applied (the write itself still succeeds, as it does for xattrs),
///   but a swap that is undone before the check is not seen. Only a file system that reports
///   ACLs as unsupported is skipped silently; any other failure, including permission denied,
///   fails the write.
/// - On Windows, the DACL of the old file is read through a handle opened with `READ_CONTROL`
///   at `create` time and applied to the replacement before the read-only attribute is set,
///   including its protection flag; a file without a DACL gets none. A file whose DACL only
///   holds entries inherited from its directory needs no copy, since the replacement inherits
///   the same entries. Applying an unprotected DACL reads the descriptor of the parent
///   directory, so a parent that denies `READ_CONTROL` fails the write. A file system without
///   security descriptors is skipped; the owner, SACL, integrity label and alternate data
///   streams are not preserved.
///
/// # Errors
///
/// Returns an I/O error if the target cannot be resolved, is not a regular file, is refused by
/// `hard_links`, or the temporary file cannot be created, written, or renamed into place. No
/// temporary file is left behind on failure.
///
/// # Examples
///
/// ```
/// use fast_yaml_parallel::HardLinkPolicy;
///
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("config.yaml");
///
/// fast_yaml_parallel::write_atomic(&path, b"key: value\n", HardLinkPolicy::WriteInPlace)?;
/// assert_eq!(std::fs::read_to_string(&path)?, "key: value\n");
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn write_atomic(path: &Path, content: &[u8], hard_links: HardLinkPolicy) -> io::Result<()> {
    let mut file = AtomicFile::create(path)?;
    #[cfg(unix)]
    if let Some(old) = file.existing.as_ref().filter(|old| old.hardlinked()) {
        if hard_links == HardLinkPolicy::Refuse {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                HardLinkRefused {
                    links: old.identity.2,
                },
            ));
        }
        if old.owned_by(&file.temp.as_file().metadata()?) {
            return write_in_place(&file.target, content, old);
        }
    }
    #[cfg(not(unix))]
    let _ = hard_links;
    file.write_all(content)?;
    file.commit()
}

/// A file written piece by piece that replaces its target atomically on [`commit`].
///
/// The data goes to an unpredictably named temporary file in the target's directory, so
/// memory use does not grow with the output. It follows the same rules as [`write_atomic`],
/// except that a target with several hard links is replaced by rename (the other links keep
/// the old content). Dropping the file without committing removes the temporary file and
/// leaves the target untouched.
///
/// [`commit`]: AtomicFile::commit
///
/// # Examples
///
/// ```
/// use std::io::Write;
///
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("report.txt");
///
/// let mut file = fast_yaml_parallel::AtomicFile::create(&path)?;
/// file.write_all(b"line 1\n")?;
/// file.write_all(b"line 2\n")?;
/// file.commit()?;
/// assert_eq!(std::fs::read_to_string(&path)?, "line 1\nline 2\n");
/// # Ok::<(), std::io::Error>(())
/// ```
#[derive(Debug)]
pub struct AtomicFile {
    temp: tempfile::NamedTempFile,
    target: PathBuf,
    existing: Option<Existing>,
}

impl AtomicFile {
    /// Starts replacing `path`, which is resolved like in [`write_atomic`].
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the target cannot be resolved, is not a regular file, or the
    /// temporary file cannot be created.
    pub fn create(path: &Path) -> io::Result<Self> {
        let (target, existing) = resolve_target(path)?;
        let dir = parent_dir(&target);
        let temp = create_temp(dir, existing.is_none()).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!(
                    "cannot create temporary file in {}: {e}",
                    DisplayPath::new(dir)
                ),
            )
        })?;
        Ok(Self {
            temp,
            target,
            existing,
        })
    }

    /// Flushes the data to disk and renames the temporary file over the target.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the owner, permissions or data cannot be set, or the rename
    /// fails. The temporary file is removed on failure.
    pub fn commit(self) -> io::Result<()> {
        let Self {
            temp,
            target,
            existing,
        } = self;
        let read_only = existing
            .as_ref()
            .is_some_and(|old| old.permissions.readonly());
        if let Some(old) = &existing {
            // Before chmod: changing the owner can clear mode bits
            #[cfg(unix)]
            let permissions = {
                let permissions = restore_owner(temp.as_file(), old)?;
                copy_xattrs(old.source.as_ref(), temp.as_file())?;
                #[cfg(target_os = "macos")]
                copy_acl(old, &target, temp.path())?;
                permissions
            };
            #[cfg(not(unix))]
            let permissions = old.permissions.clone();
            #[cfg(windows)]
            if let Some(source) = old.source.as_ref() {
                win_acl::copy_dacl(source, temp.path())?;
            }
            temp.as_file().set_permissions(permissions)?;
        }
        // The READ_CONTROL handle has no share-delete access and would block the rename fallback
        #[cfg(windows)]
        drop(existing);
        temp.as_file().sync_all()?;
        // std's rename replaces files that are open elsewhere on Windows; `persist` does not.
        let temp_path = temp.into_temp_path();
        replace(&temp_path, &target, read_only)?;
        let _ = temp_path.keep();
        sync_dir(parent_dir(&target));
        Ok(())
    }
}

impl Write for AtomicFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.temp.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.temp.flush()
    }
}

fn parent_dir(target: &Path) -> &Path {
    match target.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

/// Renames `from` over `to`; on Windows a read-only `to` is made writable first and restored.
fn replace(from: &Path, to: &Path, read_only: bool) -> io::Result<()> {
    #[cfg(windows)]
    if read_only {
        set_read_only(to, false)?;
        let renamed = fs::rename(from, to);
        // Best effort: the rename outcome is what the caller needs to hear about
        let _ = set_read_only(to, true);
        return renamed;
    }
    #[cfg(not(windows))]
    let _ = read_only;
    fs::rename(from, to)
}

#[cfg(windows)]
fn set_read_only(path: &Path, read_only: bool) -> io::Result<()> {
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_readonly(read_only);
    fs::set_permissions(path, permissions)
}

/// Flushes the directory entry created by the rename; best effort, since the data is already
/// committed and not every file system supports syncing a directory.
#[cfg(unix)]
fn sync_dir(dir: &Path) {
    if let Ok(handle) = fs::File::open(dir) {
        let _ = handle.sync_all();
    }
}

#[cfg(not(unix))]
const fn sync_dir(_dir: &Path) {}

/// Overwrites a target that other names share, so every hard link observes the new content.
///
/// The target is opened without truncation and without following a symlink, and written only
/// if it is still the inode `old` was read from.
#[cfg(unix)]
fn write_in_place(target: &Path, content: &[u8], old: &Existing) -> io::Result<()> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};

    // NONBLOCK keeps a FIFO swapped in at the path from blocking the open; it is cleared below
    let flags = OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::NOCTTY;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .custom_flags(flags.bits().cast_signed())
        .open(target)?;
    let opened = file.metadata()?;
    if !opened.is_file() || (opened.dev(), opened.ino(), opened.nlink()) != old.identity {
        return Err(io::Error::other(format!(
            "{} changed while it was being replaced",
            DisplayPath::new(target)
        )));
    }
    fcntl_setfl(&file, fcntl_getfl(&file)? - OFlags::NONBLOCK)?;
    file.set_len(0)?;
    file.write_all(content)?;
    file.sync_all()
}

/// Gives `file` the owner of the file it replaces and returns the permissions to apply.
///
/// A process without the right to assign the owner keeps its own. It then tries the group
/// alone and, failing that too, drops the group and other bits so the file is never more
/// readable than it was.
#[cfg(unix)]
fn restore_owner(file: &fs::File, old: &Existing) -> io::Result<Permissions> {
    use std::os::unix::fs::{PermissionsExt, fchown};

    let denied = |result: io::Result<()>| match result {
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => Ok(false),
        other => other.map(|()| true),
    };
    let mut permissions = old.permissions.clone();
    let (uid, gid) = old.owner;
    if !denied(fchown(file, Some(uid), Some(gid)))? && !denied(fchown(file, None, Some(gid)))? {
        permissions.set_mode(permissions.mode() & 0o700);
    }
    Ok(permissions)
}

/// Copies every extended attribute of `from` to `to`; see [`write_atomic`] for what is skipped.
#[cfg(unix)]
fn copy_xattrs(from: Option<&fs::File>, to: &fs::File) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    use xattr::FileExt;

    let unsupported = |e: &io::Error| {
        e.kind() == io::ErrorKind::Unsupported || e.raw_os_error() == Some(libc::ENOTSUP)
    };
    let Some(from) = from else {
        return Ok(());
    };
    let names = match from.list_xattr() {
        Ok(names) => names,
        Err(e) if unsupported(&e) => return Ok(()),
        Err(e) => return Err(e),
    };
    for name in names {
        let value = match from.get_xattr(&name) {
            Ok(Some(value)) => value,
            Ok(None) => continue,
            Err(e) if unsupported(&e) || e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        if to.get_xattr(&name).ok().flatten().as_deref() == Some(value.as_slice()) {
            continue;
        }
        match to.set_xattr(&name, &value) {
            Err(e) if unsupported(&e) || permission_denied_is_tolerated(name.as_bytes(), &e) => {}
            Err(e) => return Err(e),
            Ok(()) => {}
        }
    }
    Ok(())
}

/// Whether failing to set the attribute `name` with `error` is expected and harmless.
#[cfg(unix)]
fn permission_denied_is_tolerated(name: &[u8], error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::PermissionDenied
        && (name.starts_with(b"com.apple.") || name == b"security.selinux")
}

/// Gives `temp` the ACL of `old`, read through `real`, and none of the entries it inherited from
/// its directory; see [`write_atomic`] for what is skipped.
///
/// # Residual risk
///
/// The ACL is read by path, so `real` is stat-ed after the read and must still be the inode
/// recorded in `old`, otherwise nothing is copied from it. A swap that is undone before the stat
/// cannot be seen.
#[cfg(target_os = "macos")]
fn copy_acl(old: &Existing, real: &Path, temp: &Path) -> io::Result<()> {
    let entries = read_acl(old, real)?;
    match exacl::setfacl(&[temp], &entries, None) {
        Err(e) if e.kind() == io::ErrorKind::Unsupported => Ok(()),
        other => other,
    }
}

/// The ACL entries of the file `old` describes, or none when there is nothing trustworthy to read.
#[cfg(target_os = "macos")]
fn read_acl(old: &Existing, real: &Path) -> io::Result<Vec<exacl::AclEntry>> {
    use std::os::unix::fs::MetadataExt;

    if old.source.is_none() {
        return Ok(Vec::new());
    }
    let entries = match exacl::getfacl(real, None) {
        Ok(entries) => entries,
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::Unsupported | io::ErrorKind::NotFound
            ) =>
        {
            return Ok(Vec::new());
        }
        Err(e) => return Err(e),
    };
    let after = match fs::symlink_metadata(real) {
        Ok(after) => after,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let same_inode =
        after.is_file() && (after.dev(), after.ino()) == (old.identity.0, old.identity.1);
    Ok(if same_inode { entries } else { Vec::new() })
}

/// Opens the file `real` for reading its xattrs, if it is still the inode `metadata` describes.
#[cfg(unix)]
fn open_xattr_source(real: &Path, metadata: &fs::Metadata) -> io::Result<Option<fs::File>> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    let file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(real)
    {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => return Ok(None),
        Err(e) => return Err(e),
    };
    let opened = file.metadata()?;
    if !opened.is_file() || (opened.dev(), opened.ino()) != (metadata.dev(), metadata.ino()) {
        return Err(io::Error::other(format!(
            "{} changed while it was being replaced",
            DisplayPath::new(real)
        )));
    }
    Ok(Some(file))
}

/// Opens `real` with only `READ_CONTROL` access, enough to read its DACL.
///
/// A denied open fails the write: continuing would silently replace the file with one that has
/// the directory's default DACL.
#[cfg(windows)]
fn open_dacl_source(real: &Path) -> io::Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::READ_CONTROL;

    fs::OpenOptions::new().access_mode(READ_CONTROL).open(real)
}

/// Creates the temporary file; new targets get umask-filtered `0o666` instead of `0o600`.
fn create_temp(dir: &Path, new_target: bool) -> io::Result<tempfile::NamedTempFile> {
    #[cfg(unix)]
    if new_target {
        use std::os::unix::fs::PermissionsExt;
        return tempfile::Builder::new()
            .permissions(Permissions::from_mode(0o666))
            .tempfile_in(dir);
    }
    #[cfg(not(unix))]
    let _ = new_target;
    tempfile::NamedTempFile::new_in(dir)
}

/// What an existing target has that its replacement must keep.
#[derive(Debug)]
struct Existing {
    permissions: Permissions,
    /// `(uid, gid)`
    #[cfg(unix)]
    owner: (u32, u32),
    /// `(dev, ino, nlink)`
    #[cfg(unix)]
    identity: (u64, u64, u64),
    /// Handle on the old file, the source of its extended attributes (Unix) or DACL (Windows).
    #[cfg(any(unix, windows))]
    source: Option<fs::File>,
}

impl Existing {
    fn of(metadata: &fs::Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Self {
            permissions: metadata.permissions(),
            #[cfg(unix)]
            owner: (metadata.uid(), metadata.gid()),
            #[cfg(unix)]
            identity: (metadata.dev(), metadata.ino(), metadata.nlink()),
            #[cfg(any(unix, windows))]
            source: None,
        }
    }

    #[cfg(unix)]
    const fn hardlinked(&self) -> bool {
        self.identity.2 > 1
    }

    /// Whether the file belongs to the user that `process_file` (made by this process) has.
    #[cfg(unix)]
    fn owned_by(&self, process_file: &fs::Metadata) -> bool {
        use std::os::unix::fs::MetadataExt;
        self.owner.0 == process_file.uid()
    }
}

/// Resolves the real replacement target and what to preserve of it, if it exists.
fn resolve_target(path: &Path) -> io::Result<(PathBuf, Option<Existing>)> {
    match fs::canonicalize(path) {
        Ok(real) => {
            let metadata = fs::metadata(&real)?;
            if !metadata.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("not a regular file: {}", DisplayPath::new(path)),
                ));
            }
            #[allow(unused_mut)]
            let mut existing = Existing::of(&metadata);
            #[cfg(unix)]
            {
                existing.source = open_xattr_source(&real, &metadata)?;
            }
            #[cfg(windows)]
            {
                existing.source = Some(open_dacl_source(&real)?);
            }
            Ok((real, Some(existing)))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if fs::symlink_metadata(path).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("refused: dangling symlink {}", DisplayPath::new(path)),
                ));
            }
            Ok((path.to_path_buf(), None))
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_atomic(path: &Path, content: &[u8]) -> io::Result<()> {
        super::write_atomic(path, content, HardLinkPolicy::WriteInPlace)
    }

    #[test]
    fn creates_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.yaml");
        write_atomic(&path, b"a: 1\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "a: 1\n");
    }

    #[test]
    fn writes_empty_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.yaml");
        fs::write(&path, "old").unwrap();
        write_atomic(&path, b"").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "");
    }

    #[test]
    fn refuses_directory_target() {
        let dir = tempfile::tempdir().unwrap();
        let err = write_atomic(dir.path(), b"x").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[cfg(windows)]
    #[test]
    fn read_only_target_is_replaced_and_stays_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ro.yaml");
        fs::write(&path, "old").unwrap();
        set_read_only(&path, true).unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert!(fs::metadata(&path).unwrap().permissions().readonly());
        set_read_only(&path, false).unwrap();
    }

    #[cfg(unix)]
    mod unix {
        use super::*;
        use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

        fn mode(path: &Path) -> u32 {
            fs::metadata(path).unwrap().permissions().mode() & 0o777
        }

        const XATTR: &str = if cfg!(target_os = "linux") {
            "user.fy.test"
        } else {
            "com.fy.test"
        };

        #[test]
        fn xattrs_survive_replacement() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("x.yaml");
            fs::write(&path, "a: 1\n").unwrap();
            if xattr::set(&path, XATTR, b"kept").is_err() {
                return;
            }
            let inode = fs::metadata(&path).unwrap().ino();
            write_atomic(&path, b"a: 2\n").unwrap();
            assert_ne!(fs::metadata(&path).unwrap().ino(), inode);
            assert_eq!(
                xattr::get(&path, XATTR).unwrap().as_deref(),
                Some(&b"kept"[..])
            );
        }

        #[test]
        fn xattrs_come_from_the_inode_resolved_at_create() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("swap.yaml");
            fs::write(&path, "a: 1\n").unwrap();
            if xattr::set(&path, XATTR, b"original").is_err() {
                return;
            }
            let mut file = AtomicFile::create(&path).unwrap();
            fs::remove_file(&path).unwrap();
            fs::write(&path, "planted\n").unwrap();
            xattr::set(&path, XATTR, b"planted").unwrap();
            file.write_all(b"a: 2\n").unwrap();
            file.commit().unwrap();
            assert_eq!(
                xattr::get(&path, XATTR).unwrap().as_deref(),
                Some(&b"original"[..])
            );
        }

        #[test]
        fn permission_denied_is_tolerated_only_for_known_names() {
            let denied = io::Error::from(io::ErrorKind::PermissionDenied);
            let other = io::Error::from(io::ErrorKind::InvalidInput);
            assert!(permission_denied_is_tolerated(b"security.selinux", &denied));
            assert!(permission_denied_is_tolerated(
                b"com.apple.provenance",
                &denied
            ));
            assert!(!permission_denied_is_tolerated(
                b"security.capability",
                &denied
            ));
            assert!(!permission_denied_is_tolerated(
                b"system.posix_acl_access",
                &denied
            ));
            assert!(!permission_denied_is_tolerated(b"trusted.x", &denied));
            assert!(!permission_denied_is_tolerated(b"security.selinux", &other));
        }

        #[test]
        fn identical_value_on_the_replacement_is_left_alone() {
            let dir = tempfile::tempdir().unwrap();
            let from = dir.path().join("from");
            let to = dir.path().join("to");
            fs::write(&from, "").unwrap();
            fs::write(&to, "").unwrap();
            if xattr::set(&from, XATTR, b"same").is_err() {
                return;
            }
            xattr::set(&to, XATTR, b"same").unwrap();
            let source = fs::File::open(&from).unwrap();
            let target = fs::OpenOptions::new().write(true).open(&to).unwrap();
            copy_xattrs(Some(&source), &target).unwrap();
            assert_eq!(
                xattr::get(&to, XATTR).unwrap().as_deref(),
                Some(&b"same"[..])
            );
        }

        #[test]
        fn fifo_in_place_of_the_target_does_not_block() {
            let dir = tempfile::tempdir().unwrap();
            let fifo = dir.path().join("pipe.yaml");
            assert!(
                std::process::Command::new("mkfifo")
                    .arg(&fifo)
                    .status()
                    .unwrap()
                    .success()
            );
            let metadata = fs::metadata(&fifo).unwrap();
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                tx.send(open_xattr_source(&fifo, &metadata).is_err()).ok();
            });
            let refused = rx
                .recv_timeout(std::time::Duration::from_secs(10))
                .expect("opening a FIFO must not block");
            assert!(refused);
        }

        #[test]
        fn xattrs_survive_read_only_replacement() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("ro.yaml");
            fs::write(&path, "a: 1\n").unwrap();
            if xattr::set(&path, XATTR, b"kept").is_err() {
                return;
            }
            fs::set_permissions(&path, Permissions::from_mode(0o444)).unwrap();
            write_atomic(&path, b"a: 2\n").unwrap();
            assert_eq!(mode(&path), 0o444);
            assert_eq!(
                xattr::get(&path, XATTR).unwrap().as_deref(),
                Some(&b"kept"[..])
            );
        }

        #[test]
        fn refuses_dangling_symlink() {
            let dir = tempfile::tempdir().unwrap();
            let link = dir.path().join("link.yaml");
            symlink(dir.path().join("missing"), &link).unwrap();
            let err = write_atomic(&link, b"x").unwrap_err();
            assert!(err.to_string().contains("dangling symlink"));
            assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        }

        #[test]
        fn planted_tmp_symlink_does_not_clobber_victim() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("cfg.yaml");
            let victim = dir.path().join("victim.txt");
            fs::write(&path, "old").unwrap();
            fs::write(&victim, "victim").unwrap();
            symlink(&victim, dir.path().join("cfg.tmp")).unwrap();

            write_atomic(&path, b"new").unwrap();

            assert_eq!(fs::read_to_string(&victim).unwrap(), "victim");
        }

        #[test]
        fn new_file_is_not_restricted_to_owner_only() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("new.yaml");
            write_atomic(&path, b"x").unwrap();
            let plain = dir.path().join("plain.yaml");
            fs::write(&plain, "x").unwrap();
            assert_eq!(mode(&path), mode(&plain));
        }

        #[test]
        fn read_only_target_is_replaced_and_stays_read_only() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("ro.yaml");
            fs::write(&path, "old").unwrap();
            fs::set_permissions(&path, Permissions::from_mode(0o444)).unwrap();
            write_atomic(&path, b"new").unwrap();
            assert_eq!(fs::read_to_string(&path).unwrap(), "new");
            assert_eq!(mode(&path), 0o444);
        }

        #[test]
        fn hard_linked_target_is_written_in_place_and_stays_linked() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("a.yaml");
            let link = dir.path().join("b.yaml");
            fs::write(&path, "old content").unwrap();
            fs::hard_link(&path, &link).unwrap();
            let inode = fs::metadata(&path).unwrap().ino();

            write_atomic(&path, b"new").unwrap();

            assert_eq!(fs::read_to_string(&link).unwrap(), "new");
            let after = fs::metadata(&path).unwrap();
            assert_eq!((after.ino(), after.nlink()), (inode, 2));
        }

        #[test]
        fn refused_hard_link_leaves_every_link_intact() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("a.yaml");
            let link = dir.path().join("b.yaml");
            fs::write(&path, "old").unwrap();
            fs::hard_link(&path, &link).unwrap();

            let err = crate::write_atomic(&path, b"new", HardLinkPolicy::Refuse).unwrap_err();

            assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
            let refused = err
                .get_ref()
                .and_then(|e| e.downcast_ref::<HardLinkRefused>())
                .unwrap();
            assert_eq!(refused.links(), 2);
            assert_eq!(fs::read_to_string(&path).unwrap(), "old");
            assert_eq!(fs::read_to_string(&link).unwrap(), "old");
            assert_eq!(fs::metadata(&path).unwrap().nlink(), 2);
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        }

        #[test]
        fn refuse_policy_does_not_affect_unshared_files() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("a.yaml");
            fs::write(&path, "old").unwrap();
            crate::write_atomic(&path, b"new", HardLinkPolicy::Refuse).unwrap();
            assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        }

        #[test]
        fn in_place_write_to_a_fifo_does_not_block() {
            let dir = tempfile::tempdir().unwrap();
            let fifo = dir.path().join("pipe.yaml");
            assert!(
                std::process::Command::new("mkfifo")
                    .arg(&fifo)
                    .status()
                    .unwrap()
                    .success()
            );
            let old = existing_of(&fifo);
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                tx.send(write_in_place(&fifo, b"x", &old).is_err()).ok();
            });
            let refused = rx
                .recv_timeout(std::time::Duration::from_secs(10))
                .expect("opening a FIFO for writing must not block");
            assert!(refused);
        }

        #[test]
        fn unshared_target_gets_a_new_inode_with_same_owner_and_mode() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("a.yaml");
            fs::write(&path, "old").unwrap();
            fs::set_permissions(&path, Permissions::from_mode(0o640)).unwrap();
            let before = fs::metadata(&path).unwrap();

            write_atomic(&path, b"new").unwrap();

            let after = fs::metadata(&path).unwrap();
            assert_ne!(after.ino(), before.ino());
            assert_eq!((after.uid(), after.gid()), (before.uid(), before.gid()));
            assert_eq!(mode(&path), 0o640);
        }

        fn existing_of(path: &Path) -> Existing {
            Existing::of(&fs::metadata(path).unwrap())
        }

        #[test]
        fn in_place_write_refuses_a_replaced_inode() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("a.yaml");
            let other = dir.path().join("other.yaml");
            fs::write(&path, "old").unwrap();
            fs::write(&other, "other").unwrap();

            let err = write_in_place(&path, b"new", &existing_of(&other)).unwrap_err();

            assert!(err.to_string().contains("changed while"), "{err}");
            assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        }

        #[test]
        fn in_place_write_does_not_follow_a_symlink() {
            let dir = tempfile::tempdir().unwrap();
            let victim = dir.path().join("victim.yaml");
            let link = dir.path().join("link.yaml");
            fs::write(&victim, "victim").unwrap();
            symlink(&victim, &link).unwrap();

            assert!(write_in_place(&link, b"new", &existing_of(&victim)).is_err());
            assert_eq!(fs::read_to_string(&victim).unwrap(), "victim");
        }

        #[test]
        fn hard_link_owned_by_someone_else_is_not_written_in_place() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("a.yaml");
            fs::write(&path, "old").unwrap();
            let mut old = existing_of(&path);
            old.owner.0 = old.owner.0.wrapping_add(1);
            let probe = tempfile::tempfile_in(dir.path()).unwrap();
            assert!(!old.owned_by(&probe.metadata().unwrap()));
        }

        #[test]
        fn losing_the_group_drops_group_and_other_bits() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("a.yaml");
            fs::write(&path, "old").unwrap();
            fs::set_permissions(&path, Permissions::from_mode(0o640)).unwrap();
            let mut old = existing_of(&path);
            old.owner = (old.owner.0.wrapping_add(1), old.owner.1.wrapping_add(1));
            let probe = tempfile::tempfile_in(dir.path()).unwrap();
            if std::os::unix::fs::fchown(&probe, None, Some(old.owner.1)).is_ok() {
                return; // privileged, or a group the caller belongs to
            }

            let permissions = restore_owner(&probe, &old).unwrap();

            assert_eq!(permissions.mode() & 0o077, 0);
        }

        #[test]
        fn failing_to_assign_the_owner_is_not_an_error() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("a.yaml");
            fs::write(&path, "old").unwrap();
            let metadata = fs::metadata(&path).unwrap();
            let old = Existing {
                permissions: metadata.permissions(),
                owner: (metadata.uid().wrapping_add(1), metadata.gid()),
                identity: (metadata.dev(), metadata.ino(), 1),
                source: None,
            };
            let temp = tempfile::tempfile_in(dir.path()).unwrap();
            // Unprivileged: EPERM is ignored; privileged: the chown simply succeeds
            assert!(restore_owner(&temp, &old).is_ok());
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod acl_tests {
    use super::*;
    use exacl::{AclEntry, Flag, Perm};

    fn write_atomic(path: &Path, content: &[u8]) -> io::Result<()> {
        super::write_atomic(path, content, HardLinkPolicy::WriteInPlace)
    }

    fn deny_everyone_write() -> Vec<AclEntry> {
        vec![AclEntry::deny_group("everyone", Perm::WRITE, None)]
    }

    fn has_everyone_entry(path: &Path) -> bool {
        exacl::getfacl(path, None)
            .unwrap()
            .iter()
            .any(|e| e.name == "everyone")
    }

    #[test]
    fn acl_survives_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("acl.yaml");
        fs::write(&path, "a: 1\n").unwrap();
        if exacl::setfacl(&[&path], &deny_everyone_write(), None).is_err() {
            return;
        }
        write_atomic(&path, b"a: 2\n").unwrap();
        assert!(has_everyone_entry(&path));
    }

    #[test]
    fn acl_copy_tolerates_a_target_deleted_before_commit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gone.yaml");
        fs::write(&path, "a: 1\n").unwrap();
        if exacl::setfacl(&[&path], &deny_everyone_write(), None).is_err() {
            return;
        }
        let mut file = AtomicFile::create(&path).unwrap();
        fs::remove_file(&path).unwrap();
        file.write_all(b"a: 2\n").unwrap();
        file.commit().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "a: 2\n");
    }

    #[test]
    fn acl_inherited_from_the_directory_is_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plain.yaml");
        fs::write(&path, "a: 1\n").unwrap();
        let inheritable = [AclEntry::allow_group(
            "everyone",
            Perm::READ,
            Flag::FILE_INHERIT,
        )];
        if exacl::setfacl(&[dir.path()], &inheritable, None).is_err() {
            return;
        }
        let created = dir.path().join("probe.yaml");
        fs::write(&created, "").unwrap();
        if !has_everyone_entry(&created) {
            return;
        }
        write_atomic(&path, b"a: 2\n").unwrap();
        assert!(!has_everyone_entry(&path));
    }

    #[test]
    fn acl_of_a_swapped_file_is_not_copied() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("swap.yaml");
        fs::write(&path, "a: 1\n").unwrap();
        if exacl::setfacl(&[&path], &deny_everyone_write(), None).is_err() {
            return;
        }
        let file = AtomicFile::create(&path).unwrap();
        fs::remove_file(&path).unwrap();
        fs::write(&path, "planted\n").unwrap();
        exacl::setfacl(&[&path], &deny_everyone_write(), None).unwrap();
        let old = file.existing.as_ref().unwrap();
        copy_acl(old, &file.target, file.temp.path()).unwrap();
        assert!(!has_everyone_entry(file.temp.path()));
    }
}

#[cfg(all(test, windows))]
mod dacl_tests {
    use super::*;
    use std::process::Command;

    fn write_atomic(path: &Path, content: &[u8]) -> io::Result<()> {
        super::write_atomic(path, content, HardLinkPolicy::WriteInPlace)
    }

    fn run(program: &str, args: &[&str]) -> String {
        let output = Command::new(program).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{program} {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn powershell(script: &str) -> String {
        run("powershell", &["-NoProfile", "-Command", script])
    }

    fn sddl(path: &Path) -> String {
        powershell(&format!("(Get-Acl -LiteralPath '{}').Sddl", path.display()))
    }

    fn icacls(path: &Path, args: &[&str]) {
        let path = path.to_str().unwrap();
        let mut all = vec![path];
        all.extend_from_slice(args);
        run("icacls", &all);
    }

    fn assert_sddl_survives(path: &Path) {
        let before = sddl(path);
        write_atomic(path, b"a: 2\n").unwrap();
        assert_eq!(sddl(path), before);
        assert_eq!(fs::read_to_string(path).unwrap(), "a: 2\n");
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }

    fn file_in(dir: &tempfile::TempDir) -> PathBuf {
        let path = dir.path().join("acl.yaml");
        fs::write(&path, "a: 1\n").unwrap();
        path
    }

    #[test]
    fn explicit_grant_survives_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let path = file_in(&dir);
        icacls(&path, &["/grant", "*S-1-1-0:(R)"]);
        assert_sddl_survives(&path);
    }

    #[test]
    fn protected_dacl_survives_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let path = file_in(&dir);
        icacls(&path, &["/inheritance:d"]);
        icacls(&path, &["/grant", "*S-1-1-0:(R)"]);
        assert_sddl_survives(&path);
    }

    #[test]
    fn protected_read_only_grant_survives_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let path = file_in(&dir);
        icacls(&path, &["/inheritance:r", "/grant", "*S-1-1-0:(R)"]);
        assert_sddl_survives(&path);
    }

    #[test]
    fn null_dacl_survives_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let path = file_in(&dir);
        powershell(&format!(
            "$p = '{}'; $a = Get-Acl -LiteralPath $p; \
             $a.SetSecurityDescriptorSddlForm('D:NO_ACCESS_CONTROL'); \
             Set-Acl -LiteralPath $p -AclObject $a",
            path.display()
        ));
        assert_sddl_survives(&path);
    }

    #[test]
    fn plain_file_keeps_inherited_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = file_in(&dir);
        assert_sddl_survives(&path);
    }
}
