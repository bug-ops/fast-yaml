//! Rule to check comment formatting.

use super::RuleId;
use crate::config::RuleName;
use serde::{Deserialize, Serialize};

use crate::config::{Limit, RuleOptions};
use crate::{CommentKind, Finding, LintConfig, LintContext, Severity};

/// Linting rule for comment formatting.
///
/// Validates comment formatting conventions:
/// - Require space after '#' character
/// - Minimum spacing from content for inline comments
/// - Optional shebang exemption
/// - Extra leading `#` characters (`## note`, `#####`) are skipped before the space check, like
///   yamllint
///
/// Configuration options:
/// - `require-starting-space`: bool (default: true)
/// - `ignore-shebangs`: bool (default: true)
/// - `min-spaces-from-content`: integer (default: 2)
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::CommentsRule, rules::SourceRule, LintConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = CommentsRule;
/// let yaml = "# Valid comment\nkey: value  # Also valid";
///
/// let config = LintConfig::default();
/// let diagnostics = rule.diagnose(&fast_yaml_linter::LintContext::new(yaml), &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct CommentsRule;

/// Options of the comments rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct CommentsOptions {
    /// Require a space after `#`.
    pub require_starting_space: bool,
    /// Skip the check for a shebang on the first line.
    pub ignore_shebangs: bool,
    /// Minimum spaces between content and an inline comment.
    pub min_spaces_from_content: Limit,
}

impl Default for CommentsOptions {
    fn default() -> Self {
        Self {
            require_starting_space: true,
            ignore_shebangs: true,
            min_spaces_from_content: Limit::Max(2),
        }
    }
}

impl RuleOptions for CommentsOptions {}

impl super::LintRule for CommentsRule {
    fn id(&self) -> RuleId<'_> {
        RuleId::BuiltIn(RuleName::Comments)
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
}

impl super::SourceRule for CommentsRule {
    fn check(&self, context: &LintContext, config: &LintConfig) -> Vec<Finding> {
        let comments = context.comments();

        let options = &config.rules.comments.options;
        let require_starting_space = options.require_starting_space;
        let ignore_shebangs = options.ignore_shebangs;
        let min_spaces_from_content = options.min_spaces_from_content;

        let mut diagnostics = Vec::new();

        for comment in comments {
            // Skip shebangs if configured
            if comment.kind == CommentKind::Shebang && ignore_shebangs {
                continue;
            }

            // Check for space after '#'
            let text = comment.text.trim_start_matches('#');
            if require_starting_space && !text.is_empty() && !text.starts_with(' ') {
                diagnostics.push(Finding::new(
                    "comment should start with a space after '#'",
                    comment.span,
                ));
            }

            // Check spacing from content for inline comments
            if comment.kind == CommentKind::Inline {
                // Find the line and check spacing before '#'
                let line_num = comment.span.start.line();
                let line_offset = context.source_context().get_line_offset(line_num);
                if let Some(line) = context.source_context().get_line(line_num) {
                    let comment_col = comment.span.start.offset() - line_offset;

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

                    if min_spaces_from_content.unmet_by(spaces_before) {
                        diagnostics.push(
                            Finding::new(format!(
                                    "too few spaces before comment (expected at least {min_spaces_from_content}, found {spaces_before})"
                                ), comment.span),
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
    use crate::{
        config::{RuleName, test_support::config_with_rule},
        rules::SourceRule,
    };
    use fast_yaml_core::Parser;

    #[test]
    fn test_comments_valid_standalone() {
        let yaml = "# This is a comment\nkey: value";

        let rule = CommentsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_comments_valid_inline() {
        let yaml = "key: value  # This is a comment";

        let rule = CommentsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_comments_repeated_hash_needs_no_extra_space() {
        let config = LintConfig::default();
        for yaml in [
            "## note\na: 1",
            "#####\na: 1",
            "### note\na: 1",
            "a: 1  ## x",
        ] {
            let found = CommentsRule.diagnose(&LintContext::new(yaml), &config);
            assert_eq!(found, [], "{yaml:?}");
        }
        let yaml = "##note\na: 1";
        let found = CommentsRule.diagnose(&LintContext::new(yaml), &config);
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn test_comments_no_space_after_hash() {
        let yaml = "#No space\nkey: value";

        let rule = CommentsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_ne!(diagnostics, []);
        assert!(diagnostics[0].message.contains("should start with a space"));
    }

    #[test]
    fn test_comments_allow_no_space_when_disabled() {
        let yaml = "#No space\nkey: value";

        let rule = CommentsRule;
        let config = config_with_rule(RuleName::Comments, "{require-starting-space: false}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_comments_too_few_spaces_from_content() {
        let yaml = "key: value # Only 1 space";

        let rule = CommentsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_ne!(diagnostics, []);
        assert!(
            diagnostics[0]
                .message
                .contains("too few spaces before comment")
        );
    }

    #[test]
    fn test_comments_custom_min_spaces() {
        let yaml = "key: value # Only 1 space";

        let rule = CommentsRule;
        let config = config_with_rule(RuleName::Comments, "{min-spaces-from-content: 1}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_comments_shebang_ignored() {
        let yaml = "#!/usr/bin/env yaml\nkey: value";

        let rule = CommentsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_comments_shebang_not_ignored() {
        let yaml = "#!/usr/bin/env yaml\nkey: value";

        let rule = CommentsRule;
        let config = config_with_rule(RuleName::Comments, "{ignore-shebangs: false}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_ne!(diagnostics, []);
        assert!(diagnostics[0].message.contains("should start with a space"));
    }

    #[test]
    fn test_comments_empty_comment() {
        let yaml = "key: value  #";

        let rule = CommentsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        // Empty comment is valid (no content to check)
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_comments_multiple_violations() {
        let yaml = "#No space\nkey: value #one space\nanother: test";
        let _value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = CommentsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        // Should find: 1) "#No space" (no space after #), 2) "#one space" (no space after #), 3) "value #one" (too few spaces before comment)
        assert_eq!(diagnostics.len(), 3);
    }

    #[test]
    fn test_comments_shebang_in_block_scalar_no_false_positive() {
        // Regression test for #160: '#' inside a '|' block scalar must not
        // produce a comment diagnostic.
        let yaml = "script: |\n  #!/bin/bash\n  echo hello\nkey: value";

        let rule = CommentsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_comments_in_string_ignored() {
        let yaml = r#"text: "not # a comment""#;

        let rule = CommentsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_hash_inside_multiline_scalars_is_not_a_comment() {
        for yaml in [
            "a: \"one\n  #two\n  three\"\n",
            "a: 'one\n  #two\n  three'\n",
            "a: |\n  #two\n  three\n",
        ] {
            let context = LintContext::new(yaml);
            let diagnostics = CommentsRule.diagnose(&context, &LintConfig::default());
            assert!(diagnostics.is_empty(), "{yaml:?}: {diagnostics:?}");
        }
    }
}
