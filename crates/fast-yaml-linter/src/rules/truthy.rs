//! Rule to check truthy value representations.

use super::RuleId;
use crate::config::RuleName;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

use crate::echo::{KEY_LIMIT, echo};

use super::node_roles::NodeRole;
use crate::config::RuleOptions;
use crate::nodes::{Node, TagKind};
use crate::scan::YamlVersion;
use crate::{Finding, LintConfig, LintContext, Severity};
use fast_yaml_core::ScalarStyle;

/// YAML 1.1-only boolean representations — not valid in YAML 1.2.2 Core Schema.
///
/// Like yamllint's list this leaves out the single letters `y`/`n`, which are plain strings
/// in both YAML 1.2 and `PyYAML`.
pub const NON_STANDARD_BOOLS: &[&str] = &[
    "yes", "no", "Yes", "No", "YES", "NO", "on", "off", "On", "Off", "ON", "OFF",
];

/// Valid YAML 1.2.2 booleans that are not in canonical form (`true`/`false`).
const NON_CANONICAL_BOOLS: &[&str] = &["True", "False", "TRUE", "FALSE"];

/// Linting rule for truthy values.
///
/// Validates boolean value representations to ensure consistent usage.
/// YAML 1.2 standardizes on `true` and `false`, but YAML 1.1 allowed
/// many alternatives (yes/no, on/off) which can cause confusion. The single letters `y` and
/// `n` are not reported, as in yamllint. A document preceded by a `%YAML 1.2` directive is read
/// as YAML 1.2, where only the spellings of `true`/`false` are booleans; the directive applies to
/// that document only.
///
/// Configuration options:
/// - `allowed-values`: list of allowed truthy representations (default: `["true", "false"]`)
/// - `check-keys`: whether to check keys too (default: false)
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::TruthyRule, rules::SourceRule, LintConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = TruthyRule;
/// let yaml = "enabled: true";
///
/// let config = LintConfig::default();
/// let context = fast_yaml_linter::LintContext::new(yaml);
/// let diagnostics = rule.diagnose(&context, &config);
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
    fn id(&self) -> RuleId<'_> {
        RuleId::BuiltIn(RuleName::Truthy)
    }

    fn name(&self) -> &'static str {
        "Truthy Values"
    }

    fn description(&self) -> &'static str {
        "Forbids non-standard truthy value representations (yes/no, on/off)"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }
}

impl super::SourceRule for TruthyRule {
    fn check(&self, context: &LintContext, config: &LintConfig) -> Vec<Finding> {
        let options = &config.rules.truthy.options;
        let allowed: Vec<&str> = options.allowed_values.iter().map(|v| v.as_str()).collect();
        let source_context = context.source_context();
        let index = context.nodes();

        let documents = context.documents();
        let mut current = 0;

        let mut diagnostics = Vec::new();
        for node in index.nodes() {
            let Node::Scalar(scalar) = node else {
                continue;
            };
            let at = scalar.range.start().get();
            while documents
                .get(current + 1)
                .is_some_and(|next| next.start.offset() <= at)
            {
                current += 1;
            }
            let schema = BooleanSchema::of(documents.get(current).and_then(|d| d.yaml_version));
            let slot = match scalar.role {
                NodeRole::MappingKey if options.check_keys => Slot::Key,
                NodeRole::MappingValue | NodeRole::SequenceItem | NodeRole::Root => Slot::Value,
                NodeRole::MappingKey => continue,
            };
            if scalar.style != ScalarStyle::Plain || scalar.tag != TagKind::None {
                continue;
            }
            if let Some(msg) = message(slot, schema, index.text(scalar), &allowed) {
                let span = source_context.span_of_bytes(scalar.range);
                diagnostics.push(Finding::new(msg, span));
            }
        }

        diagnostics
    }
}

/// The boolean spellings a document is read with.
#[derive(Clone, Copy, PartialEq, Eq)]
enum BooleanSchema {
    /// No `%YAML 1.2` directive: yamllint's default, with `yes`/`no`/`on`/`off` as booleans.
    Yaml11,
    /// A `%YAML 1.2` directive: only `true`/`false` spellings are booleans.
    Yaml12,
}

impl BooleanSchema {
    fn of(version: Option<YamlVersion>) -> Self {
        if version == Some(YamlVersion::V1_2) {
            Self::Yaml12
        } else {
            Self::Yaml11
        }
    }
}

/// Whether a scalar is a mapping key or a value.
#[derive(Clone, Copy)]
enum Slot {
    Key,
    Value,
}

/// Builds the diagnostic message for a truthy spelling that is not allowed.
///
/// Every spelling of `true`/`false` outside `allowed` is reported, as yamllint does; with
/// `allowed-values: [yes]` even `true` is a finding.
fn message(slot: Slot, schema: BooleanSchema, text: &str, allowed: &[&str]) -> Option<String> {
    if allowed.contains(&text)
        || (schema == BooleanSchema::Yaml12 && NON_STANDARD_BOOLS.contains(&text))
    {
        return None;
    }
    let noun = match slot {
        Slot::Key => "key",
        Slot::Value => "value",
    };
    let hint = |quote: &str| {
        if allowed.is_empty() {
            "no truthy value is allowed".to_owned()
        } else {
            let spellings: Vec<String> = allowed
                .iter()
                .map(|a| format!("{quote}{a}{quote}"))
                .collect();
            format!("use {}", spellings.join(" or "))
        }
    };
    if NON_STANDARD_BOOLS.contains(&text) {
        Some(format!(
            "found non-standard truthy {noun} '{text}' ({})",
            hint("")
        ))
    } else if NON_CANONICAL_BOOLS.contains(&text) {
        let hint = hint("'");
        Some(match slot {
            Slot::Key => format!("found non-canonical boolean key '{text}', {hint}"),
            Slot::Value => format!("found non-canonical boolean '{text}', {hint}"),
        })
    } else if TruthySpelling::CANONICAL.contains(&text) {
        Some(format!(
            "found truthy {noun} '{text}' that is not allowed ({})",
            hint("")
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{RuleName, test_support::config_with_rule},
        rules::SourceRule,
    };

    #[test]
    fn test_truthy_standard_values() {
        let yaml = "enabled: true\ndisabled: false";

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_truthy_yes_no() {
        let yaml = "enabled: yes\ndisabled: no";

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("yes"));
        assert!(diagnostics[1].message.contains("no"));
    }

    #[test]
    fn test_truthy_on_off() {
        let yaml = "enabled: on\ndisabled: off";

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("on"));
        assert!(diagnostics[1].message.contains("off"));
    }

    #[test]
    fn test_truthy_capitalized() {
        let yaml = "enabled: True\ndisabled: FALSE";

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
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

        let rule = TruthyRule;
        let config = LintConfig::default();

        let diags_nonstandard = rule.diagnose(&LintContext::new(yaml_nonstandard), &config);
        let diags_noncanonical = rule.diagnose(&LintContext::new(yaml_noncanonical), &config);

        assert!(diags_nonstandard[0].message.contains("non-standard"));
        assert!(!diags_nonstandard[0].message.contains("non-canonical"));
        assert!(diags_noncanonical[0].message.contains("non-canonical"));
        assert!(!diags_noncanonical[0].message.contains("non-standard"));
    }

    #[test]
    fn test_truthy_quoted_allowed() {
        let yaml = "enabled: 'yes'\ndisabled: \"no\"";

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_truthy_reports_true_when_only_yes_is_allowed() {
        let yaml = "a: true\nb: yes\nc: False\n";
        let config = config_with_rule(RuleName::Truthy, "{allowed-values: [yes]}");
        let diagnostics = TruthyRule.diagnose(&LintContext::new(yaml), &config);
        let messages: Vec<&str> = diagnostics.iter().map(|d| &*d.message).collect();
        assert_eq!(
            messages,
            [
                "found truthy value 'true' that is not allowed (use yes)",
                "found non-canonical boolean 'False', use 'yes'"
            ]
        );
    }

    #[test]
    fn test_truthy_without_allowed_values_reports_every_spelling() {
        let config = config_with_rule(RuleName::Truthy, "{allowed-values: []}");
        let diagnostics = TruthyRule.diagnose(&LintContext::new("a: true\n"), &config);
        assert_eq!(
            diagnostics[0].message,
            "found truthy value 'true' that is not allowed (no truthy value is allowed)"
        );
    }

    #[test]
    fn test_truthy_custom_allowed_values() {
        let yaml = "enabled: yes\ndisabled: no";

        let rule = TruthyRule;
        let config = config_with_rule(RuleName::Truthy, "{allowed-values: [yes, no]}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_truthy_list_items() {
        let yaml = "items:\n  - yes\n  - no\n  - true";

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("yes"));
        assert!(diagnostics[1].message.contains("no"));
    }

    #[test]
    fn test_truthy_check_keys() {
        let yaml = "yes: value\nno: other";

        let rule = TruthyRule;
        let config = config_with_rule(RuleName::Truthy, "{check-keys: true}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("key"));
        assert!(diagnostics[1].message.contains("key"));
    }

    #[test]
    fn test_truthy_ignore_keys_by_default() {
        let yaml = "yes: value";

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics, []);
    }

    #[test]
    fn test_truthy_with_comment() {
        let yaml = "enabled: yes  # This is a comment";

        let rule = TruthyRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.diagnose(&context, &config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("yes"));
    }

    #[test]
    fn test_truthy_single_letters_are_not_reported() {
        let config = config_with_rule(RuleName::Truthy, "{check-keys: true}");
        for yaml in ["enabled: y\ndisabled: n\n", "a: Y\nb: N\n", "y: 1\nn: 2\n"] {
            assert!(
                truthy_spans(yaml, &LintConfig::default()).is_empty(),
                "{yaml:?}"
            );
            assert!(truthy_spans(yaml, &config).is_empty(), "{yaml:?}");
        }
    }

    #[test]
    fn test_truthy_single_letter_is_not_an_allowed_value() {
        for letter in ["y", "n", "Y", "N"] {
            let err = TruthySpelling::new(letter).unwrap_err();
            assert_eq!(err.input, letter);
        }
    }

    fn truthy_spans(yaml: &str, config: &LintConfig) -> Vec<(usize, usize, usize)> {
        TruthyRule
            .diagnose(&LintContext::new(yaml), config)
            .iter()
            .map(|d| {
                (
                    d.span.start.line(),
                    d.span.start.column(),
                    d.span.end.column(),
                )
            })
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
        assert_eq!(
            truthy_spans("a: yes and more\nb: no thanks\n", &LintConfig::default()),
            []
        );
        assert_eq!(truthy_spans("a: !!str yes\n", &LintConfig::default()), []);
    }

    #[test]
    fn test_truthy_key_in_quotes_is_not_a_bool_key() {
        let config = config_with_rule(RuleName::Truthy, "{check-keys: true}");
        assert_eq!(truthy_spans("\"yes\": 1\n", &config), []);
        assert_eq!(truthy_spans("\"a:b\": 1\nyes: 2\n", &config), [(2, 1, 4)]);
    }

    #[test]
    fn test_truthy_flow_pair_in_flow_sequence_is_checked() {
        let config = config_with_rule(RuleName::Truthy, "{check-keys: true}");
        assert_eq!(
            truthy_spans("k: [a: yes]\nm: [ {x: 1}, on: no ]\n", &config),
            [(1, 8, 11), (2, 14, 16), (2, 18, 20)]
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
        assert_eq!(truthy_spans("\"yes\"\n", &LintConfig::default()), []);
        assert_eq!(truthy_spans("!!str yes\n", &LintConfig::default()), []);
    }
}
