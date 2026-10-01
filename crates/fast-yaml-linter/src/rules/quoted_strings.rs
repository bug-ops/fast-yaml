//! Rule to check quoted string style.

use serde::{Deserialize, Serialize};

use crate::config::{
    BoolOrName, OptionConflict, PatternList, RuleOptions, deserialize_bool_or_name,
};
use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity,
    SourceContext, Span,
};
use fast_yaml_core::{ResolvedScalar, ScalarStyle, Value, resolve_scalar};

use super::LintRule;
use super::node_roles::NodeRole;
use super::truthy::NON_STANDARD_BOOLS;
use crate::nodes::{Node, TagKind};

/// Linting rule for quoted strings.
///
/// Validates string quoting style to ensure consistency.
/// Controls whether strings should be quoted, and if so, which quote style to use.
///
/// Configuration options:
/// - `quote-type`: "single", "double", "any" (default: "any")
/// - `required`: "always", "only-when-needed", "never" (default: "only-when-needed")
/// - `extra-required`: list of patterns that always need quotes (default: [])
/// - `extra-allowed`: list of patterns where quotes are optional (default: [])
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::QuotedStringsRule, rules::LintRule, LintConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = QuotedStringsRule;
/// let yaml = "name: 'John'";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::default();
/// let context = fast_yaml_linter::LintContext::new(yaml);
/// let diagnostics = rule.check(&context, &value, &config);
/// assert!(!diagnostics.is_empty());  // Quotes are not needed for `John`
/// ```
pub struct QuotedStringsRule;

/// Quote style enforced by the quoted-strings rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QuoteType {
    /// Either quote style.
    #[default]
    Any,
    /// Single quotes only.
    Single,
    /// Double quotes only.
    Double,
}

/// When strings have to be quoted.
///
/// Accepts `true` (always), `false` (not required), `always`, `not-required`,
/// `only-when-needed` and `never`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QuoteRequirement {
    /// Every string value must be quoted.
    Always,
    /// Quotes are optional; only the quote style is checked.
    NotRequired,
    /// Quotes are flagged when the string does not need them.
    #[default]
    OnlyWhenNeeded,
    /// Quoted strings are flagged.
    Never,
}

impl BoolOrName for QuoteRequirement {
    const EXPECTING: &'static str =
        "a boolean, 'always', 'not-required', 'only-when-needed' or 'never'";

    fn from_bool(value: bool) -> Result<Self, &'static str> {
        Ok(if value {
            Self::Always
        } else {
            Self::NotRequired
        })
    }

    fn from_name(name: &str) -> Option<Self> {
        match name {
            "always" => Some(Self::Always),
            "not-required" => Some(Self::NotRequired),
            "only-when-needed" => Some(Self::OnlyWhenNeeded),
            "never" => Some(Self::Never),
            _ => None,
        }
    }
}

impl Serialize for QuoteRequirement {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Always => "always",
            Self::NotRequired => "not-required",
            Self::OnlyWhenNeeded => "only-when-needed",
            Self::Never => "never",
        })
    }
}

impl<'de> Deserialize<'de> for QuoteRequirement {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_bool_or_name(deserializer)
    }
}

/// Options of the quoted-strings rule.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct QuotedStringsOptions {
    /// Allowed quote style.
    pub quote_type: QuoteType,
    /// When strings have to be quoted.
    pub required: QuoteRequirement,
    /// Regular expressions for values that must be quoted; invalid with `always` and `never`.
    ///
    /// Matched with `re.search` semantics against plain scalars (flagged as unquoted) and, under
    /// `only-when-needed`, against quoted scalars (keeping their quotes).
    pub extra_required: PatternList,
    /// Regular expressions for values that may stay unquoted; valid only with `only-when-needed`.
    pub extra_allowed: PatternList,
    /// Accepts the other quote style for a string that contains the configured quote character.
    pub allow_quoted_quotes: bool,
    /// Also checks mapping keys; they are skipped by default.
    pub check_keys: bool,
}

impl QuotedStringsOptions {
    fn quotes_redundant_for(&self, value: &str) -> bool {
        !self.extra_required.is_match(value) && !self.extra_allowed.is_match(value)
    }

    fn allows_quoted_quote(&self, value: &str, configured_quote: char) -> bool {
        self.allow_quoted_quotes && value.contains(configured_quote)
    }
}

impl RuleOptions for QuotedStringsOptions {
    fn conflicts(&self) -> Vec<OptionConflict> {
        let mut conflicts = Vec::new();
        if !self.extra_required.is_empty()
            && matches!(
                self.required,
                QuoteRequirement::Always | QuoteRequirement::Never
            )
        {
            conflicts.push(OptionConflict {
                key: "extra-required",
                message: "cannot be combined with `required: always` or `never`".to_owned(),
            });
        }
        if !self.extra_allowed.is_empty() && self.required != QuoteRequirement::OnlyWhenNeeded {
            conflicts.push(OptionConflict {
                key: "extra-allowed",
                message: "is valid only with `required: only-when-needed`".to_owned(),
            });
        }
        conflicts
    }
}

impl super::LintRule for QuotedStringsRule {
    fn code(&self) -> &str {
        DiagnosticCode::QUOTED_STRINGS
    }

    fn name(&self) -> &'static str {
        "Quoted Strings"
    }

    fn description(&self) -> &'static str {
        "Validates string quoting style (quote-type, required)"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let check = ScalarCheck {
            source: context.source(),
            source_ctx: context.source_context(),
            config,
        };
        let index = context.nodes();

        for node in index.nodes() {
            let Node::Scalar(scalar) = node else {
                continue;
            };
            self.check_scalar(
                &check,
                &ScalarEvent {
                    value: index.text(scalar),
                    style: scalar.style,
                    role: scalar.role,
                    in_flow: scalar.in_flow,
                    core_tagged: scalar.tag == TagKind::Core,
                    span: check.source_ctx.span_of_bytes(scalar.range),
                },
                &mut diagnostics,
            );
        }

        diagnostics
    }
}

/// Source and configuration shared by every scalar check of one lint run.
struct ScalarCheck<'a> {
    source: &'a str,
    source_ctx: &'a SourceContext<'a>,
    config: &'a LintConfig,
}

/// One scalar event with the context the rule needs to judge it.
struct ScalarEvent<'a> {
    value: &'a str,
    style: ScalarStyle,
    role: NodeRole,
    in_flow: bool,
    /// Carries a YAML core schema tag such as `!!str`, which sets the type explicitly.
    core_tagged: bool,
    span: Span,
}

impl QuotedStringsRule {
    /// Checks a single scalar event and appends diagnostics as needed.
    fn check_scalar(
        &self,
        check: &ScalarCheck<'_>,
        event: &ScalarEvent<'_>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let ScalarCheck {
            source,
            source_ctx,
            config,
        } = *check;
        let ScalarEvent {
            value,
            style,
            role,
            in_flow,
            core_tagged,
            span: scalar_span,
        } = *event;
        let options = &config.rules.quoted_strings.options;
        if core_tagged || (role == NodeRole::MappingKey && !options.check_keys) {
            return;
        }
        let severity = config
            .rules
            .quoted_strings
            .severity_or(self.default_severity());
        let mut report = |message: &str| {
            diagnostics.push(
                DiagnosticBuilder::new(self.code(), severity, message, scalar_span)
                    .build_with_context(source_ctx),
            );
        };

        match style {
            ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted => {
                if options.required == QuoteRequirement::OnlyWhenNeeded {
                    let has_escape = style == ScalarStyle::DoubleQuoted
                        && (Self::has_yaml_escape(value)
                            || Self::has_source_unicode_hex_escape(
                                source,
                                scalar_span.start.offset,
                            ));
                    let needed =
                        has_escape || Self::needs_quotes(value) || (in_flow && value.contains(','));
                    if !needed {
                        if options.quotes_redundant_for(value) {
                            report("string does not need quotes");
                        }
                        return;
                    }
                }

                match (options.quote_type, style) {
                    (QuoteType::Single, ScalarStyle::DoubleQuoted)
                        if !options.allows_quoted_quote(value, '\'') =>
                    {
                        report("string should use single quotes");
                    }
                    (QuoteType::Double, ScalarStyle::SingleQuoted)
                        if !options.allows_quoted_quote(value, '"') =>
                    {
                        report("string should use double quotes");
                    }
                    _ => {}
                }

                if options.required == QuoteRequirement::Never {
                    report("string should not be quoted");
                }
            }

            ScalarStyle::Plain
                if !is_scalar_literal(value)
                    && (options.required == QuoteRequirement::Always
                        || options.extra_required.is_match(value)) =>
            {
                report("string should be quoted");
            }

            // Literal and folded block scalars are intentional; skip.
            _ => {}
        }
    }

    /// Returns `true` if a double-quoted scalar's decoded value indicates it required escape
    /// sequences in the source.
    ///
    /// Saphyr provides the already-decoded scalar value. A double-quoted string that contained
    /// YAML escape sequences (`\n`, `\t`, `\r`, `\\`, `\"`, `\uXXXX`, etc.) will decode to
    /// a string that either contains a backslash (from `\\`) or contains control characters
    /// (from `\n`, `\t`, etc.). In either case, removing the quotes would produce a different
    /// value, so the quotes are necessary.
    fn has_yaml_escape(value: &str) -> bool {
        value.contains('\\')
            || value.chars().any(|c| {
                matches!(
                    c,
                    '\n' | '\r' | '\t' | '\x00' | '\x07' | '\x08' | '\x0C' | '\x0B' | '\x1B'
                )
            })
    }

    /// Returns `true` if the raw double-quoted scalar in `source` at byte offset `start`
    /// contains a `\u`, `\U`, or `\x` escape sequence.
    ///
    /// These escape sequences decode to Unicode/ASCII characters whose decoded form is
    /// indistinguishable from plain text, so `has_yaml_escape` (which operates on the decoded
    /// value) cannot detect them. We must inspect the raw source instead.
    fn has_source_unicode_hex_escape(source: &str, start: usize) -> bool {
        let Some(rest) = source
            .as_bytes()
            .get(start..)
            .and_then(|bytes| bytes.strip_prefix(b"\""))
        else {
            return false;
        };
        let mut bytes = rest.iter();
        while let Some(&b) = bytes.next() {
            match b {
                b'"' => return false,
                b'\\' => match bytes.next() {
                    Some(b'u' | b'U' | b'x') => return true,
                    Some(_) => {}
                    None => return false,
                },
                _ => {}
            }
        }
        false
    }

    /// Checks if a string needs quotes based on YAML syntax rules.
    fn needs_quotes(s: &str) -> bool {
        // Empty strings need quotes
        if s.is_empty() {
            return true;
        }

        if NON_STANDARD_BOOLS.contains(&s) || is_scalar_literal(s) {
            return true;
        }

        // Strings starting with special chars need quotes
        let first_char = s.chars().next().unwrap_or('\0');
        if matches!(
            first_char,
            '@' | '`'
                | '|'
                | '>'
                | '%'
                | '*'
                | '&'
                | '!'
                | '['
                | ']'
                | '{'
                | '}'
                | '#'
                | ':'
                | '-'
                | '?'
                | ','
        ) {
            return true;
        }

        // Strings with colons or hash signs need quotes
        if s.contains(':') || s.contains('#') {
            return true;
        }

        // Strings containing glob/template/cron special characters conventionally
        // benefit from quoting even when YAML could parse them unquoted. Flagging
        // them as "does not need quotes" produces false positives on real-world
        // YAML (GitHub Actions, Helm charts, Kubernetes manifests, cron schedules).
        if s.contains('*')
            || s.contains('?')
            || s.contains('{')
            || s.contains('}')
            || s.contains('[')
            || s.contains(']')
        {
            return true;
        }

        false
    }
}

/// Whether a plain `s` loads as anything but a string under the core schema.
fn is_scalar_literal(s: &str) -> bool {
    !matches!(
        resolve_scalar(s, fast_yaml_core::ScalarStyle::Plain, None),
        ResolvedScalar::Str(_)
    )
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
    fn test_quoted_strings_any_type() {
        let yaml = "name: 'John'\ncity: \"NYC\"";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Both quotes should be flagged as unnecessary in only-when-needed mode
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn test_quoted_strings_single_only() {
        let yaml = "name: \"John\"";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = config_with_rule(
            RuleName::QuotedStrings,
            "{quote-type: single, required: not-required}",
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_ne!(diagnostics, []);
        assert!(diagnostics[0].message.contains("single quotes"));
    }

    #[test]
    fn test_quoted_strings_double_only() {
        let yaml = "name: 'John'";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = config_with_rule(
            RuleName::QuotedStrings,
            "{quote-type: double, required: not-required}",
        );

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_ne!(diagnostics, []);
        assert!(diagnostics[0].message.contains("double quotes"));
    }

    #[test]
    fn test_quoted_strings_only_when_needed() {
        let yaml = "name: 'simple'";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_ne!(diagnostics, []);
        assert!(diagnostics[0].message.contains("does not need quotes"));
    }

    #[test]
    fn test_quoted_strings_needed_for_special_values() {
        let yaml = "value: 'true'\nnumber: '123'";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // These should not be flagged as they need quotes
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_quoted_strings_always() {
        let yaml = "name: John\nage: 30";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = config_with_rule(RuleName::QuotedStrings, "{required: always}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // "John" should be flagged (not age: 30, it's a number)
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("should be quoted"));
    }

    #[test]
    fn test_quoted_strings_never() {
        let yaml = "name: 'John'";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = config_with_rule(RuleName::QuotedStrings, "{required: never}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_ne!(diagnostics, []);
        assert!(diagnostics[0].message.contains("should not be quoted"));
    }

    #[test]
    fn test_quoted_strings_extra_required() {
        let yaml = "command: 'run-script'";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = config_with_rule(RuleName::QuotedStrings, "{extra-required: ['-']}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Should not flag as unnecessary because it contains '-'
        assert_eq!(diagnostics, []);
    }

    const HOSTS: &str = "[^http://, ^ftp://]";

    #[test]
    fn extra_required_flags_matching_plain_scalars_when_not_required() {
        let options = format!("{{required: false, extra-required: {HOSTS}}}");
        let ok = "- 123\n- \"123\"\n- localhost\n- \"localhost\"\n- \"http://localhost\"\n- \"ftp://localhost\"\n";
        assert_eq!(messages(ok, &options), [] as [String; 0]);
        let bad = "- http://localhost\n- ftp://localhost\n";
        assert_eq!(
            messages(bad, &options),
            ["string should be quoted", "string should be quoted"]
        );
    }

    #[test]
    fn extra_required_flags_matching_plain_scalars_when_only_needed() {
        let options = format!("{{extra-required: {HOSTS}}}");
        assert_eq!(
            messages("- http://localhost\n- localhost\n", &options),
            ["string should be quoted"]
        );
        assert_eq!(
            messages("- \"http://localhost\"\n", &options),
            [] as [String; 0]
        );
    }

    #[test]
    fn extra_allowed_keeps_plain_scalars_when_only_needed() {
        let options = format!("{{extra-allowed: {HOSTS}}}");
        let ok = "- 123\n- \"123\"\n- localhost\n- http://localhost\n- ftp://localhost\n- \"http://localhost\"\n";
        assert_eq!(messages(ok, &options), [] as [String; 0]);
        assert_eq!(
            messages("- \"localhost\"\n", &options),
            ["string does not need quotes"]
        );
    }

    #[test]
    fn comma_needs_quotes_only_inside_flow_collections() {
        assert_eq!(messages("e: [ \"a,b\" ]\n", "{}"), [] as [String; 0]);
        assert_eq!(messages("e: { k: 'a,b' }\n", "{}"), [] as [String; 0]);
        assert_eq!(messages("e: [ k: 'a,b' ]\n", "{}"), [] as [String; 0]);
        assert_eq!(
            messages("e: \"a,b\"\n", "{}"),
            ["string does not need quotes"]
        );
        assert_eq!(
            messages("e: [ \"a b\" ]\n", "{}"),
            ["string does not need quotes"]
        );
    }

    #[test]
    fn quote_type_is_still_checked_for_needed_quotes_in_flow() {
        assert_eq!(
            messages("e: [ \"a,b\" ]\n", "{quote-type: single}"),
            ["string should use single quotes"]
        );
    }

    #[test]
    fn plain_value_matching_both_extra_options_is_reported() {
        let options = "{extra-required: ['http'], extra-allowed: ['^b', 'both']}";
        assert_eq!(
            messages("- both-http\n- bare\n", options),
            ["string should be quoted"]
        );
    }

    #[test]
    fn extra_patterns_use_search_semantics() {
        let options = "{extra-required: ['b+c']}";
        assert_eq!(messages("- abbbcd\n", options), ["string should be quoted"]);
        assert_eq!(messages("- abd\n", options), Vec::<String>::new());
    }

    #[test]
    fn redundant_quotes_report_only_the_redundancy() {
        let options = "{quote-type: single}";
        assert_eq!(
            messages("a: \"John\"\n", options),
            ["string does not need quotes"]
        );
        assert_eq!(
            messages("a: \"x: y\"\n", options),
            ["string should use single quotes"]
        );
    }

    #[test]
    fn redundant_quotes_matching_extra_patterns_are_silent() {
        let options = "{quote-type: single, extra-required: ['^J']}";
        assert_eq!(messages("a: \"John\"\n", options), [] as [String; 0]);
    }

    #[test]
    fn extra_required_does_not_quote_keys_or_non_strings() {
        let options = "{extra-required: ['.']}";
        assert_eq!(
            messages("a.b: 1\nc: 1.5\nd: null\n", options),
            [] as [String; 0]
        );
    }

    #[test]
    fn option_conflicts_follow_yamllint() {
        for options in [
            "{required: always, extra-required: ['a']}",
            "{required: always, extra-allowed: ['a']}",
            "{required: false, extra-allowed: ['a']}",
            "{required: never, extra-required: ['a']}",
        ] {
            let mut rules = crate::config::RulesConfig::default();
            assert!(
                rules
                    .apply_rule(
                        RuleName::QuotedStrings,
                        serde_norway::Deserializer::from_str(options)
                    )
                    .is_err(),
                "{options}"
            );
        }
    }

    #[test]
    fn test_quoted_strings_with_colon() {
        let yaml = "url: 'http://example.com'";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Quotes are needed because of the colon
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_quoted_strings_needs_quotes() {
        assert!(QuotedStringsRule::needs_quotes(""));
        assert!(QuotedStringsRule::needs_quotes("true"));
        assert!(QuotedStringsRule::needs_quotes("123"));
        assert!(QuotedStringsRule::needs_quotes("http://example.com"));
        assert!(QuotedStringsRule::needs_quotes("#comment"));

        assert!(!QuotedStringsRule::needs_quotes("simple"));
        assert!(!QuotedStringsRule::needs_quotes("hello_world"));
    }

    // Regression tests for issue #175: false positive on double-quoted strings with escapes.

    #[test]
    fn test_no_false_positive_escape_newline() {
        // "\n" is a newline escape — removing quotes would produce a literal 'n', not a newline.
        let yaml = "message: \"line1\\nline2\"";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for double-quoted string with \\n escape, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_no_false_positive_escape_tab() {
        let yaml = "data: \"col1\\tcol2\"";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for double-quoted string with \\t escape, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_no_false_positive_escape_backslash() {
        let yaml = r#"path: "C:\\Users\\foo""#;
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for double-quoted string with \\\\ escape, got: {diagnostics:?}"
        );
    }

    // Regression tests for issue #113: false positives on quotes inside plain scalars.

    #[test]
    fn test_no_false_positive_double_quotes_in_plain_scalar() {
        // The value `echo "hello"` is a plain scalar; the " chars are literal content.
        let yaml = r#"run: echo "hello""#;
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for plain scalar with embedded double quotes, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_no_false_positive_single_quotes_in_plain_scalar() {
        // The value `${{ github.event_name == 'push' }}` is a plain scalar.
        let yaml = "if: ${{ github.event_name == 'push' }}";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for plain scalar with embedded single quotes, got: {diagnostics:?}"
        );
    }

    // Regression tests for issue #153: incorrect column and offset in diagnostics.

    #[test]
    fn test_diagnostic_location_value_after_key() {
        // `key: "unnecessary"` — the quoted value starts at column 6 (1-indexed), offset 5.
        let yaml = r#"key: "unnecessary""#;
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            !diagnostics.is_empty(),
            "expected at least one diagnostic for unnecessarily quoted value"
        );
        let span = diagnostics[0].span;
        assert_eq!(
            span.start.column, 6,
            "expected column 6 for quoted value, got {}",
            span.start.column
        );
        assert_eq!(
            span.start.offset, 5,
            "expected offset 5 for quoted value, got {}",
            span.start.offset
        );
    }

    #[test]
    fn test_diagnostic_location_value_at_line_start() {
        // A quoted value at the start of a sequence: `- "val"` — value starts at column 3, offset 2.
        // Use a plain sequence to get a quoted scalar at a known offset.
        let yaml = "- \"val\"";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = QuotedStringsRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            !diagnostics.is_empty(),
            "expected diagnostic for unnecessarily quoted sequence value"
        );
        let span = diagnostics[0].span;
        assert_eq!(
            span.start.column, 3,
            "expected column 3 for quoted value after '- ', got {}",
            span.start.column
        );
        assert_eq!(
            span.start.offset, 2,
            "expected offset 2 for quoted value after '- ', got {}",
            span.start.offset
        );
    }

    fn run(yaml: &str) -> Vec<Diagnostic> {
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        QuotedStringsRule.check(&LintContext::new(yaml), &value, &LintConfig::default())
    }

    // Regression tests for issue #308: non-ASCII text before a quoted scalar.

    #[test]
    fn test_non_ascii_key_does_not_hide_unicode_escape() {
        assert_eq!(run("—: \"\\u00e9\""), []);
        assert_eq!(run("ключ: \"\\x41\""), []);
    }

    #[test]
    fn test_non_ascii_key_quoted_value_span() {
        let yaml = "—: \"é\"";
        let diagnostics = run(yaml);
        assert_eq!(diagnostics.len(), 1);
        let span = diagnostics[0].span;
        assert_eq!((span.start.column, span.start.offset), (4, 5));
        assert_eq!((span.end.column, span.end.offset), (7, 9));
    }

    #[test]
    fn test_four_byte_key_quoted_value_span() {
        let yaml = "🎉: \"é\"";
        let diagnostics = run(yaml);
        assert_eq!(diagnostics.len(), 1);
        let span = diagnostics[0].span;
        assert_eq!((span.start.column, span.start.offset), (4, 6));
        assert_eq!((span.end.column, span.end.offset), (7, 10));
        assert_eq!(run("🎉: \"\\u00e9\""), []);
    }

    #[test]
    fn test_non_ascii_quoted_key_span() {
        let yaml = "\"ключ\": 1";
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        let config = config_with_rule(RuleName::QuotedStrings, "{check-keys: true}");
        let diagnostics = QuotedStringsRule.check(&LintContext::new(yaml), &value, &config);
        assert_eq!(diagnostics.len(), 1);
        let span = diagnostics[0].span;
        assert_eq!((span.start.column, span.start.offset), (1, 0));
        assert_eq!((span.end.column, span.end.offset), (7, 10));
    }

    #[test]
    fn test_multiline_quoted_span_ends_on_last_line() {
        let diagnostics = run("k: \"a\n  b\"\n");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].span.start.line, 1);
        assert_eq!(diagnostics[0].span.end.line, 2);
    }

    // Regression tests for issue #182: false positives on unicode/hex escape sequences.

    #[test]
    fn test_no_false_positive_unicode_escape_u4() {
        // "\u0041BC" decodes to "ABC" — without quotes it becomes literal "\u0041BC"
        let yaml = r#"key: "\u0041BC""#;
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        let rule = QuotedStringsRule;
        let config = LintConfig::default();
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for \\u escape, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_no_false_positive_unicode_escape_u8() {
        // "\U00000041BC" decodes to "ABC" — without quotes it becomes literal
        let yaml = r#"key: "\U00000041BC""#;
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        let rule = QuotedStringsRule;
        let config = LintConfig::default();
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for \\U escape, got: {diagnostics:?}"
        );
    }

    #[test]
    fn test_no_false_positive_hex_escape() {
        // "\x41BC" decodes to "ABC" — without quotes it becomes literal "\x41BC"
        let yaml = r#"key: "\x41BC""#;
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        let rule = QuotedStringsRule;
        let config = LintConfig::default();
        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for \\x escape, got: {diagnostics:?}"
        );
    }

    #[test]
    fn quotes_that_preserve_a_float_type_are_needed() {
        assert_eq!(run("a: \"+.inf\"\nb: \".5\"\nc: \"-.5e3\"\n"), []);
    }

    #[test]
    fn quotes_that_preserve_a_radix_big_int_type_are_needed() {
        assert_eq!(
            run("a: \"0xFFFFFFFFFFFFFFFFFF\"\nb: '0o7777777777777777777777'\n"),
            []
        );
    }

    #[test]
    fn plain_radix_big_int_needs_no_quotes_under_always() {
        assert_eq!(
            messages(
                "a: 0xFFFFFFFFFFFFFFFFFF\nb: 0o7777777777777777777777\n",
                "{required: always}"
            ),
            [] as [String; 0]
        );
    }

    fn messages(yaml: &str, options: &str) -> Vec<String> {
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        let config = config_with_rule(RuleName::QuotedStrings, options);
        QuotedStringsRule
            .check(&LintContext::new(yaml), &value, &config)
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    #[test]
    fn collection_key_keeps_value_role() {
        assert_eq!(messages("? {a: 1}\n: b\n", "{required: always}").len(), 1);
    }

    #[test]
    fn alias_value_keeps_next_key_role() {
        assert_eq!(
            messages("x: &v 1\ny: *v\nz: w\n", "{required: always}").len(),
            1
        );
    }

    #[test]
    fn non_float_strings_need_no_quotes() {
        assert_eq!(messages("a: '.5e'\nb: '.5'", "{}").len(), 1);
    }

    #[test]
    fn quoted_y_and_n_are_not_needed() {
        assert_eq!(
            messages("a: \"y\"\nb: 'n'\nc: \"Y\"\nd: \"N\"\n", "{}").len(),
            4
        );
        assert_eq!(messages("a: 'yes'\nb: 'No'\n", "{}"), [] as [String; 0]);
    }

    #[test]
    fn required_always_checks_root_scalar() {
        assert_eq!(messages("word\n", "{required: always}").len(), 1);
        assert_eq!(messages("12\n", "{required: always}"), [] as [String; 0]);
    }

    #[test]
    fn keys_are_skipped_unless_check_keys() {
        for yaml in ["'a': 1\n", "\"a\": 1\n", "a: 1\n"] {
            assert!(messages(yaml, "{}").is_empty(), "{yaml:?}");
            assert!(messages(yaml, "{required: always}").is_empty(), "{yaml:?}");
        }
    }

    #[test]
    fn check_keys_applies_the_rules_to_keys() {
        assert_eq!(messages("'a': 1\n", "{check-keys: true}").len(), 1);
        assert_eq!(
            messages("'a': 'b'\n", "{required: always, check-keys: true}"),
            [] as [String; 0]
        );
        assert_eq!(
            messages("key: b\n", "{required: always, check-keys: true}").len(),
            2
        );
    }

    #[test]
    fn core_tagged_scalars_are_skipped() {
        assert_eq!(
            messages("a: !!str 'x'\nb: !!str x\n", "{required: always}"),
            [] as [String; 0]
        );
        assert_eq!(
            messages("a: !!str \"x\"\n", "{quote-type: single}"),
            [] as [String; 0]
        );
        assert_eq!(messages("a: !local x\n", "{required: always}").len(), 1);
    }

    #[test]
    fn allow_quoted_quotes_accepts_the_other_style_for_configured_quote() {
        let single = "{quote-type: single, required: not-required}";
        assert_eq!(messages("a: \"it's\"\n", single).len(), 1);
        assert_eq!(
            messages(
                "a: \"it's\"\n",
                "{quote-type: single, required: not-required, allow-quoted-quotes: true}"
            ),
            [] as [String; 0]
        );
        assert_eq!(
            messages(
                "a: \"plain\"\n",
                "{quote-type: single, required: not-required, allow-quoted-quotes: true}"
            )
            .len(),
            1
        );
        let double = "{quote-type: double, required: not-required, allow-quoted-quotes: true}";
        assert_eq!(messages("a: 'say \"hi\"'\n", double), [] as [String; 0]);
        assert_eq!(messages("a: 'plain'\n", double).len(), 1);
    }
}
