use anyhow::{Context, Result};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Destination for output data
#[derive(Debug)]
pub enum OutputDestination {
    File(PathBuf),
    Stdout,
}

/// Output writer
#[derive(Debug)]
pub struct OutputWriter {
    destination: OutputDestination,
}

impl OutputWriter {
    /// Create writer from CLI arguments
    pub fn from_args(
        output: Option<PathBuf>,
        in_place: bool,
        input_file: Option<&Path>,
    ) -> Result<Self> {
        let destination = if in_place {
            // In-place editing requires input file
            let path = input_file
                .ok_or_else(|| anyhow::anyhow!("--in-place requires a file argument"))?;
            OutputDestination::File(path.to_path_buf())
        } else if let Some(out_path) = output {
            OutputDestination::File(out_path)
        } else {
            OutputDestination::Stdout
        };

        Ok(Self { destination })
    }

    /// Create stdout writer for tests
    #[cfg(test)]
    pub fn stdout() -> Self {
        Self {
            destination: OutputDestination::Stdout,
        }
    }

    /// Write output to destination
    pub fn write(&self, content: &str) -> Result<()> {
        match &self.destination {
            OutputDestination::File(path) => {
                Self::write_file(path, content)?;
            }
            OutputDestination::Stdout => {
                io::stdout()
                    .write_all(content.as_bytes())
                    .context("Failed to write to stdout")?;
            }
        }
        Ok(())
    }

    /// Write to file with atomic operation
    fn write_file(path: &Path, content: &str) -> Result<()> {
        // Write to temporary file first
        let temp_path = path.with_extension("tmp");

        fs::write(&temp_path, content)
            .with_context(|| format!("Failed to write temp file: {}", temp_path.display()))?;

        // Atomic rename
        fs::rename(&temp_path, path)
            .with_context(|| format!("Failed to replace file: {}", path.display()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
    use tempfile::NamedTempFile;

    #[test]
    fn test_from_args_stdout() {
        let writer = OutputWriter::from_args(None, false, None).unwrap();
        assert!(matches!(writer.destination, OutputDestination::Stdout));
    }

    #[test]
    fn test_from_args_output_file() {
        let output_path = PathBuf::from("/tmp/output.yaml");
        let writer = OutputWriter::from_args(Some(output_path.clone()), false, None).unwrap();
        match writer.destination {
            OutputDestination::File(path) => assert_eq!(path, output_path),
            _ => panic!("Expected File destination"),
        }
    }

    #[test]
    fn test_from_args_in_place() {
        let input_path = PathBuf::from("/tmp/input.yaml");
        let writer = OutputWriter::from_args(None, true, Some(&input_path)).unwrap();
        match writer.destination {
            OutputDestination::File(path) => assert_eq!(path, input_path),
            _ => panic!("Expected File destination"),
        }
    }

    #[test]
    fn test_from_args_in_place_without_file() {
        let result = OutputWriter::from_args(None, true, None);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("--in-place requires a file argument"));
    }

    #[test]
    fn test_write_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "original content").unwrap();
        let path = temp_file.path();

        OutputWriter::write_file(path, "new content").unwrap();

        let content = fs::read_to_string(path).unwrap();
        assert_eq!(content, "new content");
    }
}
