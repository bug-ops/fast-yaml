use anyhow::{Context, Result};
use fast_yaml_core::Parser;

#[cfg(feature = "colors")]
use colored::Colorize;

use crate::io::InputSource;

/// Parse command implementation
pub struct ParseCommand {
    show_stats: bool,
    use_color: bool,
    quiet: bool,
}

impl ParseCommand {
    pub const fn new(show_stats: bool, use_color: bool, quiet: bool) -> Self {
        Self {
            show_stats,
            use_color,
            quiet,
        }
    }

    /// Execute parse command
    pub fn execute(&self, input: &InputSource) -> Result<()> {
        // Parse YAML
        let value = Parser::parse_str(input.as_str())
            .context("Failed to parse YAML")?
            .ok_or_else(|| anyhow::anyhow!("Empty YAML document"))?;

        if !self.quiet {
            #[cfg(feature = "colors")]
            if self.use_color {
                println!("{} YAML is valid", "✓".green().bold());
            } else {
                println!("✓ YAML is valid");
            }

            #[cfg(not(feature = "colors"))]
            println!("✓ YAML is valid");

            if self.show_stats {
                self.print_statistics(&value);
            }
        }

        Ok(())
    }

    /// Print parsing statistics
    fn print_statistics(&self, value: &fast_yaml_core::Value) {
        let (key_count, max_depth) = count_keys_and_depth(value, 0);

        #[cfg(feature = "colors")]
        if self.use_color {
            println!("\n{}", "Statistics:".bold());
            println!("  Keys: {}", key_count.to_string().cyan());
            println!("  Max depth: {}", max_depth.to_string().cyan());
            return;
        }

        // Fallback for no-color or when colors feature is disabled
        println!("\nStatistics:");
        println!("  Keys: {key_count}");
        println!("  Max depth: {max_depth}");
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

        let cmd = ParseCommand::new(false, false, true);
        assert!(cmd.execute(&input).is_ok());
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let input = InputSource {
            content: "invalid: [".to_string(),
            origin: InputOrigin::Stdin,
        };

        let cmd = ParseCommand::new(false, false, true);
        assert!(cmd.execute(&input).is_err());
    }

    #[test]
    fn test_parse_empty_yaml() {
        let input = InputSource {
            content: String::new(),
            origin: InputOrigin::Stdin,
        };

        let cmd = ParseCommand::new(false, false, true);
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
