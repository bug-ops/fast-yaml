//! Typed rule configuration, config files and builders.

pub mod config_file;
#[cfg(test)]
mod options_tests;
mod rules;
mod values;

pub use config_file::{ConfigFile, ConfigFileError};
pub(crate) use rules::default_rules;
pub use rules::{
    CustomRuleCode, NoOptions, OptionConflict, RuleConfigError, RuleName, RuleOptions,
    RuleSettings, RulesConfig, UnknownRuleError,
};
pub(crate) use values::{BoolOrName, deserialize_bool_or_name};
pub use values::{EmptyInsideLimit, IndentSize, InvalidIndentSize, Limit};

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
