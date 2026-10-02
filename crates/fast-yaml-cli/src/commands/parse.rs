#![allow(clippy::needless_pass_by_ref_mut)]

use anyhow::{Context, Result};
use fast_yaml_core::Parser;
use fast_yaml_core::limits::ParseLimits;

use crate::config::CommonConfig;
use crate::io::InputSource;
use crate::reporter::{ReportEvent, Reporter};

/// Parse command implementation
pub struct ParseCommand {
    show_stats: bool,
    config: CommonConfig,
    limits: ParseLimits,
}

impl ParseCommand {
    pub const fn new(config: CommonConfig, show_stats: bool, limits: ParseLimits) -> Self {
        Self {
            show_stats,
            config,
            limits,
        }
    }

    /// Execute parse command
    pub fn execute(&self, input: &InputSource) -> Result<()> {
        let mut reporter = Reporter::new(self.config.output.clone());
        reporter.start_timing();

        let maybe_value = Parser::parse_str_with_limits(input.as_str(), &self.limits)
            .context("Failed to parse YAML")?;

        reporter
            .report(ReportEvent::Success {
                message: "YAML is valid",
            })
            .ok();

        if self.show_stats
            && let Some(ref value) = maybe_value
        {
            print_statistics(value, &reporter);
        }

        if let Some(duration) = reporter.elapsed() {
            reporter
                .report(ReportEvent::Timing {
                    operation: "parse",
                    duration,
                })
                .ok();
        }

        Ok(())
    }
}

/// Reports the key count and nesting depth of `value`.
fn print_statistics(value: &fast_yaml_core::Value, reporter: &Reporter) {
    let (keys, max_depth) = count_keys_and_depth(value, 0);
    reporter
        .report(ReportEvent::Statistics { keys, max_depth })
        .ok();
}

/// Recursively count keys and max depth
fn count_keys_and_depth(value: &fast_yaml_core::Value, current_depth: usize) -> (usize, usize) {
    use fast_yaml_core::Value;

    match value {
        Value::Mapping(map) => {
            let mut total_keys = map.len();
            let mut max_depth = current_depth + 1;

            for (_, v) in map {
                let (child_keys, child_depth) = count_keys_and_depth(v, current_depth + 1);
                total_keys += child_keys;
                max_depth = max_depth.max(child_depth);
            }

            (total_keys, max_depth)
        }
        Value::Sequence(arr) => {
            let mut max_depth = current_depth + 1;
            let mut total_keys = 0;

            for v in arr {
                let (child_keys, child_depth) = count_keys_and_depth(v, current_depth + 1);
                total_keys += child_keys;
                max_depth = max_depth.max(child_depth);
            }

            (total_keys, max_depth)
        }
        Value::Set(set) => {
            let mut max_depth = current_depth + 1;
            for member in set {
                max_depth = max_depth.max(count_keys_and_depth(member, current_depth + 1).1);
            }
            (set.len(), max_depth)
        }
        _ => (0, current_depth),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::input::InputOrigin;

    #[test]
    fn test_parse_valid_yaml() {
        let input = InputSource {
            content: "name: test\nvalue: 123".to_string(),
            origin: InputOrigin::Stdin,
        };

        let config = CommonConfig::new().with_output(
            crate::config::OutputConfig::new().with_verbosity(crate::config::Verbosity::Quiet),
        );
        let cmd = ParseCommand::new(config, false, ParseLimits::default());
        assert!(cmd.execute(&input).is_ok());
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let input = InputSource {
            content: "invalid: [".to_string(),
            origin: InputOrigin::Stdin,
        };

        let config = CommonConfig::new().with_output(
            crate::config::OutputConfig::new().with_verbosity(crate::config::Verbosity::Quiet),
        );
        let cmd = ParseCommand::new(config, false, ParseLimits::default());
        assert!(cmd.execute(&input).is_err());
    }

    #[test]
    fn test_parse_empty_yaml() {
        let input = InputSource {
            content: String::new(),
            origin: InputOrigin::Stdin,
        };

        let config = CommonConfig::new().with_output(
            crate::config::OutputConfig::new().with_verbosity(crate::config::Verbosity::Quiet),
        );
        let cmd = ParseCommand::new(config, false, ParseLimits::default());
        assert!(cmd.execute(&input).is_ok());
    }

    #[test]
    fn test_parse_null_document() {
        let input = InputSource {
            content: "~".to_string(),
            origin: InputOrigin::Stdin,
        };

        let config = CommonConfig::new().with_output(
            crate::config::OutputConfig::new().with_verbosity(crate::config::Verbosity::Quiet),
        );
        let cmd = ParseCommand::new(config, false, ParseLimits::default());
        assert!(cmd.execute(&input).is_ok());
    }

    #[test]
    fn test_parse_comment_only_document() {
        let input = InputSource {
            content: "# just a comment\n".to_string(),
            origin: InputOrigin::Stdin,
        };

        let config = CommonConfig::new().with_output(
            crate::config::OutputConfig::new().with_verbosity(crate::config::Verbosity::Quiet),
        );
        let cmd = ParseCommand::new(config, false, ParseLimits::default());
        assert!(cmd.execute(&input).is_ok());
    }

    #[test]
    fn test_count_keys_and_depth_simple() {
        let yaml = "name: test\nvalue: 123";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let (keys, depth) = count_keys_and_depth(&value, 0);
        assert_eq!(keys, 2);
        assert_eq!(depth, 1);
    }

    #[test]
    fn test_count_keys_and_depth_nested() {
        let yaml = "parent:\n  child1: value1\n  child2: value2";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let (keys, depth) = count_keys_and_depth(&value, 0);
        assert_eq!(keys, 3); // parent, child1, child2
        assert!(depth >= 2);
    }
}
