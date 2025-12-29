//! Rule to check flow mapping braces `{}` formatting.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity,
    rules::flow_common::{
        check_spaces_after_opening, check_spaces_before_closing, is_empty_collection,
    },
    tokenizer::{FlowTokenizer, TokenType},
};
use fast_yaml_core::Value;

/// Linting rule for flow mapping braces.
///
/// Validates spacing and usage of flow mappings `{}`.
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
/// use fast_yaml_linter::{rules::BracesRule, rules::LintRule, LintConfig, config::RuleConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = BracesRule;
/// let yaml = "object: {key: value}";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::new()
///     .with_rule_config("braces", RuleConfig::new().with_option("forbid", "no"));
///
/// let diagnostics = rule.check(yaml, &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct BracesRule;

impl super::LintRule for BracesRule {
    fn code(&self) -> &str {
        DiagnosticCode::BRACES
    }

    fn name(&self) -> &'static str {
        "Braces"
    }

    fn description(&self) -> &'static str {
        "Validates spacing and usage of flow mapping braces {}"
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

        let open_braces = tokenizer.find_all(TokenType::BraceOpen);
        let close_braces = tokenizer.find_all(TokenType::BraceClose);

        // Check forbid option
        match forbid {
            "all" => {
                for token in &open_braces {
                    let severity =
                        config.get_effective_severity(self.code(), self.default_severity());
                    diagnostics.push(
                        DiagnosticBuilder::new(
                            self.code(),
                            severity,
                            "flow mapping forbidden (forbid: all)",
                            token.span,
                        )
                        .build(source),
                    );
                }
                return diagnostics;
            }
            "non-empty" => {
                // Check if mapping is non-empty
                for (i, open) in open_braces.iter().enumerate() {
                    if let Some(close) = close_braces.get(i)
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
                                "non-empty flow mapping forbidden (forbid: non-empty)",
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
        for (i, open) in open_braces.iter().enumerate() {
            if let Some(close) = close_braces.get(i) {
                let is_empty =
                    is_empty_collection(source, open.span.end.offset, close.span.start.offset);

                let (min_spaces, max_spaces) = if is_empty && min_spaces_inside_empty >= 0 {
                    (min_spaces_inside_empty, max_spaces_inside_empty)
                } else {
                    (min_spaces_inside, max_spaces_inside)
                };

                // Check spaces after opening brace
                if let Some(diag) = check_spaces_after_opening(
                    source,
                    open.span.end.offset,
                    close.span.start.offset,
                    min_spaces,
                    max_spaces,
                    self.code(),
                    config,
                    "braces",
                ) {
                    diagnostics.push(diag);
                }

                // Check spaces before closing brace
                if let Some(diag) = check_spaces_before_closing(
                    source,
                    open.span.end.offset,
                    close.span.start.offset,
                    min_spaces,
                    max_spaces,
                    self.code(),
                    config,
                    "braces",
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
    fn test_braces_default_valid() {
        let yaml = "object: {key: value}";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_braces_forbid_all() {
        let yaml = "object: {key: value}";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracesRule;
        let config = LintConfig::new()
            .with_rule_config("braces", RuleConfig::new().with_option("forbid", "all"));

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("forbidden"));
    }

    #[test]
    fn test_braces_forbid_non_empty() {
        let yaml = "object: {key: value}";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracesRule;
        let config = LintConfig::new().with_rule_config(
            "braces",
            RuleConfig::new().with_option("forbid", "non-empty"),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("non-empty"));
    }

    #[test]
    fn test_braces_forbid_non_empty_allows_empty() {
        let yaml = "object: {}";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracesRule;
        let config = LintConfig::new().with_rule_config(
            "braces",
            RuleConfig::new().with_option("forbid", "non-empty"),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_braces_min_spaces_inside() {
        let yaml = "object: {key: value}";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracesRule;
        let config = LintConfig::new().with_rule_config(
            "braces",
            RuleConfig::new().with_option("min-spaces-inside", 1i64),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("too few spaces"));
    }

    #[test]
    fn test_braces_max_spaces_inside() {
        let yaml = "object: {  key: value  }";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracesRule;
        let config = LintConfig::new().with_rule_config(
            "braces",
            RuleConfig::new().with_option("max-spaces-inside", 0i64),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("too many spaces"));
    }

    #[test]
    fn test_braces_valid_with_spaces() {
        let yaml = "object: { key: value }";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracesRule;
        let config = LintConfig::new().with_rule_config(
            "braces",
            RuleConfig::new()
                .with_option("min-spaces-inside", 1i64)
                .with_option("max-spaces-inside", 1i64),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_braces_empty_mapping() {
        let yaml = "object: {}";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_braces_empty_with_spaces() {
        let yaml = "object: { }";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracesRule;
        let config = LintConfig::new().with_rule_config(
            "braces",
            RuleConfig::new()
                .with_option("min-spaces-inside-empty", 1i64)
                .with_option("max-spaces-inside-empty", 1i64),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_braces_nested() {
        let yaml = "object: {a: {b: c}}";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = BracesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_is_empty_mapping() {
        use crate::rules::flow_common::is_empty_collection;
        assert!(is_empty_collection("", 0, 0));
        assert!(is_empty_collection("{}", 1, 1));
        assert!(is_empty_collection("{  }", 1, 3));
        assert!(!is_empty_collection("{a}", 1, 2));
    }
}
