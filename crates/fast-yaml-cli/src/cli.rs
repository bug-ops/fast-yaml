#[cfg(feature = "linter")]
use crate::commands::lint::ConfigSource;
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use fast_yaml_core::limits::{
    Indent, LimitRangeError, MaxAliasBytes, MaxDepth, MaxDocuments, MaxInputBytes, MaxScanAhead,
    ParseLimits, Width,
};
#[cfg(feature = "linter")]
use fast_yaml_linter::config::{IndentSize, MaxDiagnostics};
use fast_yaml_parallel::Workers;
#[cfg(feature = "linter")]
use std::num::NonZeroUsize;
use std::path::PathBuf;

use crate::config::Verbosity;
use crate::discovery::DiscoveryConfig;
use crate::io::{OutputTarget, WriteTarget};

/// Fast YAML processor with validation and linting
#[derive(Parser, Debug)]
#[command(
    name = "fy",
    about = "Fast YAML processor with validation and linting",
    version,
    author,
    long_about = None
)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Disable colored output
    #[arg(long, global = true)]
    no_color: bool,

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
    max_input_bytes: Option<MaxInputBytes>,

    /// Maximum characters the parser may read past the last node it reported (min: 1, max: 1GiB,
    /// default: 4MiB). Accepts KiB, MiB and GiB suffixes. Bounds parser memory (about 190x this
    /// value per input): a flow collection read whole (at the root, in a `- ` entry, nested in flow
    /// or after a tab, so any JSON document longer than this), one scalar, or a run of comments
    /// longer than this is rejected. For `lint` it overrides the
    /// `max-scan-ahead` config key. Without this flag, batch runs start each file at the default
    /// divided by the worker count (at least 1MiB) and re-run a rejected file at the full
    /// default one at a time, so the result matches a single-file run; with the flag the limit
    /// is used as is
    #[arg(long, global = true, value_name = "CHARS", value_parser = parse_max_scan_ahead)]
    max_scan_ahead: Option<MaxScanAhead>,
}

/// The parsed command line after validation: the only form the rest of the program sees, so a
/// `--quiet`/`--verbose` conflict can never reach a command.
#[derive(Debug)]
pub struct ResolvedCli {
    /// The subcommand; `None` formats stdin to stdout.
    pub command: Option<Command>,
    /// `--no-color` was given.
    pub no_color: bool,
    /// Verbosity resolved from `--quiet`/`--verbose`.
    pub verbosity: Verbosity,
    /// `--max-input-bytes`, if given.
    pub max_input_bytes: Option<MaxInputBytes>,
    /// `--max-scan-ahead`, if given.
    pub max_scan_ahead: Option<MaxScanAhead>,
}

impl ResolvedCli {
    /// Input size limit of commands that have no config file key: the flag or the default.
    #[must_use]
    pub fn max_input(&self) -> MaxInputBytes {
        self.max_input_bytes.unwrap_or_default()
    }

    /// Scan-ahead limit of commands that have no config file key: the flag or the default.
    #[must_use]
    pub fn scan_ahead(&self) -> MaxScanAhead {
        self.max_scan_ahead.unwrap_or_default()
    }
}

impl Cli {
    /// Parses the process arguments, exiting with code 2 on any usage error.
    #[must_use]
    pub fn parse_validated() -> ResolvedCli {
        Self::try_parse()
            .and_then(Self::validate)
            .unwrap_or_else(|err| err.exit())
    }

    /// Resolves the parsed flags, rejecting `--quiet` together with `--verbose`.
    ///
    /// clap cannot enforce this for global flags given on both sides of the subcommand
    /// (`fy -q parse -v`), so it is checked on the merged result.
    ///
    /// # Errors
    ///
    /// Returns an argument-conflict error when both flags are set.
    pub fn validate(self) -> Result<ResolvedCli, clap::Error> {
        let verbosity = match (self.quiet, self.verbose) {
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
        Ok(ResolvedCli {
            command: self.command,
            no_color: self.no_color,
            verbosity,
            max_input_bytes: self.max_input_bytes,
            max_scan_ahead: self.max_scan_ahead,
        })
    }
}

/// Config file flags of `fy lint`.
#[cfg(feature = "linter")]
#[derive(Args, Debug, Clone, Default)]
pub struct ConfigArgs {
    /// Path to config file (default: auto-discover .fast-yaml.yaml)
    #[arg(long, value_name = "FILE", conflicts_with = "no_config")]
    config: Option<PathBuf>,

    /// Disable config file auto-discovery
    #[arg(long, conflicts_with = "config")]
    no_config: bool,
}

#[cfg(feature = "linter")]
impl ConfigArgs {
    /// Resolves the flags into the single source they name.
    #[must_use]
    pub fn source(self) -> ConfigSource {
        match (self.config, self.no_config) {
            (Some(path), _) => ConfigSource::Explicit(path),
            (None, true) => ConfigSource::Disabled,
            (None, false) => ConfigSource::Discover,
        }
    }
}

/// Report destination flag of the subcommands that write output.
#[derive(Args, Debug, Clone, Default)]
pub struct OutputArgs {
    /// Output file (default: stdout)
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,
}

impl OutputArgs {
    /// Resolves the flag into the destination it names.
    #[must_use]
    pub fn target(self) -> OutputTarget {
        self.output.map_or(OutputTarget::Stdout, OutputTarget::File)
    }
}

/// Write-destination flags of the subcommands that can also rewrite their input.
#[derive(Args, Debug, Clone, Default)]
pub struct WriteArgs {
    #[command(flatten)]
    output: OutputArgs,

    /// Edit file in-place (requires file argument)
    #[arg(short = 'i', long, conflicts_with = "output")]
    in_place: bool,
}

impl WriteArgs {
    /// Resolves the flags into the single destination they name.
    #[must_use]
    pub fn target(self) -> WriteTarget {
        if self.in_place {
            WriteTarget::InPlace
        } else {
            WriteTarget::Output(self.output.target())
        }
    }
}

/// Discovery and parallelism flags shared by every batch-capable subcommand.
#[derive(Args, Debug)]
pub struct BatchArgs {
    /// Include files matching glob pattern, case-insensitive (can be repeated; default: *.yaml, *.yml; `lint` also .yamllint)
    #[arg(long)]
    pub include: Vec<String>,

    /// Exclude files matching glob pattern, case-insensitive (can be repeated)
    #[arg(long)]
    pub exclude: Vec<String>,

    /// Don't recurse into subdirectories
    #[arg(long)]
    pub no_recursive: bool,

    /// Number of parallel jobs (0 = auto, 1-128)
    #[arg(short = 'j', long, default_value = "0", value_name = "N", value_parser = parse_jobs)]
    pub jobs: Workers,
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

    /// Returns `true` when any flag only makes sense for a batch run.
    #[must_use]
    pub const fn requests_batch(&self) -> bool {
        !self.include.is_empty()
            || !self.exclude.is_empty()
            || matches!(self.jobs, Workers::Fixed(_))
    }
}

/// Parser resource limits shared by every subcommand that parses YAML.
#[derive(Args, Debug, Clone, Copy)]
#[allow(clippy::struct_field_names)] // field names are the flag names
pub struct ParseLimitArgs {
    /// Maximum nesting depth of sequences and mappings (min: 1, max: 512); flow collections stop at 255
    #[arg(long, value_name = "N", value_parser = parse_max_depth, default_value_t = MaxDepth::DEFAULT)]
    pub max_depth: MaxDepth,

    /// Maximum bytes materialized by alias expansion per input (min: 1, max: 1GiB).
    /// Accepts KiB, MiB and GiB suffixes
    #[arg(long, value_name = "BYTES", value_parser = parse_max_alias_bytes, default_value_t = MaxAliasBytes::DEFAULT)]
    pub max_alias_bytes: MaxAliasBytes,

    /// Maximum documents per input stream (min: 1, max: 10000000)
    #[arg(long, value_name = "N", value_parser = parse_max_documents, default_value_t = MaxDocuments::DEFAULT)]
    pub max_documents: MaxDocuments,
}

impl Default for ParseLimitArgs {
    fn default() -> Self {
        Self {
            max_depth: MaxDepth::DEFAULT,
            max_alias_bytes: MaxAliasBytes::DEFAULT,
            max_documents: MaxDocuments::DEFAULT,
        }
    }
}

impl ParseLimitArgs {
    /// Builds the parser limits from the flags and the global scan-ahead limit; other limits
    /// keep their defaults.
    #[must_use]
    pub fn parse_limits(&self, max_scan_ahead: MaxScanAhead) -> ParseLimits {
        ParseLimits {
            max_depth: self.max_depth,
            max_alias_bytes: self.max_alias_bytes,
            max_documents: self.max_documents,
            max_scan_ahead,
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

/// Parses `-j`: `0` selects [`Workers::Auto`] (the CLI never runs sequentially), `1..=128` a pool.
fn parse_jobs(raw: &str) -> Result<Workers, String> {
    match Workers::from_count(parse_number(raw)?).map_err(range_error)? {
        Workers::Sequential => Ok(Workers::Auto),
        workers => Ok(workers),
    }
}

/// Parses `--max-line-length`: `1..=u32::MAX`, the range the Node.js binding accepts.
#[cfg(feature = "linter")]
fn parse_max_line_length(raw: &str) -> Result<NonZeroUsize, String> {
    let value = parse_number(raw)?;
    u32::try_from(value)
        .ok()
        .and_then(|_| NonZeroUsize::new(value))
        .ok_or_else(|| {
            range_error(LimitRangeError {
                value,
                min: 1,
                max: u32::MAX as usize,
            })
        })
}

fn parse_max_depth(raw: &str) -> Result<MaxDepth, String> {
    MaxDepth::new(parse_number(raw)?).map_err(range_error)
}

fn parse_max_documents(raw: &str) -> Result<MaxDocuments, String> {
    MaxDocuments::new(parse_number(raw)?).map_err(range_error)
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

fn parse_max_scan_ahead(raw: &str) -> Result<MaxScanAhead, String> {
    MaxScanAhead::new(parse_byte_size(raw)?).map_err(range_error)
}

fn parse_max_input_bytes(raw: &str) -> Result<MaxInputBytes, String> {
    MaxInputBytes::new(parse_byte_size(raw)?).map_err(range_error)
}

/// Arguments of `fy format`.
#[derive(Args, Debug)]
pub struct FormatArgs {
    /// Input paths (files, directories, or glob patterns).
    /// A missing path, a glob matching nothing or an explicit non-YAML file in batch mode is
    /// an error. `[` is literal unless the pattern also has `*` or `?` (write `[[]` for it then).
    /// If empty and no --stdin-files, reads from stdin
    #[arg(value_name = "PATHS")]
    pub paths: Vec<PathBuf>,

    /// Indentation width (1-9 spaces)
    #[arg(long, value_name = "N", value_parser = parse_indent, default_value_t = Indent::DEFAULT)]
    pub indent: Indent,

    /// Maximum line width (min: 20, max: 1000)
    #[arg(long, value_name = "N", value_parser = parse_width, default_value_t = Width::DEFAULT)]
    pub width: Width,

    /// Maximum nesting depth of sequences and mappings (min: 1, max: 512); flow collections stop at 255
    #[arg(long, value_name = "N", value_parser = parse_max_depth, default_value_t = MaxDepth::DEFAULT)]
    pub max_depth: MaxDepth,

    /// Maximum documents per input stream (min: 1, max: 10000000)
    #[arg(long, value_name = "N", value_parser = parse_max_documents, default_value_t = MaxDocuments::DEFAULT)]
    pub max_documents: MaxDocuments,

    /// Read file paths from stdin (one per line). A missing path, a directory, a non-YAML
    /// file or a line over 4096 bytes is an error, so filter git output:
    /// `git diff --name-only --diff-filter=d -- '*.yaml' '*.yml' | fy format --stdin-files`
    #[arg(long, conflicts_with = "paths")]
    pub stdin_files: bool,

    #[command(flatten)]
    pub batch: BatchArgs,

    /// Never write any file; only print a summary of what would change.
    /// Works for stdin too. Exits with code 5 if any file would change,
    /// 1 if any file failed (takes precedence), 0 otherwise
    #[arg(short = 'n', long, conflicts_with = "output")]
    pub dry_run: bool,

    #[command(flatten)]
    pub write: WriteArgs,

    /// Suppress the error when YAML comments are detected.
    /// Comments are not preserved by the formatter and will be stripped.
    /// Without this flag, formatting a file that contains comments exits with an error.
    #[arg(long)]
    pub strip_comments: bool,
}

#[cfg(feature = "linter")]
/// Arguments of `fy lint`.
#[derive(Args, Debug)]
pub struct LintFlags {
    /// Input paths (files, directories, or glob patterns).
    /// A missing path, a glob matching nothing or an explicit non-YAML file in batch mode is
    /// an error. `[` is literal unless the pattern also has `*` or `?` (write `[[]` for it then).
    /// If empty and no --stdin-files, reads from stdin.
    #[arg(value_name = "PATHS")]
    pub paths: Vec<PathBuf>,

    /// Read file paths from stdin (one per line). A missing path, a directory, a non-YAML
    /// file or a line over 4096 bytes is an error, so filter git output:
    /// `git diff --name-only --diff-filter=d -- '*.yaml' '*.yml' | fy lint --stdin-files`
    #[arg(long, conflicts_with = "paths")]
    pub stdin_files: bool,

    #[command(flatten)]
    pub config: ConfigArgs,

    /// Maximum line length (overrides config file)
    #[arg(long, value_name = "N", value_parser = parse_max_line_length)]
    pub max_line_length: Option<NonZeroUsize>,

    /// Indentation size (overrides config file)
    #[arg(long)]
    pub indent_size: Option<IndentSize>,

    /// Lint output format
    #[arg(long, value_enum, default_value = "text")]
    pub format: LintFormat,

    /// Allow duplicate keys — overrides config file (opt-in, suppresses duplicate key errors)
    #[arg(long, num_args = 0..=1, default_missing_value = "true", action = clap::ArgAction::Set)]
    pub allow_duplicate_keys: Option<bool>,

    /// Show at most N diagnostics per file, then one summary line; output only, the exit
    /// code is unaffected (overrides the `max-diagnostics` config key)
    #[arg(long, value_name = "N")]
    pub max_diagnostics: Option<MaxDiagnostics>,

    #[command(flatten)]
    pub batch: BatchArgs,

    #[command(flatten)]
    pub output: OutputArgs,

    #[command(flatten)]
    pub limits: ParseLimitArgs,
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
    Format(FormatArgs),

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
        write: WriteArgs,

        #[command(flatten)]
        limits: ParseLimitArgs,
    },

    #[cfg(feature = "linter")]
    /// Lint YAML with diagnostics
    ///
    /// Diagnostics can be suppressed inline with `# fy: disable [rules]`, `# fy: enable [rules]`,
    /// `# fy: disable-line [rules]` and `# fy: disable-file` (`# yamllint ...` is also accepted).
    Lint(LintFlags),
}

#[derive(ValueEnum, Clone, Debug)]
pub enum ConvertFormat {
    Yaml,
    Json,
}

#[cfg(feature = "linter")]
#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum LintFormat {
    /// Human-readable diagnostics with source context
    Text,
    /// JSON array of diagnostics
    Json,
    /// GitHub Actions workflow commands (inline annotations; GitHub caps them per step)
    Github,
    /// SARIF 2.1.0 log for code scanning
    Sarif,
    /// One `path:line:col: [level] message (code)` line per diagnostic
    Parsable,
}

/// How a [`LintFormat`] is rendered: a classic single-stream format or a CI report.
#[cfg(feature = "linter")]
#[derive(Clone, Copy, Debug)]
pub enum LintOutput {
    /// Human-readable diagnostics for one stream.
    Text,
    /// A JSON array of diagnostics.
    Json,
    /// A CI report that names each file by absolute path.
    Report(fast_yaml_linter::formatter::ReportFormat),
}

#[cfg(feature = "linter")]
impl LintFormat {
    /// Classifies this format by how it is rendered.
    pub const fn output(self) -> LintOutput {
        use fast_yaml_linter::formatter::ReportFormat;
        match self {
            Self::Text => LintOutput::Text,
            Self::Json => LintOutput::Json,
            Self::Github => LintOutput::Report(ReportFormat::Github),
            Self::Sarif => LintOutput::Report(ReportFormat::Sarif),
            Self::Parsable => LintOutput::Report(ReportFormat::Parsable),
        }
    }
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
    fn scan_ahead_accepts_suffixes_and_rejects_out_of_range() {
        assert_eq!(parse_max_scan_ahead("512").unwrap().get(), 512);
        assert_eq!(parse_max_scan_ahead("8MiB").unwrap().get(), 8 << 20);
        assert_eq!(parse_max_scan_ahead("1GiB").unwrap(), MaxScanAhead::MAX);
        assert!(parse_max_scan_ahead("0").is_err());
        assert!(parse_max_scan_ahead("2GiB").is_err());
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
    fn jobs_zero_is_auto_and_bounds_are_enforced() {
        assert_eq!(parse_jobs("0").unwrap(), Workers::Auto);
        assert!(matches!(parse_jobs("128").unwrap(), Workers::Fixed(n) if n.get() == 128));
        assert_eq!(
            parse_jobs("129").unwrap_err(),
            "must be between 0 and 128, got 129"
        );
        assert!(parse_jobs("-1").is_err());
    }

    #[cfg(feature = "linter")]
    #[test]
    fn max_line_length_names_its_range() {
        assert_eq!(parse_max_line_length("1").unwrap().get(), 1);
        assert_eq!(
            parse_max_line_length("4294967295").unwrap().get(),
            u32::MAX as usize
        );
        assert_eq!(
            parse_max_line_length("0").unwrap_err(),
            "must be between 1 and 4294967295, got 0"
        );
        assert_eq!(
            parse_max_line_length("4294967296").unwrap_err(),
            "must be between 1 and 4294967295, got 4294967296"
        );
        assert!(parse_max_line_length("x").is_err());
    }

    #[test]
    fn depth_rejects_out_of_range() {
        assert_eq!(parse_max_depth("512").unwrap(), MaxDepth::MAX);
        assert!(parse_max_depth("0").is_err());
        assert!(parse_max_depth("513").is_err());
        assert!(parse_max_depth("x").is_err());
    }
}
