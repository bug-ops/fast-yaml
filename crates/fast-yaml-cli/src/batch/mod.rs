//! Batch file processing for fast-yaml CLI.
//!
//! This module provides efficient multi-file processing capabilities by eliminating
//! process spawn overhead and leveraging internal Rayon parallelism.
//!
//! # Architecture
//!
//! The batch processing system consists of two phases:
//!
//! ## Phase 1: File Discovery
//!
//! The `discovery` module efficiently locates YAML files through:
//! - Direct file/directory paths
//! - Glob pattern expansion
//! - `.gitignore` respect via `ignore` crate
//! - Configurable include/exclude patterns
//!
//! ## Phase 2: Parallel Processing
//!
//! The `processor` module handles parallel file processing with:
//! - Rayon-based work-stealing parallelism
//! - Smart file reading (in-memory vs memory-mapped)
//! - Atomic file writes (temp + rename)
//! - Continue-on-error semantics
//!
//! # Example
//!
//! ```no_run
//! use fast_yaml_cli::batch::{FileDiscovery, DiscoveryConfig, ProcessingConfig, process_batch};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Phase 1: Discover files
//! let discovery_config = DiscoveryConfig::default();
//! let discovery = FileDiscovery::new(discovery_config);
//! let files = discovery.discover_paths(&[".".into()])?;
//!
//! // Phase 2: Process files
//! let processing_config = ProcessingConfig::new()
//!     .with_in_place(true)
//!     .with_workers(4);
//! let result = process_batch(files, processing_config);
//!
//! println!("Processed {} files ({} formatted, {} failed)",
//!     result.total, result.formatted, result.failed);
//! # Ok(())
//! # }
//! ```

pub mod config;
pub mod discovery;
pub mod error;
pub mod processor;
pub mod reader;
pub mod result;

// Phase 1: Discovery exports

// Phase 2: Processing exports
