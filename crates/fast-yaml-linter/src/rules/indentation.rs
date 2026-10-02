//! Rule to check indentation, ported from yamllint's token-based `indentation` rule.
//!
//! The parser's events carry no indicators or implicit tokens, so the `token_stream` module rebuilds the token
//! stream of `PyYAML` from the node index and the text between nodes, and the `machine` module runs
//! yamllint's stack of enclosing structures over it. Findings, columns and messages match
//! yamllint 1.38 for every document both parsers accept.

use super::{LintRule, RuleId};
use crate::config::RuleName;
use serde::{Deserialize, Deserializer, Serialize};

use crate::config::{IndentSequences, IndentSize, IndentSpaces, RuleOptions};
use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    Span,
};

mod machine;

use super::token_stream::scanner;
use machine::Machine;

/// Rule to check that each line is indented as the structure it belongs to requires.
///
/// Reports `wrong indentation: expected N but found M` at the first token of a misindented line.
/// A line that mixes tabs and spaces in its indentation is reported separately.
pub struct IndentationRule;

/// Options of the indentation rule.
///
/// The width of a level is one setting, [`spaces`](Self::spaces): `None` until a config file, a
/// flag or the formatter indent chooses one, which reads as `consistent` as in yamllint. In a
/// config file `spaces` is yamllint's key and takes `consistent`; `indent-size` is the fast-yaml
/// key for a fixed width and sets `spaces` to it, `spaces` winning when both are written.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::config::IndentSpaces;
/// use fast_yaml_linter::rules::IndentationOptions;
///
/// let options = IndentationOptions::default();
/// assert_eq!(options.indent_size().get(), 2);
/// assert!(!options.width_is_set());
///
/// let consistent: IndentationOptions = serde_norway::from_str("spaces: consistent").unwrap();
/// assert_eq!(consistent.width(), IndentSpaces::Consistent);
///
/// let fixed: IndentationOptions = serde_norway::from_str("indent-size: 4").unwrap();
/// assert_eq!(fixed.indent_size().get(), 4);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct IndentationOptions {
    /// Width of an indentation level, or `consistent`; `None` when not set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spaces: Option<IndentSpaces>,
    /// Whether a sequence nested in a mapping is indented under its key.
    pub indent_sequences: IndentSequences,
    /// Whether the lines of a multi-line scalar must be indented like its first line.
    pub check_multi_line_strings: bool,
}

/// The keys of [`IndentationOptions`] as written in a config file.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
struct IndentationOptionsKeys {
    #[serde(deserialize_with = "some_value")]
    indent_size: Option<IndentSize>,
    #[serde(deserialize_with = "some_value")]
    spaces: Option<IndentSpaces>,
    indent_sequences: IndentSequences,
    check_multi_line_strings: bool,
}

impl Default for IndentationOptionsKeys {
    fn default() -> Self {
        let defaults = IndentationOptions::default();
        Self {
            indent_size: None,
            spaces: defaults.spaces,
            indent_sequences: defaults.indent_sequences,
            check_multi_line_strings: defaults.check_multi_line_strings,
        }
    }
}

impl<'de> Deserialize<'de> for IndentationOptions {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let keys = IndentationOptionsKeys::deserialize(deserializer)?;
        Ok(Self {
            spaces: keys
                .spaces
                .or_else(|| keys.indent_size.map(IndentSpaces::Fixed)),
            indent_sequences: keys.indent_sequences,
            check_multi_line_strings: keys.check_multi_line_strings,
        })
    }
}

/// Reads a present key as `Some`, so an explicit `null` is an error like for other options.
fn some_value<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

impl IndentationOptions {
    /// Whether a config file or a flag chose a width.
    #[must_use]
    pub const fn width_is_set(&self) -> bool {
        self.spaces.is_some()
    }

    /// The width of a level: `spaces`, or `consistent` when it is not set.
    #[must_use]
    pub const fn width(&self) -> IndentSpaces {
        match self.spaces {
            Some(spaces) => spaces,
            None => IndentSpaces::Consistent,
        }
    }

    /// The fixed width of a level, 2 when it is `consistent` or unset.
    #[must_use]
    pub fn indent_size(&self) -> IndentSize {
        match self.width() {
            IndentSpaces::Fixed(size) => size,
            IndentSpaces::Consistent => IndentSize::default(),
        }
    }
}

impl RuleOptions for IndentationOptions {}

impl super::LintRule for IndentationRule {
    fn id(&self) -> RuleId<'_> {
        RuleId::BuiltIn(RuleName::Indentation)
    }

    fn name(&self) -> &'static str {
        "Indentation"
    }

    fn description(&self) -> &'static str {
        "Checks for consistent indentation throughout the YAML file"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }
}

impl super::SourceRule for IndentationRule {
    fn check(&self, context: &LintContext, config: &LintConfig) -> Vec<Diagnostic> {
        let source = context.source();
        let severity = config
            .rules
            .indentation
            .severity_or(self.default_severity());
        let options = &config.rules.indentation.options;
        let spaces = match options.width() {
            IndentSpaces::Fixed(size) => Some(size.get()),
            IndentSpaces::Consistent => None,
        };
        let problems = if is_one_flush_left_line(source) {
            Vec::new()
        } else {
            machine_problems(context, spaces, options)
        };

        let mut diagnostics = mixed_whitespace(context, severity);
        diagnostics.extend(problems.into_iter().map(|problem| {
            let width = source
                .get(problem.offset..)
                .and_then(|rest| rest.chars().next())
                .map_or(0, char::len_utf8);
            let span = Span::new(
                Location::new(problem.line, problem.column, problem.offset),
                Location::new(problem.line, problem.column + 1, problem.offset + width),
            );
            DiagnosticBuilder::new(DiagnosticCode::INDENTATION, severity, problem.message, span)
                .build()
        }));
        diagnostics.sort_by_key(|d| (d.span.start.line, d.span.start.column));
        diagnostics
    }
}

/// Whether `source` is ASCII text of a single line that starts in column 0.
///
/// The check compares a line with the ones around it, and the first token of a line only with the
/// root indent, so such a source has nothing to report and the token stream is not scanned. This
/// is what a minified JSON file is.
fn is_one_flush_left_line(source: &str) -> bool {
    let body = source.trim_end_matches(['\n', '\r']);
    body.is_ascii()
        && !body.starts_with([' ', '\t'])
        && !body.bytes().any(|byte| matches!(byte, b'\n' | b'\r'))
}

/// Runs the indentation machine over the token stream of `context`.
fn machine_problems(
    context: &LintContext,
    spaces: Option<usize>,
    options: &IndentationOptions,
) -> Vec<machine::Problem> {
    let mut machine = Machine::new(
        context.source(),
        spaces,
        options.indent_sequences,
        options.check_multi_line_strings,
    );
    scanner::scan(
        context.source(),
        context.nodes(),
        context.scan_is_complete(),
        |token| machine.push(token),
    );
    machine.finish()
}

/// Reports lines whose indentation mixes tabs and spaces.
fn mixed_whitespace(context: &LintContext, severity: Severity) -> Vec<Diagnostic> {
    let ctx = context.source_context();
    let mut diagnostics = Vec::new();
    for line_num in 1..=ctx.line_count() {
        let Some(line) = ctx.get_line(line_num) else {
            continue;
        };
        let width = line.bytes().take_while(u8::is_ascii_whitespace).count();
        let indent = line.as_bytes().get(..width).unwrap_or_default();
        let Some(&first) = indent.first() else {
            continue;
        };
        let other = if first == b' ' { b'\t' } else { b' ' };
        if !indent.contains(&other) {
            continue;
        }
        let line_offset = ctx.get_line_offset(line_num);
        let span = Span::new(
            Location::new(line_num, 1, line_offset),
            Location::new(line_num, width + 1, line_offset + width),
        );
        diagnostics.push(
            DiagnosticBuilder::new(
                DiagnosticCode::INDENTATION,
                severity,
                "mixed tabs and spaces in indentation".to_string(),
                span,
            )
            .build(),
        );
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LintConfig, LintContext,
        config::{IndentSize, RuleName, test_support::config_with_rule},
        rules::SourceRule,
    };

    #[test]
    fn a_one_line_flush_left_source_is_recognized() {
        for yes in [
            "[1,2]",
            "[1,2]\n",
            "a: [1, 2]\r\n",
            "--- {a: b}\n",
            "# c",
            "x",
        ] {
            assert!(is_one_flush_left_line(yes), "{yes:?}");
        }
        for no in [
            " [1]",
            "\t[1]",
            "a: 1\nb: 2",
            "a: 1\r\nb: 2",
            "[\u{43a}]",
            "a:\n  b",
        ] {
            assert!(!is_one_flush_left_line(no), "{no:?}");
        }
    }

    #[test]
    fn the_machine_reports_nothing_for_one_flush_left_line() {
        let options = IndentationOptions::default();
        for source in [
            "[1,2,3]\n",
            "{a: 1, b: [2, 3], c: {d: e}}\n",
            "a: [1, 2]\n",
            "a: {b: [c, {d: e}]}\n",
            "--- [1, 2]\n",
            "[[], [[]], {}]\n",
            "- a\n",
            "- - a\n",
            "? a\n",
            "&x [1, *x]\n",
            "!!seq [1]\n",
            "'quoted': \"double\"\n",
            "a: |\n",
            "[a: 1, b: 2]\n",
            "key: value # note\n",
            "[1, 2] # note\n",
            "{a: 1}\r\n",
            "-1\n",
            "a:\n",
            "[\n",
        ] {
            assert!(is_one_flush_left_line(source), "{source:?}");
            let context = LintContext::new(source);
            assert_eq!(machine_problems(&context, None, &options), [], "{source:?}");
            let context = LintContext::new(source);
            assert_eq!(
                machine_problems(&context, Some(4), &options),
                [],
                "{source:?}"
            );
        }
    }

    #[test]
    fn test_correct_2space_indent() {
        let yaml = "parent:\n  child: value\n  nested:\n    deep: ok\n";
        let rule = IndentationRule;
        let config = LintConfig::default();
        let ctx = LintContext::new(yaml);
        assert_eq!(rule.check(&ctx, &config), []);
    }

    #[test]
    fn test_wrong_indent_size() {
        let yaml = "parent:\n   child: value\n";
        let rule = IndentationRule;
        let config = LintConfig::new().with_indent_size(IndentSize::try_from(2u64).unwrap());
        let ctx = LintContext::new(yaml);
        let diagnostics = rule.check(&ctx, &config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("wrong indentation"));
        assert_eq!(diagnostics[0].span.start.line, 2);
    }

    #[test]
    fn test_mixed_tabs_and_spaces() {
        // Tab indentation is illegal YAML, so the rule gets the invalid source directly.
        // Source with mixed leading whitespace (tab then space).
        let mixed_source = "parent:\n\t child: value\n";
        let rule = IndentationRule;
        let config = LintConfig::default();
        let ctx = LintContext::new(mixed_source);
        let diagnostics = rule.check(&ctx, &config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("mixed tabs and spaces"));
    }

    #[test]
    fn test_top_level_no_indent() {
        let yaml = "key: value\nother: 42\n";
        let rule = IndentationRule;
        let config = LintConfig::default();
        let ctx = LintContext::new(yaml);
        assert_eq!(rule.check(&ctx, &config), []);
    }

    #[test]
    fn test_indent_size_4_correct() {
        let yaml = "parent:\n    child: value\n";
        let rule = IndentationRule;
        let config = LintConfig::new().with_indent_size(IndentSize::try_from(4u64).unwrap());
        let ctx = LintContext::new(yaml);
        assert_eq!(rule.check(&ctx, &config), []);
    }

    #[test]
    fn test_indent_size_4_wrong() {
        let yaml = "parent:\n  child: value\n";
        let rule = IndentationRule;
        let config = LintConfig::new().with_indent_size(IndentSize::try_from(4u64).unwrap());
        let ctx = LintContext::new(yaml);
        let diagnostics = rule.check(&ctx, &config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("wrong indentation"));
    }

    #[test]
    fn test_tabs_only_no_diagnostic() {
        // Tab indentation is rejected by the YAML parser, but the rule should
        // not emit a wrong-indent-size diagnostic for tab-only leading whitespace.
        let tab_source = "parent:\n\tchild: value\n";
        let rule = IndentationRule;
        let config = LintConfig::default();
        let ctx = LintContext::new(tab_source);
        assert_eq!(rule.check(&ctx, &config), []);
    }

    #[test]
    fn test_severity_override() {
        let yaml = "parent:\n   child: value\n";
        let rule = IndentationRule;
        let config = config_with_rule(RuleName::Indentation, "{severity: error, spaces: 2}");
        let ctx = LintContext::new(yaml);
        let diagnostics = rule.check(&ctx, &config);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }

    #[test]
    fn test_multiple_violations() {
        // Lines 2 and 3 have 3-space indent (not multiple of 2); line 4 has 6-space (ok).
        let yaml = "parent:\n   child: value\n   nested:\n      deep: bad\n";
        let rule = IndentationRule;
        let config = LintConfig::new().with_indent_size(IndentSize::try_from(2u64).unwrap());
        let ctx = LintContext::new(yaml);
        let diagnostics = rule.check(&ctx, &config);
        assert_eq!(diagnostics.len(), 2);
    }

    fn messages(yaml: &str, options: &str) -> Vec<String> {
        let config = config_with_rule(RuleName::Indentation, options);
        let ctx = LintContext::new(yaml);
        IndentationRule
            .check(&ctx, &config)
            .into_iter()
            .map(|d| {
                format!(
                    "{}:{} {}",
                    d.span.start.line, d.span.start.column, d.message
                )
            })
            .collect()
    }

    #[test]
    fn test_spaces_consistent_follows_the_first_level() {
        assert_eq!(
            messages("a:\n   b: 1\n   c: 2\n", "{spaces: consistent}"),
            [] as [String; 0]
        );
        assert_eq!(
            messages("a:\n  b:\n     c: 1\n", "{spaces: consistent}"),
            ["3:6 wrong indentation: expected 4 but found 5"]
        );
    }

    #[test]
    fn test_indent_sequences_options() {
        let plain = "a:\n- 1\n";
        let indented = "a:\n  - 1\n";
        assert_eq!(
            messages(plain, "{indent-sequences: false}"),
            [] as [String; 0]
        );
        assert_eq!(messages(indented, "{indent-sequences: false}").len(), 1);
        assert_eq!(messages(plain, "{indent-sequences: true}").len(), 1);
        assert_eq!(
            messages(indented, "{indent-sequences: whatever}"),
            [] as [String; 0]
        );
        assert_eq!(
            messages("a:\n- 1\nb:\n  - 2\n", "{indent-sequences: consistent}").len(),
            1
        );
    }

    #[test]
    fn test_flow_closer_and_continuation() {
        assert_eq!(
            messages("a: [\n  1,\n  2,\n]\n", "{spaces: 2}"),
            [] as [String; 0]
        );
        assert_eq!(
            messages("a: [\n  1,\n    2,\n]\n", "{spaces: 2}"),
            ["3:5 wrong indentation: expected 2 but found 4"]
        );
    }

    #[test]
    fn test_multi_line_strings_are_checked_on_request() {
        let yaml = "a: x\n  y\n   z\n";
        assert_eq!(messages(yaml, "{spaces: 2}"), [] as [String; 0]);
        assert_eq!(
            messages(yaml, "{spaces: 2, check-multi-line-strings: true}"),
            ["2:3 wrong indentation: expected 3 but found 2"]
        );
    }

    #[test]
    fn test_block_scalar_header_with_chomping_is_not_a_sequence_entry() {
        assert_eq!(
            messages("a: |-\n  x\nb: >+\n  y\nc: |2\n    z\n", "{spaces: 2}"),
            [] as [String; 0]
        );
    }

    #[test]
    fn test_non_ascii_columns_are_chars() {
        assert_eq!(
            messages("ключ:\n  значение: 1\n  другое: 2\n", "{spaces: 2}"),
            [] as [String; 0]
        );
    }

    fn options(yaml: &str) -> IndentationOptions {
        serde_norway::from_str(yaml).unwrap()
    }

    fn fixed(width: u64) -> IndentSpaces {
        IndentSpaces::Fixed(IndentSize::try_from(width).unwrap())
    }

    #[test]
    fn test_unset_width_is_consistent_and_not_set() {
        let unset = IndentationOptions::default();
        assert_eq!(unset.spaces, None);
        assert!(!unset.width_is_set());
        assert_eq!(unset.width(), IndentSpaces::Consistent);
        assert_eq!(options("{}"), unset);
    }

    #[test]
    fn test_indent_size_key_sets_a_fixed_width() {
        let parsed = options("indent-size: 4");
        assert_eq!(parsed.spaces, Some(fixed(4)));
        assert!(parsed.width_is_set());
    }

    #[test]
    fn test_spaces_wins_over_indent_size() {
        assert_eq!(
            options("{spaces: 2, indent-size: 4}").spaces,
            Some(fixed(2))
        );
        assert_eq!(
            options("{spaces: consistent, indent-size: 4}").spaces,
            Some(IndentSpaces::Consistent)
        );
    }

    #[test]
    fn test_width_serializes_as_spaces_only() {
        let text = serde_norway::to_string(&options("indent-size: 4")).unwrap();
        assert!(text.contains("spaces: 4"), "{text}");
        assert!(!text.contains("indent-size"), "{text}");
        assert!(
            !serde_norway::to_string(&IndentationOptions::default())
                .unwrap()
                .contains("spaces")
        );
    }

    #[test]
    fn test_null_and_unknown_keys_are_errors() {
        assert!(serde_norway::from_str::<IndentationOptions>("indent-size: null").is_err());
        assert!(serde_norway::from_str::<IndentationOptions>("spaces: null").is_err());
        assert!(serde_norway::from_str::<IndentationOptions>("indent: 2").is_err());
        assert!(serde_norway::from_str::<IndentationOptions>("indent-size: 17").is_err());
    }
}
