use anyhow::{Context, Result};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use fast_yaml_core::fs::DisplayPath;
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

/// Where a command that only writes a report sends it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum OutputTarget {
    /// Standard output
    #[default]
    Stdout,
    /// The named file; a special device path such as `/dev/stdout` selects that stream
    File(PathBuf),
}

/// Where a command that can rewrite its input sends the result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteTarget {
    /// Standard output or an explicit file
    Output(OutputTarget),
    /// The input file itself
    InPlace,
}

impl OutputWriter {
    /// Creates a writer for `target`.
    ///
    /// A special device path (`/dev/stdout`, `-`, ...) selects that stream instead of a file.
    #[must_use]
    pub fn new(target: OutputTarget) -> Self {
        let destination = match target {
            OutputTarget::Stdout => OutputDestination::Stdout,
            OutputTarget::File(path) => {
                detect_special_device(&path).unwrap_or(OutputDestination::File(path))
            }
        };
        Self { destination }
    }

    /// Creates a writer for `target`, resolving [`WriteTarget::InPlace`] to `input_file`.
    ///
    /// Unless `in_place` is set, a destination that is the same file as `input_file` is refused,
    /// since an explicit `--output` must never overwrite the input it was read from.
    ///
    /// # Errors
    ///
    /// Returns an error if the target is [`WriteTarget::InPlace`] and `input_file` is `None`, or
    /// if an explicit output is the same file as `input_file`.
    pub fn for_write(target: WriteTarget, input_file: Option<&Path>) -> Result<Self> {
        match target {
            WriteTarget::Output(output) => {
                let writer = Self::new(output);
                if let Some(input) = input_file {
                    writer.ensure_not_input(input)?;
                }
                Ok(writer)
            }
            WriteTarget::InPlace => {
                let path = input_file
                    .ok_or_else(|| anyhow::anyhow!("--in-place requires a file argument"))?;
                Ok(Self {
                    destination: OutputDestination::File(path.to_path_buf()),
                })
            }
        }
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

    #[cfg(feature = "linter")]
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
                    .with_context(|| format!("Failed to write file: {}", DisplayPath::new(path)))?,
            ),
            OutputDestination::Stdout => SinkKind::Stdout,
            OutputDestination::Stderr => SinkKind::Stderr,
        };
        Ok(OutputSink {
            kind,
            reader_gone: false,
        })
    }

    #[cfg(feature = "linter")]
    /// Refuses to overwrite `input` with the output.
    ///
    /// # Errors
    ///
    /// Returns an error if the destination file is the same file as `input`.
    pub fn ensure_not_input(&self, input: &Path) -> Result<()> {
        self.ensure_not_inputs([input])
    }

    #[cfg(feature = "linter")]
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
                DisplayPath::new(destination)
            );
        }
        Ok(())
    }

    /// Write to file via the shared secure atomic writer
    fn write_file(path: &Path, content: &str) -> Result<()> {
        fast_yaml_parallel::write_atomic(path, content.as_bytes())
            .with_context(|| format!("Failed to write file: {}", DisplayPath::new(path)))
    }
}

#[cfg(feature = "linter")]
/// What names a destination file, resolved once: its canonical path and, on Unix, its inode.
struct FileIdentity {
    canonical: Option<PathBuf>,
    #[cfg(unix)]
    inode: Option<(u64, u64)>,
}

#[cfg(feature = "linter")]
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

#[cfg(all(unix, feature = "linter"))]
fn inode_of(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|m| (m.dev(), m.ino()))
}

#[cfg(feature = "linter")]
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
    fn test_new_stdout() {
        let writer = OutputWriter::new(OutputTarget::Stdout);
        assert!(matches!(writer.destination, OutputDestination::Stdout));
    }

    #[test]
    fn test_new_output_file() {
        let output_path = PathBuf::from("/tmp/output.yaml");
        let writer = OutputWriter::new(OutputTarget::File(output_path.clone()));
        match writer.destination {
            OutputDestination::File(path) => assert_eq!(path, output_path),
            _ => panic!("Expected File destination"),
        }
    }

    #[test]
    fn test_for_write_in_place() {
        let input_path = PathBuf::from("/tmp/input.yaml");
        let writer = OutputWriter::for_write(WriteTarget::InPlace, Some(&input_path)).unwrap();
        match writer.destination {
            OutputDestination::File(path) => assert_eq!(path, input_path),
            _ => panic!("Expected File destination"),
        }
    }

    #[test]
    fn test_for_write_in_place_without_file() {
        let result = OutputWriter::for_write(WriteTarget::InPlace, None);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("--in-place requires a file argument")
        );
    }

    #[test]
    fn test_new_dev_stdout() {
        let writer = OutputWriter::new(OutputTarget::File(PathBuf::from("/dev/stdout")));
        assert!(matches!(writer.destination, OutputDestination::Stdout));
    }

    #[test]
    fn test_new_dash() {
        let writer = OutputWriter::new(OutputTarget::File(PathBuf::from("-")));
        assert!(matches!(writer.destination, OutputDestination::Stdout));
    }

    #[test]
    fn test_new_dev_fd_1() {
        let writer = OutputWriter::new(OutputTarget::File(PathBuf::from("/dev/fd/1")));
        assert!(matches!(writer.destination, OutputDestination::Stdout));
    }

    #[test]
    fn test_new_dev_stderr() {
        let writer = OutputWriter::new(OutputTarget::File(PathBuf::from("/dev/stderr")));
        assert!(matches!(writer.destination, OutputDestination::Stderr));
    }

    #[test]
    fn test_new_dev_fd_2() {
        let writer = OutputWriter::new(OutputTarget::File(PathBuf::from("/dev/fd/2")));
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
        let writer = OutputWriter::new(OutputTarget::File(path.clone()));

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
        let writer = OutputWriter::new(OutputTarget::File(path.clone()));
        let mut sink = writer.sink().unwrap();
        sink.write_all(b"partial").unwrap();
        drop(sink);
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    #[cfg(all(unix, feature = "linter"))]
    fn test_ensure_not_input_refuses_a_hard_link_to_the_input() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.yaml");
        let alias = dir.path().join("alias.yaml");
        fs::write(&path, "a: 1").unwrap();
        fs::hard_link(&path, &alias).unwrap();
        let writer = OutputWriter::new(OutputTarget::File(alias));
        assert!(writer.ensure_not_input(&path).is_err());
    }

    #[test]
    #[cfg(feature = "linter")]
    fn test_ensure_not_input_refuses_the_same_file_through_another_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.yaml");
        fs::write(&path, "a: 1").unwrap();
        let alias = dir.path().join(".").join("a.yaml");
        let writer = OutputWriter::new(OutputTarget::File(alias));
        let err = writer.ensure_not_input(&path).unwrap_err();
        assert!(err.to_string().contains("also an input file"), "{err}");
        assert!(writer.ensure_not_input(dir.path()).is_ok());
    }
}
