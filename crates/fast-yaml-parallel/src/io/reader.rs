//! Bounded reading of YAML files into memory.

use std::fs::File;
use std::io::ErrorKind;
use std::path::Path;

use fast_yaml_core::decode_input_owned;
use fast_yaml_core::fs::{ReadFileError, read_regular_file_with_handle};
use fast_yaml_core::limits::MaxInputBytes;

use crate::atomic::FileId;
use crate::error::{Error, Result};

/// Reads a YAML file into an owned string, decoding it and enforcing `max`.
///
/// Reading is done by [`read_regular_file`](fast_yaml_core::fs::read_regular_file), which
/// checks the size against `max` before any content is read and never blocks on a FIFO or
/// device; see its threat model.
///
/// # Errors
///
/// Returns [`Error::Io`] if the path does not exist, is a directory, FIFO or device rather
/// than a regular file, or cannot be read.
///
/// Returns [`Error::InputTooLarge`] if the file is larger than `max`.
///
/// Returns [`Error::Decode`] if the file starts with a UTF-16 or UTF-32 byte order mark or is
/// not valid UTF-8.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::limits::MaxInputBytes;
/// use fast_yaml_parallel::{Error, read_file};
///
/// # let temp_file = tempfile::NamedTempFile::new().unwrap();
/// # std::fs::write(temp_file.path(), "key: value\n").unwrap();
/// assert_eq!(read_file(temp_file.path(), MaxInputBytes::DEFAULT)?, "key: value\n");
///
/// let max = MaxInputBytes::new(4).unwrap();
/// assert!(matches!(
///     read_file(temp_file.path(), max),
///     Err(Error::InputTooLarge(_))
/// ));
/// # Ok::<(), Error>(())
/// ```
pub fn read_file(path: &Path, max: MaxInputBytes) -> Result<String> {
    read_decoded(path, max).map(|(content, _)| content)
}

/// [`read_file`] that also returns the [`FileId`] of the file the content was read from, or
/// `None` when the platform or file system reports no identity.
///
/// Pass the identity to [`write_atomic_bound`](crate::write_atomic_bound) to write back only to
/// that file.
///
/// # Errors
///
/// The errors of [`read_file`], and [`Error::Io`] if the identity of the opened file cannot be
/// read.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::limits::MaxInputBytes;
/// use fast_yaml_parallel::{HardLinkPolicy, read_file_identified, write_atomic_bound};
///
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("a.yaml");
/// std::fs::write(&path, "a: 1\n")?;
///
/// let (content, id) = read_file_identified(&path, MaxInputBytes::DEFAULT)?;
/// write_atomic_bound(&path, content.to_uppercase().as_bytes(), HardLinkPolicy::WriteInPlace, id)?;
/// assert_eq!(std::fs::read_to_string(&path)?, "A: 1\n");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn read_file_identified(path: &Path, max: MaxInputBytes) -> Result<(String, Option<FileId>)> {
    let (content, file) = read_decoded(path, max)?;
    let id = FileId::of(&file).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok((content, id))
}

fn read_decoded(path: &Path, max: MaxInputBytes) -> Result<(String, File)> {
    let io_error = |source| Error::Io {
        path: path.to_path_buf(),
        source,
    };
    let (bytes, file) = read_regular_file_with_handle(path, max).map_err(|error| match error {
        ReadFileError::Io(source) => io_error(source),
        ReadFileError::NotRegular(kind) => io_error(std::io::Error::new(
            ErrorKind::InvalidInput,
            ReadFileError::NotRegular(kind),
        )),
        ReadFileError::TooLarge(too_large) => Error::InputTooLarge(too_large),
        other => io_error(std::io::Error::other(other)),
    })?;
    let content = decode_input_owned(bytes).map_err(|source| Error::Decode {
        path: path.to_path_buf(),
        source,
    })?;
    Ok((content, file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_read_identified_returns_the_id_of_the_file_read() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "a: 1").unwrap();

        let (content, id) = read_file_identified(file.path(), MaxInputBytes::DEFAULT).unwrap();

        assert_eq!(content, "a: 1");
        assert_eq!(id, FileId::of(&File::open(file.path()).unwrap()).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn test_read_identified_through_a_symlink_returns_the_target_id() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.yaml");
        let link = dir.path().join("link.yaml");
        std::fs::write(&target, "a: 1").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let (_, id) = read_file_identified(&link, MaxInputBytes::DEFAULT).unwrap();

        assert!(id.is_some());
        assert_eq!(id, FileId::of(&File::open(&target).unwrap()).unwrap());
    }

    fn read(file: &NamedTempFile) -> Result<String> {
        read_file(file.path(), MaxInputBytes::DEFAULT)
    }

    #[test]
    fn test_read_small_file() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "small: content").unwrap();
        assert_eq!(read(&file).unwrap(), "small: content");
    }

    #[test]
    fn test_read_large_file() {
        let mut file = NamedTempFile::new().unwrap();
        let content = "valid: utf8 content\n".repeat(30_000);
        write!(file, "{content}").unwrap();
        assert_eq!(read(&file).unwrap(), content);
    }

    #[test]
    fn test_read_nonexistent_file() {
        let result = read_file(Path::new("/nonexistent/file.yaml"), MaxInputBytes::DEFAULT);
        assert!(matches!(result, Err(Error::Io { .. })));
    }

    #[test]
    fn test_zero_length_file() {
        let file = NamedTempFile::new().unwrap();
        assert_eq!(read(&file).unwrap(), "");
    }

    const BOMS: [(&[u8], &str); 4] = [
        (&[0xFF, 0xFE, b'a', 0x00], "UTF-16LE"),
        (&[0xFE, 0xFF, 0x00, b'a'], "UTF-16BE"),
        (&[0xFF, 0xFE, 0x00, 0x00, b'a', 0, 0, 0], "UTF-32LE"),
        (&[0x00, 0x00, 0xFE, 0xFF, 0, 0, 0, b'a'], "UTF-32BE"),
    ];

    #[test]
    fn test_file_with_unsupported_bom_fails() {
        for (bom, name) in BOMS {
            let mut file = NamedTempFile::new().unwrap();
            file.write_all(bom).unwrap();

            let err = read(&file).unwrap_err();
            assert!(matches!(err, Error::Decode { .. }), "{err:?}");
            let text = err.to_string();
            assert!(text.contains("unsupported encoding"), "{text}");
            assert!(text.contains(name), "{text}");
            let Error::Decode { path, .. } = err else {
                unreachable!()
            };
            assert_eq!(path, file.path());
        }
    }

    #[test]
    fn test_large_file_with_unsupported_bom_fails() {
        for (bom, name) in BOMS {
            let mut file = NamedTempFile::new().unwrap();
            file.write_all(bom).unwrap();
            file.write_all(&vec![b'x'; 600 * 1024]).unwrap();

            let err = read(&file).unwrap_err();
            assert!(matches!(err, Error::Decode { .. }), "{err:?}");
            assert!(err.to_string().contains(name), "{err}");
        }
    }

    #[test]
    fn test_invalid_utf8_is_not_reported_as_encoding() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"a: \xC3\x28\n").unwrap();

        let err = read(&file).unwrap_err();
        assert!(err.to_string().contains("not valid UTF-8"), "{err}");
    }

    #[test]
    fn test_utf8_bom_is_accepted() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"\xEF\xBB\xBFa: 1\n").unwrap();
        assert!(read(&file).is_ok());
    }

    #[test]
    #[cfg(unix)]
    fn test_symlink_handling() {
        use std::os::unix::fs::symlink;

        let temp_dir = tempfile::tempdir().unwrap();
        let target = temp_dir.path().join("target.yaml");
        let link = temp_dir.path().join("link.yaml");
        std::fs::write(&target, "key: value\n").unwrap();
        symlink(&target, &link).unwrap();

        assert_eq!(
            read_file(&link, MaxInputBytes::DEFAULT).unwrap(),
            "key: value\n"
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_broken_symlink_error() {
        use std::os::unix::fs::symlink;

        let temp_dir = tempfile::tempdir().unwrap();
        let link = temp_dir.path().join("broken_link.yaml");
        symlink(temp_dir.path().join("nonexistent.yaml"), &link).unwrap();

        assert!(read_file(&link, MaxInputBytes::DEFAULT).is_err());
    }

    #[test]
    fn test_directory_instead_of_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let result = read_file(temp_dir.path(), MaxInputBytes::DEFAULT);
        assert!(matches!(result, Err(Error::Io { .. })));
    }

    #[test]
    #[cfg(unix)]
    fn test_directory_symlink_rejection() {
        use std::os::unix::fs::symlink;

        let temp_dir = tempfile::tempdir().unwrap();
        let target_dir = temp_dir.path().join("target_dir");
        let link = temp_dir.path().join("dir_link");
        std::fs::create_dir(&target_dir).unwrap();
        symlink(&target_dir, &link).unwrap();

        match read_file(&link, MaxInputBytes::DEFAULT) {
            Err(Error::Io { source, .. }) => {
                assert_eq!(source.kind(), std::io::ErrorKind::InvalidInput);
            }
            other => panic!("expected Io error, got {other:?}"),
        }
    }

    #[test]
    fn test_read_of_a_directory_carries_the_typed_not_regular_payload() {
        use fast_yaml_core::fs::{NotRegularKind, ReadFileError};

        let temp_dir = tempfile::tempdir().unwrap();

        let Err(Error::Io { source, .. }) = read_file(temp_dir.path(), MaxInputBytes::DEFAULT)
        else {
            panic!("expected an Io error");
        };

        assert_eq!(source.kind(), std::io::ErrorKind::InvalidInput);
        assert!(matches!(
            source
                .get_ref()
                .and_then(|e| e.downcast_ref::<ReadFileError>()),
            Some(ReadFileError::NotRegular(NotRegularKind::Directory))
        ));
    }

    #[test]
    fn test_read_rejects_file_above_limit_before_reading() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "key: value").unwrap();
        let max = MaxInputBytes::new(9).unwrap();

        let err = read_file(file.path(), max).unwrap_err();
        assert!(
            matches!(&err, Error::InputTooLarge(e) if e.size == 10 && e.limit == max),
            "{err:?}"
        );
    }

    #[test]
    fn test_read_accepts_file_at_limit() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "key: value").unwrap();
        let max = MaxInputBytes::new(10).unwrap();
        assert_eq!(read_file(file.path(), max).unwrap(), "key: value");
    }

    #[cfg(unix)]
    #[test]
    fn test_read_rejects_fifo_without_blocking() {
        let dir = tempfile::TempDir::new().unwrap();
        let fifo = dir.path().join("pipe.yaml");
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap();
        assert!(status.success());

        let err = read_file(&fifo, MaxInputBytes::DEFAULT).unwrap_err();
        assert!(err.to_string().contains("not a regular file"), "{err}");
    }
}
