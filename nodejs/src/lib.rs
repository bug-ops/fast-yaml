//! fast-yaml-nodejs: Fast YAML parser for Node.js, powered by Rust
//!
//! This module provides NAPI-RS bindings for the fast-yaml library, offering
//! high-performance YAML 1.2.2 parsing, linting, and parallel processing for Node.js.
//!
//! # Features
//!
//! - 5-10x faster than js-yaml
//! - YAML 1.2.2 compliance (Core Schema)
//! - Built-in linter with rich diagnostics
//! - Parallel processing for large files
//! - Full TypeScript support
//!
//! # Example
//!
//! ```javascript
//! const { version } = require('@fast-yaml/core');
//! console.log(version()); // "0.1.0"
//! ```

// Note: NAPI-RS uses unsafe code internally, so we can't forbid it here
#![warn(missing_docs)]

use napi_derive::napi;
use options::checked_u32;

mod batch;
mod conversion;
mod emitter;
mod limits;
mod lint;
mod options;
mod parallel;
mod parser;
mod rule_input;

// Re-export public API
pub use batch::{
    BatchConfig, BatchError, BatchResult, FileOutcome, FileResult, FormatResult, format_files,
    format_files_in_place, process_files,
};
pub use emitter::{DumpOptions, safe_dump, safe_dump_all};
pub use lint::{
    ContextLine, Diagnostic, DiagnosticContext, LintConfig, Linter, Location, Severity, Span,
    Suggestion, lint,
};
pub use parallel::{ParallelConfig, parse_parallel, parse_parallel_async};
pub use parser::{LoadOptions, load, load_all, safe_load, safe_load_all};

// ============================================================================
// Schema Types (js-yaml compatibility)
// ============================================================================

/// YAML schema types for parsing behavior (js-yaml compatible).
///
/// All schemas currently behave as `SAFE_SCHEMA` (safe by default).
/// The schema parameter is accepted for API compatibility with js-yaml.
#[napi(string_enum)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Schema {
    /// Safe schema - only safe data types (default).
    /// Equivalent to `PyYAML`'s `SafeLoader`.
    #[default]
    SafeSchema,
    /// JSON schema - strict JSON subset of YAML.
    JsonSchema,
    /// Core schema - YAML 1.2.2 Core Schema.
    CoreSchema,
    /// Failsafe schema - minimal safe subset.
    FailsafeSchema,
}

// ============================================================================
// Mark Class (error location tracking)
// ============================================================================

/// Represents a position in a YAML source file.
///
/// Used to indicate where errors occur during parsing.
///
/// # Example
///
/// ```javascript
/// const { Mark } = require('@fast-yaml/core');
///
/// const mark = new Mark('<input>', 5, 10);
/// console.log(mark.name);   // '<input>'
/// console.log(mark.line);   // 5
/// console.log(mark.column); // 10
/// console.log(mark.toString()); // '<input>:5:10'
/// ```
#[napi]
#[derive(Clone, Debug)]
pub struct Mark {
    /// The name of the source (e.g., filename or '<input>').
    #[napi(readonly)]
    pub name: String,
    /// The line number (0-indexed).
    #[napi(readonly)]
    pub line: u32,
    /// The column number (0-indexed).
    #[napi(readonly)]
    pub column: u32,
}

#[napi]
impl Mark {
    /// Create a new Mark instance.
    ///
    /// # Arguments
    ///
    /// * `name` - The source name (e.g., filename)
    /// * `line` - The line number (0-indexed)
    /// * `column` - The column number (0-indexed)
    ///
    /// # Errors
    ///
    /// Returns an `InvalidArg` error if `line` or `column` is not an integer in `0..=4294967295`.
    #[napi(constructor, catch_unwind)]
    pub fn new(name: String, line: f64, column: f64) -> napi::Result<Self> {
        Ok(Self {
            name,
            line: checked_u32("line", line)?,
            column: checked_u32("column", column)?,
        })
    }

    /// Get a string representation of the mark.
    ///
    /// Returns format: "name:line:column"
    #[napi(catch_unwind)]
    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        format!("{}:{}:{}", self.name, self.line, self.column)
    }
}

/// Get the library version.
///
/// Returns the version string of the fast-yaml-nodejs crate.
///
/// # Examples
///
/// ```javascript
/// const { version } = require('@fast-yaml/core');
/// console.log(version()); // "0.1.0"
/// ```
#[napi(catch_unwind)]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Panics on purpose so tests can verify panics surface as catchable JS errors.
///
/// Only compiled with the `test-panic` feature; never part of release builds.
#[cfg(feature = "test-panic")]
#[napi(catch_unwind)]
pub fn test_panic() -> String {
    panic!("test-panic: intentional panic");
}

/// Async counterpart of [`test_panic`] that panics on the libuv worker thread.
#[cfg(feature = "test-panic")]
#[napi(catch_unwind)]
pub fn test_panic_async() -> napi::bindgen_prelude::AsyncTask<parallel::PanicTask> {
    napi::bindgen_prelude::AsyncTask::new(parallel::PanicTask)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        let v = version();
        assert!(!v.is_empty());
        assert!(v.starts_with('0'));
    }

    #[test]
    fn test_schema_default() {
        let schema = Schema::default();
        assert_eq!(schema, Schema::SafeSchema);
    }

    #[test]
    fn test_schema_variants() {
        assert_ne!(Schema::SafeSchema, Schema::JsonSchema);
        assert_ne!(Schema::SafeSchema, Schema::CoreSchema);
        assert_ne!(Schema::SafeSchema, Schema::FailsafeSchema);
    }

    #[test]
    fn test_mark_new() {
        let mark = Mark::new("<input>".to_string(), 5.0, 10.0).unwrap();
        assert_eq!(mark.name, "<input>");
        assert_eq!(mark.line, 5);
        assert_eq!(mark.column, 10);
    }

    #[test]
    fn test_mark_to_string() {
        let mark = Mark::new("test.yaml".to_string(), 42.0, 15.0).unwrap();
        assert_eq!(mark.to_string(), "test.yaml:42:15");
    }

    #[test]
    fn test_mark_zero_indexed() {
        let mark = Mark::new("test.yaml".to_string(), 0.0, 0.0).unwrap();
        assert_eq!(mark.line, 0);
        assert_eq!(mark.column, 0);
        assert_eq!(mark.to_string(), "test.yaml:0:0");
    }
}
