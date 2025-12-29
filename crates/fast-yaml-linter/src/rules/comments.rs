//! Rule to check comment formatting.

use crate::{Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity};
use fast_yaml_core::Value;

/// Linting rule for comment formatting.
///
/// Validates comment formatting conventions:
/// - Require space after '#' character
/// - Minimum spacing from content for inline comments
/// - Optional shebang exemption
///
/// Configuration options:
/// - `require-starting-space`: bool (default: true)
/// - `ignore-shebangs`: bool (default: true)
/// - `min-spaces-from-content`: integer (default: 2)
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::CommentsRule, rules::LintRule, LintConfig, config::RuleConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = CommentsRule;
/// let yaml = "# Valid comment\nkey: value  # Also valid";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::default();
/// let diagnostics = rule.check(yaml, &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct CommentsRule;

impl super::LintRule for CommentsRule {
    fn code(&self) -> &str {
        DiagnosticCode::COMMENTS
    }

    fn name(&self) -> &'static str {
        "Comments"
    }

    fn description(&self) -> &'static str {
        "Validates comment formatting (space after #, spacing from content)"
    }

    fn default_severity(&self) -> Severity {
        Severity::Info
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let source = context.source();
        let comments = context.comments();

        let rule_config = config.get_rule_config(self.code());
        let require_starting_space = rule_config
            .and_then(|rc| rc.options.get_bool("require-starting-space"))
            .unwrap_or(true);

        let ignore_shebangs = rule_config
            .and_then(|rc| rc.options.get_bool("ignore-shebangs"))
            .unwrap_or(true);

        let min_spaces_from_content = rule_config
            .and_then(|rc| rc.options.get_int("min-spaces-from-content"))
            .unwrap_or(2);

        let mut diagnostics = Vec::new();

        for comment in comments {
            // Skip shebangs if configured
            if comment.is_shebang && ignore_shebangs {
                continue;
            }

            // Check for space after '#'
            if require_starting_space
                && !comment.content.is_empty()
                && !comment.content.starts_with(' ')
            {
                let severity = config.get_effective_severity(self.code(), self.default_severity());

                diagnostics.push(
                    DiagnosticBuilder::new(
                        self.code(),
                        severity,
                        "comment should start with a space after '#'",
                        comment.span,
                    )
                    .build(source),
                );
            }

            // Check spacing from content for inline comments
            if comment.is_inline && min_spaces_from_content > 0 {
                // Find the line and check spacing before '#'
                let line_num = comment.span.start.line;
                let line_offset = context.source_context().get_line_offset(line_num);
                if let Some(line) = source.lines().nth(line_num - 1) {
                    let comment_col = comment.span.start.offset - line_offset;

                    // Count spaces before '#'
                    let mut spaces_before = 0;
                    let mut idx = comment_col;

                    while idx > 0 {
                        idx -= 1;
                        match line.as_bytes().get(idx) {
                            Some(&b' ') => spaces_before += 1,
                            Some(_) | None => break,
                        }
                    }

                    #[allow(
                        clippy::cast_possible_truncation,
                        clippy::cast_possible_wrap,
                        clippy::cast_lossless
                    )]
                    let spaces_i64 = spaces_before as i64;

                    if spaces_i64 < min_spaces_from_content {
                        let severity =
                            config.get_effective_severity(self.code(), self.default_severity());

                        diagnostics.push(
                            DiagnosticBuilder::new(
                                self.code(),
                                severity,
                                format!(
                                    "too few spaces before comment (expected at least {min_spaces_from_content}, found {spaces_before})"
                                ),
                                comment.span,
                            )
                            .build(source),
                        );
                    }
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
    fn test_comments_valid_standalone() {
        let yaml = "# This is a comment\nkey: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_comments_valid_inline() {
        let yaml = "key: value  # This is a comment";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_comments_no_space_after_hash() {
        let yaml = "#No space\nkey: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("should start with a space"));
    }

    #[test]
    fn test_comments_allow_no_space_when_disabled() {
        let yaml = "#No space\nkey: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsRule;
        let config = LintConfig::new().with_rule_config(
            "comments",
            RuleConfig::new().with_option("require-starting-space", false),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_comments_too_few_spaces_from_content() {
        let yaml = "key: value # Only 1 space";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(
            diagnostics[0]
                .message
                .contains("too few spaces before comment")
        );
    }

    #[test]
    fn test_comments_custom_min_spaces() {
        let yaml = "key: value # Only 1 space";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsRule;
        let config = LintConfig::new().with_rule_config(
            "comments",
            RuleConfig::new().with_option("min-spaces-from-content", 1i64),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_comments_shebang_ignored() {
        let yaml = "#!/usr/bin/env yaml\nkey: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_comments_shebang_not_ignored() {
        let yaml = "#!/usr/bin/env yaml\nkey: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsRule;
        let config = LintConfig::new().with_rule_config(
            "comments",
            RuleConfig::new().with_option("ignore-shebangs", false),
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("should start with a space"));
    }

    #[test]
    fn test_comments_empty_comment() {
        let yaml = "key: value  #";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Empty comment is valid (no content to check)
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_comments_multiple_violations() {
        let yaml = "#No space\nkey: value #one space\nanother: test";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Should find: 1) "#No space" (no space after #), 2) "#one space" (no space after #), 3) "value #one" (too few spaces before comment)
        assert_eq!(diagnostics.len(), 3);
    }

    #[test]
    fn test_comments_in_string_ignored() {
        let yaml = r#"text: "not # a comment""#;
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }
}
