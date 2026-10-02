//! Rule to check comment indentation.

use super::RuleId;
use crate::config::RuleName;
use crate::context::LineMetadata;
use crate::{CommentKind, Finding, LintConfig, LintContext, Severity};

/// Linting rule for comment indentation.
///
/// Ensures comments have the same indentation as surrounding content.
/// A standalone comment (on its own line) must be indented like the next content line
/// (column 0 at the end of the file) or like the content line before it; after another
/// standalone comment of the same run it must match that comment instead. This is yamllint's
/// rule. The first comment after a block scalar is not checked.
///
/// A multi-line quoted or plain scalar before a comment counts with the indent of its last line,
/// yamllint uses the line where the scalar starts.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::CommentsIndentationRule, rules::SourceRule, LintConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = CommentsIndentationRule;
/// let yaml = "list:\n  - item1\n  # Comment at correct level\n  - item2";
///
/// let config = LintConfig::default();
/// let diagnostics = rule.diagnose(&fast_yaml_linter::LintContext::new(yaml), &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct CommentsIndentationRule;

impl super::LintRule for CommentsIndentationRule {
    fn id(&self) -> RuleId<'_> {
        RuleId::BuiltIn(RuleName::CommentsIndentation)
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
}

impl super::SourceRule for CommentsIndentationRule {
    fn check(&self, context: &LintContext, _config: &LintConfig) -> Vec<Finding> {
        let comments = context.comments();
        if comments.is_empty() {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        let line_info = context.line_metadata();

        let lines = ContentLines::new(context, line_info);

        // Last checked own-line comment with the content line before its gap and its indent
        let mut previous_standalone: Option<(Option<usize>, usize)> = None;

        for comment in comments {
            // Skip inline comments (they follow content indentation)
            if comment.kind == CommentKind::Inline {
                continue;
            }

            let comment_line = comment.span.start.line;
            if comment_line == 0 || comment_line > line_info.len() {
                continue;
            }

            let comment_line_idx = comment_line - 1;

            let Some(comment_indent) = line_info.get(comment_line_idx).map(|info| info.indent)
            else {
                continue;
            };
            let gap = lines.prev_line.get(comment_line_idx).copied().flatten();

            // yamllint does not check the first non-blank line after a block scalar, but later
            // comments still follow it
            if gap.is_some_and(|line| {
                lines.in_scalar.get(line - 1).copied().unwrap_or(false)
                    && line_info
                        .get(line..comment_line_idx)
                        .is_some_and(|between| between.iter().all(|info| info.is_empty))
            }) {
                previous_standalone = Some((gap, comment_indent));
                continue;
            }

            let next_indent = lines
                .next
                .get(comment_line_idx)
                .copied()
                .flatten()
                .unwrap_or(0);
            let prev_indent = match previous_standalone {
                Some((previous_gap, indent)) if previous_gap == gap => indent,
                _ => lines
                    .prev
                    .get(comment_line_idx)
                    .copied()
                    .flatten()
                    .unwrap_or(0)
                    .max(next_indent),
            };
            previous_standalone = Some((gap, comment_indent));

            if comment_indent != next_indent && comment_indent != prev_indent {
                let expected = next_indent;

                diagnostics.push(
                    Finding::new(format!(
                            "comment indentation does not match surrounding content (expected {expected} spaces, found {comment_indent})"
                        ), comment.span),
                );
            }
        }

        diagnostics
    }
}

/// Per-line view of the content lines around each comment; indices are 0-based line indices.
struct ContentLines {
    /// Whether the line is inside a literal or folded scalar.
    in_scalar: Vec<bool>,
    /// Indent of the nearest content line strictly after the line.
    next: Vec<Option<usize>>,
    /// Indent of the token that ends the nearest content line strictly before the line; a block
    /// scalar counts with the indent of the line that opens it, like its yamllint token.
    prev: Vec<Option<usize>>,
    /// 1-based number of the nearest content line strictly before the line.
    prev_line: Vec<Option<usize>>,
}

impl ContentLines {
    fn new(context: &LintContext, line_info: &[LineMetadata]) -> Self {
        let in_scalar: Vec<bool> = (1..=line_info.len())
            .map(|line| context.in_block_scalar(line))
            .collect();
        // Block scalar lines are content even when they look like comments
        let content_indent = |idx: usize, info: &LineMetadata| {
            (!info.is_empty && (!info.is_comment || in_scalar.get(idx).copied().unwrap_or(false)))
                .then_some(info.indent)
        };

        let mut next: Vec<Option<usize>> = line_info
            .iter()
            .enumerate()
            .rev()
            .scan(None, |next, (idx, info)| {
                let current = *next;
                *next = content_indent(idx, info).or(*next);
                Some(current)
            })
            .collect();
        next.reverse();
        let mut token_indent = None;
        let prev: Vec<Option<usize>> = line_info
            .iter()
            .enumerate()
            .map(|(idx, info)| {
                let current = token_indent;
                if let Some(indent) = content_indent(idx, info)
                    && !in_scalar.get(idx).copied().unwrap_or(false)
                {
                    token_indent = Some(indent);
                }
                current
            })
            .collect();
        let prev_line: Vec<Option<usize>> = line_info
            .iter()
            .enumerate()
            .scan(None, |prev, (idx, info)| {
                let current = *prev;
                if content_indent(idx, info).is_some() {
                    *prev = Some(idx + 1);
                }
                Some(current)
            })
            .collect();
        Self {
            in_scalar,
            next,
            prev,
            prev_line,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Diagnostic;
    use crate::rules::SourceRule;

    fn check_source(yaml: &str, _parsed: &str) -> Vec<Diagnostic> {
        CommentsIndentationRule.diagnose(&LintContext::new(yaml), &LintConfig::default())
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
        assert_eq!(check_source(yaml, "a: 1").len(), 1);
        assert_eq!(check_source("# a\n# b\n", "a: 1").len(), 0);
    }

    #[test]
    fn test_comment_may_match_the_previous_line_indent() {
        assert_eq!(diag_count("a:\n  b: 1\n  # c\nd: 1\n"), 0);
        assert_eq!(diag_count("a:\n  b: 1\n# c\nd: 1\n"), 0);
        assert_eq!(diag_count("a:\n  b: 1\n    # c\nd: 1\n"), 1);
    }

    #[test]
    fn test_comment_follows_the_previous_comment_when_it_went_back_to_the_next_indent() {
        assert_eq!(diag_count("a:\n  - 1\n# c1\n  # c2\nb: 1\n"), 1);
        assert_eq!(diag_count("a:\n  - 1\n  # c1\n# c2\nb: 1\n"), 0);
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

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_comments_indentation_invalid() {
        let yaml = "list:\n  - item1\n# Wrong indentation\n  - item2";

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_ne!(diagnostics, []);
        assert!(
            diagnostics[0]
                .message
                .contains("comment indentation does not match")
        );
    }

    #[test]
    fn test_comments_indentation_inline_ignored() {
        let yaml = "key: value  # Inline comment";

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        // Inline comments are not checked for indentation
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_comments_indentation_nested() {
        let yaml = "root:\n  nested:\n    # Comment at level 2\n    key: value";

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_comments_indentation_nested_invalid() {
        let yaml = "root:\n  nested:\n  # Comment at wrong level\n    key: value";

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_ne!(diagnostics, []);
    }

    #[test]
    fn test_comments_indentation_first_line() {
        let yaml = "# Comment at root level\nkey: value";

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_comments_indentation_multiple_comments() {
        let yaml = "# Comment 1\n# Comment 2\nkey: value";

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_comments_indentation_after_content() {
        let yaml = "key: value\n# Comment after content\n";

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_comments_indentation_list() {
        let yaml = "items:\n  - one\n  # Comment\n  - two";

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    // Regression tests for issue #166: top-level comment after nested block false positive

    #[test]
    fn test_toplevel_comment_after_nested_block() {
        // Top-level comment after a nested block should not be flagged
        let yaml = "a:\n  b: 2\n# top-level comment\nc: 3\n";

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert!(
            diagnostics.is_empty(),
            "top-level comment after nested block should not produce diagnostics"
        );
    }

    #[test]
    fn test_toplevel_comment_at_start() {
        // Top-level comment before any content should not be flagged
        let yaml = "# top-level header\na: 1\nb: 2\n";

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert!(
            diagnostics.is_empty(),
            "top-level comment at file start should not produce diagnostics"
        );
    }

    #[test]
    fn test_toplevel_comment_between_top_level_keys() {
        // Top-level comment between two top-level keys should not be flagged
        let yaml = "a: 1\n# separator\nb: 2\n";

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert!(
            diagnostics.is_empty(),
            "top-level comment between top-level keys should not produce diagnostics"
        );
    }

    #[test]
    fn test_indented_comment_in_nested_block_still_checked() {
        // A comment indented to match nested block should still work correctly
        let yaml = "root:\n  nested:\n    # comment at level 2\n    key: value\n";

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert!(
            diagnostics.is_empty(),
            "correctly indented nested comment should not produce diagnostics"
        );
    }

    #[test]
    fn test_indented_comment_wrong_level_still_flagged() {
        // A comment with wrong indentation inside a nested block should still be flagged
        let yaml = "root:\n  nested:\n  # wrong level comment\n    key: value\n";

        let rule = CommentsIndentationRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert!(
            !diagnostics.is_empty(),
            "incorrectly indented nested comment should produce diagnostics"
        );
    }

    #[test]
    fn test_hash_inside_multiline_scalars_is_not_a_comment() {
        for yaml in [
            "a:\n  b: \"one\n      #two\n    three\"\n  c: 1\n",
            "a:\n  b: |\n    three\n      #two\n    more\n  c: 1\n",
        ] {
            assert_eq!(diag_count(yaml), 0, "{yaml:?}");
        }
    }

    #[test]
    fn test_hash_line_inside_scalar_counts_as_content() {
        let yaml = "a:\n  b: |\n    #x\n  # real\n  c: 1\n";
        assert_eq!(diag_count(yaml), 0);
        let yaml = "a:\n  b: \"p\n      #x\"\n# misplaced\n  c: 1\n";
        assert_eq!(diag_count(yaml), 1);
    }

    #[test]
    fn test_comments_after_a_block_scalar_are_not_checked() {
        for yaml in [
            "a: |-\n  x\n # c\nb: 1\n",
            "a: |-\n  x\n  # c\nb: 1\n",
            "a: |-\n  x\n# c\nb: 1\n",
            "a: |-\n  x\n   # c\nb: 1\n",
            "a: |-\n  x\n # c\n",
            "a: >\n   x\n  # c\n",
            "k:\n  a: |\n    x\n  # c\n  b: 1\n",
            "k:\n  a: |\n    x\n   # c\n  b: 1\n",
            "k:\n  a: |\n    x\n # c\n  b: 1\n",
            "a: |\n  x\n\n # c\n # d\nb: 1\n",
        ] {
            assert_eq!(diag_count(yaml), 0, "{yaml:?}");
        }
    }

    #[test]
    fn test_comments_after_other_content_are_still_checked() {
        for yaml in ["a: 1\n # c\nb: 1\n", "a:\n  - x\n # c\nb: 1\n"] {
            assert_eq!(diag_count(yaml), 1, "{yaml:?}");
        }
    }

    #[test]
    fn test_comment_after_a_block_scalar_that_is_followed_by_more_content() {
        assert_eq!(diag_count("a: |\n  x\nb: 1\n # c\nc: 1\n"), 1);
    }
}
