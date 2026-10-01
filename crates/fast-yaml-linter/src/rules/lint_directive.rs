//! Registration of the `lint-directive` rule.

use super::RuleId;
use crate::config::RuleName;
use crate::{Diagnostic, LintConfig, LintContext, Severity};
use fast_yaml_core::Value;

/// Settings holder for problems in inline lint directives.
///
/// Its diagnostics are produced by the directive scanner (see `crate::directives`), which
/// needs the whole comment set; the rule itself reports nothing and exists so that
/// `lint-directive` is enabled and given a severity like every other rule.
pub struct LintDirectiveRule;

impl super::LintRule for LintDirectiveRule {
    fn id(&self) -> RuleId<'_> {
        RuleId::BuiltIn(RuleName::LintDirective)
    }

    fn name(&self) -> &'static str {
        "Lint Directive"
    }

    fn description(&self) -> &'static str {
        "Reports invalid inline lint directives (config-only, never suppressible)"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(
        &self,
        _context: &LintContext,
        _value: &Value,
        _config: &LintConfig,
    ) -> Vec<Diagnostic> {
        Vec::new()
    }
}
