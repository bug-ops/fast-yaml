//! YAML linter with rich diagnostics
//!
#![forbid(unsafe_code)]
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
//! use fast_yaml_linter::{Linter, TextFormatter, Formatter};
//!
//! let yaml = r#"
//! name: John
//! age: 30
//! "#;
//!
//! let linter = Linter::with_all_rules();
//! let diagnostics = linter.lint(yaml).unwrap();
//!
//! let formatter = TextFormatter::new();
//! let output = formatter.format(&diagnostics, yaml);
//! println!("{}", output);
//! ```

mod context;
mod diagnostic;
mod linter;
mod location;
mod severity;

pub mod formatter;
pub mod rules;

pub use context::SourceContext;
pub use diagnostic::{
    ContextLine, Diagnostic, DiagnosticBuilder, DiagnosticCode, DiagnosticContext, Suggestion,
};
pub use formatter::{Formatter, TextFormatter};
pub use linter::{LintConfig, LintError, Linter};
pub use location::{Location, Span};
pub use severity::Severity;

#[cfg(feature = "json-output")]
pub use formatter::JsonFormatter;

#[cfg(feature = "sarif-output")]
pub use formatter::SarifFormatter;
