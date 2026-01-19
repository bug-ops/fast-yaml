//! File-level parallelism for batch YAML operations.
//!
//! This module provides [`FileProcessor`] for processing multiple YAML files
//! in parallel using Rayon's work-stealing scheduler.
//!
//! # Features
//!
//! - **Parallel processing**: Automatic parallelism for large batches
//! - **Smart reading**: Integrates with [`SmartReader`](crate::SmartReader) for optimal I/O
//! - **Batch results**: Detailed success/failure/changed tracking
//! - **Security**: `DoS` protection via file size limits
//!
//! # Automatic Parallelism
//!
//! The processor automatically chooses between sequential and parallel processing:
//! - **Sequential**: < 4 files OR < 1MB total AND < 10 files
//! - **Parallel**: Otherwise (uses Rayon thread pool)
//!
//! # Key Types
//!
//! - [`FileProcessor`] - Main processor for batch file operations
//!
//! # Examples
//!
//! ```
//! use fast_yaml_parallel::FileProcessor;
//! use std::path::PathBuf;
//!
//! let processor = FileProcessor::new();
//! # let temp_dir = tempfile::tempdir().unwrap();
//! # let path1 = temp_dir.path().join("file1.yaml");
//! # std::fs::write(&path1, "key: value\n").unwrap();
//! let paths = vec![path1];
//! let result = processor.parse_files(&paths);
//!
//! assert!(result.is_success());
//! ```

mod processor;

pub use processor::FileProcessor;
