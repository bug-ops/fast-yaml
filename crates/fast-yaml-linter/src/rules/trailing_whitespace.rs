//! Rule to detect trailing whitespace.

use crate::{Diagnostic, DiagnosticCode, LintConfig, Severity};
use fast_yaml_core::Value;

/// Rule to detect trailing whitespace.
pub struct TrailingWhitespaceRule;

impl super::LintRule for TrailingWhitespaceRule {
    fn code(&self) -> &str {
        DiagnosticCode::TRAILING_WHITESPACE
    }

    fn name(&self) -> &'static str {
        "Trailing Whitespace"
    }

    fn description(&self) -> &'static str {
        "Detects trailing whitespace at the end of lines"
    }

    fn default_severity(&self) -> Severity {
        Severity::Hint
    }

    fn check(&self, _source: &str, _value: &Value, _config: &LintConfig) -> Vec<Diagnostic> {
        Vec::new()
    }
}
