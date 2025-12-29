//! Rule to check spacing after list item hyphens.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    Span,
    tokenizer::{FlowTokenizer, TokenType},
};
use fast_yaml_core::Value;

/// Linting rule for hyphen spacing.
///
/// Validates spacing after list item hyphens `-`.
///
/// Configuration options:
/// - `max-spaces-after`: integer (default: 1)
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::HyphensRule, rules::LintRule, LintConfig, config::RuleConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = HyphensRule;
/// let yaml = "- item1\n- item2";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::new()
///     .with_rule_config("hyphens", RuleConfig::new().with_option("max-spaces-after", 1i64));
///
/// let diagnostics = rule.check(yaml, &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct HyphensRule;

impl super::LintRule for HyphensRule {
    fn code(&self) -> &str {
        DiagnosticCode::HYPHENS
    }

    fn name(&self) -> &'static str {
        "Hyphens"
    }

    fn description(&self) -> &'static str {
        "Validates spacing after list item hyphens"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let source = context.source();
        let source_context = context.source_context();
        let tokenizer = FlowTokenizer::new(source, source_context);

        let rule_config = config.get_rule_config(self.code());
        let max_spaces_after = rule_config
            .and_then(|rc| rc.options.get_int("max-spaces-after"))
            .unwrap_or(1);

        let mut diagnostics = Vec::new();
        let hyphens = tokenizer.find_all(TokenType::Hyphen);

        for hyphen in hyphens {
            if let Some(diag) = check_spaces_after_hyphen(
                source,
                hyphen.span.start.offset,
                max_spaces_after,
                self.code(),
                config,
            ) {
                diagnostics.push(diag);
            }
        }

        diagnostics
    }
}

/// Checks spaces after a hyphen.
fn check_spaces_after_hyphen(
    source: &str,
    hyphen_offset: usize,
    max_spaces: i64,
    code: &str,
    config: &LintConfig,
) -> Option<Diagnostic> {
    if hyphen_offset + 1 >= source.len() {
        return None;
    }

    // Count spaces after hyphen
    let mut spaces = 0;
    let mut offset = hyphen_offset + 1;

    while offset < source.len() {
        if let Some(ch) = source.chars().nth(offset) {
            if ch == ' ' {
                spaces += 1;
                offset += 1;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_lossless
    )]
    let spaces_i64 = spaces as i64;

    // Require at least one space after hyphen (unless it's at end of line)
    if spaces == 0 && offset < source.len() {
        let next_char = source.chars().nth(offset);
        if let Some(ch) = next_char
            && ch != '\n'
            && ch != '\r'
        {
            let severity = config.get_effective_severity(code, Severity::Warning);
            let loc = Location::new(1, 1, hyphen_offset + 1);
            let span = Span::new(loc, loc);

            return Some(
                DiagnosticBuilder::new(code, severity, "missing space after hyphen", span)
                    .build(source),
            );
        }
    }

    if max_spaces >= 0 && spaces_i64 > max_spaces {
        let severity = config.get_effective_severity(code, Severity::Warning);
        let loc = Location::new(1, 1, hyphen_offset + 1);
        let span = Span::new(loc, loc);

        return Some(
            DiagnosticBuilder::new(
                code,
                severity,
                format!(
                    "too many spaces after hyphen (expected at most {max_spaces}, found {spaces})"
                ),
                span,
            )
            .build(source),
        );
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::RuleConfig, rules::LintRule};
    use fast_yaml_core::Parser;

    #[test]
    fn test_hyphens_default_valid() {
        let yaml = "- item1\n- item2\n- item3";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_hyphens_missing_space() {
        let yaml = "-item1\n-item2";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("missing space"));
    }

    #[test]
    fn test_hyphens_too_many_spaces() {
        let yaml = "-  item1\n-  item2";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("too many spaces"));
    }

    #[test]
    fn test_hyphens_allow_multiple_spaces() {
        let yaml = "-  item1\n-  item2";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = LintConfig::new().with_rule_config(
            "hyphens",
            RuleConfig::new().with_option("max-spaces-after", 2i64),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_hyphens_nested_lists() {
        let yaml = "- item1\n  - nested1\n  - nested2\n- item2";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_hyphens_indented_lists() {
        let yaml = "list:\n  - item1\n  - item2";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_hyphens_empty_list_item() {
        let yaml = "-\n-";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Empty list items (hyphen at end of line) are allowed
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_hyphens_list_with_mappings() {
        let yaml = "- name: John\n  age: 30\n- name: Jane\n  age: 25";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_hyphens_mixed_violations() {
        let yaml = "-item1\n-  item2\n- item3";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = HyphensRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Should have 2 violations (missing space and too many spaces)
        assert_eq!(diagnostics.len(), 2);
    }
}
