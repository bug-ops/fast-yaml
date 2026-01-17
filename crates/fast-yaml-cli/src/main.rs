//! Fast YAML CLI tool (`fy`) for parsing, validating, formatting, and converting YAML files.
//!
//! # Usage
//!
//! ```bash
//! # Parse and validate YAML
//! fy parse config.yaml
//!
//! # Format YAML with consistent style
//! fy format --indent 4 messy.yaml
//!
//! # Convert YAML to JSON
//! fy convert json config.yaml
//!
//! # Convert JSON to YAML
//! fy convert yaml data.json
//! ```
//!
//! # Features
//!
//! - Fast YAML parsing and validation
//! - Consistent formatting with customizable indentation
//! - Bidirectional YAML/JSON conversion
//! - Optional linting with diagnostics (requires `linter` feature)
//! - Colored output support (requires `colors` feature)

// Forbid panic/unwrap in production code - use proper error handling instead
// These lints are allowed in test code via cfg_attr
#![cfg_attr(not(test), deny(clippy::unwrap_used))]
#![cfg_attr(not(test), deny(clippy::expect_used))]
#![cfg_attr(not(test), deny(clippy::panic))]
#![allow(clippy::too_many_lines)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::cast_possible_truncation)]

use anyhow::Result;
use clap::Parser;

mod batch;
mod cli;
mod commands;
mod config;
mod error;
mod io;
mod reporter;

use cli::{Cli, Command};
use error::{ExitCode, format_error};
use io::{InputSource, OutputWriter};

fn main() {
    let exit_code = match run() {
        Ok(code) => code,
        Err(err) => {
            // Use OutputConfig to determine color usage
            let cli = Cli::parse();
            let output_config =
                config::OutputConfig::from_cli(cli.quiet, cli.verbose, cli.no_color);
            eprintln!("{}", format_error(&err, output_config.use_color()));
            ExitCode::ParseError
        }
    };

    std::process::exit(exit_code.as_i32());
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();

    // Create common config early to avoid borrow issues
    let common_config = config::CommonConfig::from_cli(&cli);

    // Execute command
    let exit_code = match cli.command {
        Some(Command::Parse { file, stats }) => {
            let input = InputSource::from_args(file)?;
            let cmd = commands::parse::ParseCommand::new(common_config, stats);
            cmd.execute(&input)?;
            ExitCode::Success
        }
        Some(Command::Format {
            paths,
            indent,
            width,
            jobs,
            stdin_files,
            include,
            exclude,
            no_recursive,
            dry_run,
        }) => {
            // Determine if this is batch mode
            let is_batch = is_batch_mode(&paths, stdin_files, &include, &exclude, jobs);

            if is_batch {
                // BATCH MODE - using composed BatchConfig
                let mut discovery_config = batch::DiscoveryConfig::new();

                // Apply include patterns if provided
                if !include.is_empty() {
                    discovery_config = discovery_config.with_include_patterns(include);
                }

                // Apply exclude patterns if provided
                if !exclude.is_empty() {
                    discovery_config = discovery_config.with_exclude_patterns(exclude);
                }

                // Set recursion depth
                if no_recursive {
                    discovery_config = discovery_config.with_max_depth(Some(1));
                }

                // Build batch config from common config
                let batch_config = commands::format_batch::BatchConfig::new(
                    common_config
                        .clone()
                        .with_formatter(
                            config::FormatterConfig::new()
                                .with_indent(indent)
                                .with_width(width),
                        )
                        .with_parallel(config::ParallelConfig::new().with_workers(jobs)),
                )
                .with_discovery(discovery_config)
                .with_dry_run(dry_run)
                .with_in_place(cli.in_place);

                commands::format_batch::execute_batch(&batch_config, &paths, stdin_files)?
            } else if paths.is_empty() {
                // STDIN MODE - backward compatible
                if cli.in_place {
                    anyhow::bail!("--in-place (-i) requires a file argument");
                }
                let input = InputSource::from_stdin()?;
                let output = OutputWriter::from_args(cli.output.clone(), false, None)?;
                let format_config = common_config.clone().with_formatter(
                    config::FormatterConfig::new()
                        .with_indent(indent)
                        .with_width(width),
                );
                let cmd = commands::format::FormatCommand::new(format_config);
                cmd.execute(&input, &output)?;
                ExitCode::Success
            } else {
                // SINGLE FILE MODE - backward compatible
                let file_path = &paths[0];
                let input = InputSource::from_file(file_path)?;
                let output =
                    OutputWriter::from_args(cli.output.clone(), cli.in_place, Some(file_path))?;
                let format_config = common_config.clone().with_formatter(
                    config::FormatterConfig::new()
                        .with_indent(indent)
                        .with_width(width),
                );
                let cmd = commands::format::FormatCommand::new(format_config);
                cmd.execute(&input, &output)?;
                ExitCode::Success
            }
        }
        Some(Command::Convert { to, file, pretty }) => {
            let input = InputSource::from_args(file)?;
            let output =
                OutputWriter::from_args(cli.output.clone(), cli.in_place, input.file_path())?;
            let cmd = commands::convert::ConvertCommand::new(common_config, to, pretty);
            cmd.execute(&input, &output)?;
            ExitCode::Success
        }
        #[cfg(feature = "linter")]
        Some(Command::Lint {
            file,
            max_line_length,
            indent_size,
            format,
        }) => {
            let input = InputSource::from_args(file)?;
            let lint_config = common_config
                .clone()
                .with_formatter(config::FormatterConfig::new().with_indent(indent_size as u8));
            let cmd = commands::lint::LintCommand::new(lint_config, max_line_length, format);
            cmd.execute(&input)?
        }
        None => {
            // Default: parse and format (passthrough) from stdin
            let input = InputSource::from_stdin()?;
            let output = OutputWriter::from_args(cli.output.clone(), false, None)?;
            let format_config = common_config
                .clone()
                .with_formatter(config::FormatterConfig::new().with_indent(2).with_width(80));
            let cmd = commands::format::FormatCommand::new(format_config);
            cmd.execute(&input, &output)?;
            ExitCode::Success
        }
    };

    Ok(exit_code)
}

/// Determines if format command should use batch mode.
fn is_batch_mode(
    paths: &[std::path::PathBuf],
    stdin_files: bool,
    include: &[String],
    exclude: &[String],
    jobs: usize,
) -> bool {
    // stdin-files flag explicitly requests batch mode
    if stdin_files {
        return true;
    }

    // Multiple paths = batch mode
    if paths.len() > 1 {
        return true;
    }

    // Single path that is a directory or glob = batch mode
    if paths.len() == 1 && is_batch_path(&paths[0]) {
        return true;
    }

    // Include/exclude patterns = batch mode
    if !include.is_empty() || !exclude.is_empty() {
        return true;
    }

    // Explicit job count > 0 suggests batch mode
    if jobs > 0 {
        return true;
    }

    false
}

/// Determines if a path should trigger batch mode.
fn is_batch_path(path: &std::path::Path) -> bool {
    path.is_dir() || contains_glob_chars(&path.to_string_lossy())
}

/// Checks if path contains glob special characters.
fn contains_glob_chars(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[')
}
