//! Smart file I/O with automatic strategy selection.
//!
//! This module provides [`SmartReader`] which automatically chooses
//! between in-memory reading and memory-mapped files based on file size.
//!
//! # Strategy
//!
//! - **Small files** (< 512KB by default): Loaded into memory as `String`
//! - **Large files** (>= 512KB): Memory-mapped for zero-copy access
//! - **Fallback**: Falls back to `read_to_string` if mmap fails
//!
//! # Key Types
//!
//! - [`SmartReader`] - Adaptive file reader with configurable threshold
//! - [`FileContent`] - Content container (String or Mmap)
//!
//! # Examples
//!
//! ```
//! use fast_yaml_parallel::SmartReader;
//!
//! let reader = SmartReader::new();
//! # let temp_file = tempfile::NamedTempFile::new().unwrap();
//! # std::fs::write(temp_file.path(), "key: value\n").unwrap();
//! let content = reader.read(temp_file.path())?;
//! let yaml_str = content.as_str()?;
//! # Ok::<(), fast_yaml_parallel::Error>(())
//! ```

pub mod reader;

pub use reader::{FileContent, SmartReader};
