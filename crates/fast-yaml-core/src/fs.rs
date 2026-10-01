//! Bounded reading of regular files.
//!
//! [`read_regular_file`] is the one way `fy` reads user-named files into memory (inputs,
//! config files, `extends` targets and ignore files).
//!
//! # Threat model
//!
//! The path may name anything the user, or a directory walk, can reach, and the file system may
//! change under us between calls:
//!
//! - A FIFO or device would block `open(2)` or yield unbounded data. The path is checked before it
//!   is opened, and on Unix the file is opened with `O_NONBLOCK | O_NOCTTY` (a terminal device
//!   never becomes the controlling terminal), so a path swapped for a FIFO
//!   after the check still does not block; the opened handle is then checked again.
//! - A file larger than the limit, or one that grows while it is read, is bounded by reading at
//!   most one byte past the limit.
//! - Symlinks are followed: the checks apply to the target.
//!
//! # Known limitation
//!
//! `O_NONBLOCK` stays set on the handle of a regular file, because clearing it needs `fcntl`,
//! which would take `unsafe` code or an extra dependency. It has no effect on local file systems,
//! but a FUSE or network file system that honors it could make a read fail with `WouldBlock`,
//! which is reported as an I/O error.
//!
//! The returned bytes are a snapshot; nothing observes later changes to the file.

use std::fs::{File, Metadata, OpenOptions};
use std::io::Read;
use std::path::Path;

use thiserror::Error;

use crate::limits::{InputTooLarge, MaxInputBytes};

/// What a path that is not a regular file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotRegularKind {
    /// A directory.
    Directory,
    /// A FIFO, socket or device.
    Special,
}

impl NotRegularKind {
    fn of(metadata: &Metadata) -> Option<Self> {
        if metadata.is_dir() {
            Some(Self::Directory)
        } else if !metadata.is_file() {
            Some(Self::Special)
        } else {
            None
        }
    }

    /// Returns a message naming what the path is.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::Directory => "path is a directory, not a file",
            Self::Special => "path is not a regular file",
        }
    }
}

/// Why a file could not be read by [`read_regular_file`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ReadFileError {
    /// The file does not exist, cannot be opened or failed while being read.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// The path is a directory, FIFO, socket or device.
    #[error("{}", .0.message())]
    NotRegular(NotRegularKind),

    /// The file is larger than the limit.
    #[error(transparent)]
    TooLarge(#[from] InputTooLarge),
}

#[cfg(unix)]
fn open_non_blocking(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(path)
}

#[cfg(not(unix))]
fn open_non_blocking(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().read(true).open(path)
}

/// Reads a regular file of at most `max` bytes.
///
/// The size is checked against `max` before any content is read, and the read is capped at
/// `max + 1` bytes, so a file that grows after the size check is still rejected. See the
/// [module documentation](self) for the threat model.
///
/// # Errors
///
/// Returns [`ReadFileError::Io`] when the path does not exist or cannot be opened or read,
/// [`ReadFileError::NotRegular`] for a directory, FIFO, socket or device, and
/// [`ReadFileError::TooLarge`] when the file is larger than `max`.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::fs::{ReadFileError, read_regular_file};
/// use fast_yaml_core::limits::MaxInputBytes;
///
/// let dir = std::env::temp_dir();
/// assert!(matches!(
///     read_regular_file(&dir, MaxInputBytes::DEFAULT),
///     Err(ReadFileError::NotRegular(_))
/// ));
/// ```
pub fn read_regular_file(path: &Path, max: MaxInputBytes) -> Result<Vec<u8>, ReadFileError> {
    if let Some(kind) = NotRegularKind::of(&std::fs::metadata(path)?) {
        return Err(ReadFileError::NotRegular(kind));
    }
    let file = open_non_blocking(path)?;
    let metadata = file.metadata()?;
    if let Some(kind) = NotRegularKind::of(&metadata) {
        return Err(ReadFileError::NotRegular(kind));
    }
    let size = metadata.len();
    max.check_file_len(size)?;
    read_bounded(file, max, usize::try_from(size).unwrap_or_default())
}

/// Reads `reader` to its end, taking at most `max` bytes.
///
/// At most `max + 1` bytes are buffered, so an endless reader (standard input) cannot exhaust
/// memory, and one byte over the limit is enough to reject it. `capacity` is only an
/// allocation hint, such as the length a file system reports.
///
/// # Errors
///
/// Returns [`ReadFileError::Io`] when reading fails and [`ReadFileError::TooLarge`] when the
/// reader yields more than `max` bytes.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::fs::{ReadFileError, read_bounded};
/// use fast_yaml_core::limits::MaxInputBytes;
///
/// let max = MaxInputBytes::new(4).unwrap();
/// assert_eq!(read_bounded(&b"abcd"[..], max, 0).unwrap(), b"abcd");
/// assert!(matches!(
///     read_bounded(&b"abcde"[..], max, 0),
///     Err(ReadFileError::TooLarge(_))
/// ));
/// ```
pub fn read_bounded(
    reader: impl Read,
    max: MaxInputBytes,
    capacity: usize,
) -> Result<Vec<u8>, ReadFileError> {
    let mut bytes = Vec::with_capacity(capacity.min(max.get()));
    reader.take(max.get() as u64 + 1).read_to_end(&mut bytes)?;
    max.check(bytes.len())?;
    Ok(bytes)
}

/// A path rendered for a terminal, with control characters escaped.
///
/// A file name can hold ESC, BEL or newline bytes, so printing it raw lets whoever names a file
/// clear the screen, retitle the window or forge output lines. Every human-readable message that
/// names a path renders it through this type; machine formats (JSON, SARIF) keep the real
/// name, which they escape themselves. The escaping is the one diagnostic messages use, but a
/// path is never truncated. Bytes that are not valid UTF-8 print as U+FFFD.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use fast_yaml_core::fs::DisplayPath;
///
/// assert_eq!(DisplayPath::new(Path::new("a/b.yaml")).to_string(), "a/b.yaml");
/// assert_eq!(
///     DisplayPath::new(Path::new("x\u{1b}[2Jy\n.yaml")).to_string(),
///     "x\\u{1b}[2Jy\\n.yaml"
/// );
/// ```
#[derive(Debug, Clone, Copy)]
pub struct DisplayPath<'a>(&'a Path);

impl<'a> DisplayPath<'a> {
    /// Wraps `path` for display.
    #[must_use]
    pub const fn new(path: &'a Path) -> Self {
        Self(path)
    }
}

impl std::fmt::Display for DisplayPath<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use std::fmt::Write as _;
        for c in self.0.to_string_lossy().chars() {
            if c.is_control() {
                for escaped in c.escape_debug() {
                    f.write_char(escaped)?;
                }
            } else {
                f.write_char(c)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod display_path_tests {
    use super::*;

    #[test]
    fn escapes_terminal_sequences_in_a_path() {
        let path = Path::new("d/x\u{1b}]0;PWNED\u{7}\u{1b}[2Jy.yaml");
        let shown = DisplayPath::new(path).to_string();
        assert_eq!(shown, "d/x\\u{1b}]0;PWNED\\u{7}\\u{1b}[2Jy.yaml");
        assert!(!shown.chars().any(char::is_control));
    }

    #[test]
    fn leaves_ordinary_and_non_ascii_names_alone() {
        let path = Path::new("dir/\u{43a}\u{43b}\u{44e}\u{447} \u{65e5}.yaml");
        assert_eq!(
            DisplayPath::new(path).to_string(),
            path.display().to_string()
        );
    }

    #[test]
    fn does_not_truncate_a_long_path() {
        let long = "x".repeat(10_000);
        assert_eq!(DisplayPath::new(Path::new(&long)).to_string(), long);
    }

    #[cfg(unix)]
    #[test]
    fn escapes_c1_controls_and_invalid_utf8() {
        use std::os::unix::ffi::OsStrExt as _;
        let csi = Path::new("a\u{9b}31m.yaml");
        assert_eq!(DisplayPath::new(csi).to_string(), "a\\u{9b}31m.yaml");
        let bad = Path::new(std::ffi::OsStr::from_bytes(b"a\xffb.yaml"));
        assert_eq!(DisplayPath::new(bad).to_string(), "a\u{fffd}b.yaml");
    }
}
