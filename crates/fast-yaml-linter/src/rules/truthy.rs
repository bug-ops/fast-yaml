//! Rule to check truthy value representations.

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

use crate::echo::{KEY_LIMIT, echo};

use crate::config::RuleOptions;
use crate::{Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity};
use fast_yaml_core::Value;
use std::collections::HashSet;

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

    #[allow(clippy::too_many_lines)]
    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let options = &config.rules.truthy.options;
        let allowed_values: Vec<&str> = options.allowed_values.iter().map(|v| v.as_str()).collect();
        let check_keys = options.check_keys;

        // Pre-build HashSets for O(1) lookup
        let non_standard_set: HashSet<&str> = NON_STANDARD_BOOLS.iter().copied().collect();
        let non_canonical_set: HashSet<&str> = NON_CANONICAL_BOOLS.iter().copied().collect();
        let allowed_set: HashSet<&str> = allowed_values.iter().copied().collect();

        let mut diagnostics = Vec::new();

        // Use cached lines and metadata from context
        let lines = context.lines();
        let line_metadata = context.line_metadata();

        for (line_idx, (line, metadata)) in lines.iter().zip(line_metadata).enumerate() {
            let line_num = line_idx + 1;
            let line_start = context.source_context().line_start(line_num);

            // Skip comment lines using cached metadata
            if metadata.is_comment {
                continue;
            }

            // Find key-value pairs (after ':')
            if let Some((key_part, value_part)) = line.split_once(':') {
                let colon_pos = key_part.len();

                // Check key if configured
                if check_keys {
                    let key_trimmed = key_part.trim();
                    let key_msg = if non_standard_set.contains(key_trimmed)
                        && !allowed_set.contains(key_trimmed)
                    {
                        Some(format!(
                            "found non-standard truthy key '{key_trimmed}' (use {})",
                            allowed_values.join(" or ")
                        ))
                    } else if non_canonical_set.contains(key_trimmed)
                        && !allowed_set.contains(key_trimmed)
                    {
                        Some(format!(
                            "found non-canonical boolean key '{key_trimmed}', use 'true' or 'false'"
                        ))
                    } else {
                        None
                    };
                    if let Some(msg) = key_msg {
                        let key_start = key_part.len() - key_part.trim_start().len();
                        let severity = config.rules.truthy.severity_or(self.default_severity());
                        let span = context
                            .source_context()
                            .span_at(line_start.add_bytes(key_start), key_trimmed.len());
                        diagnostics.push(
                            DiagnosticBuilder::new(self.code(), severity, msg, span)
                                .build_with_context(context.source_context()),
                        );
                    }
                }

                // Check value
                let value_trimmed = value_part.trim();

                // Skip if empty, quoted, or starts with flow collection markers
                if value_trimmed.is_empty()
                    || value_trimmed.starts_with('"')
                    || value_trimmed.starts_with('\'')
                    || value_trimmed.starts_with('[')
                    || value_trimmed.starts_with('{')
                {
                    continue;
                }

                // Extract the value token (before any comment or space)
                let value_token = value_trimmed
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.split('#').next())
                    .unwrap_or(value_trimmed);

                let val_msg = if non_standard_set.contains(value_token)
                    && !allowed_set.contains(value_token)
                {
                    Some(format!(
                        "found non-standard truthy value '{value_token}' (use {})",
                        allowed_values.join(" or ")
                    ))
                } else if non_canonical_set.contains(value_token)
                    && !allowed_set.contains(value_token)
                {
                    Some(format!(
                        "found non-canonical boolean '{value_token}', use 'true' or 'false'"
                    ))
                } else {
                    None
                };
                if let Some(msg) = val_msg {
                    let value_start =
                        colon_pos + 1 + value_part.len() - value_part.trim_start().len();
                    let severity = config.rules.truthy.severity_or(self.default_severity());
                    let span = context
                        .source_context()
                        .span_at(line_start.add_bytes(value_start), value_token.len());
                    diagnostics.push(
                        DiagnosticBuilder::new(self.code(), severity, msg, span)
                            .build_with_context(context.source_context()),
                    );
                }
            }

            // Check list items (after '- ')
            if let Some((before_hyphen, after_hyphen)) = line.split_once('-') {
                let hyphen_pos = before_hyphen.len();
                let value_trimmed = after_hyphen.trim();

                // Skip if this is a mapping key (contains ':')
                if value_trimmed.contains(':') {
                    continue;
                }

                // Skip if empty, quoted, or starts with flow collection markers
                if value_trimmed.is_empty()
                    || value_trimmed.starts_with('"')
                    || value_trimmed.starts_with('\'')
                    || value_trimmed.starts_with('[')
                    || value_trimmed.starts_with('{')
                {
                    continue;
                }

                let value_token = value_trimmed
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.split('#').next())
                    .unwrap_or(value_trimmed);

                let list_msg = if non_standard_set.contains(value_token)
                    && !allowed_set.contains(value_token)
                {
                    Some(format!(
                        "found non-standard truthy value '{value_token}' (use {})",
                        allowed_values.join(" or ")
                    ))
                } else if non_canonical_set.contains(value_token)
                    && !allowed_set.contains(value_token)
                {
                    Some(format!(
                        "found non-canonical boolean '{value_token}', use 'true' or 'false'"
                    ))
                } else {
                    None
                };
                if let Some(msg) = list_msg {
                    let value_start =
                        hyphen_pos + 1 + after_hyphen.len() - after_hyphen.trim_start().len();
                    let severity = config.rules.truthy.severity_or(self.default_severity());
                    let span = context
                        .source_context()
                        .span_at(line_start.add_bytes(value_start), value_token.len());
                    diagnostics.push(
                        DiagnosticBuilder::new(self.code(), severity, msg, span)
                            .build_with_context(context.source_context()),
                    );
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
}
