use anyhow::{Context, Result};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Destination for output data
#[derive(Debug)]
pub enum OutputDestination {
    File(PathBuf),
    Stdout,
    Stderr,
}

/// Output writer
#[derive(Debug)]
pub struct OutputWriter {
    destination: OutputDestination,
}

/// Returns the `OutputDestination` for paths that should bypass the temp-file strategy.
///
/// Detects `/dev/stdout`, `/dev/stderr`, `/dev/fd/1`, `/dev/fd/2`, and `-` (stdout convention).
/// Comparison is intentionally on the raw, non-canonicalized path: clap does not canonicalize
/// `PathBuf` arguments, so the user-supplied string is matched directly.
///
/// Note: `/proc/self/fd/1` and `/proc/self/fd/2` (Linux symlink targets of `/dev/stdout`
/// and `/dev/stderr`) are not detected; add them here if a user report surfaces.
fn detect_special_device(path: &Path) -> Option<OutputDestination> {
    let s = path.as_os_str();
    if s == "/dev/stdout" || s == "/dev/fd/1" || s == "-" {
        Some(OutputDestination::Stdout)
    } else if s == "/dev/stderr" || s == "/dev/fd/2" {
        Some(OutputDestination::Stderr)
    } else {
        None
    }
}

impl OutputWriter {
    /// Create writer from CLI arguments.
    ///
    /// # Errors
    ///
    /// Returns an error if `in_place` is `true` and `input_file` is `None`.
    pub fn from_args(
        output: Option<PathBuf>,
        in_place: bool,
        input_file: Option<&Path>,
    ) -> Result<Self> {
        let destination = if in_place {
            // In-place editing requires input file
            let path =
                input_file.ok_or_else(|| anyhow::anyhow!("--in-place requires a file argument"))?;
            OutputDestination::File(path.to_path_buf())
        } else if let Some(out_path) = output {
            // Detect special device paths before falling through to temp-file strategy
            detect_special_device(&out_path).unwrap_or(OutputDestination::File(out_path))
        } else {
            OutputDestination::Stdout
        };

        Ok(Self { destination })
    }

    /// Create stdout writer for tests
    #[cfg(test)]
    pub const fn stdout() -> Self {
        Self {
            destination: OutputDestination::Stdout,
        }
    }

    /// Write output to destination.
    ///
    /// # Errors
    ///
    /// Returns an error on I/O failure for any destination variant (file write, stdout, or stderr).
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
            OutputDestination::Stderr => {
                io::stderr()
                    .write_all(content.as_bytes())
                    .context("Failed to write to stderr")?;
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
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("--in-place requires a file argument")
        );
    }

    #[test]
    fn test_from_args_dev_stdout() {
        let writer =
            OutputWriter::from_args(Some(PathBuf::from("/dev/stdout")), false, None).unwrap();
        assert!(matches!(writer.destination, OutputDestination::Stdout));
    }

    #[test]
    fn test_from_args_dash() {
        let writer = OutputWriter::from_args(Some(PathBuf::from("-")), false, None).unwrap();
        assert!(matches!(writer.destination, OutputDestination::Stdout));
    }

    #[test]
    fn test_from_args_dev_fd_1() {
        let writer =
            OutputWriter::from_args(Some(PathBuf::from("/dev/fd/1")), false, None).unwrap();
        assert!(matches!(writer.destination, OutputDestination::Stdout));
    }

    #[test]
    fn test_from_args_dev_stderr() {
        let writer =
            OutputWriter::from_args(Some(PathBuf::from("/dev/stderr")), false, None).unwrap();
        assert!(matches!(writer.destination, OutputDestination::Stderr));
    }

    #[test]
    fn test_from_args_dev_fd_2() {
        let writer =
            OutputWriter::from_args(Some(PathBuf::from("/dev/fd/2")), false, None).unwrap();
        assert!(matches!(writer.destination, OutputDestination::Stderr));
    }

    #[test]
    fn test_detect_special_device_regular() {
        assert!(detect_special_device(Path::new("/tmp/output.yaml")).is_none());
        assert!(detect_special_device(Path::new("output.yaml")).is_none());
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
