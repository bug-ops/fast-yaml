use anyhow::{Context, Result};
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
    pub fn from_args(file: Option<PathBuf>) -> Result<Self> {
        match file {
            Some(path) => Self::from_file(&path),
            None => Self::from_stdin(),
        }
    }

    /// Read from file
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read file: {}", path.display()))?;

        Ok(Self {
            content,
            origin: InputOrigin::File(path.to_path_buf()),
        })
    }

    /// Read from stdin
    pub fn from_stdin() -> Result<Self> {
        let mut content = String::new();
        io::stdin()
            .read_to_string(&mut content)
            .context("Failed to read from stdin")?;

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

        let input = InputSource::from_file(temp_file.path()).unwrap();
        assert_eq!(input.as_str(), "test: value");
        assert!(matches!(input.origin, InputOrigin::File(_)));
        assert_eq!(input.file_path(), Some(temp_file.path()));
    }

    #[test]
    fn test_from_file_not_found() {
        let result = InputSource::from_file(Path::new("/nonexistent/file.yaml"));
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
}
