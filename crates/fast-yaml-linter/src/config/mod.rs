//! Typed rule configuration, config files and builders.

pub mod config_file;
#[cfg(test)]
mod options_tests;
mod path_patterns;
mod preset;
mod rules;
mod values;

pub use config_file::{
    ConfigFile, ConfigFileError, FileSelection, MAX_CONFIG_FILE_BYTES, MAX_EXTENDS_DEPTH,
    TopLevelKey,
};
pub use path_patterns::{IgnorePatterns, InvalidPathPattern, MAX_PATH_PATTERNS, YamlFiles};
pub use preset::{Preset, UnknownPresetError};
pub(crate) use rules::default_rules;
pub use rules::{
    CustomRuleCode, NoOptions, OptionConflict, RuleConfigError, RuleName, RuleOptions,
    RuleSettings, RulesConfig, UnknownRuleError,
};
pub use values::{
    AlwaysTrue, EmptyInsideLimit, IndentSize, InvalidIndentSize, InvalidPatternList,
    InvalidRegexPattern, Limit, MarkerPresence, PatternList,
};
pub(crate) use values::{BoolOrName, deserialize_bool_or_name};

#[cfg(test)]
pub(crate) mod test_support {
    use super::{RuleName, RulesConfig};
    use crate::LintConfig;

    /// Builds a config where one rule is configured from a YAML entry.
    pub fn config_with_rule(name: RuleName, entry: &str) -> LintConfig {
        let mut rules = RulesConfig::default();
        rules
            .apply_rule(name, serde_norway::Deserializer::from_str(entry))
            .unwrap();
        LintConfig {
            rules,
            ..LintConfig::default()
        }
    }
}
