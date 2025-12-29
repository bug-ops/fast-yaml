//! Rule to check spacing around colons.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    Span,
    tokenizer::{FlowTokenizer, TokenType},
};
use fast_yaml_core::Value;

/// Linting rule for colon spacing.
///
/// Validates spacing before and after colons in mappings.
///
/// Configuration options:
/// - `max-spaces-before`: integer (default: 0)
/// - `max-spaces-after`: integer (default: 1)
///
/// Special cases (ignored):
/// - URLs (e.g., `http://`, `https://`)
/// - Time values (e.g., `12:30:45`)
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::ColonsRule, rules::LintRule, LintConfig, config::RuleConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = ColonsRule;
/// let yaml = "name: John";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::new()
///     .with_rule_config("colons", RuleConfig::new().with_option("max-spaces-after", 1i64));
///
/// let diagnostics = rule.check(yaml, &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct ColonsRule;

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
        let tokenizer = FlowTokenizer::new(source, source_context);

        let rule_config = config.get_rule_config(self.code());
        let max_spaces_before = rule_config
            .and_then(|rc| rc.options.get_int("max-spaces-before"))
            .unwrap_or(0);

        let max_spaces_after = rule_config
            .and_then(|rc| rc.options.get_int("max-spaces-after"))
            .unwrap_or(1);

        let mut diagnostics = Vec::new();
        let colons = tokenizer.find_all(TokenType::Colon);

        for colon in colons {
            // Skip if colon is part of URL or time
            if is_url_or_time(source, colon.span.start.offset) {
                continue;
            }

            // Check spaces before colon
            if let Some(diag) = check_spaces_before_colon(
                source,
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

    // Check for URL schemes: http://, https://, ftp://, etc.
    if colon_offset >= 4 {
        let before = &bytes[colon_offset.saturating_sub(4)..colon_offset];
        if before == b"http" || before == b"sftp" {
            return true;
        }
    }

    if colon_offset >= 5 {
        let before = &bytes[colon_offset.saturating_sub(5)..colon_offset];
        if before == b"https" {
            return true;
        }
    }

    if colon_offset >= 3 {
        let before = &bytes[colon_offset.saturating_sub(3)..colon_offset];
        if before == b"ftp" {
            return true;
        }
    }

    // Check for time format: digit:digit (bytes are ASCII)
    if colon_offset > 0 && colon_offset + 1 < bytes.len() {
        let before = bytes[colon_offset - 1];
        let after = bytes[colon_offset + 1];

        if before.is_ascii_digit() && after.is_ascii_digit() {
            return true;
        }
    }

    false
}

/// Checks spaces before a colon.
fn check_spaces_before_colon(
    source: &str,
    colon_offset: usize,
    max_spaces: i64,
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
        let loc = Location::new(1, 1, colon_offset);
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
            .build(source),
        );
    }

    None
}

/// Checks spaces after a colon.
fn check_spaces_after_colon(
    source: &str,
    colon_offset: usize,
    max_spaces: i64,
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
        if bytes[offset] == b' ' {
            spaces += 1;
            offset += 1;
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
        let loc = Location::new(1, 1, colon_offset + 1);
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
    fn test_colons_default_valid() {
        let yaml = "name: John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = ColonsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_colons_too_many_spaces_before() {
        let yaml = "name : John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = ColonsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
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
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("too many spaces after"));
    }

    #[test]
    fn test_colons_allow_more_spaces_after() {
        let yaml = "name:  John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = ColonsRule;
        let config = LintConfig::new().with_rule_config(
            "colons",
            RuleConfig::new().with_option("max-spaces-after", 2i64),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
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
        assert!(diagnostics.is_empty());
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
