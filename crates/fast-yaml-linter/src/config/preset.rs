//! Built-in presets mirroring yamllint's `default` and `relaxed` configurations.

use std::fmt;
use std::str::FromStr;

use crate::Severity;
use crate::config::{Limit, MarkerPresence, NoOptions, RuleSettings, RulesConfig};
use crate::echo::{KEY_LIMIT, echo};
use crate::rules::{DocumentEndOptions, DocumentStartOptions, QuoteRequirement};
use crate::rules::{FloatValuesOptions, QuotedStringsOptions, TruthyOptions};

/// Error returned when a string is not a preset name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
#[error("unknown preset '{}', expected 'default' or 'relaxed'", echo(.name, KEY_LIMIT))]
pub struct UnknownPresetError {
    /// The rejected preset name.
    pub name: String,
}

/// A yamllint configuration that `extends` can start from.
///
/// Every preset spells out all yamllint rules, enabled or not, with yamllint's option
/// defaults, so a rule that a preset disables behaves like yamllint once it is re-enabled.
/// This is not full yamllint parity. Options that fast-yaml does not implement are left out, and
/// rules that depend on them behave differently from yamllint:
///
/// - `indentation.spaces` stays at the fast-yaml default; `indent-sequences` and
///   `check-multi-line-strings` are missing;
/// - `line-length.allow-non-breakable-words` (yamllint default true) and
///   `allow-non-breakable-inline-mappings` (true in `relaxed`);
/// - `key-ordering.ignored-keys`;
/// - `quoted-strings.allow-quoted-quotes` and `check-keys`;
/// - `key-duplicates.forbid-duplicated-merge-keys` and `anchors.forbid-*`.
///
/// Rules that only fast-yaml has (`lint-directive`) keep their fast-yaml defaults.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::Severity;
/// use fast_yaml_linter::config::Preset;
///
/// let default = "default".parse::<Preset>().unwrap().rules();
/// assert!(default.document_start.enabled);
/// assert!(!default.quoted_strings.enabled);
///
/// let relaxed = Preset::Relaxed.rules();
/// assert!(!relaxed.document_start.enabled);
/// assert_eq!(relaxed.colons.severity, Some(Severity::Warning));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// yamllint's `default` configuration.
    Default,
    /// yamllint's `relaxed` configuration.
    Relaxed,
}

impl Preset {
    /// Returns the preset name used in `extends`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Relaxed => "relaxed",
        }
    }

    /// Returns the rule settings of the preset.
    #[must_use]
    pub fn rules(self) -> RulesConfig {
        match self {
            Self::Default => default_rules(),
            Self::Relaxed => relaxed_rules(),
        }
    }
}

impl fmt::Display for Preset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Preset {
    type Err = UnknownPresetError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "default" => Ok(Self::Default),
            "relaxed" => Ok(Self::Relaxed),
            _ => Err(UnknownPresetError { name: s.to_owned() }),
        }
    }
}

const fn on<O>(severity: Severity, options: O) -> RuleSettings<O> {
    RuleSettings {
        enabled: true,
        severity: Some(severity),
        options,
    }
}

fn error<O: Default>() -> RuleSettings<O> {
    on(Severity::Error, O::default())
}

fn warning<O: Default>() -> RuleSettings<O> {
    on(Severity::Warning, O::default())
}

/// A rule the preset disables keeps yamllint's `error` level for when it is re-enabled.
fn off_with<O>(options: O) -> RuleSettings<O> {
    RuleSettings {
        enabled: false,
        ..on(Severity::Error, options)
    }
}

fn off<O: Default>() -> RuleSettings<O> {
    off_with(O::default())
}

const fn disable<O>(settings: &mut RuleSettings<O>) {
    settings.enabled = false;
    settings.severity = Some(Severity::Error);
}

fn default_rules() -> RulesConfig {
    RulesConfig {
        duplicate_key: error(),
        line_length: error(),
        trailing_whitespace: error(),
        document_start: on(
            Severity::Warning,
            DocumentStartOptions {
                present: MarkerPresence::Required,
            },
        ),
        document_end: off_with(DocumentEndOptions {
            present: MarkerPresence::Required,
        }),
        empty_values: off(),
        new_line_at_end_of_file: error(),
        braces: error(),
        brackets: error(),
        colons: error(),
        commas: error(),
        hyphens: error(),
        comments: warning(),
        comments_indentation: warning(),
        empty_lines: error(),
        new_lines: error(),
        octal_values: off(),
        truthy: on(
            Severity::Warning,
            TruthyOptions {
                check_keys: true,
                ..TruthyOptions::default()
            },
        ),
        quoted_strings: off_with(QuotedStringsOptions {
            required: QuoteRequirement::Always,
            ..QuotedStringsOptions::default()
        }),
        key_ordering: off(),
        float_values: off_with(FloatValuesOptions {
            require_numeral_before_decimal: false,
            ..FloatValuesOptions::default()
        }),
        invalid_anchor: error(),
        indentation: error(),
        set_values: error(),
        lint_directive: RuleSettings::<NoOptions>::default(),
    }
}

fn relaxed_rules() -> RulesConfig {
    let mut rules = default_rules();
    for spaces in [&mut rules.braces, &mut rules.brackets] {
        spaces.severity = Some(Severity::Warning);
        spaces.options.max_spaces_inside = Limit::Max(1);
    }
    rules.colons.severity = Some(Severity::Warning);
    rules.commas.severity = Some(Severity::Warning);
    rules.empty_lines.severity = Some(Severity::Warning);
    rules.hyphens.severity = Some(Severity::Warning);
    rules.indentation.severity = Some(Severity::Warning);
    rules.line_length.severity = Some(Severity::Warning);
    disable(&mut rules.comments);
    disable(&mut rules.comments_indentation);
    disable(&mut rules.document_start);
    disable(&mut rules.truthy);
    rules
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RuleName;

    const DISABLED_BY_DEFAULT: [RuleName; 6] = [
        RuleName::DocumentEnd,
        RuleName::EmptyValues,
        RuleName::FloatValues,
        RuleName::KeyOrdering,
        RuleName::OctalValues,
        RuleName::QuotedStrings,
    ];

    const WARNING_IN_DEFAULT: [RuleName; 3] = [
        RuleName::Comments,
        RuleName::CommentsIndentation,
        RuleName::DocumentStart,
    ];

    #[test]
    fn names_round_trip() {
        for preset in [Preset::Default, Preset::Relaxed] {
            assert_eq!(preset.as_str().parse::<Preset>(), Ok(preset));
            assert_eq!(preset.to_string(), preset.as_str());
        }
        assert_eq!(
            "strict".parse::<Preset>().unwrap_err().to_string(),
            "unknown preset 'strict', expected 'default' or 'relaxed'"
        );
    }

    #[test]
    fn default_enablement_and_severity_follow_yamllint() {
        let rules = Preset::Default.rules();
        for name in RuleName::ALL {
            if name == RuleName::LintDirective {
                assert!(rules.is_enabled(name));
                assert_eq!(rules.severity(name), None);
                continue;
            }
            assert_eq!(
                rules.is_enabled(name),
                !DISABLED_BY_DEFAULT.contains(&name),
                "{name}"
            );
            let expected = if name == RuleName::Truthy || WARNING_IN_DEFAULT.contains(&name) {
                Severity::Warning
            } else {
                Severity::Error
            };
            assert_eq!(rules.severity(name), Some(expected), "{name}");
        }
    }

    #[test]
    fn default_options_follow_yamllint_defaults() {
        let rules = Preset::Default.rules();
        assert_eq!(
            rules.document_start.options.present,
            MarkerPresence::Required
        );
        assert_eq!(rules.document_end.options.present, MarkerPresence::Required);
        assert_eq!(
            rules.quoted_strings.options.required,
            QuoteRequirement::Always
        );
        assert!(!rules.float_values.options.require_numeral_before_decimal);
        assert!(rules.truthy.options.check_keys);
        assert_eq!(
            rules.line_length.options.max.map(std::num::NonZero::get),
            Some(80)
        );
        assert_eq!(
            rules.comments.options.min_spaces_from_content,
            Limit::Max(2)
        );
        assert_eq!(rules.empty_lines.options.max, Limit::Max(2));
    }

    #[test]
    fn relaxed_overlays_default() {
        let relaxed = Preset::Relaxed.rules();
        let default = Preset::Default.rules();
        for name in [
            RuleName::Comments,
            RuleName::CommentsIndentation,
            RuleName::DocumentStart,
            RuleName::Truthy,
        ] {
            assert!(!relaxed.is_enabled(name), "{name}");
            assert_eq!(relaxed.severity(name), Some(Severity::Error), "{name}");
        }
        for name in [
            RuleName::Braces,
            RuleName::Brackets,
            RuleName::Colons,
            RuleName::Commas,
            RuleName::EmptyLines,
            RuleName::Hyphens,
            RuleName::Indentation,
            RuleName::LineLength,
        ] {
            assert!(relaxed.is_enabled(name), "{name}");
            assert_eq!(relaxed.severity(name), Some(Severity::Warning), "{name}");
        }
        assert_eq!(relaxed.braces.options.max_spaces_inside, Limit::Max(1));
        assert_eq!(relaxed.brackets.options.max_spaces_inside, Limit::Max(1));
        for name in [
            RuleName::DuplicateKey,
            RuleName::TrailingWhitespace,
            RuleName::NewLineAtEndOfFile,
            RuleName::NewLines,
            RuleName::InvalidAnchor,
        ] {
            assert_eq!(relaxed.is_enabled(name), default.is_enabled(name), "{name}");
            assert_eq!(relaxed.severity(name), default.severity(name), "{name}");
        }
        assert_eq!(relaxed.truthy.options, default.truthy.options);
    }
}
