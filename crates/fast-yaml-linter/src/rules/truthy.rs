//! Rule to check truthy value representations.

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

use crate::echo::{KEY_LIMIT, echo};

use super::node_roles::{NodeRole, RoleTracker};
use crate::config::RuleOptions;
use crate::{Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity};
use fast_yaml_core::Value;
use saphyr_parser::{Event, Parser as SaphyrParser, ScalarStyle};

/// YAML 1.1-only boolean representations — not valid in YAML 1.2.2 Core Schema.
pub const NON_STANDARD_BOOLS: &[&str] = &[
    "yes", "no", "Yes", "No", "YES", "NO", "on", "off", "On", "Off", "ON", "OFF", "y", "n", "Y",
    "N",
];

/// Valid YAML 1.2.2 booleans that are not in canonical form (`true`/`false`).
const NON_CANONICAL_BOOLS: &[&str] = &["True", "False", "TRUE", "FALSE"];

/// Linting rule for truthy values.
///
/// Validates boolean value representations to ensure consistent usage.
/// YAML 1.2 standardizes on `true` and `false`, but YAML 1.1 allowed
/// many alternatives (yes/no, on/off, y/n, etc.) which can cause confusion.
///
/// Configuration options:
/// - `allowed-values`: list of allowed truthy representations (default: `["true", "false"]`)
/// - `check-keys`: whether to check keys too (default: false)
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::TruthyRule, rules::LintRule, LintConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = TruthyRule;
/// let yaml = "enabled: true";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::default();
/// let context = fast_yaml_linter::LintContext::new(yaml);
/// let diagnostics = rule.check(&context, &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct TruthyRule;

/// A truthy spelling accepted by the `allowed-values` option.
///
/// Validated against the spellings the rule knows about. Spellings must be quoted strings; an
/// unquoted YAML boolean is rejected because `True` and `true` would otherwise be conflated.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::rules::TruthySpelling;
///
/// assert_eq!(TruthySpelling::new("yes").unwrap().as_str(), "yes");
/// assert!(TruthySpelling::new("maybe").is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TruthySpelling(&'static str);

/// Error returned for a spelling the truthy rule does not know.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
#[error(
    "unknown truthy spelling '{}', expected one of: {}",
    echo(.input, KEY_LIMIT),
    TruthySpelling::known()
)]
pub struct UnknownTruthySpelling {
    /// The rejected spelling.
    pub input: String,
}

impl TruthySpelling {
    const CANONICAL: [&'static str; 2] = ["true", "false"];

    fn all() -> impl Iterator<Item = &'static str> {
        Self::CANONICAL
            .into_iter()
            .chain(NON_CANONICAL_BOOLS.iter().copied())
            .chain(NON_STANDARD_BOOLS.iter().copied())
    }

    fn known() -> String {
        Self::all().collect::<Vec<_>>().join(", ")
    }

    /// Validates a spelling.
    ///
    /// # Errors
    ///
    /// Returns an error when `spelling` is not one of the known truthy spellings.
    pub fn new(spelling: &str) -> Result<Self, UnknownTruthySpelling> {
        Self::all()
            .find(|known| *known == spelling)
            .map(Self)
            .ok_or_else(|| UnknownTruthySpelling {
                input: spelling.to_owned(),
            })
    }

    /// Returns the spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

struct SpellingVisitor;

impl Visitor<'_> for SpellingVisitor {
    type Value = TruthySpelling;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a truthy spelling such as 'true', 'false', 'yes' or 'off'")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
        let lower = value.to_string();
        let title = if value { "True" } else { "False" };
        let upper = lower.to_uppercase();
        Err(E::custom(format!(
            "unquoted boolean is ambiguous (YAML 1.1 reads it as a spelling, 1.2 as a boolean); quote the spelling you mean, e.g. '{lower}', '{title}' or '{upper}'"
        )))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        TruthySpelling::new(value).map_err(|_| {
            E::custom(format!(
                "unknown truthy spelling '{}', expected one of: {}",
                echo(value, KEY_LIMIT),
                TruthySpelling::known()
            ))
        })
    }
}

impl Serialize for TruthySpelling {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0)
    }
}

impl<'de> Deserialize<'de> for TruthySpelling {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(SpellingVisitor)
    }
}

/// Options of the truthy rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct TruthyOptions {
    /// Spellings that are not reported.
    pub allowed_values: Vec<TruthySpelling>,
    /// Also check mapping keys.
    pub check_keys: bool,
}

impl Default for TruthyOptions {
    fn default() -> Self {
        Self {
            allowed_values: TruthySpelling::CANONICAL.map(TruthySpelling).to_vec(),
            check_keys: false,
        }
    }
}

impl RuleOptions for TruthyOptions {}

impl super::LintRule for TruthyRule {
    fn code(&self) -> &str {
        DiagnosticCode::TRUTHY
    }

    fn name(&self) -> &'static str {
        "Truthy Values"
    }

    fn description(&self) -> &'static str {
        "Forbids non-standard truthy value representations (yes/no, on/off, y/n, etc.)"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let options = &config.rules.truthy.options;
        let allowed: Vec<&str> = options.allowed_values.iter().map(|v| v.as_str()).collect();
        let severity = config.rules.truthy.severity_or(self.default_severity());
        let source_context = context.source_context();

        let mut diagnostics = Vec::new();
        let mut roles = RoleTracker::default();
        let mut parser = SaphyrParser::new_from_str(context.source());

        while let Some(Ok((event, span))) = parser.next_event() {
            match event {
                Event::Scalar(text, style, _, tag) => {
                    let slot = match roles.node() {
                        NodeRole::MappingKey if options.check_keys => Slot::Key,
                        NodeRole::MappingValue | NodeRole::SequenceItem | NodeRole::Root => {
                            Slot::Value
                        }
                        NodeRole::MappingKey => continue,
                    };
                    if style != ScalarStyle::Plain || tag.is_some() {
                        continue;
                    }
                    if let Some(msg) = message(slot, &text, &allowed) {
                        let span = source_context.span_of_bytes(source_context.byte_range_of(span));
                        diagnostics.push(
                            DiagnosticBuilder::new(self.code(), severity, msg, span)
                                .build_with_context(source_context),
                        );
                    }
                }
                Event::MappingStart(..) => {
                    roles.start_mapping(context.source(), source_context.byte_range_of(span));
                }
                Event::SequenceStart(..) => {
                    roles.start_sequence(context.source(), source_context.byte_range_of(span));
                }
                Event::MappingEnd | Event::SequenceEnd => roles.leave(),
                Event::Alias(..) => {
                    roles.node();
                }
                _ => {}
            }
        }

        diagnostics
    }
}

/// Whether a scalar is a mapping key or a value.
#[derive(Clone, Copy)]
enum Slot {
    Key,
    Value,
}

/// Builds the diagnostic message for a truthy spelling that is not allowed.
fn message(slot: Slot, text: &str, allowed: &[&str]) -> Option<String> {
    if allowed.contains(&text) {
        return None;
    }
    let noun = match slot {
        Slot::Key => "key",
        Slot::Value => "value",
    };
    if NON_STANDARD_BOOLS.contains(&text) {
        Some(format!(
            "found non-standard truthy {noun} '{text}' (use {})",
            allowed.join(" or ")
        ))
    } else if NON_CANONICAL_BOOLS.contains(&text) {
        Some(match slot {
            Slot::Key => format!("found non-canonical boolean key '{text}', use 'true' or 'false'"),
            Slot::Value => format!("found non-canonical boolean '{text}', use 'true' or 'false'"),
        })
    } else {
        None
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
    fn test_truthy_standard_values() {
        let yaml = "enabled: true\ndisabled: false";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_truthy_yes_no() {
        let yaml = "enabled: yes\ndisabled: no";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("yes"));
        assert!(diagnostics[1].message.contains("no"));
    }

    #[test]
    fn test_truthy_on_off() {
        let yaml = "enabled: on\ndisabled: off";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("on"));
        assert!(diagnostics[1].message.contains("off"));
    }

    #[test]
    fn test_truthy_capitalized() {
        let yaml = "enabled: True\ndisabled: FALSE";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 2);
        // Must use "non-canonical" message, not "non-standard", since True/FALSE are valid YAML 1.2.2
        assert!(diagnostics[0].message.contains("non-canonical"));
        assert!(diagnostics[1].message.contains("non-canonical"));
    }

    #[test]
    fn test_truthy_capitalized_vs_nonstandard_message() {
        // yes/no → "non-standard"; True/FALSE → "non-canonical"
        let yaml_nonstandard = "enabled: yes";
        let yaml_noncanonical = "enabled: True";
        let value_nonstandard = Parser::parse_str(yaml_nonstandard).unwrap().unwrap();
        let value_noncanonical = Parser::parse_str(yaml_noncanonical).unwrap().unwrap();

        let rule = TruthyRule;
        let config = LintConfig::default();

        let diags_nonstandard = rule.check(
            &LintContext::new(yaml_nonstandard),
            &value_nonstandard,
            &config,
        );
        let diags_noncanonical = rule.check(
            &LintContext::new(yaml_noncanonical),
            &value_noncanonical,
            &config,
        );

        assert!(diags_nonstandard[0].message.contains("non-standard"));
        assert!(!diags_nonstandard[0].message.contains("non-canonical"));
        assert!(diags_noncanonical[0].message.contains("non-canonical"));
        assert!(!diags_noncanonical[0].message.contains("non-standard"));
    }

    #[test]
    fn test_truthy_quoted_allowed() {
        let yaml = "enabled: 'yes'\ndisabled: \"no\"";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_truthy_custom_allowed_values() {
        let yaml = "enabled: yes\ndisabled: no";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = config_with_rule(RuleName::Truthy, "{allowed-values: [yes, no]}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_truthy_list_items() {
        let yaml = "items:\n  - yes\n  - no\n  - true";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("yes"));
        assert!(diagnostics[1].message.contains("no"));
    }

    #[test]
    fn test_truthy_check_keys() {
        let yaml = "yes: value\nno: other";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = config_with_rule(RuleName::Truthy, "{check-keys: true}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("key"));
        assert!(diagnostics[1].message.contains("key"));
    }

    #[test]
    fn test_truthy_ignore_keys_by_default() {
        let yaml = "yes: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_truthy_with_comment() {
        let yaml = "enabled: yes  # This is a comment";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("yes"));
    }

    #[test]
    fn test_truthy_single_letter() {
        let yaml = "enabled: y\ndisabled: n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert_eq!(diagnostics.len(), 2);
    }

    fn truthy_spans(yaml: &str, config: &LintConfig) -> Vec<(usize, usize, usize)> {
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        TruthyRule
            .check(&LintContext::new(yaml), &value, config)
            .iter()
            .map(|d| (d.span.start.line, d.span.start.column, d.span.end.column))
            .collect()
    }

    #[test]
    fn test_truthy_quoted_key_containing_colon() {
        assert_eq!(
            truthy_spans("\"a:b\": yes\n", &LintConfig::default()),
            [(1, 8, 11)]
        );
        assert_eq!(
            truthy_spans("'x: y': no\n", &LintConfig::default()),
            [(1, 9, 11)]
        );
    }

    #[test]
    fn test_truthy_only_whole_scalar_is_reported() {
        assert!(truthy_spans("a: yes and more\nb: no thanks\n", &LintConfig::default()).is_empty());
        assert!(truthy_spans("a: !!str yes\n", &LintConfig::default()).is_empty());
    }

    #[test]
    fn test_truthy_key_in_quotes_is_not_a_bool_key() {
        let config = config_with_rule(RuleName::Truthy, "{check-keys: true}");
        assert!(truthy_spans("\"yes\": 1\n", &config).is_empty());
        assert_eq!(truthy_spans("\"a:b\": 1\nyes: 2\n", &config), [(2, 1, 4)]);
    }

    #[test]
    fn test_truthy_flow_pair_in_flow_sequence_is_checked() {
        let config = config_with_rule(RuleName::Truthy, "{check-keys: true}");
        assert_eq!(
            truthy_spans("k: [a: yes]\nm: [ {x: 1}, y: no ]\n", &config),
            [(1, 8, 11), (2, 14, 15), (2, 17, 19)]
        );
    }

    #[test]
    fn test_truthy_after_document_start() {
        assert_eq!(
            truthy_spans("---\na: yes\n", &LintConfig::default()),
            [(2, 4, 7)]
        );
    }

    #[test]
    fn test_truthy_flow_collections_are_checked() {
        assert_eq!(
            truthy_spans("a: [yes, no]\nb: {c: yes}\n", &LintConfig::default()),
            [(1, 5, 8), (1, 10, 12), (2, 8, 11)]
        );
    }

    #[test]
    fn test_truthy_root_scalar_and_flow_root() {
        assert_eq!(truthy_spans("yes\n", &LintConfig::default()), [(1, 1, 4)]);
        assert_eq!(truthy_spans("[yes]\n", &LintConfig::default()), [(1, 2, 5)]);
    }

    #[test]
    fn test_truthy_multi_document_root() {
        assert_eq!(
            truthy_spans("--- yes\n--- no\n", &LintConfig::default()),
            [(1, 5, 8), (2, 5, 7)]
        );
    }

    #[test]
    fn test_truthy_quoted_or_tagged_root_is_skipped() {
        assert!(truthy_spans("\"yes\"\n", &LintConfig::default()).is_empty());
        assert!(truthy_spans("!!str yes\n", &LintConfig::default()).is_empty());
    }
}
