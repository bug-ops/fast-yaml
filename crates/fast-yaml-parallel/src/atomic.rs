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
/// Limitations:
/// - A Unix target with several hard links is **not** replaced atomically: the content is
///   written in place so every link sees it. A crash or error mid-write can leave it
///   truncated or partial, and a read-only hard-linked file is refused.
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
    let (target, existing) = resolve_target(path)?;

    #[cfg(unix)]
    if existing.as_ref().is_some_and(|old| old.hardlinked) {
        return write_in_place(&target, content);
    }

    let dir = match target.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };

    let mut temp = create_temp(dir, existing.is_none()).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("cannot create temporary file in {}: {e}", dir.display()),
        )
    })?;
    temp.write_all(content)?;
    let read_only = existing
        .as_ref()
        .is_some_and(|old| old.permissions.readonly());
    if let Some(old) = &existing {
        // Before chmod: changing the owner can clear mode bits
        #[cfg(unix)]
        restore_owner(temp.as_file(), old)?;
        temp.as_file().set_permissions(old.permissions.clone())?;
    }
    temp.as_file().sync_all()?;
    // std's rename replaces files that are open elsewhere on Windows; `persist` does not.
    let temp_path = temp.into_temp_path();
    replace(&temp_path, &target, read_only)?;
    let _ = temp_path.keep();
    sync_dir(dir);
    Ok(())
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
#[cfg(unix)]
fn write_in_place(target: &Path, content: &[u8]) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(target)?;
    file.write_all(content)?;
    file.sync_all()
}

/// Gives `file` the owner of the file it replaces; a process without the right keeps its own.
#[cfg(unix)]
fn restore_owner(file: &fs::File, old: &Existing) -> io::Result<()> {
    match std::os::unix::fs::fchown(file, Some(old.uid), Some(old.gid)) {
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => Ok(()),
        other => other,
    }
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
struct Existing {
    permissions: Permissions,
    #[cfg(unix)]
    uid: u32,
    #[cfg(unix)]
    gid: u32,
    #[cfg(unix)]
    hardlinked: bool,
}

impl Existing {
    fn of(metadata: &fs::Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Self {
            permissions: metadata.permissions(),
            #[cfg(unix)]
            uid: metadata.uid(),
            #[cfg(unix)]
            gid: metadata.gid(),
            #[cfg(unix)]
            hardlinked: metadata.nlink() > 1,
        }
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

        #[test]
        fn failing_to_assign_the_owner_is_not_an_error() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("a.yaml");
            fs::write(&path, "old").unwrap();
            let metadata = fs::metadata(&path).unwrap();
            let old = Existing {
                permissions: metadata.permissions(),
                uid: metadata.uid().wrapping_add(1),
                gid: metadata.gid(),
                hardlinked: false,
            };
            let temp = tempfile::tempfile_in(dir.path()).unwrap();
            // Unprivileged: EPERM is ignored; privileged: the chown simply succeeds
            assert!(restore_owner(&temp, &old).is_ok());
        }
    }
}
