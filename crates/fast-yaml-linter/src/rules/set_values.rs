//! Rule to detect `!!set` members that carry a value.

use super::{LintRule, RuleId};
use crate::config::RuleName;
use crate::echo::{KEY_LIMIT, echo};
use crate::set_members::SetMember;
use crate::{Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity};

/// Rule to detect `!!set` members that carry a value.
///
/// A `!!set` is a mapping whose values are all null (`? a` or `{a, b}`). A member with any other
/// value is reported at its key, every one of them, so the rest of the file is still linted. The
/// loader drops such values, so the data silently differs from what was written.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::Linter;
///
/// let diagnostics = Linter::with_all_rules().lint("!!set {a: 1, b}\n").unwrap();
/// assert!(diagnostics.iter().any(|d| d.code.as_str() == "set-values"));
/// ```
pub struct SetValuesRule;

impl super::LintRule for SetValuesRule {
    fn id(&self) -> RuleId<'_> {
        RuleId::BuiltIn(RuleName::SetValues)
    }

    fn name(&self) -> &'static str {
        "Set Values"
    }

    fn description(&self) -> &'static str {
        "Detects !!set members that carry a value"
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }
}

impl super::SourceRule for SetValuesRule {
    fn check(&self, context: &LintContext, config: &LintConfig) -> Vec<Diagnostic> {
        let severity = config.rules.set_values.severity_or(self.default_severity());
        let source_context = context.source_context();
        context
            .set_members()
            .iter()
            .map(|SetMember { key, range }| {
                let span = source_context.span_of_bytes(*range);
                let member = key
                    .as_deref()
                    .map_or_else(String::new, |key| format!(" '{}'", echo(key, KEY_LIMIT)));
                let message =
                    format!("!!set member{member} has a value; set members are keys only");
                DiagnosticBuilder::new(DiagnosticCode::SET_VALUES, severity, message, span).build()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::SourceRule;
    use crate::set_members::may_contain_set;

    fn run(yaml: &str) -> Vec<Diagnostic> {
        SetValuesRule.check(&LintContext::new(yaml), &LintConfig::default())
    }

    fn lines(yaml: &str) -> Vec<usize> {
        run(yaml).iter().map(|d| d.span.start.line).collect()
    }

    #[test]
    fn valueless_sets_are_clean() {
        for yaml in [
            "!!set {a, b}\n",
            "!!set\n? a\n? b\n",
            "!!set {a: ~, b: null, c: !!null ''}\n",
            "!!set {a: , b}\n",
            "x: &n ~\ns: !!set {a: *n}\n",
        ] {
            assert!(run(yaml).is_empty(), "{yaml}");
        }
    }

    #[test]
    fn plain_mappings_may_have_values() {
        assert_eq!(run("a: 1\nb: [1, 2]\n"), []);
    }

    #[test]
    fn scalar_value_is_reported_at_the_key() {
        let diags = run("!!set {a: 1}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code.as_str(), "set-values");
        assert_eq!(diags[0].severity, Severity::Error);
        assert_eq!(diags[0].span.start.column, 8);
        assert!(diags[0].message.contains("'a'"));
    }

    #[test]
    fn block_set_value_is_reported() {
        assert_eq!(lines("!!set\n? a\nb: 1\n"), vec![3]);
    }

    #[test]
    fn explicit_key_with_a_value_is_reported() {
        assert_eq!(lines("a: !!set\n  ? x\n  ? y\n  : 1\n"), vec![3]);
        assert_eq!(lines("a: !!set\n  ? [y]\n  : 1\n"), vec![2]);
    }

    #[test]
    fn forms_the_loader_accepts_are_clean() {
        for yaml in [
            "a: !!set [x]\n",
            "a: !!set {}\n",
            "a: !!set\n",
            "a: !!set\n  ? y : 1\n",
        ] {
            assert!(run(yaml).is_empty(), "{yaml}");
        }
    }

    #[test]
    fn long_key_is_truncated_in_the_message() {
        let yaml = format!("!!set {{{}: 1}}\n", "k".repeat(10_000));
        let diags = run(&yaml);
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0].message.chars().count() < 200,
            "{}",
            diags[0].message
        );
    }

    #[test]
    fn verbatim_and_redefined_handle_sets_are_found() {
        assert_eq!(run("!<tag:yaml.org,2002:set> {a: 1}\n").len(), 1);
        assert_eq!(
            run("%TAG !e! tag:yaml.org,2002:\n---\n!e!set {a: 1}\n").len(),
            1
        );
    }

    #[test]
    fn guard_skips_files_without_a_tag() {
        assert!(!may_contain_set("settings: 1\noffset: 2\n"));
        assert!(!may_contain_set("a: !!str 1\n"));
        assert!(may_contain_set("a: !!set {x}\n"));
        assert!(may_contain_set(
            "%TAG !e! tag:yaml.org,2002:\n---\n!e!set {x}\n"
        ));
        assert_eq!(run("settings: {a: 1}\n"), []);
    }

    #[test]
    fn empty_quoted_value_is_not_null() {
        assert_eq!(run("!!set {a: ''}\n").len(), 1);
    }

    #[test]
    fn alias_value_follows_its_anchor() {
        assert_eq!(run("x: &n ~\ns: !!set {a: *n}\n"), []);
        assert_eq!(run("x: &v 1\ns: !!set {a: *v}\n").len(), 1);
    }

    #[test]
    fn collection_values_are_reported() {
        assert_eq!(run("!!set {a: [1], b: {c: d}}\n").len(), 2);
        assert_eq!(run("x: &m {k: v}\ns: !!set {a: *m}\n").len(), 1);
    }

    #[test]
    fn every_violation_is_reported() {
        assert_eq!(lines("!!set\na: 1\nb:\nc: 3\nd: x\n"), vec![2, 4, 5]);
    }

    #[test]
    fn nested_set_inside_set_member_is_scanned() {
        assert_eq!(run("!!set {a: !!set {b: 1}}\n").len(), 2);
    }

    #[test]
    fn sets_in_later_documents_are_scanned() {
        assert_eq!(run("a: 1\n---\n!!set {x: 1}\n").len(), 1);
    }

    #[test]
    fn linter_keeps_other_diagnostics_in_the_same_file() {
        use crate::Linter;
        let diagnostics = Linter::with_all_rules()
            .lint("s: !!set {a: 1}\nk: 1\nk: 2\n")
            .unwrap();
        let codes: Vec<_> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(codes.contains(&"set-values"));
        assert!(codes.contains(&"duplicate-key"));
    }

    #[test]
    fn disabled_rule_is_skipped() {
        use crate::{Linter, config::RuleName};
        let config = LintConfig::new().with_disabled_rule(RuleName::SetValues);
        let diagnostics = Linter::with_config(config).lint("!!set {a: 1}\n").unwrap();
        assert!(!diagnostics.iter().any(|d| d.code.as_str() == "set-values"));
    }

    #[test]
    fn disable_line_directive_suppresses_the_member() {
        use crate::Linter;
        let diagnostics = Linter::with_all_rules()
            .lint("!!set {a: 1} # fy: disable-line set-values\n")
            .unwrap();
        assert!(!diagnostics.iter().any(|d| d.code.as_str() == "set-values"));
    }
}
