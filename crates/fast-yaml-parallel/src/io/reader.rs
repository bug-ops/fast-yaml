//! Smart file reading with automatic strategy selection based on file size.

use std::fs::File;
use std::path::Path;

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
    Mmap(Mmap),
}

impl FileContent {
    /// Returns the content as a string slice.
    ///
    /// For String variant, returns the string directly.
    /// For Mmap variant, validates UTF-8 encoding first.
    pub fn as_str(&self) -> Result<&str> {
        match self {
            Self::String(s) => Ok(s),
            Self::Mmap(mmap) => std::str::from_utf8(mmap).map_err(|source| Error::Utf8 { source }),
        }
    }

    /// Returns true if content is memory-mapped
    pub const fn is_mmap(&self) -> bool {
        matches!(self, Self::Mmap(_))
    }

    /// Returns the size of the content in bytes
    pub fn len(&self) -> usize {
        match self {
            Self::String(s) => s.len(),
            Self::Mmap(mmap) => mmap.len(),
        }
    }

    /// Returns true if the content is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Smart file reader that chooses optimal reading strategy based on file size.
///
/// For files smaller than the threshold, uses `std::fs::read_to_string` for simplicity.
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
    /// use fast_yaml_parallel::SmartReader;
    /// use std::path::Path;
    ///
    /// let reader = SmartReader::new();
    /// # let temp_file = tempfile::NamedTempFile::new().unwrap();
    /// # std::fs::write(temp_file.path(), "key: value\n").unwrap();
    /// let content = reader.read(temp_file.path())?;
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
    /// - `read_to_string` for files < threshold
    /// - `mmap` for files >= threshold
    ///
    /// Falls back to `read_to_string` if mmap fails.
    ///
    /// # Errors
    ///
    /// Returns `Error::Io` if:
    /// - Path does not exist
    /// - Path is a directory
    /// - Insufficient permissions
    pub fn read(&self, path: &Path) -> Result<FileContent> {
        let metadata = std::fs::metadata(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;

        if metadata.is_dir() {
            return Err(Error::Io {
                path: path.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "path is a directory, not a file",
                ),
            });
        }

        let size = metadata.len();

        if size >= self.mmap_threshold {
            Self::read_mmap(path).or_else(|_| {
                // Fallback to read_to_string if mmap fails
                Self::read_string(path)
            })
        } else {
            Self::read_string(path)
        }
    }

    /// Reads file into memory as a String
    fn read_string(path: &Path) -> Result<FileContent> {
        let content = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(FileContent::String(content))
    }

    /// Reads file using memory-mapped file
    #[allow(unsafe_code)]
    fn read_mmap(path: &Path) -> Result<FileContent> {
        let file = File::open(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;

        // SAFETY: We're opening the file read-only and mapping it.
        // The file could be modified by another process during reading,
        // but this is acceptable for a parser tool:
        // - If modified, worst case is a parse error (which is handled)
        // - User expectation is that files aren't modified during parsing
        // - Same race condition exists with read_to_string
        // - The mmap is read-only, so we won't write to mapped memory
        // - Mmap type ensures memory is unmapped when dropped
        let mmap = unsafe {
            Mmap::map(&file).map_err(|source| Error::Io {
                path: path.to_path_buf(),
                source,
            })?
        };

        Ok(FileContent::Mmap(mmap))
    }
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
        let content = reader.read(file.path()).unwrap();

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
        let content = reader.read(file.path()).unwrap();

        assert!(content.is_mmap());
        assert_eq!(content.len(), large_content.len());
    }

    #[test]
    fn test_reader_custom_threshold() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "test content").unwrap();

        // Threshold of 5 bytes should trigger mmap for our 12-byte file
        let reader = SmartReader::with_threshold(5);
        let content = reader.read(file.path()).unwrap();

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
        let result = reader.read(Path::new("/nonexistent/file.yaml"));
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
        let file_content = reader.read(file.path()).unwrap();

        // Should be mmap and valid UTF-8
        assert!(file_content.is_mmap());
        assert!(file_content.as_str().is_ok());
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
        let content = reader.read(&link).unwrap();

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
        let result = reader.read(&link);

        assert!(result.is_err());
    }

    #[test]
    fn test_file_exactly_at_threshold() {
        let mut file = NamedTempFile::new().unwrap();

        // Write exactly 512KB
        let content = "x".repeat(512 * 1024);
        write!(file, "{content}").unwrap();

        let reader = SmartReader::new();
        let file_content = reader.read(file.path()).unwrap();

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
        let file_content = reader.read(file.path()).unwrap();

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
        let file_content = reader.read(file.path()).unwrap();

        // Above threshold, should use mmap
        assert!(file_content.is_mmap());
        assert_eq!(file_content.len(), 512 * 1024 + 1);
    }

    #[test]
    fn test_zero_length_file() {
        let file = NamedTempFile::new().unwrap();
        // Don't write anything - file is empty

        let reader = SmartReader::new();
        let content = reader.read(file.path()).unwrap();

        assert!(content.is_empty());
        assert_eq!(content.len(), 0);
        assert_eq!(content.as_str().unwrap(), "");
    }

    #[test]
    fn test_directory_instead_of_file() {
        let temp_dir = tempfile::tempdir().unwrap();

        let reader = SmartReader::new();
        let result = reader.read(temp_dir.path());

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
        let result = reader.read(&path);

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
        let file_content = reader.read(&path).unwrap();

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
        let content = reader.read(&path).unwrap();

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
        let file_content = reader.read(file.path()).unwrap();

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
        let result = reader.read(&link);

        assert!(result.is_err());
        match result {
            Err(Error::Io { source, .. }) => {
                assert_eq!(source.kind(), std::io::ErrorKind::InvalidInput);
            }
            _ => panic!("expected Io error"),
        }
    }
}
