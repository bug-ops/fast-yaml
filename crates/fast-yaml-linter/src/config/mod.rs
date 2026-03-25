//! Rule configuration types and builders.

pub mod config_file;
mod rule_config;

pub use config_file::{ConfigFile, ConfigFileError};
pub use rule_config::{RuleConfig, RuleOption, RuleOptions};
