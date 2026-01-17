//! Unified configuration module for CLI commands.
//!
//! This module provides composable configuration types that eliminate
//! duplication across commands while maintaining backward compatibility.

mod common;
mod formatter;
mod io;
mod output;
mod parallel;

pub use common::CommonConfig;
pub use formatter::FormatterConfig;
pub use io::IoConfig;
pub use output::OutputConfig;
pub use parallel::ParallelConfig;
