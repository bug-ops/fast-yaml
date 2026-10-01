//! fast-yaml-core: Core YAML 1.2.2 parser and emitter.
//!
//! This crate provides the core functionality for parsing and emitting YAML documents,
//! with a resolved [`Value`] model that does not expose the underlying parser.
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
//! use fast_yaml_core::{Emitter, Value};
//!
//! let value = Value::String("test".to_string());
//! let yaml = Emitter::emit_str(&value)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![forbid(unsafe_code)]

/// Comment detection for inputs the formatter would strip comments from.
pub mod comments;
/// YAML emitter for serializing documents to strings.
pub mod emitter;
/// Byte-level input decoding with byte order mark detection.
pub mod encoding;
/// Error types for parsing and emitting operations.
pub mod error;
/// Guarded parser event stream for language bindings.
pub mod events;
/// Validated, BOM-normalized parser input.
pub mod input;
mod keys;
/// Resource limits (nesting depth, alias expansion) enforced while parsing.
pub mod limits;
/// YAML 1.1 merge key (`<<`) resolution shared by the core loader and bindings.
pub mod merge;
mod merge_check;
/// Load-time policy: key domain and repeated merge keys.
pub mod options;
/// YAML parser for deserializing strings to documents.
pub mod parser;
/// YAML 1.2 core-schema scalar resolution shared by the core loader and bindings.
pub mod scalar;
/// Resolved value types representing YAML data structures.
pub mod value;

/// Streaming YAML formatter module.
///
/// Provides high-performance formatting by processing parser events directly
/// without building an intermediate DOM representation.
pub mod streaming;

pub use comments::{CommentScanner, find_comments, has_comments, has_comments_normalized};
pub use emitter::{Emitter, EmitterConfig};
pub use encoding::{
    DecodeError, EncodingEvidence, UnsupportedEncoding, decode_input, decode_input_owned,
};
pub use error::{EmitError, EmitResult, ParseError, ParseResult, SourcePosition, SyntaxError};
pub use events::ScalarStyle;
pub use input::NormalizedInput;
pub use keys::{KeyError, KeyKind};
pub use limits::{
    DumpBudget, Indent, InputTooLarge, LimitKind, LimitRangeError, MaxAliasBytes, MaxDepth,
    MaxDocuments, MaxDumpNodes, MaxInputBytes, MaxOutputBytes, MaxTagBytes, ParseLimits,
    StreamBudget, Width,
};
pub use merge::{MergeError, MergeSource, MergeTarget, NodeRole, merge_into};
pub use options::{DuplicateMergeKeys, KeyDomain, LoadOptions};
pub use parser::{Parser, strip_bom};
pub use scalar::{BigIntRef, IntRadix, ResolvedScalar, resolve_scalar};
pub use value::{BigInt, Float, Mapping, Set, Value};
