use anyhow::{Context, Result};
use fast_yaml_core::decode_input_owned;
use fast_yaml_core::limits::MaxInputBytes;
use fast_yaml_parallel::read_file;
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
    /// Read input from file or stdin based on arguments, reading at most `max` bytes
    #[allow(clippy::option_if_let_else)]
    pub fn from_args(file: Option<PathBuf>, max: MaxInputBytes) -> Result<Self> {
        match file {
            Some(path) => Self::from_file(&path, max),
            None => Self::from_stdin(max),
        }
    }

    /// Read from file; the read never buffers more than `max` bytes plus one
    pub fn from_file(path: &Path, max: MaxInputBytes) -> Result<Self> {
        let content = read_file(path, max)
            .with_context(|| format!("Failed to read file: {}", path.display()))?;

        Ok(Self {
            content,
            origin: InputOrigin::File(path.to_path_buf()),
        })
    }

    /// Read from stdin; the read never buffers more than `max` bytes plus one
    pub fn from_stdin(max: MaxInputBytes) -> Result<Self> {
        let mut bytes = Vec::new();
        io::stdin()
            .lock()
            .take(max.get() as u64 + 1)
            .read_to_end(&mut bytes)
            .context("Failed to read from stdin")?;
        max.check(bytes.len())
            .context("Failed to read from stdin")?;
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
    fn test_from_file_honors_limit_boundary() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "a: 1234").unwrap();

        let exact = MaxInputBytes::new(7).unwrap();
        assert!(InputSource::from_file(temp_file.path(), exact).is_ok());
        let below = MaxInputBytes::new(6).unwrap();
        let err = InputSource::from_file(temp_file.path(), below).unwrap_err();
        assert!(
            crate::error::RaiseHint::of(err.as_ref()).is_some(),
            "{err:?}"
        );
    }

    #[test]
    fn test_from_file_keeps_bom() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "\u{FEFF}a: 1").unwrap();

        let input = InputSource::from_file(temp_file.path(), MaxInputBytes::DEFAULT).unwrap();
        assert_eq!(input.as_str(), "\u{FEFF}a: 1");
    }
}
