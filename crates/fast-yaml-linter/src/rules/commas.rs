//! Rule to check spacing around commas in flow collections.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    Span,
    tokenizer::{FlowTokenizer, TokenType},
};
use fast_yaml_core::Value;

/// Linting rule for comma spacing.
///
/// Validates spacing before and after commas in flow collections.
///
/// Configuration options:
/// - `max-spaces-before`: integer (default: 0)
/// - `min-spaces-after`: integer (default: 1)
/// - `max-spaces-after`: integer (default: 1)
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::CommasRule, rules::LintRule, LintConfig, config::RuleConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = CommasRule;
/// let yaml = "list: [1, 2, 3]";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::new()
///     .with_rule_config("commas", RuleConfig::new().with_option("max-spaces-before", 0i64));
///
/// let diagnostics = rule.check(yaml, &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct CommasRule;

impl super::LintRule for CommasRule {
    fn code(&self) -> &str {
        DiagnosticCode::COMMAS
    }

    fn name(&self) -> &'static str {
        "Commas"
    }

    fn description(&self) -> &'static str {
        "Validates spacing around commas in flow collections"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let source = context.source();
        let source_context = context.source_context();
        let tokenizer = FlowTokenizer::new(source, source_context);

        let rule_config = config.get_rule_config(self.code());
        let max_spaces_before = rule_config
            .and_then(|rc| rc.options.get_int("max-spaces-before"))
            .unwrap_or(0);

        let min_spaces_after = rule_config
            .and_then(|rc| rc.options.get_int("min-spaces-after"))
            .unwrap_or(1);

        let max_spaces_after = rule_config
            .and_then(|rc| rc.options.get_int("max-spaces-after"))
            .unwrap_or(1);

        let mut diagnostics = Vec::new();
        let commas = tokenizer.find_all(TokenType::Comma);

        for comma in commas {
            // Check spaces before comma
            if let Some(diag) = check_spaces_before_comma(
                source,
                comma.span.start.offset,
                max_spaces_before,
                self.code(),
                config,
            ) {
                diagnostics.push(diag);
            }

            // Check spaces after comma
            if let Some(diag) = check_spaces_after_comma(
                source,
                comma.span.start.offset,
                min_spaces_after,
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

/// Checks spaces before a comma.
fn check_spaces_before_comma(
    source: &str,
    comma_offset: usize,
    max_spaces: i64,
    code: &str,
    config: &LintConfig,
) -> Option<Diagnostic> {
    if comma_offset == 0 {
        return None;
    }

    // Count spaces before comma using byte indexing for O(n) instead of O(n²)
    let bytes = source.as_bytes();
    let mut spaces = 0;
    let mut offset = comma_offset;

    while offset > 0 {
        offset -= 1;
        if bytes[offset] == b' ' {
            spaces += 1;
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

    if max_spaces >= 0 && spaces_i64 > max_spaces {
        let severity = config.get_effective_severity(code, Severity::Warning);
        let loc = Location::new(1, 1, comma_offset);
        let span = Span::new(loc, loc);

        return Some(
            DiagnosticBuilder::new(
                code,
                severity,
                format!(
                    "too many spaces before comma (expected at most {max_spaces}, found {spaces})"
                ),
                span,
            )
            .build(source),
        );
    }

    None
}

/// Checks spaces after a comma.
fn check_spaces_after_comma(
    source: &str,
    comma_offset: usize,
    min_spaces: i64,
    max_spaces: i64,
    code: &str,
    config: &LintConfig,
) -> Option<Diagnostic> {
    let bytes = source.as_bytes();
    if comma_offset + 1 >= bytes.len() {
        return None;
    }

    // Count spaces after comma using byte indexing for O(n) instead of O(n²)
    let mut spaces = 0;
    let mut offset = comma_offset + 1;
    let mut has_newline = false;

    while offset < bytes.len() {
        if bytes[offset] == b' ' {
            spaces += 1;
            offset += 1;
        } else {
            if bytes[offset] == b'\n' || bytes[offset] == b'\r' {
                has_newline = true;
            }
            break;
        }
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_lossless
    )]
    let spaces_i64 = spaces as i64;

    // Don't check min spaces if followed by newline
    if min_spaces >= 0 && spaces_i64 < min_spaces && !has_newline {
        let severity = config.get_effective_severity(code, Severity::Warning);
        let loc = Location::new(1, 1, comma_offset + 1);
        let span = Span::new(loc, loc);

        return Some(
            DiagnosticBuilder::new(
                code,
                severity,
                format!(
                    "too few spaces after comma (expected at least {min_spaces}, found {spaces})"
                ),
                span,
            )
            .build(source),
        );
    }

    if max_spaces >= 0 && spaces_i64 > max_spaces {
        let severity = config.get_effective_severity(code, Severity::Warning);
        let loc = Location::new(1, 1, comma_offset + 1);
        let span = Span::new(loc, loc);

        return Some(
            DiagnosticBuilder::new(
                code,
                severity,
                format!(
                    "too many spaces after comma (expected at most {max_spaces}, found {spaces})"
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
    fn test_commas_default_valid() {
        let yaml = "list: [1, 2, 3]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommasRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_commas_too_many_spaces_before() {
        let yaml = "list: [1 , 2 , 3]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommasRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("too many spaces before"));
    }

    #[test]
    fn test_commas_too_few_spaces_after() {
        let yaml = "list: [1,2,3]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommasRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("too few spaces after"));
    }

    #[test]
    fn test_commas_too_many_spaces_after() {
        let yaml = "list: [1,  2,  3]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommasRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("too many spaces after"));
    }

    #[test]
    fn test_commas_allow_no_spaces_after() {
        let yaml = "list: [1,2,3]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommasRule;
        let config = LintConfig::new().with_rule_config(
            "commas",
            RuleConfig::new()
                .with_option("min-spaces-after", 0i64)
                .with_option("max-spaces-after", 0i64),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_commas_allow_multiple_spaces_after() {
        let yaml = "list: [1,  2,  3]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommasRule;
        let config = LintConfig::new().with_rule_config(
            "commas",
            RuleConfig::new().with_option("max-spaces-after", 2i64),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_commas_flow_mapping() {
        let yaml = "{name: John, age: 30}";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommasRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_commas_nested_flow() {
        let yaml = "data: [[1, 2], [3, 4]]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommasRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_commas_multiline_flow() {
        let yaml = "list: [\n  1,\n  2,\n  3\n]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommasRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Commas followed by newlines should be handled gracefully
        assert!(
            diagnostics.is_empty() || diagnostics.iter().all(|d| !d.message.contains("too few"))
        );
    }

    #[test]
    fn test_commas_multiple_violations() {
        let yaml = "list: [1 ,2,3 , 4]";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommasRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Should have multiple violations
        assert!(diagnostics.len() >= 2);
    }
}
