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
//!   is opened, and on Unix the file is opened with `O_NONBLOCK`, so a path swapped for a FIFO
//!   after the check still does not block; the opened handle is then checked again.
//! - A file larger than the limit, or one that grows while it is read, is bounded by reading at
//!   most one byte past the limit.
//! - Symlinks are followed: the checks apply to the target.
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
        .custom_flags(libc::O_NONBLOCK)
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

    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or_default());
    file.take(max.get() as u64 + 1).read_to_end(&mut bytes)?;
    max.check(bytes.len())?;
    Ok(bytes)
}
