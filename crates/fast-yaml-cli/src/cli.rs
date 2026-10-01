use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use fast_yaml_core::limits::{
    Indent, LimitRangeError, MaxAliasBytes, MaxDepth, MaxInputBytes, ParseLimits, Width,
};
#[cfg(feature = "linter")]
use fast_yaml_linter::config::IndentSize;
use std::num::NonZeroUsize;
use std::path::PathBuf;

use crate::config::Verbosity;
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
    quiet: bool,

    /// Verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Maximum size of each input file or of stdin (min: 1, max: 1GiB, default: 100MiB).
    /// Accepts KiB, MiB and GiB suffixes. Applies to single inputs and to batch runs;
    /// for `lint` it overrides the `max-input-bytes` config key
    #[arg(long, global = true, alias = "max-input-size", value_name = "BYTES", value_parser = parse_max_input_bytes)]
    pub max_input_bytes: Option<MaxInputBytes>,

    /// Verbosity resolved from `--quiet`/`--verbose` by [`Cli::validate`]; `Normal` before that.
    #[arg(skip)]
    pub verbosity: Verbosity,
}

impl Cli {
    /// Input size limit of commands that have no config file key: the flag or the default.
    #[must_use]
    pub fn max_input(&self) -> MaxInputBytes {
        self.max_input_bytes.unwrap_or_default()
    }

    /// Parses the process arguments, exiting with code 2 on any usage error.
    #[must_use]
    pub fn parse_validated() -> Self {
        Self::try_parse()
            .and_then(Self::validate)
            .unwrap_or_else(|err| err.exit())
    }

    /// Resolves [`verbosity`](Self::verbosity), rejecting `--quiet` together with `--verbose`.
    ///
    /// clap cannot enforce this for global flags given on both sides of the subcommand
    /// (`fy -q parse -v`), so it is checked on the merged result.
    ///
    /// # Errors
    ///
    /// Returns an argument-conflict error when both flags are set.
    pub fn validate(mut self) -> Result<Self, clap::Error> {
        self.verbosity = match (self.quiet, self.verbose) {
            (true, true) => {
                return Err(Self::command().error(
                    clap::error::ErrorKind::ArgumentConflict,
                    "the argument '--quiet' cannot be used with '--verbose'",
                ));
            }
            (true, false) => Verbosity::Quiet,
            (false, true) => Verbosity::Verbose,
            (false, false) => Verbosity::Normal,
        };
        Ok(self)
    }
}

/// Discovery and parallelism flags shared by every batch-capable subcommand.
#[derive(Args, Debug)]
pub struct BatchArgs {
    /// Include files matching glob pattern, case-insensitive (can be repeated; default: *.yaml, *.yml)
    #[arg(long)]
    pub include: Vec<String>,

    /// Exclude files matching glob pattern, case-insensitive (can be repeated)
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

/// Parser resource limits shared by every subcommand that parses YAML.
#[derive(Args, Debug, Clone, Copy)]
pub struct ParseLimitArgs {
    /// Maximum nesting depth of sequences and mappings (min: 1, max: 512); flow collections stop at 255
    #[arg(long, value_name = "N", value_parser = parse_max_depth, default_value_t = MaxDepth::DEFAULT)]
    pub max_depth: MaxDepth,

    /// Maximum bytes materialized by alias expansion per input (min: 1, max: 1GiB).
    /// Accepts KiB, MiB and GiB suffixes
    #[arg(long, value_name = "BYTES", value_parser = parse_max_alias_bytes, default_value_t = MaxAliasBytes::DEFAULT)]
    pub max_alias_bytes: MaxAliasBytes,
}

impl ParseLimitArgs {
    /// Builds the parser limits from the flags; other limits keep their defaults.
    #[must_use]
    pub fn parse_limits(&self) -> ParseLimits {
        ParseLimits {
            max_depth: self.max_depth,
            max_alias_bytes: self.max_alias_bytes,
            ..ParseLimits::default()
        }
    }
}

fn range_error(err: LimitRangeError) -> String {
    err.to_string()
}

fn parse_number(raw: &str) -> Result<usize, String> {
    raw.parse()
        .map_err(|e| format!("invalid integer '{raw}': {e}"))
}

fn parse_indent(raw: &str) -> Result<Indent, String> {
    Indent::new(parse_number(raw)?).map_err(range_error)
}

fn parse_width(raw: &str) -> Result<Width, String> {
    Width::new(parse_number(raw)?).map_err(range_error)
}

fn parse_max_depth(raw: &str) -> Result<MaxDepth, String> {
    MaxDepth::new(parse_number(raw)?).map_err(range_error)
}

/// Binary size suffixes accepted by the byte-size flags, longest first.
const SIZE_SUFFIXES: [(&str, usize); 3] = [("GiB", 1 << 30), ("MiB", 1 << 20), ("KiB", 1 << 10)];

fn parse_byte_size(raw: &str) -> Result<usize, String> {
    let (digits, unit) = SIZE_SUFFIXES
        .iter()
        .find_map(|&(suffix, unit)| raw.strip_suffix(suffix).map(|digits| (digits, unit)))
        .unwrap_or((raw, 1));
    let count: usize = digits
        .trim()
        .parse()
        .map_err(|e| format!("invalid size '{raw}': {e}"))?;
    count
        .checked_mul(unit)
        .ok_or_else(|| format!("size '{raw}' is too large"))
}

fn parse_max_alias_bytes(raw: &str) -> Result<MaxAliasBytes, String> {
    MaxAliasBytes::new(parse_byte_size(raw)?).map_err(range_error)
}

fn parse_max_input_bytes(raw: &str) -> Result<MaxInputBytes, String> {
    MaxInputBytes::new(parse_byte_size(raw)?).map_err(range_error)
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

        #[command(flatten)]
        limits: ParseLimitArgs,
    },

    /// Format YAML with consistent style
    Format {
        /// Input paths (files, directories, or glob patterns).
        /// A missing path, a glob matching nothing or an explicit non-YAML file in batch mode is
        /// an error. `[` is literal unless the pattern also has `*` or `?` (write `[[]` for it then).
        /// If empty and no --stdin-files, reads from stdin
        #[arg(value_name = "PATHS")]
        paths: Vec<PathBuf>,

        /// Indentation width (1-9 spaces)
        #[arg(long, value_name = "N", value_parser = parse_indent, default_value_t = Indent::DEFAULT)]
        indent: Indent,

        /// Maximum line width (min: 20, max: 1000)
        #[arg(long, value_name = "N", value_parser = parse_width, default_value_t = Width::DEFAULT)]
        width: Width,

        /// Maximum nesting depth of sequences and mappings (min: 1, max: 512); flow collections stop at 255
        #[arg(long, value_name = "N", value_parser = parse_max_depth, default_value_t = MaxDepth::DEFAULT)]
        max_depth: MaxDepth,

        /// Read file paths from stdin (one per line). A missing path, a directory, a non-YAML
        /// file or a line over 4096 bytes is an error, so filter git output:
        /// `git diff --name-only --diff-filter=d -- '*.yaml' '*.yml' | fy format --stdin-files`
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
    #[command(
        after_help = "--max-depth and --max-alias-bytes apply to YAML input only; \
                            JSON input is not affected"
    )]
    Convert {
        /// Target format
        #[arg(value_enum)]
        to: ConvertFormat,

        /// Input file (default: stdin)
        file: Option<PathBuf>,

        /// Pretty-print JSON output
        #[arg(long, default_value_t = true, num_args = 0..=1, default_missing_value = "true", action = clap::ArgAction::Set)]
        pretty: bool,

        #[command(flatten)]
        limits: ParseLimitArgs,
    },

    #[cfg(feature = "linter")]
    /// Lint YAML with diagnostics
    ///
    /// Diagnostics can be suppressed inline with `# fy: disable [rules]`, `# fy: enable [rules]`,
    /// `# fy: disable-line [rules]` and `# fy: disable-file` (`# yamllint ...` is also accepted).
    Lint {
        /// Input paths (files, directories, or glob patterns).
        /// A missing path, a glob matching nothing or an explicit non-YAML file in batch mode is
        /// an error. `[` is literal unless the pattern also has `*` or `?` (write `[[]` for it then).
        /// If empty and no --stdin-files, reads from stdin.
        #[arg(value_name = "PATHS")]
        paths: Vec<PathBuf>,

        /// Read file paths from stdin (one per line). A missing path, a directory, a non-YAML
        /// file or a line over 4096 bytes is an error, so filter git output:
        /// `git diff --name-only --diff-filter=d -- '*.yaml' '*.yml' | fy lint --stdin-files`
        #[arg(long, conflicts_with = "paths")]
        stdin_files: bool,

        /// Path to config file (default: auto-discover .fast-yaml.yaml)
        #[arg(long, value_name = "FILE", conflicts_with = "no_config")]
        config: Option<PathBuf>,

        /// Disable config file auto-discovery
        #[arg(long, conflicts_with = "config")]
        no_config: bool,

        /// Maximum line length (overrides config file)
        #[arg(long)]
        max_line_length: Option<NonZeroUsize>,

        /// Indentation size (overrides config file)
        #[arg(long)]
        indent_size: Option<IndentSize>,

        /// Lint output format
        #[arg(long, value_enum, default_value = "text")]
        format: LintFormat,

        /// Allow duplicate keys — overrides config file (opt-in, suppresses duplicate key errors)
        #[arg(long, num_args = 0..=1, default_missing_value = "true", action = clap::ArgAction::Set)]
        allow_duplicate_keys: Option<bool>,

        #[command(flatten)]
        batch: BatchArgs,

        #[command(flatten)]
        limits: ParseLimitArgs,
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

    #[test]
    fn limit_help_matches_type_constants() {
        use clap::CommandFactory;
        assert_eq!(MaxAliasBytes::MAX.get(), 1 << 30);
        let mut cli = Cli::command();
        for name in ["parse", "convert", "lint"] {
            let help = cli
                .find_subcommand_mut(name)
                .unwrap()
                .render_long_help()
                .to_string()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            assert!(
                help.contains(&format!("max: {}", MaxDepth::MAX.get())),
                "{name}: {help}"
            );
            assert!(help.contains("max: 1GiB"), "{name}: {help}");
            assert!(
                help.contains(&format!("[default: {}]", MaxDepth::DEFAULT)),
                "{name}: {help}"
            );
            assert!(
                help.contains(&format!("[default: {}]", MaxAliasBytes::DEFAULT)),
                "{name}: {help}"
            );
        }
    }

    #[test]
    fn convert_help_states_yaml_only() {
        use clap::CommandFactory;
        let help = Cli::command()
            .find_subcommand_mut("convert")
            .unwrap()
            .render_long_help()
            .to_string()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(help.contains("apply to YAML input only"));
    }

    #[test]
    fn alias_bytes_accepts_suffixes() {
        assert_eq!(parse_max_alias_bytes("1024").unwrap().get(), 1024);
        assert_eq!(parse_max_alias_bytes("64KiB").unwrap().get(), 64 << 10);
        assert_eq!(parse_max_alias_bytes("128MiB").unwrap().get(), 128 << 20);
        assert_eq!(parse_max_alias_bytes("1GiB").unwrap(), MaxAliasBytes::MAX);
    }

    #[test]
    fn alias_bytes_rejects_out_of_range_and_garbage() {
        assert!(
            parse_max_alias_bytes("0")
                .unwrap_err()
                .contains("between 1 and")
        );
        assert!(parse_max_alias_bytes("2GiB").is_err());
        assert!(parse_max_alias_bytes("99999999999999999999GiB").is_err());
        assert!(parse_max_alias_bytes("-1").is_err());
        assert!(parse_max_alias_bytes("MiB").is_err());
        assert!(parse_max_alias_bytes("1.5MiB").is_err());
    }

    #[test]
    fn input_size_parses_and_bounds() {
        assert_eq!(parse_max_input_bytes("1").unwrap().get(), 1);
        assert_eq!(parse_max_input_bytes("2MiB").unwrap().get(), 2 << 20);
        assert_eq!(parse_max_input_bytes("1GiB").unwrap(), MaxInputBytes::MAX);
        assert!(parse_max_input_bytes("0").is_err());
        assert!(parse_max_input_bytes("2GiB").is_err());
    }

    #[test]
    fn verbosity_follows_flags() {
        for (args, expected) in [
            (&["fy", "parse"][..], Verbosity::Normal),
            (&["fy", "-v", "parse"], Verbosity::Verbose),
            (&["fy", "parse", "-q"], Verbosity::Quiet),
        ] {
            let cli = Cli::try_parse_from(args).and_then(Cli::validate).unwrap();
            assert_eq!(cli.verbosity, expected, "{args:?}");
        }
    }

    #[test]
    fn byte_size_overflow_is_rejected() {
        assert!(
            parse_byte_size("17179869184GiB")
                .unwrap_err()
                .contains("too large")
        );
        assert!(parse_byte_size("18446744073709551615KiB").is_err());
    }

    #[test]
    fn depth_rejects_out_of_range() {
        assert_eq!(parse_max_depth("512").unwrap(), MaxDepth::MAX);
        assert!(parse_max_depth("0").is_err());
        assert!(parse_max_depth("513").is_err());
        assert!(parse_max_depth("x").is_err());
    }
}
