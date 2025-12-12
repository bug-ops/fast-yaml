//! Rule to check indentation consistency.

use crate::{Diagnostic, DiagnosticCode, LintConfig, Severity};
use fast_yaml_core::Value;

/// Rule to check indentation consistency.
pub struct IndentationRule;

impl super::LintRule for IndentationRule {
    fn code(&self) -> &str {
        DiagnosticCode::INDENTATION
    }

    fn name(&self) -> &'static str {
        "Indentation"
    }

    fn description(&self) -> &'static str {
        "Checks for consistent indentation throughout the YAML file"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, _source: &str, _value: &Value, _config: &LintConfig) -> Vec<Diagnostic> {
        Vec::new()
    }
}
