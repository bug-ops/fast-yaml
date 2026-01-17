//! Smart file reading with automatic strategy selection based on file size.

use std::fs::File;
use std::path::Path;

use memmap2::Mmap;

use super::error::ProcessingError;

/// Memory-map threshold constant: 512KB
const MMAP_THRESHOLD: u64 = 512 * 1024;

/// File content holder that abstracts over in-memory strings and memory-mapped files.
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
    pub fn as_str(&self) -> Result<&str, ProcessingError> {
        match self {
            Self::String(s) => Ok(s),
            Self::Mmap(mmap) => std::str::from_utf8(mmap).map_err(ProcessingError::Utf8Error),
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
pub struct SmartFileReader {
    mmap_threshold: u64,
}

impl SmartFileReader {
    /// Creates a new `SmartFileReader` with the default threshold (1MB)
    pub const fn new() -> Self {
        Self::with_threshold(MMAP_THRESHOLD)
    }

    /// Creates a new `SmartFileReader` with a custom threshold
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
    pub fn read(&self, path: &Path) -> Result<FileContent, ProcessingError> {
        let metadata = std::fs::metadata(path).map_err(ProcessingError::ReadError)?;

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
    fn read_string(path: &Path) -> Result<FileContent, ProcessingError> {
        let content = std::fs::read_to_string(path).map_err(ProcessingError::ReadError)?;
        Ok(FileContent::String(content))
    }

    /// Reads file using memory-mapped file
    ///
    /// Uses unsafe memory-mapping for performance. See SAFETY note below.
    #[allow(unsafe_code)]
    fn read_mmap(path: &Path) -> Result<FileContent, ProcessingError> {
        let file = File::open(path).map_err(ProcessingError::MmapError)?;

        // SAFETY: We're opening the file read-only and mapping it.
        // The file could be modified by another process during reading,
        // but this is acceptable for a formatter tool:
        // - If modified, worst case is a parse error (which is handled)
        // - User expectation is that files aren't modified during formatting
        // - Same race condition exists with read_to_string
        // - The mmap is read-only, so we won't write to mapped memory
        // - Mmap type ensures memory is unmapped when dropped
        let mmap = unsafe { Mmap::map(&file).map_err(ProcessingError::MmapError)? };

        Ok(FileContent::Mmap(mmap))
    }
}

impl Default for SmartFileReader {
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

        let reader = SmartFileReader::new();
        let content = reader.read(file.path()).unwrap();

        assert!(!content.is_mmap());
        assert_eq!(content.as_str().unwrap(), "small: content");
    }

    #[test]
    fn test_reader_large_file_uses_mmap() {
        let mut file = NamedTempFile::new().unwrap();

        // Write content larger than 1MB threshold
        let large_content = "x".repeat(2 * 1024 * 1024);
        write!(file, "{large_content}").unwrap();

        let reader = SmartFileReader::new();
        let content = reader.read(file.path()).unwrap();

        assert!(content.is_mmap());
        assert_eq!(content.len(), large_content.len());
    }

    #[test]
    fn test_reader_custom_threshold() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "test content").unwrap();

        // Threshold of 5 bytes should trigger mmap for our 12-byte file
        let reader = SmartFileReader::with_threshold(5);
        let content = reader.read(file.path()).unwrap();

        // Should use mmap since file > 5 bytes
        assert!(content.is_mmap());
    }

    #[test]
    fn test_reader_default_equals_new() {
        let reader1 = SmartFileReader::new();
        let reader2 = SmartFileReader::default();

        assert_eq!(reader1.mmap_threshold, reader2.mmap_threshold);
    }

    #[test]
    fn test_read_nonexistent_file() {
        let reader = SmartFileReader::new();
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
        let content = "valid: utf8 content\n".repeat(100_000);
        write!(file, "{content}").unwrap();

        let reader = SmartFileReader::new();
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
        let reader = SmartFileReader::new();
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
        let reader = SmartFileReader::new();
        let result = reader.read(&link);

        assert!(result.is_err());
    }

    #[test]
    #[cfg(unix)]
    fn test_symlink_loop_detection() {
        use std::os::unix::fs::symlink;

        let temp_dir = tempfile::tempdir().unwrap();
        let link1 = temp_dir.path().join("link1.yaml");
        let link2 = temp_dir.path().join("link2.yaml");

        // Create symlink loop: link1 -> link2 -> link1
        symlink(&link2, &link1).unwrap();
        symlink(&link1, &link2).unwrap();

        // Reading symlink loop should fail (OS detects ELOOP)
        let reader = SmartFileReader::new();
        let result = reader.read(&link1);

        assert!(result.is_err());
    }

    #[test]
    #[cfg(unix)]
    fn test_symlink_to_directory_error() {
        use std::os::unix::fs::symlink;

        let temp_dir = tempfile::tempdir().unwrap();
        let dir = temp_dir.path().join("subdir");
        let link = temp_dir.path().join("dir_link.yaml");

        // Create directory and symlink to it
        std::fs::create_dir(&dir).unwrap();
        symlink(&dir, &link).unwrap();

        // Reading symlink to directory should fail
        let reader = SmartFileReader::new();
        let result = reader.read(&link);

        assert!(result.is_err());
    }
}
