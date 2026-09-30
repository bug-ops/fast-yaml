use clap::{Args, Parser, Subcommand, ValueEnum};
use std::num::NonZeroUsize;
use std::path::PathBuf;

use crate::discovery::DiscoveryConfig;

/// Fast YAML processor with validation and linting
#[derive(Parser, Debug)]
#[command(
    name = "fy",
    about = "Fast YAML processor with validation and linting",
    version,
    author,
    long_about = None
)]
#[allow(clippy::struct_excessive_bools)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Edit file in-place (requires file argument)
    #[arg(short = 'i', long, global = true)]
    pub in_place: bool,

    /// Output file (default: stdout)
    #[arg(short, long, global = true, value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Output format
    #[arg(short = 'f', long, value_enum, default_value = "yaml")]
    pub format: OutputFormat,

    /// Disable colored output
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Quiet mode (errors only)
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

/// Discovery and parallelism flags shared by every batch-capable subcommand.
#[derive(Args, Debug)]
pub struct BatchArgs {
    /// Include files matching glob pattern (can be repeated)
    #[arg(long)]
    pub include: Vec<String>,

    /// Exclude files matching glob pattern (can be repeated)
    #[arg(long)]
    pub exclude: Vec<String>,

    /// Don't recurse into subdirectories
    #[arg(long)]
    pub no_recursive: bool,

    /// Number of parallel jobs (0 = auto-detect)
    #[arg(short = 'j', long, default_value = "0")]
    pub jobs: usize,
}

impl BatchArgs {
    /// Builds the discovery configuration from the include/exclude/recursion flags.
    #[must_use]
    pub fn discovery_config(&self) -> DiscoveryConfig {
        let mut config = DiscoveryConfig::new();
        if !self.include.is_empty() {
            config = config.with_include_patterns(self.include.clone());
        }
        if !self.exclude.is_empty() {
            config = config.with_exclude_patterns(self.exclude.clone());
        }
        if self.no_recursive {
            config = config.with_max_depth(Some(1));
        }
        config
    }

    /// Returns the explicit worker count, or `None` to auto-detect.
    #[must_use]
    pub const fn workers(&self) -> Option<NonZeroUsize> {
        NonZeroUsize::new(self.jobs)
    }

    /// Returns `true` when any flag only makes sense for a batch run.
    #[must_use]
    pub const fn requests_batch(&self) -> bool {
        !self.include.is_empty() || !self.exclude.is_empty() || self.jobs > 0
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Parse and validate YAML
    Parse {
        /// Input file (default: stdin)
        file: Option<PathBuf>,

        /// Show parse statistics
        #[arg(long)]
        stats: bool,
    },

    /// Format YAML with consistent style
    Format {
        /// Input paths (files, directories, or glob patterns).
        /// If empty and no --stdin-files, reads from stdin
        #[arg(value_name = "PATHS")]
        paths: Vec<PathBuf>,

        /// Indentation width (2-8 spaces)
        #[arg(long, default_value = "2", value_parser = clap::value_parser!(u8).range(2..=8))]
        indent: u8,

        /// Maximum line width
        #[arg(long, default_value = "80")]
        width: usize,

        /// Read file paths from stdin (one per line)
        #[arg(long, conflicts_with = "paths")]
        stdin_files: bool,

        #[command(flatten)]
        batch: BatchArgs,

        /// Never write any file; only print a summary of what would change.
        /// Works for stdin too. Exits with code 5 if any file would change,
        /// 1 if any file failed (takes precedence), 0 otherwise
        #[arg(short = 'n', long, conflicts_with = "output")]
        dry_run: bool,

        /// Suppress the error when YAML comments are detected.
        /// Comments are not preserved by the formatter and will be stripped.
        /// Without this flag, formatting a file that contains comments exits with an error.
        #[arg(long)]
        strip_comments: bool,
    },

    /// Convert between YAML and JSON
    Convert {
        /// Target format
        #[arg(value_enum)]
        to: ConvertFormat,

        /// Input file (default: stdin)
        file: Option<PathBuf>,

        /// Pretty-print JSON output
        #[arg(long, default_value_t = true, num_args = 0..=1, default_missing_value = "true", action = clap::ArgAction::Set)]
        pretty: bool,
    },

    #[cfg(feature = "linter")]
    /// Lint YAML with diagnostics
    Lint {
        /// Input paths (files, directories, or glob patterns).
        /// If empty, reads from stdin.
        #[arg(value_name = "PATHS")]
        paths: Vec<PathBuf>,

        /// Path to config file (default: auto-discover .fast-yaml.yaml)
        #[arg(long, value_name = "FILE", conflicts_with = "no_config")]
        config: Option<PathBuf>,

        /// Disable config file auto-discovery
        #[arg(long, conflicts_with = "config")]
        no_config: bool,

        /// Maximum line length (overrides config file)
        #[arg(long)]
        max_line_length: Option<usize>,

        /// Indentation size (overrides config file)
        #[arg(long)]
        indent_size: Option<usize>,

        /// Lint output format
        #[arg(long, value_enum, default_value = "text")]
        format: LintFormat,

        /// Allow duplicate keys — overrides config file (opt-in, suppresses duplicate key errors)
        #[arg(long, num_args = 0..=1, default_missing_value = "true", action = clap::ArgAction::Set)]
        allow_duplicate_keys: Option<bool>,

        #[command(flatten)]
        batch: BatchArgs,
    },
}

#[derive(ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    Yaml,
    Json,
    Compact,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum ConvertFormat {
    Yaml,
    Json,
}

#[cfg(feature = "linter")]
#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum LintFormat {
    Text,
    Json,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_cli() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
