//! Unified configuration module for CLI commands.
//!
//! This module provides composable configuration types that eliminate
//! duplication across commands while maintaining backward compatibility.

mod common;
mod formatter;
mod io;
mod output;

pub use common::CommonConfig;
pub use formatter::FormatterConfig;
pub use io::IoConfig;
pub use output::OutputConfig;

// Re-export Config from fast-yaml-parallel as ParallelConfig for compatibility
pub use fast_yaml_parallel::Config as ParallelConfig;
