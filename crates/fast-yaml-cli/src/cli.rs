use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

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
    /// Input file (default: stdin)
    pub file: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,

    /// Edit file in-place (requires file argument)
    #[arg(short = 'i', long, requires = "file")]
    pub in_place: bool,

    /// Output file (default: stdout)
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Output format
    #[arg(short = 'f', long, value_enum, default_value = "yaml")]
    pub format: OutputFormat,

    /// Disable colored output
    #[arg(long)]
    pub no_color: bool,

    /// Quiet mode (errors only)
    #[arg(short, long)]
    pub quiet: bool,

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Parse and validate YAML
    Parse {
        /// Show parse statistics
        #[arg(long)]
        stats: bool,
    },

    /// Format YAML with consistent style
    Format {
        /// Indentation width (2-8 spaces)
        #[arg(long, default_value = "2", value_parser = clap::value_parser!(u8).range(2..=8))]
        indent: u8,

        /// Maximum line width
        #[arg(long, default_value = "80")]
        width: usize,
    },

    /// Convert between YAML and JSON
    Convert {
        /// Target format
        #[arg(value_enum)]
        to: ConvertFormat,

        /// Pretty-print JSON output
        #[arg(long, default_value = "true")]
        pretty: bool,
    },

    #[cfg(feature = "linter")]
    /// Lint YAML with diagnostics
    Lint {
        /// Maximum line length
        #[arg(long, default_value = "120")]
        max_line_length: usize,

        /// Indentation size
        #[arg(long, default_value = "2")]
        indent_size: usize,

        /// Lint output format
        #[arg(long, value_enum, default_value = "text")]
        format: LintFormat,
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
#[derive(ValueEnum, Clone, Debug)]
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
