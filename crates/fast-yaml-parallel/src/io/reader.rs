//! Smart file reading with automatic strategy selection based on file size.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use fast_yaml_core::limits::MaxInputBytes;
use fast_yaml_core::{decode_input, decode_input_owned};
use memmap2::Mmap;

use crate::error::{Error, Result};

/// Memory-map threshold constant: 512KB
const MMAP_THRESHOLD: u64 = 512 * 1024;

/// File content holder that abstracts over in-memory strings and memory-mapped files.
#[derive(Debug)]
pub enum FileContent {
    /// Content loaded into memory as a String
    String(String),
    /// Content accessed via memory-mapped file
    Mmap {
        /// The mapped file bytes.
        map: Mmap,
        /// Path the file was mapped from, used in decode errors.
        path: PathBuf,
    },
}

impl FileContent {
    /// Returns the content as a string slice.
    ///
    /// For String variant, returns the string directly.
    /// For Mmap variant, decodes the bytes first.
    pub fn as_str(&self) -> Result<&str> {
        match self {
            Self::String(s) => Ok(s),
            Self::Mmap { map, path } => decode_input(map).map_err(|source| Error::Decode {
                path: path.clone(),
                source,
            }),
        }
    }

    /// Consumes the content and returns it as an owned string.
    ///
    /// A string read is returned as is; a mapped file is decoded and copied.
    ///
    /// # Errors
    ///
    /// Returns `Error::Decode` if a mapped file is not valid UTF-8 text.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_parallel::FileContent;
    ///
    /// let content = FileContent::String("key: value\n".to_string());
    /// assert_eq!(content.into_string()?, "key: value\n");
    /// # Ok::<(), fast_yaml_parallel::Error>(())
    /// ```
    pub fn into_string(self) -> Result<String> {
        match self {
            Self::String(s) => Ok(s),
            mapped @ Self::Mmap { .. } => mapped.as_str().map(str::to_owned),
        }
    }

    /// Returns true if content is memory-mapped
    pub const fn is_mmap(&self) -> bool {
        matches!(self, Self::Mmap { .. })
    }

    /// Returns the size of the content in bytes
    pub fn len(&self) -> usize {
        match self {
            Self::String(s) => s.len(),
            Self::Mmap { map, .. } => map.len(),
        }
    }

    /// Returns true if the content is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Smart file reader that chooses optimal reading strategy based on file size.
///
/// For files smaller than the threshold, reads the bytes into memory and decodes them.
/// For larger files, uses memory-mapped files to avoid loading entire content into heap.
#[derive(Debug)]
pub struct SmartReader {
    mmap_threshold: u64,
}

impl SmartReader {
    /// Creates a new `SmartReader` with the default threshold (512KB).
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::limits::MaxInputBytes;
    /// use fast_yaml_parallel::SmartReader;
    ///
    /// let reader = SmartReader::new();
    /// # let temp_file = tempfile::NamedTempFile::new().unwrap();
    /// # std::fs::write(temp_file.path(), "key: value\n").unwrap();
    /// let content = reader.read(temp_file.path(), MaxInputBytes::DEFAULT)?;
    /// let yaml = content.as_str()?;
    /// assert!(yaml.contains("key"));
    /// # Ok::<(), fast_yaml_parallel::Error>(())
    /// ```
    pub const fn new() -> Self {
        Self::with_threshold(MMAP_THRESHOLD)
    }

    /// Creates a new `SmartReader` with a custom threshold.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_parallel::SmartReader;
    ///
    /// // Use mmap for files larger than 1MB
    /// let reader = SmartReader::with_threshold(1024 * 1024);
    /// ```
    pub const fn with_threshold(threshold: u64) -> Self {
        Self {
            mmap_threshold: threshold,
        }
    }

    /// Reads file content using the optimal strategy based on file size.
    ///
    /// Returns `FileContent` and automatically chooses between:
    /// - an in-memory read for files < threshold
    /// - `mmap` for files >= threshold
    ///
    /// Falls back to an in-memory read if mmap fails. The file is opened once and its size is
    /// checked against `max` before any content is read; the in-memory read is additionally
    /// capped at `max + 1` bytes and a mapping is re-measured, so a file that grows after the
    /// size check is still rejected.
    ///
    /// # Errors
    ///
    /// Returns `Error::Io` if:
    /// - Path does not exist
    /// - Path is a directory, FIFO or device rather than a regular file
    /// - Insufficient permissions
    ///
    /// Returns `Error::InputTooLarge` if the file is larger than `max`.
    ///
    /// Returns `Error::Decode` if a file below the threshold starts with a UTF-16 or
    /// UTF-32 byte order mark or is not valid UTF-8. Memory-mapped files report the
    /// same error from [`FileContent::as_str`].
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::limits::MaxInputBytes;
    /// use fast_yaml_parallel::{Error, SmartReader};
    ///
    /// # let temp_file = tempfile::NamedTempFile::new().unwrap();
    /// # std::fs::write(temp_file.path(), "key: value\n").unwrap();
    /// let max = MaxInputBytes::new(4).unwrap();
    /// assert!(matches!(
    ///     SmartReader::new().read(temp_file.path(), max),
    ///     Err(Error::InputTooLarge(_))
    /// ));
    /// ```
    pub fn read(&self, path: &Path, max: MaxInputBytes) -> Result<FileContent> {
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

        if size >= self.mmap_threshold {
            match Self::read_mmap(&file, path, max) {
                Err(e @ Error::InputTooLarge(_)) => Err(e),
                // Fallback to reading into memory if mmap fails
                Err(_) => Self::read_string(&file, path, size, max),
                Ok(content) => Ok(content),
            }
        } else {
            Self::read_string(&file, path, size, max)
        }
    }

    /// Reads file into memory as a String, reading at most one byte past `max`
    fn read_string(file: &File, path: &Path, size: u64, max: MaxInputBytes) -> Result<FileContent> {
        let io_error = |source| Error::Io {
            path: path.to_path_buf(),
            source,
        };
        let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or_default());
        file.take(max.get() as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(io_error)?;
        max.check(bytes.len())?;
        let content = decode_input_owned(bytes).map_err(|source| Error::Decode {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(FileContent::String(content))
    }

    /// Maps the file read-only and rejects a mapping larger than `max`
    #[allow(unsafe_code)]
    fn read_mmap(file: &File, path: &Path, max: MaxInputBytes) -> Result<FileContent> {
        // SAFETY: read-only mapping, unmapped on drop. Memory safety still depends on no process
        // truncating or rewriting the file while it is mapped: truncation raises SIGBUS, and a
        // rewrite after `as_str` validated UTF-8 invalidates the `&str` the parser reads.
        // Unlike `read_string`, which snapshots the bytes, this is a real race. Callers accept it
        // for files of at least `mmap_threshold` bytes and can raise the threshold to avoid it.
        let mmap = unsafe {
            Mmap::map(file).map_err(|source| Error::Io {
                path: path.to_path_buf(),
                source,
            })?
        };
        max.check(mmap.len())?;

        Ok(FileContent::Mmap {
            map: mmap,
            path: path.to_path_buf(),
        })
    }
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

impl Default for SmartReader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_file_content_as_str_string() {
        let content = FileContent::String("test content".to_string());
        assert_eq!(content.as_str().unwrap(), "test content");
        assert!(!content.is_mmap());
        assert_eq!(content.len(), 12);
        assert!(!content.is_empty());
    }

    #[test]
    fn test_file_content_is_empty() {
        let content = FileContent::String(String::new());
        assert!(content.is_empty());
    }

    #[test]
    fn test_reader_small_file_uses_string() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "small: content").unwrap();

        let reader = SmartReader::new();
        let content = reader.read(file.path(), MaxInputBytes::DEFAULT).unwrap();

        assert!(!content.is_mmap());
        assert_eq!(content.as_str().unwrap(), "small: content");
    }

    #[test]
    fn test_reader_large_file_uses_mmap() {
        let mut file = NamedTempFile::new().unwrap();

        // Write content larger than 512KB threshold
        let large_content = "x".repeat(600 * 1024);
        write!(file, "{large_content}").unwrap();

        let reader = SmartReader::new();
        let content = reader.read(file.path(), MaxInputBytes::DEFAULT).unwrap();

        assert!(content.is_mmap());
        assert_eq!(content.len(), large_content.len());
    }

    #[test]
    fn test_reader_custom_threshold() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "test content").unwrap();

        // Threshold of 5 bytes should trigger mmap for our 12-byte file
        let reader = SmartReader::with_threshold(5);
        let content = reader.read(file.path(), MaxInputBytes::DEFAULT).unwrap();

        // Should use mmap since file > 5 bytes
        assert!(content.is_mmap());
    }

    #[test]
    fn test_reader_default_equals_new() {
        let reader1 = SmartReader::new();
        let reader2 = SmartReader::default();

        assert_eq!(reader1.mmap_threshold, reader2.mmap_threshold);
    }

    #[test]
    fn test_read_nonexistent_file() {
        let reader = SmartReader::new();
        let result = reader.read(Path::new("/nonexistent/file.yaml"), MaxInputBytes::DEFAULT);
        assert!(result.is_err());
    }

    #[test]
    fn test_file_content_len() {
        let content = FileContent::String("hello".to_string());
        assert_eq!(content.len(), 5);
    }

    #[test]
    fn test_read_utf8_validation_with_mmap() {
        let mut file = NamedTempFile::new().unwrap();

        // Write valid UTF-8 content larger than threshold
        let content = "valid: utf8 content\n".repeat(30_000);
        write!(file, "{content}").unwrap();

        let reader = SmartReader::new();
        let file_content = reader.read(file.path(), MaxInputBytes::DEFAULT).unwrap();

        // Should be mmap and valid UTF-8
        assert!(file_content.is_mmap());
        assert!(file_content.as_str().is_ok());
    }

    const BOMS: [(&[u8], &str); 4] = [
        (&[0xFF, 0xFE, b'a', 0x00], "UTF-16LE"),
        (&[0xFE, 0xFF, 0x00, b'a'], "UTF-16BE"),
        (&[0xFF, 0xFE, 0x00, 0x00, b'a', 0, 0, 0], "UTF-32LE"),
        (&[0x00, 0x00, 0xFE, 0xFF, 0, 0, 0, b'a'], "UTF-32BE"),
    ];

    #[test]
    fn test_small_file_with_unsupported_bom_fails() {
        for (bom, name) in BOMS {
            let mut file = NamedTempFile::new().unwrap();
            file.write_all(bom).unwrap();

            let err = SmartReader::new()
                .read(file.path(), MaxInputBytes::DEFAULT)
                .unwrap_err();
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
    fn test_mmap_file_with_unsupported_bom_fails() {
        for (bom, name) in BOMS {
            let mut file = NamedTempFile::new().unwrap();
            file.write_all(bom).unwrap();
            file.write_all(&vec![b'x'; 600 * 1024]).unwrap();

            let content = SmartReader::new()
                .read(file.path(), MaxInputBytes::DEFAULT)
                .unwrap();
            assert!(content.is_mmap());
            let err = content.as_str().unwrap_err();
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
    fn test_invalid_utf8_is_not_reported_as_encoding() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"a: \xC3\x28\n").unwrap();

        let err = SmartReader::new()
            .read(file.path(), MaxInputBytes::DEFAULT)
            .unwrap_err();
        assert!(err.to_string().contains("not valid UTF-8"), "{err}");
    }

    #[test]
    fn test_utf8_bom_is_accepted() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"\xEF\xBB\xBFa: 1\n").unwrap();

        let content = SmartReader::new()
            .read(file.path(), MaxInputBytes::DEFAULT)
            .unwrap();
        assert!(content.as_str().is_ok());
    }

    #[test]
    #[cfg(unix)]
    fn test_symlink_handling() {
        use std::os::unix::fs::symlink;

        let temp_dir = tempfile::tempdir().unwrap();
        let target = temp_dir.path().join("target.yaml");
        let link = temp_dir.path().join("link.yaml");

        // Create target file
        std::fs::write(&target, "key: value\n").unwrap();

        // Create symlink
        symlink(&target, &link).unwrap();

        // Reader should follow symlink and read content
        let reader = SmartReader::new();
        let content = reader.read(&link, MaxInputBytes::DEFAULT).unwrap();

        assert_eq!(content.as_str().unwrap(), "key: value\n");
    }

    #[test]
    #[cfg(unix)]
    fn test_broken_symlink_error() {
        use std::os::unix::fs::symlink;

        let temp_dir = tempfile::tempdir().unwrap();
        let nonexistent = temp_dir.path().join("nonexistent.yaml");
        let link = temp_dir.path().join("broken_link.yaml");

        // Create symlink to nonexistent file
        symlink(&nonexistent, &link).unwrap();

        // Reading broken symlink should fail
        let reader = SmartReader::new();
        let result = reader.read(&link, MaxInputBytes::DEFAULT);

        assert!(result.is_err());
    }

    #[test]
    fn test_file_exactly_at_threshold() {
        let mut file = NamedTempFile::new().unwrap();

        // Write exactly 512KB
        let content = "x".repeat(512 * 1024);
        write!(file, "{content}").unwrap();

        let reader = SmartReader::new();
        let file_content = reader.read(file.path(), MaxInputBytes::DEFAULT).unwrap();

        // At threshold, should use mmap
        assert!(file_content.is_mmap());
        assert_eq!(file_content.len(), 512 * 1024);
    }

    #[test]
    fn test_file_just_below_threshold() {
        let mut file = NamedTempFile::new().unwrap();

        // Write 512KB - 1 byte
        let content = "x".repeat(512 * 1024 - 1);
        write!(file, "{content}").unwrap();

        let reader = SmartReader::new();
        let file_content = reader.read(file.path(), MaxInputBytes::DEFAULT).unwrap();

        // Below threshold, should use String
        assert!(!file_content.is_mmap());
        assert_eq!(file_content.len(), 512 * 1024 - 1);
    }

    #[test]
    fn test_file_just_above_threshold() {
        let mut file = NamedTempFile::new().unwrap();

        // Write 512KB + 1 byte
        let content = "x".repeat(512 * 1024 + 1);
        write!(file, "{content}").unwrap();

        let reader = SmartReader::new();
        let file_content = reader.read(file.path(), MaxInputBytes::DEFAULT).unwrap();

        // Above threshold, should use mmap
        assert!(file_content.is_mmap());
        assert_eq!(file_content.len(), 512 * 1024 + 1);
    }

    #[test]
    fn test_zero_length_file() {
        let file = NamedTempFile::new().unwrap();
        // Don't write anything - file is empty

        let reader = SmartReader::new();
        let content = reader.read(file.path(), MaxInputBytes::DEFAULT).unwrap();

        assert!(content.is_empty());
        assert_eq!(content.len(), 0);
        assert_eq!(content.as_str().unwrap(), "");
    }

    #[test]
    fn test_directory_instead_of_file() {
        let temp_dir = tempfile::tempdir().unwrap();

        let reader = SmartReader::new();
        let result = reader.read(temp_dir.path(), MaxInputBytes::DEFAULT);

        // Reading a directory should fail
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_utf8_with_string() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("invalid.bin");

        // Write invalid UTF-8 bytes (small file, uses String path)
        let invalid_bytes = b"\xFF\xFE invalid utf8";
        std::fs::write(&path, invalid_bytes).unwrap();

        let reader = SmartReader::new();
        let result = reader.read(&path, MaxInputBytes::DEFAULT);

        // Should fail on UTF-8 validation
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_utf8_with_mmap() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("invalid_large.bin");

        // Write invalid UTF-8 bytes (large file, uses mmap)
        let mut invalid_content = vec![0xFF; 600 * 1024];
        invalid_content.extend_from_slice(b" invalid utf8");
        std::fs::write(&path, invalid_content).unwrap();

        let reader = SmartReader::new();
        let file_content = reader.read(&path, MaxInputBytes::DEFAULT).unwrap();

        // File read succeeds (mmap created)
        assert!(file_content.is_mmap());

        // But as_str() fails on UTF-8 validation
        let result = file_content.as_str();
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_mmap_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("empty.yaml");

        // Create empty file
        std::fs::write(&path, "").unwrap();

        // Force mmap with low threshold
        let reader = SmartReader::with_threshold(0);
        let content = reader.read(&path, MaxInputBytes::DEFAULT).unwrap();

        // Empty files might use String path even with low threshold
        // This is OK - just verify it works
        assert!(content.is_empty());
        assert_eq!(content.as_str().unwrap(), "");
    }

    #[test]
    fn test_file_content_mmap_is_mmap() {
        let mut file = NamedTempFile::new().unwrap();
        let content = "x".repeat(600 * 1024);
        write!(file, "{content}").unwrap();

        let reader = SmartReader::new();
        let file_content = reader.read(file.path(), MaxInputBytes::DEFAULT).unwrap();

        assert!(file_content.is_mmap());
        assert_eq!(file_content.len(), 600 * 1024);
    }

    #[test]
    #[cfg(unix)]
    fn test_directory_symlink_rejection() {
        use std::os::unix::fs::symlink;

        let temp_dir = tempfile::tempdir().unwrap();
        let target_dir = temp_dir.path().join("target_dir");
        let link = temp_dir.path().join("dir_link");

        // Create target directory
        std::fs::create_dir(&target_dir).unwrap();

        // Create symlink to directory
        symlink(&target_dir, &link).unwrap();

        // Reading directory symlink should fail
        let reader = SmartReader::new();
        let result = reader.read(&link, MaxInputBytes::DEFAULT);

        assert!(result.is_err());
        match result {
            Err(Error::Io { source, .. }) => {
                assert_eq!(source.kind(), std::io::ErrorKind::InvalidInput);
            }
            _ => panic!("expected Io error"),
        }
    }

    #[test]
    fn test_read_rejects_file_above_limit_before_reading() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "key: value").unwrap();
        let max = MaxInputBytes::new(9).unwrap();

        for reader in [SmartReader::new(), SmartReader::with_threshold(1)] {
            let err = reader.read(file.path(), max).unwrap_err();
            assert!(
                matches!(&err, Error::InputTooLarge(e) if e.size == 10 && e.limit == max),
                "{err:?}"
            );
        }
    }

    #[test]
    fn test_read_accepts_file_at_limit() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "key: value").unwrap();
        let max = MaxInputBytes::new(10).unwrap();

        for reader in [SmartReader::new(), SmartReader::with_threshold(1)] {
            let content = reader.read(file.path(), max).unwrap();
            assert_eq!(content.into_string().unwrap(), "key: value");
        }
    }

    #[test]
    fn test_into_string_decodes_mapped_file() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "a: 1").unwrap();
        let content = SmartReader::with_threshold(1)
            .read(file.path(), MaxInputBytes::DEFAULT)
            .unwrap();
        assert!(content.is_mmap());
        assert_eq!(content.into_string().unwrap(), "a: 1");
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

        let err = SmartReader::new()
            .read(&fifo, MaxInputBytes::DEFAULT)
            .unwrap_err();
        assert!(err.to_string().contains("not a regular file"), "{err}");
    }
}
