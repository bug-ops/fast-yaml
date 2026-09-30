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

/// Comment detection for inputs the formatter would strip comments from.
pub mod comments;
/// YAML emitter for serializing documents to strings.
pub mod emitter;
/// Error types for parsing and emitting operations.
pub mod error;
/// Resource limits (nesting depth, alias expansion) enforced while parsing.
pub mod limits;
/// YAML 1.1 merge key (`<<`) resolution shared by the core loader and bindings.
pub mod merge;
/// YAML parser for deserializing strings to documents.
pub mod parser;
/// YAML 1.2 core-schema scalar resolution shared by the core loader and bindings.
pub mod scalar;
/// Value types representing YAML data structures.
pub mod value;

/// Streaming YAML formatter module.
///
/// Provides high-performance formatting by processing parser events directly
/// without building an intermediate DOM representation.
pub mod streaming;

pub use comments::{find_comments, has_comments};
pub use emitter::{Emitter, EmitterConfig};
pub use error::{EmitError, EmitResult, ParseError, ParseResult};
pub use limits::{
    DumpBudget, LimitGuard, LimitKind, LimitRangeError, MaxAliasBytes, MaxDepth, MaxDumpNodes,
    MaxOutputBytes, MaxTagBytes, ParseLimits, StreamBudget,
};
pub use merge::{MergeError, MergeSource, MergeTarget, merge_into};
pub use parser::{Parser, canonicalize, reject_nul, strip_bom};
pub use scalar::{BigInt, IntRadix, ResolvedScalar, resolve_scalar};
pub use value::{Array, Map, OrderedFloat, ScalarOwned, Value};
