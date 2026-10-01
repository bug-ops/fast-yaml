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
#![warn(dead_code)]

use anyhow::Result;
#[cfg(feature = "linter")]
use fast_yaml_cli::file_filter;
use fast_yaml_cli::{discovery, error};
use fast_yaml_parallel::{CommentPolicy, ScanAheadPolicy};

mod cli;
mod commands;
mod config;
mod invocation;
mod io;
mod reporter;

use cli::{Cli, Command};
use commands::format::{EditIntent, FormatCommand, WriteMode};
use commands::format_batch::BatchWrite;
use config::{CommonConfig, FormatterConfig};
use error::{ExitCode, format_error};
use invocation::Target;
use io::{InputSource, OutputWriter};

fn main() {
    let exit_code = match run() {
        Ok(code) => code,
        Err(err) => {
            // Use OutputConfig to determine color usage
            let cli = Cli::parse_validated();
            let output_config = config::OutputConfig::from_cli(cli.verbosity, cli.no_color);
            error::stderr_line(format_args!(
                "{}",
                format_error(&err, output_config.use_color())
            ));
            ExitCode::ParseError
        }
    };

    std::process::exit(exit_code.as_i32());
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse_validated();

    let common_config = CommonConfig::from_cli(&cli);
    let max_input = cli.max_input();
    let max_scan_ahead = cli.scan_ahead();

    let exit_code = match cli.command {
        Some(Command::Parse {
            file,
            stats,
            limits,
        }) => {
            let input = InputSource::from_args(file, max_input)?;
            let cmd = commands::parse::ParseCommand::new(
                common_config,
                stats,
                limits.parse_limits(max_scan_ahead),
            );
            cmd.execute(&input)?;
            ExitCode::Success
        }
        Some(Command::Format {
            paths,
            indent,
            width,
            max_depth,
            max_documents,
            stdin_files,
            batch,
            dry_run,
            strip_comments,
        }) => {
            let comments = if strip_comments {
                CommentPolicy::Strip
            } else {
                CommentPolicy::Reject
            };
            let intent = EditIntent::from_flags(dry_run, cli.in_place);
            let common = common_config.with_formatter(
                FormatterConfig::new()
                    .with_indent(indent)
                    .with_width(width)
                    .with_max_depth(max_depth)
                    .with_max_documents(max_documents)
                    .with_max_scan_ahead(max_scan_ahead),
            );

            match Target::resolve(paths, stdin_files, &batch)? {
                Target::Stdin => {
                    let mode = stdin_write_mode(intent, cli.output)?;
                    FormatCommand::new(common, comments)
                        .run(&InputSource::from_stdin(max_input)?, &mode)?
                }
                Target::File(path) => {
                    let input = InputSource::from_file(&path, max_input)?;
                    let mode = WriteMode::new(intent, cli.output, Some(&path))?;
                    FormatCommand::new(common, comments).run(&input, &mode)?
                }
                Target::Batch(target) => {
                    let write = match intent {
                        EditIntent::Preview => BatchWrite::DryRun,
                        EditIntent::InPlace => BatchWrite::InPlace,
                        EditIntent::Print => anyhow::bail!(
                            "use -i to format files in-place or --dry-run to preview changes"
                        ),
                    };
                    let scan_ahead = ScanAheadPolicy::from(cli.max_scan_ahead);
                    commands::format_batch::execute_batch(
                        &common, &target, write, comments, max_input, scan_ahead,
                    )?
                }
            }
        }
        Some(Command::Convert {
            to,
            file,
            pretty,
            limits,
        }) => {
            let input = InputSource::from_args(file, max_input)?;
            let output =
                OutputWriter::from_args(cli.output.clone(), cli.in_place, input.file_path())?;
            let cmd = commands::convert::ConvertCommand::new(
                to,
                pretty,
                limits.parse_limits(max_scan_ahead),
            );
            cmd.execute(&input, &output)?;
            ExitCode::Success
        }
        #[cfg(feature = "linter")]
        Some(Command::Lint {
            paths,
            stdin_files,
            config: config_path,
            no_config,
            max_line_length,
            indent_size,
            format,
            allow_duplicate_keys,
            batch,
            limits,
        }) => {
            if cli.in_place {
                anyhow::bail!(
                    "--in-place is not supported by `fy lint` (auto-fix is not implemented)"
                );
            }
            let target = Target::resolve(paths, stdin_files, &batch).map_err(|err| {
                commands::lint::report_unresolved(format, cli.output.clone(), err)
            })?;
            let args = commands::lint::LintArgs {
                config_path,
                no_config,
                max_line_length,
                indent_size,
                format,
                allow_duplicate_keys,
                max_input_bytes: cli.max_input_bytes,
                max_scan_ahead: cli.max_scan_ahead,
                limits,
                output: cli.output.clone(),
            };

            match target {
                Target::Stdin => {
                    let placeholder = empty_input(io::input::InputOrigin::Stdin);
                    let cmd =
                        commands::lint::LintCommand::build(common_config, args, &placeholder)?;
                    let input = InputSource::from_stdin(cmd.lint_config.max_input_bytes)
                        .map_err(|err| cmd.report_unreadable(None, err))?;
                    cmd.execute(&input)?
                }
                Target::File(path) => {
                    // Config first: an ignored file must not be read.
                    let placeholder = empty_input(io::input::InputOrigin::File(path.clone()));
                    let cmd =
                        commands::lint::LintCommand::build(common_config, args, &placeholder)?;
                    if cmd.is_ignored(&path) {
                        cmd.execute_ignored()?
                    } else {
                        let input = InputSource::from_file(&path, cmd.lint_config.max_input_bytes)
                            .map_err(|err| cmd.report_unreadable(Some(&path), err))?;
                        cmd.execute(&input)?
                    }
                }
                Target::Batch(mut target) => {
                    // Synthetic stdin input: config discovery is CWD-based, same as yamllint.
                    let stdin_fallback = empty_input(io::input::InputOrigin::Stdin);
                    let format = args.format;
                    let cmd = commands::lint::LintCommand::build(
                        common_config.clone(),
                        args,
                        &stdin_fallback,
                    )?;
                    target.discovery.file_filter = cmd.file_filter.clone();
                    if target.discovery.include == discovery::IncludePatterns::Default {
                        target.discovery.include = discovery::IncludePatterns::DefaultWithYamllint;
                    }
                    commands::lint_batch::execute_lint_batch(
                        &common_config,
                        &target,
                        &cmd.lint_config,
                        format,
                        cmd.scan_ahead,
                        &cmd.output,
                    )?
                }
            }
        }
        None => {
            let mode = stdin_write_mode(EditIntent::from_flags(false, cli.in_place), cli.output)?;
            FormatCommand::new(common_config, CommentPolicy::Reject)
                .run(&InputSource::from_stdin(max_input)?, &mode)?
        }
    };

    Ok(exit_code)
}

/// Resolves the write mode for stdin input, which has no file to edit in place.
fn stdin_write_mode(intent: EditIntent, output: Option<std::path::PathBuf>) -> Result<WriteMode> {
    if intent == EditIntent::InPlace {
        anyhow::bail!("--in-place (-i) requires a file argument");
    }
    WriteMode::new(intent, output, None)
}

/// Content-free input that only tells config discovery where the run started.
#[cfg(feature = "linter")]
const fn empty_input(origin: io::input::InputOrigin) -> InputSource {
    InputSource {
        content: String::new(),
        origin,
    }
}
