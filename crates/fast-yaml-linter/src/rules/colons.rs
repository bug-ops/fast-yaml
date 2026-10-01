//! Rule to check spacing around colons.

use serde::{Deserialize, Serialize};

use crate::config::{Limit, RuleOptions};
use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity,
    SourceContext, Span, tokenizer::TokenType,
};
use fast_yaml_core::Value;

/// Linting rule for colon spacing.
///
/// Validates spacing before and after colons in mappings.
///
/// Configuration options (see [`ColonsOptions`]):
/// - `max-spaces-before`: integer, -1 disables (default: 0)
/// - `max-spaces-after`: integer, -1 disables (default: 1)
///
/// Special cases (ignored):
/// - URLs (e.g., `http://`, `https://`)
/// - Time values (e.g., `12:30:45`)
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::ColonsRule, rules::LintRule, LintConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = ColonsRule;
/// let yaml = "name: John";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::default();
///
/// let diagnostics = rule.check(&fast_yaml_linter::LintContext::new(yaml), &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct ColonsRule;

/// Options of the colons rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct ColonsOptions {
    /// Maximum spaces before a colon.
    pub max_spaces_before: Limit,
    /// Maximum spaces after a colon.
    pub max_spaces_after: Limit,
}

impl Default for ColonsOptions {
    fn default() -> Self {
        Self {
            max_spaces_before: Limit::Max(0),
            max_spaces_after: Limit::Max(1),
        }
    }
}

impl RuleOptions for ColonsOptions {}

impl super::LintRule for ColonsRule {
    fn code(&self) -> &str {
        DiagnosticCode::COLONS
    }

    fn name(&self) -> &'static str {
        "Colons"
    }

    fn description(&self) -> &'static str {
        "Validates spacing around colons in mappings"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let source = context.source();
        let source_context = context.source_context();
        let tokenizer = context.flow_tokenizer();

        let options = &config.rules.colons.options;
        let max_spaces_before = options.max_spaces_before;
        let max_spaces_after = options.max_spaces_after;

        let mut diagnostics = Vec::new();
        let colons = tokenizer.tokens(TokenType::Colon);

        for colon in colons {
            // Skip if colon is part of URL or time
            if is_url_or_time(source, colon.span.start.offset) {
                continue;
            }

            // Check spaces before colon
            if let Some(diag) = check_spaces_before_colon(
                source,
                source_context,
                colon.span.start.offset,
                max_spaces_before,
                self.code(),
                config,
            ) {
                diagnostics.push(diag);
            }

            // Check spaces after colon
            if let Some(diag) = check_spaces_after_colon(
                source,
                source_context,
                colon.span.start.offset,
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

/// Checks if a colon is part of a URL or time value.
fn is_url_or_time(source: &str, colon_offset: usize) -> bool {
    let bytes = source.as_bytes();

    let head = bytes.get(..colon_offset).unwrap_or_default();
    if [b"http".as_slice(), b"https", b"sftp", b"ftp"]
        .iter()
        .any(|scheme| head.ends_with(scheme))
    {
        return true;
    }

    // Check for time format: digit:digit (bytes are ASCII)
    if let (Some(before), Some(after)) = (
        colon_offset.checked_sub(1).and_then(|i| bytes.get(i)),
        bytes.get(colon_offset + 1),
    ) && before.is_ascii_digit()
        && after.is_ascii_digit()
    {
        return true;
    }

    false
}

/// Checks spaces before a colon.
fn check_spaces_before_colon(
    source: &str,
    source_context: &SourceContext<'_>,
    colon_offset: usize,
    max_spaces: Limit,
    code: &str,
    config: &LintConfig,
) -> Option<Diagnostic> {
    if colon_offset == 0 {
        return None;
    }

    // Count spaces before colon using byte indexing for O(n) instead of O(n²)
    let bytes = source.as_bytes();
    let mut spaces = 0;
    let mut offset = colon_offset;

    while offset > 0 {
        offset -= 1;
        if bytes.get(offset) == Some(&b' ') {
            spaces += 1;
        } else {
            break;
        }
    }

    if max_spaces.exceeded_by(spaces) {
        let severity = config.rules.colons.severity_or(Severity::Warning);
        let loc = source_context.offset_to_location(colon_offset);
        let span = Span::new(loc, loc);

        return Some(
            DiagnosticBuilder::new(
                code,
                severity,
                format!(
                    "too many spaces before colon (expected at most {max_spaces}, found {spaces})"
                ),
                span,
            )
            .build(),
        );
    }

    None
}

/// Checks spaces after a colon.
fn check_spaces_after_colon(
    source: &str,
    source_context: &SourceContext<'_>,
    colon_offset: usize,
    max_spaces: Limit,
    code: &str,
    config: &LintConfig,
) -> Option<Diagnostic> {
    let bytes = source.as_bytes();
    if colon_offset + 1 >= bytes.len() {
        return None;
    }

    // Count spaces after colon using byte indexing for O(n) instead of O(n²)
    let mut spaces = 0;
    let mut offset = colon_offset + 1;

    while offset < bytes.len() {
        if bytes.get(offset) == Some(&b' ') {
            spaces += 1;
            offset += 1;
        } else {
            break;
        }
    }

    // No token follows on this line (end of line, end of file or a comment), so yamllint
    // measures nothing.
    let next_is_eol_or_eof =
        offset >= bytes.len() || matches!(bytes.get(offset), Some(b'\n' | b'\r' | b'#'));
    if next_is_eol_or_eof {
        return None;
    }

    if max_spaces.exceeded_by(spaces) {
        let severity = config.rules.colons.severity_or(Severity::Warning);
        let loc = source_context.offset_to_location(colon_offset + 1);
        let span = Span::new(loc, loc);

        return Some(
            DiagnosticBuilder::new(
                code,
                severity,
                format!(
                    "too many spaces after colon (expected at most {max_spaces}, found {spaces})"
                ),
                span,
            )
            .build(),
        );
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{RuleName, test_support::config_with_rule},
        rules::LintRule,
    };
    use fast_yaml_core::Parser;

    #[test]
    fn test_colons_default_valid() {
        let yaml = "name: John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = ColonsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_colons_too_many_spaces_before() {
        let yaml = "name : John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = ColonsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_ne!(diagnostics, []);
        assert!(diagnostics[0].message.contains("too many spaces before"));
    }

    #[test]
    fn test_colons_too_many_spaces_after() {
        let yaml = "name:  John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = ColonsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_ne!(diagnostics, []);
        assert!(diagnostics[0].message.contains("too many spaces after"));
    }

    #[test]
    fn test_colons_allow_more_spaces_after() {
        let yaml = "name:  John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = ColonsRule;
        let config = config_with_rule(RuleName::Colons, "{max-spaces-after: 2}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_colons_url_ignored() {
        let yaml = r#"url: "http://example.com""#;
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = ColonsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Should only flag the mapping colon, not the one in the URL
        assert!(diagnostics.is_empty() || diagnostics.len() <= 1);
    }

    #[test]
    fn test_colons_https_url_ignored() {
        let yaml = r#"url: "https://example.com""#;
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = ColonsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty() || diagnostics.len() <= 1);
    }

    #[test]
    fn test_colons_time_ignored() {
        let yaml = "time: 12:30:45";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = ColonsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Should only flag the mapping colon, not the ones in the time
        assert!(diagnostics.is_empty() || diagnostics.len() <= 1);
    }

    #[test]
    fn test_colons_flow_mapping() {
        let yaml = "{name: John, age: 30}";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = ColonsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_colons_correct_location() {
        // Violation at line 3, not line 1
        let yaml = "line1: ok\nline2: ok\nline3 : bad";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = ColonsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_ne!(diagnostics, []);
        assert_eq!(
            diagnostics[0].span.start.line, 3,
            "violation should be on line 3, got: {}",
            diagnostics[0].span.start.line
        );
    }

    #[test]
    fn test_colons_multiple_violations() {
        let yaml = "name : John\nage :  30";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = ColonsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // At least 2 violations (spaces before colons)
        assert!(diagnostics.len() >= 2);
    }

    /// Regression test for #190: trailing spaces after key colon must not be flagged.
    #[test]
    fn test_colons_trailing_whitespace_no_false_positive() {
        // "nested:  " has two trailing spaces but no inline value — must not fire.
        let yaml = "nested:  \n  a: 1\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = ColonsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for trailing whitespace after key colon, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_is_url_or_time() {
        let http_url = "url: http://example.com";
        assert!(is_url_or_time(http_url, 9)); // colon in "http:"

        let https = "url: https://example.com";
        assert!(is_url_or_time(https, 10)); // colon in "https:"

        let time = "time: 12:30";
        assert!(is_url_or_time(time, 8)); // colon in "12:30"

        let mapping = "name: John";
        assert!(!is_url_or_time(mapping, 4)); // mapping colon
    }
}
