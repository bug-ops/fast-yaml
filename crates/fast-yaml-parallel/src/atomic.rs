//! Secure atomic file replacement shared by the CLI and batch file processing.

use crate::trace::debug_event;
use fast_yaml_core::fs::DisplayPath;
use std::fs::{self, Permissions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

mod error;
#[cfg(windows)]
#[allow(unsafe_code)]
mod win_acl;

pub use error::AtomicWriteError;

/// The identity of an open file on its volume, independent of the path it was reached by.
///
/// On Unix it is the device and inode, on Windows the volume serial number and the 64-bit file
/// index. Take it from the handle a file was read through, then pass it to
/// [`write_atomic_bound`] so the write cannot land on a file that replaced the one read.
///
/// An identity is only as good as what the file system reports. On a platform other than Unix
/// and Windows, and on Windows when the file system reports a zero file index (some network
/// shares), [`FileId::of`] returns `None` rather than an identity that matches every file, and
/// a write that has no identity is not bound. The 64-bit index is not guaranteed unique on
/// `ReFS`, and a file system with unstable inode numbers (some FUSE mounts) can refuse a write
/// to an unchanged file; running the command again resolves that.
///
/// # Examples
///
/// ```
/// use fast_yaml_parallel::FileId;
///
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("a.yaml");
/// std::fs::write(&path, "a: 1\n")?;
/// let id = FileId::of(&std::fs::File::open(&path)?)?;
/// assert_eq!(id, FileId::of(&std::fs::File::open(&path)?)?);
/// # Ok::<(), std::io::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId {
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
    #[cfg(windows)]
    volume: u32,
    #[cfg(windows)]
    index: u64,
}

impl FileId {
    /// The identity of the file `file` is open on, or `None` when the platform or file system
    /// cannot tell files apart.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the system cannot describe the handle.
    pub fn of(file: &fs::File) -> io::Result<Option<Self>> {
        #[cfg(unix)]
        return Ok(Some(Self::of_metadata(&file.metadata()?)));
        #[cfg(windows)]
        return win_acl::file_id(file)
            .map(|(volume, index)| (index != 0).then_some(Self { volume, index }));
        #[cfg(not(any(unix, windows)))]
        {
            let _ = file;
            Ok(None)
        }
    }

    /// The identity of the file `path` resolves to, following symlinks, or `None` when the
    /// platform or file system cannot tell files apart.
    ///
    /// It names the same file as [`FileId::of`] on a handle opened through `path`, so a symlink
    /// and its target, or two hard links, compare equal. Unlike a canonical path it also
    /// recognises hard links on Windows.
    ///
    /// # Errors
    ///
    /// Returns an I/O error, `NotFound` for a missing path, if the path cannot be examined.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_parallel::FileId;
    ///
    /// let dir = tempfile::tempdir()?;
    /// let path = dir.path().join("a.yaml");
    /// std::fs::write(&path, "a: 1\n")?;
    /// assert_eq!(FileId::of_path(&path)?, FileId::of(&std::fs::File::open(&path)?)?);
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn of_path(path: &Path) -> io::Result<Option<Self>> {
        stat_identified(path).map(|(_, id)| id)
    }

    #[cfg(unix)]
    fn of_metadata(metadata: &fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;
        Self {
            dev: metadata.dev(),
            ino: metadata.ino(),
        }
    }
}

/// The metadata of the file `path` resolves to with its identity, both taken from one stat.
///
/// On Windows both come from one handle opened through reparse points, so they describe a single
/// file even if the path is swapped meanwhile.
pub fn stat_identified(path: &Path) -> io::Result<(fs::Metadata, Option<FileId>)> {
    #[cfg(unix)]
    {
        let metadata = fs::metadata(path)?;
        let id = Some(FileId::of_metadata(&metadata));
        Ok((metadata, id))
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES,
        };

        let file = fs::OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)?;
        let id = FileId::of(&file)?;
        Ok((file.metadata()?, id))
    }
    #[cfg(not(any(unix, windows)))]
    fs::metadata(path).map(|metadata| (metadata, None))
}

/// The payload of the error returned by [`write_atomic_bound`] when the target is not the file
/// that was read.
///
/// It is the payload of [`AtomicWriteError::TargetChanged`]. Callers find that with
/// [`io::Error::get_ref`] and `downcast_ref`, which tells a file that changed concurrently apart
/// from an ordinary I/O failure.
///
/// # Examples
///
/// ```
/// use fast_yaml_parallel::{AtomicWriteError, FileId, HardLinkPolicy, write_atomic_bound};
///
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("a.yaml");
/// std::fs::write(&path, "a: 1\n")?;
/// let id = FileId::of(&std::fs::File::open(&path)?)?;
/// std::fs::remove_file(&path)?;
///
/// # if id.is_some() {
/// let err = write_atomic_bound(&path, b"a: 2\n", HardLinkPolicy::WriteInPlace, id).unwrap_err();
/// let payload = err.get_ref().and_then(|e| e.downcast_ref::<AtomicWriteError>());
/// assert!(matches!(payload, Some(AtomicWriteError::TargetChanged(_))));
/// # }
/// # Ok::<(), std::io::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetChanged {
    path: PathBuf,
}

impl TargetChanged {
    /// The path that no longer names the file that was read.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl std::fmt::Display for TargetChanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} is not the file that was read; run the command again",
            DisplayPath::new(&self.path)
        )
    }
}

impl std::error::Error for TargetChanged {}

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
    /// Fail with an [`io::Error`] that carries [`AtomicWriteError::HardLinkRefused`] and leave the file untouched.
    Refuse,
}

/// The payload of the [`io::ErrorKind::InvalidInput`] error returned for [`HardLinkPolicy::Refuse`].
///
/// It is the payload of [`AtomicWriteError::HardLinkRefused`]. Callers find that with
/// [`io::Error::get_ref`] and `downcast_ref`, which lets them add their own advice to the
/// neutral message.
///
/// # Examples
///
/// ```
/// # #[cfg(unix)] {
/// use fast_yaml_parallel::{AtomicWriteError, HardLinkPolicy, write_atomic};
///
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("a.yaml");
/// std::fs::write(&path, "a: 1\n")?;
/// std::fs::hard_link(&path, dir.path().join("b.yaml"))?;
///
/// let err = write_atomic(&path, b"a: 2\n", HardLinkPolicy::Refuse).unwrap_err();
/// let payload = err.get_ref().and_then(|e| e.downcast_ref::<AtomicWriteError>());
/// let Some(AtomicWriteError::HardLinkRefused(refused)) = payload else { panic!() };
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
/// - On Windows, the old file is opened at `create` time without following a reparse point, and
///   its permissions and DACL are both taken from that handle: a file swapped in at the path
///   afterwards is not read, and a directory or symlink found there fails the write. The DACL
///   is applied to the replacement through a handle on it, before the read-only attribute is
///   set, including its protection flag; a file without a DACL gets none. A file whose DACL
///   only holds entries inherited from its directory needs no copy, since the replacement
///   inherits the same entries. Applying an unprotected DACL reads the descriptor of the
///   parent directory, so a parent that denies `READ_CONTROL` fails the write. A file system
///   without security descriptors is skipped. The owner is not copied (the replacement belongs
///   to the writer, so inherited creator-owner entries resolve to the writer), nor are the
///   SACL and the integrity label. Of the alternate data streams only `Zone.Identifier` (the
///   mark of the web) is copied, before the DACL, so that a downloaded file stays marked; it is
///   read by path and skipped if the path no longer names the file opened at `create` time
///   (for a write bound by [`write_atomic_bound`] the identity check before the rename then
///   fails the write). A file system without streams is skipped, and any other stream is
///   dropped. A stream that exists but cannot be read, for example because a browser or virus
///   scanner holds it with a sharing violation, fails the write rather than silently dropping
///   the mark of the web.
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
    write_resolved(AtomicFile::create(path)?, content, hard_links)
}

/// Like [`write_atomic`], but only replaces the file whose identity is `expected`.
///
/// A caller that read the content of `path` and writes a result derived from it passes the
/// [`FileId`] of the handle it read through. If the path names another file by the time the
/// target is resolved (renamed over, deleted, or re-pointed through a symlink), nothing is
/// written and the call fails with [`TargetChanged`], so the old file's content is never
/// replaced by output computed from a different file. The identity is checked again right
/// before the rename; a swap in the instant between that check and the rename is not seen.
///
/// `expected` is `None` when the read could not take an identity (see [`FileId`]); the write
/// is then unbound and behaves like [`write_atomic`].
///
/// # Errors
///
/// Returns the errors of [`write_atomic`], and an I/O error carrying [`TargetChanged`] if the
/// target is missing or is not the file `expected` describes.
///
/// # Examples
///
/// ```
/// use fast_yaml_parallel::{FileId, HardLinkPolicy, write_atomic_bound};
///
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("a.yaml");
/// std::fs::write(&path, "a: 1\n")?;
/// let id = FileId::of(&std::fs::File::open(&path)?)?;
///
/// write_atomic_bound(&path, b"a: 2\n", HardLinkPolicy::WriteInPlace, id)?;
/// assert_eq!(std::fs::read_to_string(&path)?, "a: 2\n");
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn write_atomic_bound(
    path: &Path,
    content: &[u8],
    hard_links: HardLinkPolicy,
    expected: Option<FileId>,
) -> io::Result<()> {
    write_resolved(
        AtomicFile::create_bound(path, expected)?,
        content,
        hard_links,
    )
}

fn write_resolved(
    mut file: AtomicFile,
    content: &[u8],
    hard_links: HardLinkPolicy,
) -> io::Result<()> {
    #[cfg(unix)]
    if let Some(old) = file.existing.as_ref().filter(|old| old.hardlinked()) {
        if hard_links == HardLinkPolicy::Refuse {
            return Err(AtomicWriteError::from(HardLinkRefused { links: old.links }).into());
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
    expected: Option<FileId>,
}

impl AtomicFile {
    /// Starts replacing `path`, which is resolved like in [`write_atomic`].
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the target cannot be resolved, is not a regular file, or the
    /// temporary file cannot be created.
    pub fn create(path: &Path) -> io::Result<Self> {
        Self::create_bound(path, None)
    }

    /// [`create`](Self::create) that, given `expected`, fails unless the target is that file.
    fn create_bound(path: &Path, expected: Option<FileId>) -> io::Result<Self> {
        let (target, existing) = resolve_target(path, expected)?;
        let dir = parent_dir(&target);
        let temp =
            create_temp(dir, existing.is_none()).map_err(|source| AtomicWriteError::TempFile {
                dir: dir.to_path_buf(),
                source,
            })?;
        Ok(Self {
            temp,
            target,
            existing,
            expected,
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
            expected,
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
            copy_zone_identifier(old, &target, temp.path())?;
            #[cfg(windows)]
            if let Some(source) = old.source.as_ref() {
                win_acl::copy_dacl(source, temp.as_file())?;
            }
            temp.as_file().set_permissions(permissions)?;
        }
        // Close the source handle first so the rename never meets another open handle on the target
        #[cfg(windows)]
        drop(existing);
        temp.as_file().sync_all()?;
        // std's rename replaces files that are open elsewhere on Windows; `persist` does not.
        let temp_path = temp.into_temp_path();
        ensure_still_expected(&target, expected)?;
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
        if let Err(e) = set_read_only(to, true) {
            debug_event!(
                "could not restore read-only on {}: {e}",
                DisplayPath::new(to)
            );
        }
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
    match fs::File::open(dir) {
        Ok(handle) => {
            if let Err(e) = handle.sync_all() {
                debug_event!("could not sync directory {}: {e}", DisplayPath::new(dir));
            }
        }
        Err(e) => debug_event!("could not open directory {}: {e}", DisplayPath::new(dir)),
    }
}

#[cfg(not(unix))]
const fn sync_dir(_dir: &Path) {}

/// Flags for opening a file that may have been swapped for a symlink or FIFO.
///
/// `NONBLOCK` keeps a FIFO from blocking the open; callers that write clear it afterwards.
#[cfg(unix)]
const NO_FOLLOW_OPEN: rustix::fs::OFlags = rustix::fs::OFlags::NOFOLLOW
    .union(rustix::fs::OFlags::NONBLOCK)
    .union(rustix::fs::OFlags::NOCTTY);

/// The error for a target that is not the file the caller read or the writer resolved.
fn not_the_file_read(path: &Path) -> io::Error {
    AtomicWriteError::from(TargetChanged {
        path: path.to_path_buf(),
    })
    .into()
}

/// Fails with [`TargetChanged`] unless `target` still names the file `expected`; best effort,
/// since it cannot close the instant between this check and the rename that follows.
fn ensure_still_expected(target: &Path, expected: Option<FileId>) -> io::Result<()> {
    let Some(expected) = expected else {
        return Ok(());
    };
    #[cfg(unix)]
    let now = fs::metadata(target).map(|metadata| Some(FileId::of_metadata(&metadata)));
    #[cfg(windows)]
    let now = open_dacl_source(target).and_then(|(file, _)| FileId::of(&file));
    #[cfg(not(any(unix, windows)))]
    let now: io::Result<Option<FileId>> = Ok(None);
    match now {
        Ok(Some(id)) if id == expected => Ok(()),
        Ok(_) => Err(not_the_file_read(target)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(not_the_file_read(target)),
        Err(e) => Err(e),
    }
}

/// The largest `Zone.Identifier` stream that is copied; real ones are a few dozen bytes.
#[cfg(windows)]
const MAX_ZONE_IDENTIFIER_BYTES: u64 = 64 * 1024;

#[cfg(windows)]
fn zone_identifier_path(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(":Zone.Identifier");
    PathBuf::from(name)
}

/// Whether `error` says the file system has no alternate data streams (or none of that name).
#[cfg(windows)]
fn no_stream(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::InvalidFilename | io::ErrorKind::Unsupported
    )
}

/// Copies the `Zone.Identifier` stream of `real` onto the not yet renamed `temp`; any failure
/// other than "no such stream" is returned.
///
/// The stream is read by path, since std cannot open a stream relative to a handle, so `real` is
/// opened again afterwards and nothing is written unless it is still the file `old` describes.
#[cfg(windows)]
fn copy_zone_identifier(old: &Existing, real: &Path, temp: &Path) -> io::Result<()> {
    use std::io::Read;

    let source = match fs::File::open(zone_identifier_path(real)) {
        Ok(source) => source,
        Err(e) if no_stream(&e) => return Ok(()),
        Err(e) => return Err(e),
    };
    let mut data = Vec::new();
    source
        .take(MAX_ZONE_IDENTIFIER_BYTES + 1)
        .read_to_end(&mut data)?;
    if data.len() as u64 > MAX_ZONE_IDENTIFIER_BYTES {
        return Err(AtomicWriteError::ZoneIdentifierTooLarge {
            path: real.to_path_buf(),
        }
        .into());
    }
    let still_old = open_dacl_source(real)
        .and_then(|(file, _)| FileId::of(&file))
        .is_ok_and(|id| id.is_some() && id == old.id);
    if !still_old {
        debug_event!(
            "Zone.Identifier of {} not copied: the path no longer names the file",
            DisplayPath::new(real)
        );
        return Ok(());
    }
    match fs::File::create_new(zone_identifier_path(temp)) {
        Ok(mut stream) => stream.write_all(&data),
        Err(e) if no_stream(&e) => {
            debug_event!(
                "Zone.Identifier not written to {}: {e}",
                DisplayPath::new(temp)
            );
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Overwrites a target that other names share, so every hard link observes the new content.
///
/// The target is opened without truncation and without following a symlink, and written only
/// if it is still the inode `old` was read from.
#[cfg(unix)]
fn write_in_place(target: &Path, content: &[u8], old: &Existing) -> io::Result<()> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};

    let mut file = fs::OpenOptions::new()
        .write(true)
        .custom_flags(NO_FOLLOW_OPEN.bits().cast_signed())
        .open(target)?;
    let opened = file.metadata()?;
    if !opened.is_file()
        || Some(FileId::of_metadata(&opened)) != old.id
        || opened.nlink() != old.links
    {
        return Err(not_the_file_read(target));
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
    let Owner { uid, gid } = old.owner;
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
        e.kind() == io::ErrorKind::Unsupported
            || e.raw_os_error() == Some(rustix::io::Errno::NOTSUP.raw_os_error())
    };
    let Some(from) = from else {
        return Ok(());
    };
    let names = match from.list_xattr() {
        Ok(names) => names,
        Err(e) if unsupported(&e) => {
            debug_event!("extended attributes are not supported: {e}");
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    for name in names {
        let value = match from.get_xattr(&name) {
            Ok(Some(value)) => value,
            Ok(None) => continue,
            Err(e) if unsupported(&e) || e.kind() == io::ErrorKind::NotFound => {
                debug_event!("skipped extended attribute {name:?}: {e}");
                continue;
            }
            Err(e) => return Err(e),
        };
        if to.get_xattr(&name).ok().flatten().as_deref() == Some(value.as_slice()) {
            continue;
        }
        match to.set_xattr(&name, &value) {
            Err(e) if unsupported(&e) || permission_denied_is_tolerated(name.as_bytes(), &e) => {
                debug_event!("did not copy extended attribute {name:?}: {e}");
            }
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
        Err(e) if e.kind() == io::ErrorKind::Unsupported => {
            debug_event!("ACLs are not supported on {}: {e}", DisplayPath::new(temp));
            Ok(())
        }
        other => other,
    }
}

/// The ACL entries of the file `old` describes, or none when there is nothing trustworthy to read.
#[cfg(target_os = "macos")]
fn read_acl(old: &Existing, real: &Path) -> io::Result<Vec<exacl::AclEntry>> {
    if old.source.is_none() {
        debug_event!(
            "ACL of {} not copied: no source handle",
            DisplayPath::new(real)
        );
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
            debug_event!("ACL of {} not copied: {e}", DisplayPath::new(real));
            return Ok(Vec::new());
        }
        Err(e) => return Err(e),
    };
    let after = match fs::symlink_metadata(real) {
        Ok(after) => after,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            debug_event!("ACL of {} not copied: {e}", DisplayPath::new(real));
            return Ok(Vec::new());
        }
        Err(e) => return Err(e),
    };
    let same_inode = after.is_file() && Some(FileId::of_metadata(&after)) == old.id;
    if !same_inode {
        debug_event!(
            "ACL of {} not copied: the path no longer names the file",
            DisplayPath::new(real)
        );
    }
    Ok(if same_inode { entries } else { Vec::new() })
}

/// Opens the file `real` for reading its xattrs, if it is still the inode `metadata` describes.
#[cfg(unix)]
fn open_xattr_source(real: &Path, metadata: &fs::Metadata) -> io::Result<Option<fs::File>> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(NO_FOLLOW_OPEN.bits().cast_signed())
        .open(real)
    {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => return Ok(None),
        Err(e) => return Err(e),
    };
    let opened = file.metadata()?;
    if !opened.is_file() || FileId::of_metadata(&opened) != FileId::of_metadata(metadata) {
        return Err(not_the_file_read(real));
    }
    Ok(Some(file))
}

/// Opens `real` without following a reparse point, with only the access that reading its DACL
/// and attributes takes, and returns the file with the metadata of what was opened.
///
/// The returned metadata comes from the handle, so the permissions and the DACL describe one
/// file even if the path is swapped meanwhile. `BACKUP_SEMANTICS` lets a swapped-in directory
/// open and fail the `is_file` check with the same error as on Unix.
///
/// A denied open fails the write: continuing would silently replace the file with one that has
/// the directory's default DACL.
#[cfg(windows)]
fn open_dacl_source(real: &Path) -> io::Result<(fs::File, fs::Metadata)> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        READ_CONTROL,
    };

    let file = fs::OpenOptions::new()
        .access_mode(READ_CONTROL | FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(real)?;
    let opened = file.metadata()?;
    if !opened.is_file() {
        return Err(not_the_file_read(real));
    }
    Ok((file, opened))
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

/// The user and group that own a file.
#[cfg(unix)]
#[derive(Debug, Clone, Copy)]
struct Owner {
    uid: u32,
    gid: u32,
}

/// What an existing target has that its replacement must keep.
#[derive(Debug)]
struct Existing {
    permissions: Permissions,
    #[cfg(unix)]
    owner: Owner,
    /// Number of hard links.
    #[cfg(unix)]
    links: u64,
    /// Identity of the file the other fields describe, when the file system reports one.
    id: Option<FileId>,
    /// Handle on the old file, the source of its extended attributes (Unix) or DACL (Windows).
    #[cfg(any(unix, windows))]
    source: Option<fs::File>,
}

impl Existing {
    fn of(metadata: &fs::Metadata, id: Option<FileId>) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Self {
            id,
            permissions: metadata.permissions(),
            #[cfg(unix)]
            owner: Owner {
                uid: metadata.uid(),
                gid: metadata.gid(),
            },
            #[cfg(unix)]
            links: metadata.nlink(),
            #[cfg(any(unix, windows))]
            source: None,
        }
    }

    /// What `real`, a regular file described by `metadata`, has that a replacement must keep.
    #[cfg(unix)]
    fn open(real: &Path, metadata: &fs::Metadata) -> io::Result<Self> {
        Ok(Self {
            source: open_xattr_source(real, metadata)?,
            ..Self::of(metadata, Some(FileId::of_metadata(metadata)))
        })
    }

    /// The same on Windows, where the permissions come from the handle the DACL is read from.
    #[cfg(windows)]
    fn open(real: &Path, _path_metadata: &fs::Metadata) -> io::Result<Self> {
        let (source, opened) = open_dacl_source(real)?;
        let id = FileId::of(&source)?;
        Ok(Self {
            source: Some(source),
            ..Self::of(&opened, id)
        })
    }

    #[cfg(unix)]
    const fn hardlinked(&self) -> bool {
        self.links > 1
    }

    /// Whether the file belongs to the user that `process_file` (made by this process) has.
    #[cfg(unix)]
    fn owned_by(&self, process_file: &fs::Metadata) -> bool {
        use std::os::unix::fs::MetadataExt;
        self.owner.uid == process_file.uid()
    }
}

/// Resolves the real replacement target and what to preserve of it, if it exists.
///
/// With `expected`, the target must exist and be that file.
fn resolve_target(
    path: &Path,
    expected: Option<FileId>,
) -> io::Result<(PathBuf, Option<Existing>)> {
    let resolved = resolve_unbound(path)?;
    if let Some(expected) = expected
        && resolved
            .1
            .as_ref()
            .is_none_or(|old| old.id != Some(expected))
    {
        return Err(not_the_file_read(path));
    }
    Ok(resolved)
}

fn resolve_unbound(path: &Path) -> io::Result<(PathBuf, Option<Existing>)> {
    match fs::canonicalize(path) {
        Ok(real) => {
            let metadata = fs::metadata(&real)?;
            if !metadata.is_file() {
                return Err(AtomicWriteError::NotRegular {
                    path: path.to_path_buf(),
                }
                .into());
            }
            #[cfg(any(unix, windows))]
            let existing = Existing::open(&real, &metadata)?;
            #[cfg(not(any(unix, windows)))]
            let existing = Existing::of(&metadata, None);
            Ok((real, Some(existing)))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if fs::symlink_metadata(path).is_ok() {
                return Err(AtomicWriteError::DanglingSymlink {
                    path: path.to_path_buf(),
                }
                .into());
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

    fn id_of(path: &Path) -> Option<FileId> {
        FileId::of(&fs::File::open(path).unwrap()).unwrap()
    }

    fn bound(path: &Path, content: &[u8], expected: Option<FileId>) -> io::Result<()> {
        write_atomic_bound(path, content, HardLinkPolicy::WriteInPlace, expected)
    }

    fn is_target_changed(error: &io::Error) -> bool {
        matches!(
            error
                .get_ref()
                .and_then(|e| e.downcast_ref::<AtomicWriteError>()),
            Some(AtomicWriteError::TargetChanged(_))
        )
    }

    #[test]
    fn a_directory_target_is_refused_with_a_typed_payload() {
        let dir = tempfile::tempdir().unwrap();
        let err = write_atomic(dir.path(), b"x").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(matches!(
            err.get_ref()
                .and_then(|e| e.downcast_ref::<AtomicWriteError>()),
            Some(AtomicWriteError::NotRegular { .. })
        ));
    }

    #[test]
    fn of_path_matches_the_identity_of_an_open_handle() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.yaml");
        fs::write(&path, "a").unwrap();
        assert_eq!(FileId::of_path(&path).unwrap(), id_of(&path));
    }

    #[test]
    fn of_path_reports_a_missing_file_as_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let err = FileId::of_path(&dir.path().join("missing")).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn of_path_names_a_hard_link_as_its_target() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.yaml");
        fs::write(&path, "a").unwrap();
        let link = dir.path().join("b.yaml");
        fs::hard_link(&path, &link).unwrap();
        assert_eq!(
            FileId::of_path(&link).unwrap(),
            FileId::of_path(&path).unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn of_path_follows_a_symlink_to_its_target() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.yaml");
        fs::write(&path, "a").unwrap();
        let link = dir.path().join("link.yaml");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert_eq!(FileId::of_path(&link).unwrap(), id_of(&path));
    }

    #[test]
    fn bound_write_replaces_the_file_that_was_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.yaml");
        fs::write(&path, "old").unwrap();
        bound(&path, b"new", id_of(&path)).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn bound_write_refuses_a_file_renamed_over_the_target() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.yaml");
        let other = dir.path().join("other.yaml");
        fs::write(&path, "read").unwrap();
        let read = id_of(&path);
        fs::write(&other, "unrelated").unwrap();
        fs::rename(&other, &path).unwrap();

        let err = bound(&path, b"new", read).unwrap_err();

        assert!(is_target_changed(&err));
        assert_eq!(fs::read_to_string(&path).unwrap(), "unrelated");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn unbound_write_replaces_any_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.yaml");
        fs::write(&path, "old").unwrap();
        bound(&path, b"new", None).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn final_check_refuses_a_file_swapped_after_resolution() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.yaml");
        let other = dir.path().join("other.yaml");
        fs::write(&path, "read").unwrap();
        let read = id_of(&path);
        assert!(ensure_still_expected(&path, read).is_ok());
        fs::write(&other, "unrelated").unwrap();
        fs::rename(&other, &path).unwrap();
        assert!(is_target_changed(
            &ensure_still_expected(&path, read).unwrap_err()
        ));
        assert!(ensure_still_expected(&path, None).is_ok());
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn bound_write_refuses_a_deleted_target() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.yaml");
        fs::write(&path, "read").unwrap();
        let read = id_of(&path);
        fs::remove_file(&path).unwrap();

        assert!(is_target_changed(&bound(&path, b"new", read).unwrap_err()));
        assert!(!path.exists());
    }

    #[test]
    fn bound_write_follows_a_symlink_to_the_file_that_was_read() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.yaml");
        let link = dir.path().join("link.yaml");
        fs::write(&real, "old").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &link).unwrap();
        #[cfg(windows)]
        if std::os::windows::fs::symlink_file(&real, &link).is_err() {
            return;
        }
        #[cfg(not(any(unix, windows)))]
        return;
        bound(&link, b"new", id_of(&link)).unwrap();
        assert_eq!(fs::read_to_string(&real).unwrap(), "new");
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
            assert_eq!(err.kind(), io::ErrorKind::NotFound);
            assert!(matches!(
                err.get_ref()
                    .and_then(|e| e.downcast_ref::<AtomicWriteError>()),
                Some(AtomicWriteError::DanglingSymlink { .. })
            ));
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
            let Some(AtomicWriteError::HardLinkRefused(refused)) = err
                .get_ref()
                .and_then(|e| e.downcast_ref::<AtomicWriteError>())
            else {
                panic!("not HardLinkRefused: {err}");
            };
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
            let metadata = fs::metadata(path).unwrap();
            Existing::of(&metadata, Some(FileId::of_metadata(&metadata)))
        }

        #[test]
        fn in_place_write_refuses_a_replaced_inode() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("a.yaml");
            let other = dir.path().join("other.yaml");
            fs::write(&path, "old").unwrap();
            fs::write(&other, "other").unwrap();

            let err = write_in_place(&path, b"new", &existing_of(&other)).unwrap_err();

            assert!(is_target_changed(&err), "{err}");
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
            old.owner.uid = old.owner.uid.wrapping_add(1);
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
            old.owner = Owner {
                uid: old.owner.uid.wrapping_add(1),
                gid: old.owner.gid.wrapping_add(1),
            };
            let probe = tempfile::tempfile_in(dir.path()).unwrap();
            if std::os::unix::fs::fchown(&probe, None, Some(old.owner.gid)).is_ok() {
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
                owner: Owner {
                    uid: metadata.uid().wrapping_add(1),
                    gid: metadata.gid(),
                },
                links: 1,
                id: Some(FileId::of_metadata(&metadata)),
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
        // Windows PowerShell 5.1 cannot autoload modules when the runner's
        // PSModulePath points at PowerShell 7 module directories.
        let output = Command::new(program)
            .args(args)
            .env_remove("PSModulePath")
            .output()
            .unwrap();
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

    #[test]
    fn read_only_file_with_an_explicit_grant_survives_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let path = file_in(&dir);
        icacls(&path, &["/grant", "*S-1-1-0:(R)"]);
        set_read_only(&path, true).unwrap();
        let before = sddl(&path);
        write_atomic(&path, b"a: 2\n").unwrap();
        assert_eq!(sddl(&path), before);
        assert!(fs::metadata(&path).unwrap().permissions().readonly());
        set_read_only(&path, false).unwrap();
    }

    #[test]
    fn open_dacl_source_refuses_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let err = open_dacl_source(dir.path()).unwrap_err();
        assert!(err.to_string().contains("changed while"), "{err}");
    }

    #[test]
    fn open_dacl_source_does_not_follow_a_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let path = file_in(&dir);
        let link = dir.path().join("link.yaml");
        if let Err(e) = std::os::windows::fs::symlink_file(&path, &link) {
            // Creating symlinks takes a privilege that CI runners have
            assert!(
                std::env::var_os("CI").is_none(),
                "cannot create symlink: {e}"
            );
            return;
        }
        let err = open_dacl_source(&link).unwrap_err();
        assert!(err.to_string().contains("changed while"), "{err}");
    }

    const ZONE: &[u8] = b"[ZoneTransfer]\r\nZoneId=3\r\n";

    fn stream_of(path: &Path) -> io::Result<Vec<u8>> {
        fs::read(zone_identifier_path(path))
    }

    #[test]
    fn zone_identifier_survives_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let path = file_in(&dir);
        fs::write(zone_identifier_path(&path), ZONE).unwrap();

        write_atomic(&path, b"a: 2\n").unwrap();

        assert_eq!(stream_of(&path).unwrap(), ZONE);
        assert_eq!(fs::read_to_string(&path).unwrap(), "a: 2\n");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn zone_identifier_survives_replacement_of_a_read_only_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = file_in(&dir);
        fs::write(zone_identifier_path(&path), ZONE).unwrap();
        set_read_only(&path, true).unwrap();

        write_atomic(&path, b"a: 2\n").unwrap();

        assert_eq!(stream_of(&path).unwrap(), ZONE);
        assert!(fs::metadata(&path).unwrap().permissions().readonly());
    }

    #[test]
    fn file_without_zone_identifier_gets_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = file_in(&dir);

        write_atomic(&path, b"a: 2\n").unwrap();

        assert_eq!(
            stream_of(&path).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn zone_identifier_of_a_replaced_file_is_not_copied() {
        let dir = tempfile::tempdir().unwrap();
        let path = file_in(&dir);
        fs::write(zone_identifier_path(&path), ZONE).unwrap();
        let old = Existing::open(&path, &fs::metadata(&path).unwrap()).unwrap();
        let other = dir.path().join("other.yaml");
        fs::write(&other, "b: 1\n").unwrap();
        fs::remove_file(&path).unwrap();
        fs::rename(&other, &path).unwrap();
        fs::write(
            zone_identifier_path(&path),
            b"[ZoneTransfer]\r\nZoneId=4\r\n",
        )
        .unwrap();
        let temp = dir.path().join("temp.tmp");
        fs::write(&temp, "").unwrap();

        copy_zone_identifier(&old, &path, &temp).unwrap();

        assert_eq!(
            stream_of(&temp).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn file_id_tells_files_apart_and_names_one_file_by_any_path() {
        let dir = tempfile::tempdir().unwrap();
        let a = file_in(&dir);
        let b = dir.path().join("b.yaml");
        fs::write(&b, "b: 1\n").unwrap();
        let spelled = dir.path().join(".").join("acl.yaml");

        let id = |path: &Path| FileId::of(&fs::File::open(path).unwrap()).unwrap().unwrap();

        assert_eq!(id(&a), id(&spelled));
        assert_ne!(id(&a), id(&b));
    }
}
