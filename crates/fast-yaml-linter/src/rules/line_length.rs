//! Rule to check line length limits.

use std::num::NonZeroUsize;

use serde::{Deserialize, Serialize};

use crate::config::RuleOptions;
use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity, SourceContext,
};
use fast_yaml_core::events::{Event, EventStream};
use fast_yaml_core::limits::ParseLimits;
use fast_yaml_core::{NormalizedInput, Value};

use super::node_roles::{CollectionStyle, NodeRole, RoleTracker};

/// Rule to check line length limits.
///
/// A line over the limit is not reported when it cannot be broken: with
/// `allow-non-breakable-words` (the default) its content, after indentation and a `#` or `-`
/// marker, is one word; with `allow-non-breakable-inline-mappings` it also may be a mapping
/// whose value is one word, as in yamllint.
pub struct LineLengthRule;

/// Options of the line-length rule.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::rules::LineLengthOptions;
///
/// assert_eq!(LineLengthOptions::default().max.map(|max| max.get()), Some(80));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct LineLengthOptions {
    /// Maximum line length in characters; `null` removes the limit.
    pub max: Option<NonZeroUsize>,
    /// Accepts a long line whose content is a single word, such as a URL.
    pub allow_non_breakable_words: bool,
    /// Also accepts `key: word` lines; implies `allow-non-breakable-words`.
    pub allow_non_breakable_inline_mappings: bool,
}

impl Default for LineLengthOptions {
    fn default() -> Self {
        Self {
            max: NonZeroUsize::new(80),
            allow_non_breakable_words: true,
            allow_non_breakable_inline_mappings: false,
        }
    }
}

impl RuleOptions for LineLengthOptions {
    const NULLABLE: &'static [&'static str] = &["max"];
}

impl LineLengthOptions {
    /// Whether an over-long `line` is let through because it cannot be broken (yamllint's
    /// algorithm).
    fn is_non_breakable(&self, line: &str) -> bool {
        if !(self.allow_non_breakable_words || self.allow_non_breakable_inline_mappings) {
            return false;
        }
        let indent = line.bytes().take_while(|&b| b == b' ').count();
        let Some(rest) = line.get(indent..).filter(|rest| !rest.is_empty()) else {
            return false;
        };
        let marker_chars = match rest.chars().next() {
            Some('#') => rest.chars().take_while(|&c| c == '#').count() + 1,
            Some('-') => 2,
            _ => 0,
        };
        let content = rest
            .char_indices()
            .nth(marker_chars)
            .and_then(|(at, _)| rest.get(at..));
        if content.is_none_or(|content| !content.contains(' ')) {
            return true;
        }
        self.allow_non_breakable_inline_mappings && is_inline_mapping_of_one_word(line)
    }
}

/// Whether `line` is, from its first content char to its last, one flow collection (`[...]` or
/// `{...}`, after indentation and `- ` markers), which is never an inline mapping.
///
/// Decided from the text, so a multi-megabyte flow line is not handed to the parser, whose
/// scanner buffers tokens far beyond the line's size.
fn is_whole_flow_collection(line: &str) -> bool {
    let mut content = line.trim_start_matches(' ');
    while let Some(rest) = content.strip_prefix("- ") {
        content = rest.trim_start_matches(' ');
    }
    let content = content.trim_end();
    (content.starts_with('[') && content.ends_with(']'))
        || (content.starts_with('{') && content.ends_with('}'))
}

/// Whether `line` holds a block mapping whose first plain or quoted value, without anchor or tag,
/// has no space from its start to the end of the line (yamllint's inline mapping check).
///
/// A line that does not parse on its own, or whose first mapping is a flow mapping, is not one.
fn is_inline_mapping_of_one_word(line: &str) -> bool {
    if is_whole_flow_collection(line) {
        return false;
    }
    let Ok(input) = NormalizedInput::new(line) else {
        return false;
    };
    let line = input.as_str();
    let context = SourceContext::new(line);
    let mut roles = RoleTracker::default();
    let mut seen_mapping = false;
    for item in EventStream::new(&input, ParseLimits::default()) {
        let Ok(item) = item else {
            break;
        };
        let range = context.byte_range_between(item.at, item.end);
        match item.event {
            Event::MappingStart { .. } => {
                if !seen_mapping && CollectionStyle::of_start(line, range) == CollectionStyle::Flow
                {
                    return false;
                }
                seen_mapping = true;
                roles.start_mapping(line, range);
            }
            Event::SequenceStart { .. } => {
                // Whatever a root flow sequence holds is flow too, so no block mapping follows
                if !seen_mapping && CollectionStyle::of_start(line, range) == CollectionStyle::Flow
                {
                    return false;
                }
                roles.start_sequence(line, range);
            }
            Event::MappingEnd | Event::SequenceEnd => roles.leave(),
            Event::Alias(_) => {
                roles.node();
            }
            Event::Scalar { anchor, tag, .. } => {
                let role = roles.node();
                if role == NodeRole::MappingValue
                    && seen_mapping
                    && anchor.is_none()
                    && tag.is_none()
                {
                    return line
                        .get(range.start().get()..)
                        .is_some_and(|rest| !rest.contains(' '));
                }
            }
            Event::StreamStart
            | Event::StreamEnd
            | Event::DocumentStart { .. }
            | Event::DocumentEnd => {}
        }
    }
    false
}

impl super::LintRule for LineLengthRule {
    fn code(&self) -> &str {
        DiagnosticCode::LINE_LENGTH
    }

    fn name(&self) -> &'static str {
        "Line Length"
    }

    fn description(&self) -> &'static str {
        "Checks that lines do not exceed the configured maximum length"
    }

    fn default_severity(&self) -> Severity {
        Severity::Info
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let options = &config.rules.line_length.options;
        let Some(max_length) = options.max.map(NonZeroUsize::get) else {
            return Vec::new();
        };

        let mut diagnostics = Vec::new();
        let ctx = context.source_context();

        for line_num in 1..=ctx.line_count() {
            if let Some(line_content) = ctx.get_line(line_num) {
                let line_len = line_content.chars().count();
                if line_len > max_length && !options.is_non_breakable(line_content) {
                    let span = ctx.span_at(ctx.line_start(line_num), line_content.len());

                    let diagnostic = DiagnosticBuilder::new(
                        DiagnosticCode::LINE_LENGTH,
                        config.rules.line_length.severity_or(self.default_severity()),
                        format!(
                            "line exceeds maximum length of {max_length} characters (current: {line_len})"
                        ),
                        span,
                    )
                    .build_with_context(context.source_context());

                    diagnostics.push(diagnostic);
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
        rules::LintRule,
    };
    use fast_yaml_core::Parser;

    #[test]
    fn test_line_within_limit() {
        let yaml = "key: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = LineLengthRule;
        let config = LintConfig::default();
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_no_limit_configured() {
        let yaml = "key: this is a very long line that would normally exceed any reasonable limit but should not trigger warnings";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = LineLengthRule;
        let config = LintConfig::new().with_max_line_length(None);
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_line_exceeds_limit() {
        let yaml = "key: this is a very long value that definitely exceeds eighty characters without any doubt whatsoever";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = LineLengthRule;
        let config = LintConfig::new().with_max_line_length(NonZeroUsize::new(80));
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("exceeds maximum length"));
        assert!(diagnostics[0].message.contains("80"));
    }

    #[test]
    fn test_line_at_exact_limit() {
        // This line is exactly 77 characters long
        let yaml = "name: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = LineLengthRule;
        let config = LintConfig::new().with_max_line_length(NonZeroUsize::new(77));
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        // Exactly at limit should not trigger
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_line_one_over_limit() {
        // This line is 78 characters long
        let yaml = "name: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = LineLengthRule;
        let config = LintConfig::new().with_max_line_length(NonZeroUsize::new(77));
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        // One over should trigger
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn test_multiple_long_lines() {
        let yaml = "first: this is a very long line that exceeds the maximum character limit\n\
                    second: another extremely long line that also exceeds the character limit\n\
                    short: ok";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = LineLengthRule;
        let config = LintConfig::new().with_max_line_length(NonZeroUsize::new(50));
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn test_utf8_multibyte_characters() {
        // 5 Japanese characters (日本語日本語日本語日本語日本語) + "key: " = ~29 chars
        let yaml = "key: 日本語日本語日本語日本語日本語日本語日本語日本語";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = LineLengthRule;
        let config = LintConfig::new().with_max_line_length(NonZeroUsize::new(20));
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        // Should count characters, not bytes
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn test_empty_lines_ignored() {
        let yaml = "key: value\n\n\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = LineLengthRule;
        let config = LintConfig::new().with_max_line_length(NonZeroUsize::new(5));
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        // Should only report the first line (10 chars), not the empty lines
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn test_severity_override() {
        let yaml = "key: this is a very long value that definitely exceeds eighty characters without any doubt";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = LineLengthRule;
        let config = config_with_rule(RuleName::LineLength, "{max: 10, severity: error}");
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }

    #[test]
    fn test_diagnostic_location_accuracy() {
        let yaml = "first: ok\nvery_long_key_name: this is a very long value that definitely exceeds fifty chars\nthird: ok";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = LineLengthRule;
        let config = LintConfig::new().with_max_line_length(NonZeroUsize::new(50));
        let lint_context = LintContext::new(yaml);
        let diagnostics = rule.check(&lint_context, &value, &config);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].span.start.line, 2); // Second line
    }

    fn flagged(yaml: &str, options: &str) -> Vec<usize> {
        let config = config_with_rule(RuleName::LineLength, options);
        LineLengthRule
            .check(&LintContext::new(yaml), &Value::Null, &config)
            .iter()
            .map(|d| d.span.start.line)
            .collect()
    }

    const URL: &str = "http://localhost/very/very/very/very/very/very/very/very/long/url";

    #[test]
    fn single_word_lines_are_allowed_by_default() {
        for yaml in [
            format!("{URL}\n"),
            format!("  {URL}\n"),
            format!("- {URL}\n"),
            format!("# {URL}\n"),
            format!("### {URL}\n"),
            format!("a:\n  {URL}\n"),
        ] {
            assert!(flagged(&yaml, "{max: 20}").is_empty(), "{yaml:?}");
        }
    }

    #[test]
    fn lines_with_a_space_after_the_marker_are_flagged() {
        for yaml in [
            "# a b http://localhost/very/very/very/long/url\n",
            "- a b http://localhost/very/very/very/long/url\n",
            "a http://localhost/very/very/very/very/long/url\n",
        ] {
            assert_eq!(flagged(yaml, "{max: 20}"), [1], "{yaml:?}");
        }
    }

    #[test]
    fn single_word_lines_are_flagged_when_words_are_not_allowed() {
        assert_eq!(
            flagged(
                &format!("- {URL}\n"),
                "{max: 20, allow-non-breakable-words: false}"
            ),
            [1]
        );
    }

    #[test]
    fn inline_mappings_with_one_word_are_allowed_on_request() {
        let on = "{max: 20, allow-non-breakable-inline-mappings: true}";
        for yaml in [
            format!("key: {URL}\n"),
            format!("- key: {URL}\n"),
            format!("  nested: {URL}\n"),
            format!("- key: \"{}\"\n", URL.replace('/', "")),
            format!("key: {{a: {URL}}}\n"),
        ] {
            assert!(flagged(&yaml, on).is_empty(), "{yaml:?}");
        }
        for yaml in [
            "key: a b http://localhost/very/very/very/long/url\n".to_owned(),
            format!("key: !!str {URL}\n"),
            format!("key: &x {URL}\n"),
        ] {
            assert_eq!(flagged(&yaml, on), [1], "{yaml:?}");
        }
    }

    #[test]
    fn a_very_long_single_word_stays_exempt_in_an_inline_mapping() {
        let on = "{max: 20, allow-non-breakable-inline-mappings: true}";
        let word = "QUJD".repeat(50_000);
        for yaml in [format!("key: {word}\n"), format!("- key: \"{word}\"\n")] {
            assert!(flagged(&yaml, on).is_empty(), "{} bytes", yaml.len());
        }
    }

    #[test]
    fn a_line_that_is_one_flow_collection_is_not_an_inline_mapping() {
        let on = "{max: 20, allow-non-breakable-inline-mappings: true}";
        for yaml in [
            "[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]\n",
            "- {a: 1, b: 2, c: 3, d: 4}\n",
        ] {
            assert_eq!(flagged(yaml, on), [1], "{yaml:?}");
        }
        assert_eq!(
            flagged("[a, b]: http://localhost/very/very/very/long/url\n", on).len(),
            0
        );
    }

    #[test]
    fn inline_mappings_imply_non_breakable_words() {
        let yaml = format!("{URL}\n");
        let options = "{max: 20, allow-non-breakable-words: false, allow-non-breakable-inline-mappings: true}";
        assert_eq!(flagged(&yaml, options), [] as [usize; 0]);
    }

    #[test]
    fn indented_unparsable_line_is_not_an_inline_mapping() {
        let on = "{max: 20, allow-non-breakable-inline-mappings: true}";
        assert_eq!(
            flagged("  a: b: http://localhost/very/very/very/long/url\n", on),
            [1]
        );
    }
}
