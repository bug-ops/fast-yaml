//! Secure atomic file replacement shared by the CLI and batch file processing.

use std::fs::{self, Permissions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

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
///
/// - If the owner or group cannot be restored, the replacement never becomes more readable
///   than the original: without the group, the group and other permission bits are dropped.
///
/// Limitations:
/// - A Unix target with several hard links that the caller owns is **not** replaced
///   atomically: the content is written in place so every link sees it. The file is opened
///   with `O_NOFOLLOW` and must still be the inode that was resolved; a crash or error
///   mid-write can leave it truncated or partial, and a read-only hard-linked file is refused.
///   A hard-linked target owned by someone else is replaced through the rename instead, which
///   breaks the link rather than writing into a file the caller may not control.
/// - Extended attributes and ACLs of the old file are not preserved.
///
/// # Errors
///
/// Returns an I/O error if the target cannot be resolved, is not a regular file, or the
/// temporary file cannot be created, written, or renamed into place. No temporary file is
/// left behind on failure.
///
/// # Examples
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("config.yaml");
///
/// fast_yaml_parallel::write_atomic(&path, b"key: value\n")?;
/// assert_eq!(std::fs::read_to_string(&path)?, "key: value\n");
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn write_atomic(path: &Path, content: &[u8]) -> io::Result<()> {
    let mut file = AtomicFile::create(path)?;
    #[cfg(unix)]
    if let Some(old) = file.existing.as_ref().filter(|old| old.hardlinked())
        && old.owned_by(&file.temp.as_file().metadata()?)
    {
        return write_in_place(&file.target, content, old);
    }
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
                format!("cannot create temporary file in {}: {e}", dir.display()),
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
            let permissions = restore_owner(temp.as_file(), old)?;
            #[cfg(not(unix))]
            let permissions = old.permissions.clone();
            temp.as_file().set_permissions(permissions)?;
        }
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
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(handle) = fs::File::open(dir) {
        let _ = handle.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}

/// Overwrites a target that other names share, so every hard link observes the new content.
///
/// The target is opened without truncation and without following a symlink, and written only
/// if it is still the inode `old` was read from.
#[cfg(unix)]
fn write_in_place(target: &Path, content: &[u8], old: &Existing) -> io::Result<()> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    let mut file = fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(target)?;
    let opened = file.metadata()?;
    if !opened.is_file() || (opened.dev(), opened.ino(), opened.nlink()) != old.identity {
        return Err(io::Error::other(format!(
            "{} changed while it was being replaced",
            target.display()
        )));
    }
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
                    format!("not a regular file: {}", path.display()),
                ));
            }
            Ok((real, Some(Existing::of(&metadata))))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if fs::symlink_metadata(path).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("refused: dangling symlink {}", path.display()),
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
            };
            let temp = tempfile::tempfile_in(dir.path()).unwrap();
            // Unprivileged: EPERM is ignored; privileged: the chown simply succeeds
            assert!(restore_owner(&temp, &old).is_ok());
        }
    }
}
