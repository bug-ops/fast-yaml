//! The typed payload of the I/O errors the atomic writer builds itself.

use fast_yaml_core::fs::DisplayPath;
use std::io;
use std::path::PathBuf;

use super::{HardLinkRefused, TargetChanged};

/// The payload of every [`io::Error`] the atomic writer builds itself, as opposed to one the
/// operating system reported.
///
/// The writer's functions return [`io::Error`] and keep this enum behind it, so a caller tells
/// the failures apart with [`io::Error::get_ref`] and `downcast_ref` instead of matching message
/// text. A variant that has a cause shows it in its message and returns it from
/// [`source`](std::error::Error::source); `io::Error`'s own `source` skips the payload, so read the chain
/// from the payload.
///
/// # Examples
///
/// ```
/// use fast_yaml_parallel::{AtomicWriteError, HardLinkPolicy, write_atomic};
///
/// # #[cfg(unix)] {
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("a.yaml");
/// std::fs::write(&path, "a: 1\n")?;
/// std::fs::hard_link(&path, dir.path().join("b.yaml"))?;
///
/// let err = write_atomic(&path, b"a: 2\n", HardLinkPolicy::Refuse).unwrap_err();
/// let payload = err.get_ref().and_then(|e| e.downcast_ref::<AtomicWriteError>());
/// assert!(matches!(payload, Some(AtomicWriteError::HardLinkRefused(_))));
/// # }
/// # Ok::<(), std::io::Error>(())
/// ```
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AtomicWriteError {
    /// The target is no longer the file that was read or resolved. Run the command again.
    #[error(transparent)]
    TargetChanged(#[from] TargetChanged),

    /// The target has several hard links and the policy refuses to write through them.
    #[error(transparent)]
    HardLinkRefused(#[from] HardLinkRefused),

    /// The target resolves to something that is not a regular file.
    #[error("not a regular file: {}", DisplayPath::new(.path))]
    NotRegular {
        /// The path as given by the caller.
        path: PathBuf,
    },

    /// The target is a symlink whose destination does not exist.
    #[error("refused: dangling symlink {}", DisplayPath::new(.path))]
    DanglingSymlink {
        /// The path as given by the caller.
        path: PathBuf,
    },

    /// The temporary file next to the target could not be created.
    #[error("cannot create temporary file in {}: {source}", DisplayPath::new(.dir))]
    TempFile {
        /// The directory the temporary file was to be created in.
        dir: PathBuf,
        /// The operating system's error.
        #[source]
        source: io::Error,
    },

    /// The `Zone.Identifier` stream of the target is too large to copy.
    #[error("Zone.Identifier of {} is too large to copy", DisplayPath::new(.path))]
    ZoneIdentifierTooLarge {
        /// The file whose stream was refused.
        path: PathBuf,
    },
}

impl AtomicWriteError {
    fn kind(&self) -> io::ErrorKind {
        match self {
            Self::TargetChanged(_) | Self::ZoneIdentifierTooLarge { .. } => io::ErrorKind::Other,
            Self::HardLinkRefused(_) | Self::NotRegular { .. } => io::ErrorKind::InvalidInput,
            Self::DanglingSymlink { .. } => io::ErrorKind::NotFound,
            Self::TempFile { source, .. } => source.kind(),
        }
    }
}

impl From<AtomicWriteError> for io::Error {
    fn from(error: AtomicWriteError) -> Self {
        Self::new(error.kind(), error)
    }
}
