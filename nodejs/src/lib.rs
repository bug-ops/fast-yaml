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

mod conversion;
mod emitter;
mod parser;

// Re-export public API
pub use emitter::{DumpOptions, safe_dump, safe_dump_all};
pub use parser::{safe_load, safe_load_all, load, load_all, LoadOptions};

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
    #[napi(constructor)]
    pub fn new(name: String, line: u32, column: u32) -> Self {
        Self { name, line, column }
    }

    /// Get a string representation of the mark.
    ///
    /// Returns format: "name:line:column"
    #[napi]
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
#[napi]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
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
        let mark = Mark::new("<input>".to_string(), 5, 10);
        assert_eq!(mark.name, "<input>");
        assert_eq!(mark.line, 5);
        assert_eq!(mark.column, 10);
    }

    #[test]
    fn test_mark_to_string() {
        let mark = Mark::new("test.yaml".to_string(), 42, 15);
        assert_eq!(mark.to_string(), "test.yaml:42:15");
    }

    #[test]
    fn test_mark_zero_indexed() {
        let mark = Mark::new("test.yaml".to_string(), 0, 0);
        assert_eq!(mark.line, 0);
        assert_eq!(mark.column, 0);
        assert_eq!(mark.to_string(), "test.yaml:0:0");
    }
}
