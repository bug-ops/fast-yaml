//! YAML linter with rich diagnostics
//!
#![forbid(unsafe_code)]
#![cfg_attr(not(test), warn(clippy::string_slice, clippy::indexing_slicing))]
//!
//! This crate provides a comprehensive YAML linting engine with:
//! - Precise error location tracking (line, column, byte offset)
//! - Rich diagnostic messages with source context
//! - Pluggable rule system for extensibility
//! - Multiple output formats (text, JSON, SARIF)
//!
//! # Examples
//!
//! ```
//! use fast_yaml_linter::formatter::Findings;
//! use fast_yaml_linter::{Formatter, Linter, TextFormatter};
//!
//! let yaml = r#"
//! name: John
//! age: 30
//! "#;
//!
//! let linter = Linter::with_all_rules();
//! let source = linter.source(yaml).unwrap();
//! let diagnostics = linter.lint_source(&source).unwrap();
//!
//! let context = source.context();
//! let formatter = TextFormatter::new();
//! let output = formatter.format(Findings::FromSource {
//!     diagnostics: &diagnostics,
//!     source: &context,
//! });
//! println!("{}", output);
//! ```

mod comments;
mod context;
mod diagnostic;
mod directives;
mod echo;
mod lint_source;
mod linter;
mod location;
mod nodes;
mod scan;
mod set_members;
mod severity;

pub mod config;
pub mod formatter;
pub mod rules;
pub mod source;
mod tokenizer;

pub use comments::{Comment, CommentKind};
pub use config::{ConfigFile, ConfigFileError};
pub use context::{LineMetadata, LintContext, MAX_CONTEXT_COLUMNS, SourceContext};
pub use diagnostic::{
    ContextLine, Diagnostic, DiagnosticBuilder, DiagnosticCode, DiagnosticContext, Excerpt,
    Finding, Suggestion,
};
pub use formatter::{Formatter, TextFormatter};
pub use lint_source::LintSource;
pub use linter::{LintConfig, LintError, Linter};
pub use location::{Location, Span};
pub use severity::{ParseSeverityError, Severity};

#[cfg(feature = "json-output")]
pub use formatter::JsonFormatter;
