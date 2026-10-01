//! Bounded file input.
//!
//! [`read_file`] reads a whole file into memory after checking its size, so every parser sees
//! a snapshot that no other process can change underneath it. Memory mapping is deliberately
//! absent: a file truncated or rewritten while mapped crashes or corrupts the reader.
//!
//! # Examples
//!
//! ```
//! use fast_yaml_core::limits::MaxInputBytes;
//! use fast_yaml_parallel::read_file;
//!
//! # let temp_file = tempfile::NamedTempFile::new().unwrap();
//! # std::fs::write(temp_file.path(), "key: value\n").unwrap();
//! let yaml_str = read_file(temp_file.path(), MaxInputBytes::DEFAULT)?;
//! # Ok::<(), fast_yaml_parallel::Error>(())
//! ```

pub mod reader;

pub use reader::read_file;
