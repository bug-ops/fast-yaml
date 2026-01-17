#![allow(clippy::needless_pass_by_ref_mut)]

use anyhow::{Context, Result};
use fast_yaml_core::Parser;

use crate::config::CommonConfig;
use crate::io::InputSource;
use crate::reporter::{ReportEvent, Reporter};

/// Parse command implementation
pub struct ParseCommand {
    show_stats: bool,
    config: CommonConfig,
}

impl ParseCommand {
    pub const fn new(config: CommonConfig, show_stats: bool) -> Self {
        Self { show_stats, config }
    }

    /// Execute parse command
    pub fn execute(&self, input: &InputSource) -> Result<()> {
        let mut reporter = Reporter::new(self.config.output.clone());
        reporter.start_timing();

        let value = Parser::parse_str(input.as_str())
            .context("Failed to parse YAML")?
            .ok_or_else(|| anyhow::anyhow!("Empty YAML document"))?;

        reporter
            .report(ReportEvent::Success {
                message: "YAML is valid",
            })
            .ok();

        if self.show_stats {
            self.print_statistics(&value, &reporter);
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

    /// Print parsing statistics
    fn print_statistics(&self, value: &fast_yaml_core::Value, reporter: &Reporter) {
        let (key_count, max_depth) = count_keys_and_depth(value, 0);

        #[cfg(feature = "colors")]
        if self.config.output.use_color() {
            use colored::Colorize;
            println!("\n{}", "Statistics:".bold());
            println!("  Keys: {}", key_count.to_string().cyan());
            println!("  Max depth: {}", max_depth.to_string().cyan());
            return;
        }
        #[cfg(not(feature = "colors"))]
        {
            let _ = self.config.output.use_color();
        }

        println!("\nStatistics:");
        println!("  Keys: {key_count}");
        println!("  Max depth: {max_depth}");
        let _ = reporter;
    }
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

        let config =
            CommonConfig::new().with_output(crate::config::OutputConfig::new().with_quiet(true));
        let cmd = ParseCommand::new(config, false);
        assert!(cmd.execute(&input).is_ok());
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let input = InputSource {
            content: "invalid: [".to_string(),
            origin: InputOrigin::Stdin,
        };

        let config =
            CommonConfig::new().with_output(crate::config::OutputConfig::new().with_quiet(true));
        let cmd = ParseCommand::new(config, false);
        assert!(cmd.execute(&input).is_err());
    }

    #[test]
    fn test_parse_empty_yaml() {
        let input = InputSource {
            content: String::new(),
            origin: InputOrigin::Stdin,
        };

        let config =
            CommonConfig::new().with_output(crate::config::OutputConfig::new().with_quiet(true));
        let cmd = ParseCommand::new(config, false);
        let result = cmd.execute(&input);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Empty YAML document")
        );
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
