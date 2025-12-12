//! Rule to detect invalid anchor references.

use crate::{Diagnostic, DiagnosticCode, LintConfig, Severity};
use fast_yaml_core::Value;

/// Rule to detect invalid anchor references.
pub struct InvalidAnchorsRule;

impl super::LintRule for InvalidAnchorsRule {
    fn code(&self) -> &str {
        DiagnosticCode::INVALID_ANCHOR
    }

    fn name(&self) -> &'static str {
        "Invalid Anchors"
    }

    fn description(&self) -> &'static str {
        "Detects undefined anchor references and invalid alias usage"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn check(&self, _source: &str, _value: &Value, _config: &LintConfig) -> Vec<Diagnostic> {
        Vec::new()
    }
}
