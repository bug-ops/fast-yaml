use anyhow::{Context, Result};
use fast_yaml_core::decode_input_owned;
use fast_yaml_core::limits::MaxInputBytes;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// Source of input data
#[derive(Debug)]
pub struct InputSource {
    pub content: String,
    pub origin: InputOrigin,
}

/// Origin of input (file or stdin)
#[derive(Debug, Clone)]
pub enum InputOrigin {
    File(PathBuf),
    Stdin,
}

impl InputSource {
    /// Read input from file or stdin based on arguments
    #[allow(clippy::option_if_let_else)]
    pub fn from_args(file: Option<PathBuf>, max: MaxInputBytes) -> Result<Self> {
        match file {
            Some(path) => Self::from_file(&path, max),
            None => Self::from_stdin(max),
        }
    }

    /// Read from file, rejecting content larger than `max`
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be read, exceeds `max` bytes, or cannot be decoded.
    pub fn from_file(path: &Path, max: MaxInputBytes) -> Result<Self> {
        let bytes = read_file_capped(path, max)
            .with_context(|| format!("Failed to read file: {}", path.display()))?;
        let content = decode_input_owned(bytes)
            .with_context(|| format!("Failed to read file: {}", path.display()))?;

        Ok(Self {
            content,
            origin: InputOrigin::File(path.to_path_buf()),
        })
    }

    /// Read from stdin, rejecting content larger than `max`
    ///
    /// # Errors
    ///
    /// Fails when stdin cannot be read, exceeds `max` bytes, or cannot be decoded.
    pub fn from_stdin(max: MaxInputBytes) -> Result<Self> {
        Self::from_reader(io::stdin(), max)
    }

    fn from_reader(reader: impl Read, max: MaxInputBytes) -> Result<Self> {
        let bytes = read_capped(reader, max, 0).context("Failed to read from stdin")?;
        let content = decode_input_owned(bytes).context("Failed to read from stdin")?;

        Ok(Self {
            content,
            origin: InputOrigin::Stdin,
        })
    }

    /// Get reference to content
    pub fn as_str(&self) -> &str {
        &self.content
    }

    /// Get file path if input is from file
    pub fn file_path(&self) -> Option<&Path> {
        match &self.origin {
            InputOrigin::File(path) => Some(path),
            InputOrigin::Stdin => None,
        }
    }
}

/// Reads a whole file, failing instead of buffering anything beyond `max` bytes.
///
/// # Errors
///
/// Fails when the file cannot be opened or read, or holds more than `max` bytes.
pub fn read_file_capped(path: &Path, max: MaxInputBytes) -> Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    let size_hint = file.metadata().map_or(0, |m| m.len());
    read_capped(file, max, size_hint)
}

/// Reads at most `max` bytes, failing instead of buffering anything beyond it.
///
/// `size_hint` is the expected length, used to pre-allocate and capped at `max`.
fn read_capped(reader: impl Read, max: MaxInputBytes, size_hint: u64) -> Result<Vec<u8>> {
    let capacity = usize::try_from(size_hint).map_or_else(|_| max.get(), |n| n.min(max.get()));
    let mut bytes = Vec::with_capacity(capacity);
    reader
        .take(u64::try_from(max.get())?.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() > max.get() {
        anyhow::bail!(
            "input exceeds the maximum size of {max} bytes (raise with --max-input-size)"
        );
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_from_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "test: value").unwrap();

        let input = InputSource::from_file(temp_file.path(), MaxInputBytes::DEFAULT).unwrap();
        assert_eq!(input.as_str(), "test: value");
        assert!(matches!(input.origin, InputOrigin::File(_)));
        assert_eq!(input.file_path(), Some(temp_file.path()));
    }

    #[test]
    fn test_from_file_not_found() {
        let result =
            InputSource::from_file(Path::new("/nonexistent/file.yaml"), MaxInputBytes::DEFAULT);
        assert!(result.is_err());
    }

    #[test]
    fn test_file_path_stdin() {
        let input = InputSource {
            content: String::from("test"),
            origin: InputOrigin::Stdin,
        };
        assert_eq!(input.file_path(), None);
    }

    #[test]
    fn test_from_file_keeps_bom() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "\u{FEFF}a: 1").unwrap();

        let input = InputSource::from_file(temp_file.path(), MaxInputBytes::DEFAULT).unwrap();
        assert_eq!(input.as_str(), "\u{FEFF}a: 1");
    }

    #[test]
    fn test_read_capped_stops_at_limit() {
        let max = MaxInputBytes::new(3).unwrap();
        assert_eq!(read_capped(&b"abc"[..], max, 0).unwrap(), b"abc");
        assert!(read_capped(&b"abcd"[..], max, 0).is_err());
    }
}
