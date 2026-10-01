//! Bounded reading of YAML files into memory.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use fast_yaml_core::decode_input_owned;
use fast_yaml_core::limits::MaxInputBytes;

use crate::error::{Error, Result};

/// Reads a YAML file into an owned string, decoding it and enforcing `max`.
///
/// The file is opened once and its size is checked against `max` before any content is read.
/// The read is additionally capped at `max + 1` bytes, so a file that grows after the size
/// check is still rejected. The bytes are a snapshot: nothing observes later changes to the
/// file.
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
    let io_error = |source| Error::Io {
        path: path.to_path_buf(),
        source,
    };
    // Checked before opening: opening a FIFO blocks and opening a directory fails on Windows
    ensure_regular_file(&std::fs::metadata(path).map_err(io_error)?).map_err(io_error)?;
    let file = File::open(path).map_err(io_error)?;
    let metadata = file.metadata().map_err(io_error)?;
    ensure_regular_file(&metadata).map_err(io_error)?;

    let size = metadata.len();
    max.check_file_len(size)?;

    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or_default());
    file.take(max.get() as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    max.check(bytes.len())?;
    decode_input_owned(bytes).map_err(|source| Error::Decode {
        path: path.to_path_buf(),
        source,
    })
}

/// Rejects directories, FIFOs and devices, whose length says nothing about how much they yield.
fn ensure_regular_file(metadata: &std::fs::Metadata) -> std::io::Result<()> {
    let message = if metadata.is_dir() {
        "path is a directory, not a file"
    } else if !metadata.is_file() {
        "path is not a regular file"
    } else {
        return Ok(());
    };
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        message,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

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
