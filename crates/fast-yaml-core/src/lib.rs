//! fast-yaml-core: Core YAML 1.2.2 parser and emitter.
//!
//! This crate provides the core functionality for parsing and emitting YAML documents,
//! wrapping the saphyr library with a consistent, stable API.
//!
//! # YAML 1.2.2 Compliance
//!
//! This library implements the YAML 1.2.2 specification with the Core Schema:
//!
//! - **Null**: `~`, `null`, `Null`, `NULL`, or empty value
//! - **Boolean**: `true`/`false` (case-insensitive) - NOT yes/no/on/off (YAML 1.1)
//! - **Integer**: Decimal, `0o` octal, `0x` hexadecimal
//! - **Float**: Standard notation, `.inf`, `-.inf`, `.nan`
//! - **String**: Plain, single-quoted, double-quoted, literal (`|`), folded (`>`)
//!
//! # Examples
//!
//! Parsing YAML:
//!
//! ```
//! use fast_yaml_core::Parser;
//!
//! let yaml = "name: test\nvalue: 123";
//! let doc = Parser::parse_str(yaml)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Emitting YAML:
//!
//! ```
//! use fast_yaml_core::{Emitter, Value, ScalarOwned};
//!
//! let value = Value::Value(ScalarOwned::String("test".to_string()));
//! let yaml = Emitter::emit_str(&value)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

/// YAML emitter for serializing documents to strings.
pub mod emitter;
/// Error types for parsing and emitting operations.
pub mod error;
/// YAML parser for deserializing strings to documents.
pub mod parser;
/// Value types representing YAML data structures.
pub mod value;

/// Streaming YAML formatter module.
///
/// Provides high-performance formatting by processing parser events directly
/// without building an intermediate DOM representation.
#[cfg(feature = "streaming")]
pub mod streaming;

pub use emitter::{Emitter, EmitterConfig};
pub use error::{EmitError, EmitResult, ParseError, ParseResult};
pub use parser::Parser;
pub use value::{Array, Map, OrderedFloat, ScalarOwned, Value};
