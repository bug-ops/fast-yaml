//! Secure atomic file replacement shared by the CLI and batch file processing.

use std::fs::{self, Permissions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Atomically replaces the contents of `path` with `content`.
///
/// The data is written to an unpredictably named temporary file created with `O_EXCL`
/// in the destination directory and then renamed over the target. This avoids following
/// planted `<name>.tmp` symlinks and never touches unrelated sibling files.
///
/// Behavior guarantees:
/// - An existing target keeps its permission bits (including the executable bit).
/// - A symlink target is resolved and the real file is replaced; the link itself is preserved.
/// - A dangling symlink or a non-regular target (directory, FIFO, device) is refused.
/// - A missing target is created like a regular new file: mode `0o666` filtered by the
///   process umask on Unix.
///
/// Limitations:
/// - Hard links to the target are broken: the rename creates a new inode.
/// - Owner, group, extended attributes, and ACLs of the old file are not preserved.
/// - The data is not `fsync`ed before the rename.
/// - On Unix, a read-only (`0o444`) target is still replaced when its directory is writable,
///   and stays read-only afterwards; on Windows, renaming over a read-only file fails.
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
    if let Some(permissions) = existing {
        temp.as_file().set_permissions(permissions)?;
    }
    // std's rename replaces files that are open elsewhere on Windows; `persist` does not.
    let temp_path = temp.into_temp_path();
    fs::rename(&temp_path, &target)?;
    let _ = temp_path.keep();
    Ok(())
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

/// Resolves the real replacement target and the permissions to preserve, if it exists.
fn resolve_target(path: &Path) -> io::Result<(PathBuf, Option<Permissions>)> {
    match fs::canonicalize(path) {
        Ok(real) => {
            let metadata = fs::metadata(&real)?;
            if !metadata.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("not a regular file: {}", path.display()),
                ));
            }
            Ok((real, Some(metadata.permissions())))
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

    #[cfg(unix)]
    mod unix {
        use super::*;
        use std::os::unix::fs::{PermissionsExt, symlink};

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
    }
}
