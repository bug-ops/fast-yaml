//! Rule to check flow sequence brackets `[]` formatting.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity,
    rules::flow_common::{
        check_spaces_after_opening, check_spaces_before_closing, is_empty_collection,
    },
    tokenizer::{FlowTokenizer, TokenType},
};
use fast_yaml_core::Value;

/// Linting rule for flow sequence brackets.
///
/// Validates spacing and usage of flow sequences `[]`.
///
/// Configuration options:
/// - `forbid`: "no" | "non-empty" | "all" (default: "no")
/// - `min-spaces-inside`: integer (default: 0)
/// - `max-spaces-inside`: integer (default: 0)
/// - `min-spaces-inside-empty`: integer (default: -1, disabled)
/// - `max-spaces-inside-empty`: integer (default: -1, disabled)
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::BracketsRule, rules::LintRule, LintConfig, config::RuleConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = BracketsRule;
/// let yaml = "list: [1, 2, 3]";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::new()
///     .with_rule_config("brackets", RuleConfig::new().with_option("forbid", "no"));
///
/// let diagnostics = rule.check(yaml, &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct BracketsRule;

impl super::LintRule for BracketsRule {
    fn code(&self) -> &str {
        DiagnosticCode::BRACKETS
    }

    fn name(&self) -> &'static str {
        "Brackets"
    }

    fn description(&self) -> &'static str {
        "Validates spacing and usage of flow sequence brackets []"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    #[allow(clippy::too_many_lines)]
    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let source = context.source();
        let source_context = context.source_context();
        let tokenizer = FlowTokenizer::new(source, source_context);

        let rule_config = config.get_rule_config(self.code());
        let forbid = rule_config
            .and_then(|rc| rc.options.get_string("forbid"))
            .unwrap_or("no");

        let min_spaces_inside = rule_config
            .and_then(|rc| rc.options.get_int("min-spaces-inside"))
            .unwrap_or(0);

        let max_spaces_inside = rule_config
            .and_then(|rc| rc.options.get_int("max-spaces-inside"))
            .unwrap_or(0);

        let min_spaces_inside_empty = rule_config
            .and_then(|rc| rc.options.get_int("min-spaces-inside-empty"))
            .unwrap_or(-1);

        let max_spaces_inside_empty = rule_config
            .and_then(|rc| rc.options.get_int("max-spaces-inside-empty"))
            .unwrap_or(-1);

        let mut diagnostics = Vec::new();

        let open_brackets = tokenizer.find_all(TokenType::BracketOpen);
        let close_brackets = tokenizer.find_all(TokenType::BracketClose);

        // Check forbid option
        match forbid {
            "all" => {
                for token in &open_brackets {
                    let severity =
                        config.get_effective_severity(self.code(), self.default_severity());
                    diagnostics.push(
                        DiagnosticBuilder::new(
                            self.code(),
                            severity,
                            "flow sequence forbidden (forbid: all)",
                            token.span,
                        )
                        .build(source),
                    );
                }
                return diagnostics;
            }
            "non-empty" => {
                // Check if sequence is non-empty
                for (i, open) in open_brackets.iter().enumerate() {
                    if let Some(close) = close_brackets.get(i)
                        && !is_empty_collection(
                            source,
                            open.span.end.offset,
                            close.span.start.offset,
                        )
                    {
                        let severity =
                            config.get_effective_severity(self.code(), self.default_severity());
                        diagnostics.push(
                            DiagnosticBuilder::new(
                                self.code(),
                                severity,
                                "non-empty flow sequence forbidden (forbid: non-empty)",
                                open.span,
                            )
                            .build(source),
                        );
                    }
                }
                return diagnostics;
            }
            _ => {} // "no" - continue with spacing checks
        }

        // Check spacing
        for (i, open) in open_brackets.iter().enumerate() {
            if let Some(close) = close_brackets.get(i) {
                let is_empty =
                    is_empty_collection(source, open.span.end.offset, close.span.start.offset);

                let (min_spaces, max_spaces) = if is_empty && min_spaces_inside_empty >= 0 {
                    (min_spaces_inside_empty, max_spaces_inside_empty)
                } else {
                    (min_spaces_inside, max_spaces_inside)
                };

                // Check spaces after opening bracket
                if let Some(diag) = check_spaces_after_opening(
                    source,
                    open.span.end.offset,
                    close.span.start.offset,
                    min_spaces,
                    max_spaces,
                    self.code(),
                    config,
                    "brackets",
                ) {
                    diagnostics.push(diag);
                }

                // Check spaces before closing bracket
                if let Some(diag) = check_spaces_before_closing(
                    source,
                    open.span.end.offset,
                    close.span.start.offset,
                    min_spaces,
                    max_spaces,
                    self.code(),
                    config,
                    "brackets",
                ) {
                    diagnostics.push(diag);
                }
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::RuleConfig, rules::LintRule};
    use fast_yaml_core::Parser;

    #[test]
    fn test_brackets_default_valid() {
        let yaml = "list: [1, 2, 3]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracketsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_brackets_forbid_all() {
        let yaml = "list: [1, 2, 3]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracketsRule;
        let config = LintConfig::new()
            .with_rule_config("brackets", RuleConfig::new().with_option("forbid", "all"));

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("forbidden"));
    }

    #[test]
    fn test_brackets_forbid_non_empty() {
        let yaml = "list: [1, 2]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracketsRule;
        let config = LintConfig::new().with_rule_config(
            "brackets",
            RuleConfig::new().with_option("forbid", "non-empty"),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("non-empty"));
    }

    #[test]
    fn test_brackets_forbid_non_empty_allows_empty() {
        let yaml = "list: []";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracketsRule;
        let config = LintConfig::new().with_rule_config(
            "brackets",
            RuleConfig::new().with_option("forbid", "non-empty"),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_brackets_min_spaces_inside() {
        let yaml = "list: [1, 2, 3]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracketsRule;
        let config = LintConfig::new().with_rule_config(
            "brackets",
            RuleConfig::new().with_option("min-spaces-inside", 1i64),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("too few spaces"));
    }

    #[test]
    fn test_brackets_max_spaces_inside() {
        let yaml = "list: [  1, 2, 3  ]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracketsRule;
        let config = LintConfig::new().with_rule_config(
            "brackets",
            RuleConfig::new().with_option("max-spaces-inside", 0i64),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("too many spaces"));
    }

    #[test]
    fn test_brackets_valid_with_spaces() {
        let yaml = "list: [ 1, 2, 3 ]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracketsRule;
        let config = LintConfig::new().with_rule_config(
            "brackets",
            RuleConfig::new()
                .with_option("min-spaces-inside", 1i64)
                .with_option("max-spaces-inside", 1i64),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_brackets_empty_sequence() {
        let yaml = "list: []";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracketsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_brackets_empty_with_spaces() {
        let yaml = "list: [ ]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracketsRule;
        let config = LintConfig::new().with_rule_config(
            "brackets",
            RuleConfig::new()
                .with_option("min-spaces-inside-empty", 1i64)
                .with_option("max-spaces-inside-empty", 1i64),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_brackets_nested() {
        let yaml = "list: [[1, 2], [3, 4]]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracketsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }
}
