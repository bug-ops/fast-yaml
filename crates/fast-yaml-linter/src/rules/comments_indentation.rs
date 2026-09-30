//! Rule to check comment indentation.

use crate::context::lines_of;
use crate::{Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity};
use fast_yaml_core::Value;

/// Metadata about a line for efficient indentation checking.
struct LineInfo {
    /// Number of leading spaces
    indent: usize,
    /// true if line is empty or only whitespace
    is_empty: bool,
    /// true if line is a comment
    is_comment: bool,
}

/// Linting rule for comment indentation.
///
/// Ensures comments have the same indentation as surrounding content.
/// Standalone comments (on their own line) should match the indentation
/// of the next non-empty, non-comment line.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::CommentsIndentationRule, rules::LintRule, LintConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = CommentsIndentationRule;
/// let yaml = "list:\n  - item1\n  # Comment at correct level\n  - item2";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::default();
/// let diagnostics = rule.check(&fast_yaml_linter::LintContext::new(yaml), &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct CommentsIndentationRule;

impl super::LintRule for CommentsIndentationRule {
    fn code(&self) -> &str {
        DiagnosticCode::COMMENTS_INDENTATION
    }

    fn name(&self) -> &'static str {
        "Comments Indentation"
    }

    fn description(&self) -> &'static str {
        "Ensures comments have same indentation as surrounding content"
    }

    fn default_severity(&self) -> Severity {
        Severity::Info
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let source = context.source();
        let comments = context.comments();

        let mut diagnostics = Vec::new();

        let lines: Vec<&str> = lines_of(source).collect();

        // Pre-compute line metadata to avoid O(n²) complexity
        let line_info: Vec<LineInfo> = lines
            .iter()
            .map(|line| {
                let trimmed = line.trim_start();
                LineInfo {
                    indent: get_line_indentation(line),
                    is_empty: trimmed.is_empty(),
                    is_comment: trimmed.starts_with('#'),
                }
            })
            .collect();

        let content_indent =
            |info: &LineInfo| (!info.is_empty && !info.is_comment).then_some(info.indent);

        // next_content[i] / prev_content[i]: indent of the nearest content line strictly after / before i
        let mut next_content: Vec<Option<usize>> = line_info
            .iter()
            .rev()
            .scan(None, |next, info| {
                let current = *next;
                *next = content_indent(info).or(*next);
                Some(current)
            })
            .collect();
        next_content.reverse();
        let prev_content: Vec<Option<usize>> = line_info
            .iter()
            .scan(None, |prev, info| {
                let current = *prev;
                *prev = content_indent(info).or(*prev);
                Some(current)
            })
            .collect();

        for comment in comments {
            // Skip inline comments (they follow content indentation)
            if comment.is_inline {
                continue;
            }

            let comment_line = comment.span.start.line;
            if comment_line == 0 || comment_line > lines.len() {
                continue;
            }

            let comment_line_idx = comment_line - 1;
            let Some(comment_indent) = line_info.get(comment_line_idx).map(|info| info.indent)
            else {
                continue;
            };

            // Column-0 comments belong to the top level: never borrow indentation
            // from a preceding nested block.
            let expected_indent = next_content
                .get(comment_line_idx)
                .copied()
                .flatten()
                .or_else(|| {
                    if comment_indent == 0 {
                        None
                    } else {
                        prev_content.get(comment_line_idx).copied().flatten()
                    }
                });

            // Check if indentation matches
            if let Some(expected) = expected_indent
                && comment_indent != expected
            {
                let severity = config
                    .rules
                    .comments_indentation
                    .severity_or(self.default_severity());

                diagnostics.push(
                    DiagnosticBuilder::new(
                        self.code(),
                        severity,
                        format!(
                            "comment indentation does not match surrounding content (expected {expected} spaces, found {comment_indent})"
                        ),
                        comment.span,
                    )
                    .build_with_context(context.source_context()),
                );
            }
        }

        diagnostics
    }
}

/// Gets the indentation level of a line (number of leading spaces).
fn get_line_indentation(line: &str) -> usize {
    line.chars().take_while(|&c| c == ' ').count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::LintRule;
    use fast_yaml_core::Parser;

    fn check_source(yaml: &str, parsed: &str) -> Vec<Diagnostic> {
        let value = Parser::parse_str(parsed).unwrap().unwrap();
        CommentsIndentationRule.check(&LintContext::new(yaml), &value, &LintConfig::default())
    }

    fn diag_count(yaml: &str) -> usize {
        check_source(yaml, yaml).len()
    }

    #[test]
    fn test_comment_run_uses_next_content_indent() {
        assert_eq!(diag_count("a:\n  # c1\n  # c2\n  b: 1\n"), 0);
        assert_eq!(diag_count("a:\n# c1\n  # c2\n  b: 1\n"), 1);
    }

    #[test]
    fn test_trailing_comments_fall_back_to_previous_content() {
        assert_eq!(diag_count("a:\n  b: 1\n  # t1\n  # t2\n"), 0);
        assert_eq!(diag_count("a:\n  b: 1\n    # t1\n"), 1);
        assert_eq!(diag_count("a:\n  b: 1\n# top\n# top2\n"), 0);
    }

    #[test]
    fn test_large_comment_run_has_no_diagnostics() {
        let yaml = format!("{}a: 1\n", "# comment\n".repeat(100_000));
        assert_eq!(diag_count(&yaml), 0);
    }

    #[test]
    fn test_column_zero_trailing_comment_after_nested_block_is_ignored() {
        assert_eq!(diag_count("a:\n  b:\n    c: 1\n# end\n"), 0);
    }

    #[test]
    fn test_blank_line_separated_comment_runs() {
        assert_eq!(diag_count("a:\n  # one\n\n  # two\n\n  b: 1\n"), 0);
        assert_eq!(diag_count("a:\n  # one\n\n# two\n\n  b: 1\n"), 1);
    }

    #[test]
    fn test_comment_only_file() {
        let yaml = "# a\n  # b\n# c\n";
        assert_eq!(check_source(yaml, "a: 1").len(), 0);
    }

    #[test]
    fn test_first_line_comment_uses_next_content() {
        assert_eq!(diag_count("# top\na: 1\n"), 0);
        assert_eq!(diag_count("  # top\na: 1\n"), 1);
    }

    #[test]
    fn test_crlf_comment_runs() {
        assert_eq!(diag_count("a:\r\n  # c1\r\n  # c2\r\n  b: 1\r\n"), 0);
        assert_eq!(diag_count("a:\r\n  # c1\r\n# c2\r\n  b: 1\r\n"), 1);
    }

    #[test]
    fn test_tab_indented_comment_is_compared_by_leading_spaces() {
        assert_eq!(diag_count("a:\n  b: 1\n\t# tab\n  c: 2\n"), 1);
    }

    #[test]
    fn test_comments_indentation_valid() {
        let yaml = "list:\n  - item1\n  # Comment at correct level\n  - item2";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_comments_indentation_invalid() {
        let yaml = "list:\n  - item1\n# Wrong indentation\n  - item2";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(
            diagnostics[0]
                .message
                .contains("comment indentation does not match")
        );
    }

    #[test]
    fn test_comments_indentation_inline_ignored() {
        let yaml = "key: value  # Inline comment";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Inline comments are not checked for indentation
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_comments_indentation_nested() {
        let yaml = "root:\n  nested:\n    # Comment at level 2\n    key: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_comments_indentation_nested_invalid() {
        let yaml = "root:\n  nested:\n  # Comment at wrong level\n    key: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn test_comments_indentation_first_line() {
        let yaml = "# Comment at root level\nkey: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_comments_indentation_multiple_comments() {
        let yaml = "# Comment 1\n# Comment 2\nkey: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_comments_indentation_after_content() {
        let yaml = "key: value\n# Comment after content\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_get_line_indentation() {
        assert_eq!(get_line_indentation("no indent"), 0);
        assert_eq!(get_line_indentation("  two spaces"), 2);
        assert_eq!(get_line_indentation("    four spaces"), 4);
        assert_eq!(get_line_indentation(""), 0);
    }

    #[test]
    fn test_comments_indentation_list() {
        let yaml = "items:\n  - one\n  # Comment\n  - two";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    // Regression tests for issue #166: top-level comment after nested block false positive

    #[test]
    fn test_toplevel_comment_after_nested_block() {
        // Top-level comment after a nested block should not be flagged
        let yaml = "a:\n  b: 2\n# top-level comment\nc: 3\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "top-level comment after nested block should not produce diagnostics"
        );
    }

    #[test]
    fn test_toplevel_comment_at_start() {
        // Top-level comment before any content should not be flagged
        let yaml = "# top-level header\na: 1\nb: 2\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "top-level comment at file start should not produce diagnostics"
        );
    }

    #[test]
    fn test_toplevel_comment_between_top_level_keys() {
        // Top-level comment between two top-level keys should not be flagged
        let yaml = "a: 1\n# separator\nb: 2\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "top-level comment between top-level keys should not produce diagnostics"
        );
    }

    #[test]
    fn test_indented_comment_in_nested_block_still_checked() {
        // A comment indented to match nested block should still work correctly
        let yaml = "root:\n  nested:\n    # comment at level 2\n    key: value\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "correctly indented nested comment should not produce diagnostics"
        );
    }

    #[test]
    fn test_indented_comment_wrong_level_still_flagged() {
        // A comment with wrong indentation inside a nested block should still be flagged
        let yaml = "root:\n  nested:\n  # wrong level comment\n    key: value\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            !diagnostics.is_empty(),
            "incorrectly indented nested comment should produce diagnostics"
        );
    }
}
