use anyhow::{Context, Result};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use fast_yaml_parallel::AtomicFile;

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

    /// Returns `true` if this writer overwrites the file at `path`.
    pub fn targets(&self, path: &Path) -> bool {
        matches!(&self.destination, OutputDestination::File(p) if p == path)
    }

    /// Write output to destination.
    ///
    /// A closed stdout or stderr (`EPIPE`) ends the output silently so the caller keeps its
    /// own exit status.
    ///
    /// # Errors
    ///
    /// Returns an error on I/O failure for any destination variant, except a closed pipe.
    pub fn write(&self, content: &str) -> Result<()> {
        match &self.destination {
            OutputDestination::File(path) => Self::write_file(path, content),
            OutputDestination::Stdout | OutputDestination::Stderr => {
                let mut sink = self.sink()?;
                sink.write_all(content.as_bytes())
                    .and_then(|()| sink.flush())
                    .context("Failed to write output")?;
                sink.finish()
            }
        }
    }

    /// Write a finished report; a closed stdout or stderr ends the output silently.
    ///
    /// # Errors
    ///
    /// Returns an error on any I/O failure other than a closed pipe.
    pub fn write_report(&self, content: &str) -> Result<()> {
        let mut sink = self.sink()?;
        sink.write_all(content.as_bytes())
            .and_then(|()| sink.flush())
            .context("Failed to write lint output")?;
        sink.finish()
    }

    /// Opens a streaming writer for output produced piece by piece.
    ///
    /// Stdout and stderr are written as the pieces arrive and stop silently once the reader
    /// closes the pipe. A file destination streams into a temporary file next to it, which
    /// [`OutputSink::finish`] moves into place atomically, so memory use does not grow with
    /// the output.
    ///
    /// # Errors
    ///
    /// Returns an error if the temporary file for a file destination cannot be created.
    pub fn sink(&self) -> Result<OutputSink> {
        let kind = match &self.destination {
            OutputDestination::File(path) => SinkKind::File(
                AtomicFile::create(path)
                    .with_context(|| format!("Failed to write file: {}", path.display()))?,
            ),
            OutputDestination::Stdout => SinkKind::Stdout,
            OutputDestination::Stderr => SinkKind::Stderr,
        };
        Ok(OutputSink {
            kind,
            reader_gone: false,
        })
    }

    /// Refuses to overwrite `input` with the output.
    ///
    /// # Errors
    ///
    /// Returns an error if the destination file is the same file as `input`.
    pub fn ensure_not_input(&self, input: &Path) -> Result<()> {
        self.ensure_not_inputs([input])
    }

    /// Refuses to overwrite any of `inputs` with the output, resolving the destination once.
    ///
    /// # Errors
    ///
    /// Returns an error if the destination file is the same file as one of `inputs`.
    pub fn ensure_not_inputs<'a>(&self, inputs: impl IntoIterator<Item = &'a Path>) -> Result<()> {
        let OutputDestination::File(destination) = &self.destination else {
            return Ok(());
        };
        let identity = FileIdentity::of(destination);
        if inputs.into_iter().any(|input| identity.is(input)) {
            anyhow::bail!(
                "--output '{}' is also an input file; refusing to overwrite it",
                destination.display()
            );
        }
        Ok(())
    }

    /// Write to file via the shared secure atomic writer
    fn write_file(path: &Path, content: &str) -> Result<()> {
        fast_yaml_parallel::write_atomic(path, content.as_bytes())
            .with_context(|| format!("Failed to write file: {}", path.display()))
    }
}

/// What names a destination file, resolved once: its canonical path and, on Unix, its inode.
struct FileIdentity {
    canonical: Option<PathBuf>,
    #[cfg(unix)]
    inode: Option<(u64, u64)>,
}

impl FileIdentity {
    fn of(path: &Path) -> Self {
        Self {
            canonical: path.canonicalize().ok(),
            #[cfg(unix)]
            inode: inode_of(path),
        }
    }

    #[cfg(unix)]
    const fn resolves_to(_: &Path, _: &Path) -> bool {
        false
    }

    #[cfg(not(unix))]
    fn resolves_to(other: &Path, canonical: &Path) -> bool {
        other.canonicalize().is_ok_and(|o| o == canonical)
    }

    /// Whether `other` is the same file: the same canonical path, or on Unix the same inode
    /// (a hard link).
    fn is(&self, other: &Path) -> bool {
        if let Some(canonical) = &self.canonical
            && (canonical == other || Self::resolves_to(other, canonical))
        {
            return true;
        }
        #[cfg(unix)]
        if let (Some(a), Some(b)) = (self.inode, inode_of(other)) {
            return a == b;
        }
        false
    }
}

#[cfg(unix)]
fn inode_of(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|m| (m.dev(), m.ino()))
}

/// A stderr writer that, like [`OutputWriter::sink`], goes quiet once the pipe is closed.
pub const fn stderr_sink() -> OutputSink {
    OutputSink {
        kind: SinkKind::Stderr,
        reader_gone: false,
    }
}

#[derive(Debug)]
enum SinkKind {
    Stdout,
    Stderr,
    File(AtomicFile),
}

/// Streaming writer over an [`OutputWriter`] destination that survives a closed pipe.
///
/// Once the reader of stdout or stderr is gone (`EPIPE`), every later write succeeds without
/// writing, so the producer finishes its work and the caller keeps its own exit status.
#[derive(Debug)]
pub struct OutputSink {
    kind: SinkKind,
    reader_gone: bool,
}

impl OutputSink {
    /// Moves the output of a file destination into place; stream destinations have nothing left.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn finish(self) -> Result<()> {
        match self.kind {
            SinkKind::File(file) => file.commit().context("Failed to write the output file"),
            SinkKind::Stdout | SinkKind::Stderr => Ok(()),
        }
    }

    fn stream(&mut self, op: impl FnOnce(&mut dyn Write) -> io::Result<()>) -> io::Result<()> {
        if self.reader_gone {
            return Ok(());
        }
        let result = match &mut self.kind {
            SinkKind::Stdout => op(&mut io::stdout().lock()),
            SinkKind::Stderr => op(&mut io::stderr().lock()),
            SinkKind::File(file) => return op(file),
        };
        match result {
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => {
                self.reader_gone = true;
                Ok(())
            }
            other => other,
        }
    }
}

impl Write for OutputSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream(|w| w.write_all(buf))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream(|w| w.flush())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
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

    #[test]
    fn test_file_sink_streams_to_a_temp_file_and_commits_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report.txt");
        fs::write(&path, "old").unwrap();
        let writer = OutputWriter::from_args(Some(path.clone()), false, None).unwrap();

        let mut sink = writer.sink().unwrap();
        sink.write_all(b"new report").unwrap();
        sink.flush().unwrap();
        let streamed: u64 = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| fs::metadata(entry.unwrap().path()).unwrap().len())
            .max()
            .unwrap();
        assert_eq!(streamed, "new report".len() as u64);
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");

        sink.finish().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new report");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn test_dropped_file_sink_leaves_the_target_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report.txt");
        fs::write(&path, "old").unwrap();
        let writer = OutputWriter::from_args(Some(path.clone()), false, None).unwrap();
        let mut sink = writer.sink().unwrap();
        sink.write_all(b"partial").unwrap();
        drop(sink);
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    #[cfg(unix)]
    fn test_ensure_not_input_refuses_a_hard_link_to_the_input() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.yaml");
        let alias = dir.path().join("alias.yaml");
        fs::write(&path, "a: 1").unwrap();
        fs::hard_link(&path, &alias).unwrap();
        let writer = OutputWriter::from_args(Some(alias), false, None).unwrap();
        assert!(writer.ensure_not_input(&path).is_err());
    }

    #[test]
    fn test_ensure_not_input_refuses_the_same_file_through_another_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.yaml");
        fs::write(&path, "a: 1").unwrap();
        let alias = dir.path().join(".").join("a.yaml");
        let writer = OutputWriter::from_args(Some(alias), false, None).unwrap();
        let err = writer.ensure_not_input(&path).unwrap_err();
        assert!(err.to_string().contains("also an input file"), "{err}");
        assert!(writer.ensure_not_input(dir.path()).is_ok());
    }
}
