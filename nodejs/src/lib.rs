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
pub use parser::{safe_load, safe_load_all};

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
}
