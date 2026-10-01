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
#![warn(dead_code)]

use anyhow::Result;
#[cfg(feature = "linter")]
use fast_yaml_cli::file_filter;
use fast_yaml_cli::{discovery, error};
use fast_yaml_core::limits::{MaxInputBytes, MaxScanAhead};
use fast_yaml_parallel::{CommentPolicy, ScanAheadPolicy};

mod cli;
mod commands;
mod config;
mod invocation;
mod io;
mod logging;
mod reporter;

#[cfg(feature = "linter")]
use cli::LintFlags;
use cli::{Cli, Command, FormatArgs, ResolvedCli};
use commands::format::{EditIntent, FormatCommand, WriteMode};
use commands::format_batch::BatchWrite;
use config::{CommonConfig, FormatterConfig};
use error::{ExitCode, format_error};
use invocation::Target;
use io::{InputSource, OutputTarget, OutputWriter, WriteTarget};

/// Stack size of the command thread; deep values recurse per level (larger frames with
/// `preserve_order`) and would overflow the 1 MiB Windows main-thread stack.
const COMMAND_STACK_SIZE: usize = 8 * 1024 * 1024;

/// Exit status of a panicked command thread, matching the Rust default panic exit.
const PANIC_EXIT_STATUS: i32 = 101;

fn main() {
    logging::init();
    let status = match std::thread::Builder::new()
        .name("fy-main".into())
        .stack_size(COMMAND_STACK_SIZE)
        .spawn(run_reporting_errors)
    {
        Ok(handle) => handle.join().unwrap_or(PANIC_EXIT_STATUS),
        Err(err) => {
            error::stderr_line(format_args!("error: failed to start command thread: {err}"));
            ExitCode::ParseError.as_i32()
        }
    };

    std::process::exit(status);
}

fn run_reporting_errors() -> i32 {
    let cli = Cli::parse_validated();
    let output_config = config::OutputConfig::from_cli(cli.verbosity, cli.no_color);
    let exit_code = match run(cli) {
        Ok(code) => code,
        Err(err) => {
            error::stderr_line(format_args!(
                "{}",
                format_error(&err, output_config.use_color())
            ));
            ExitCode::ParseError
        }
    };
    exit_code.as_i32()
}

/// The global flags every command reads, resolved once.
struct Context {
    common: CommonConfig,
    max_input: MaxInputBytes,
    max_scan_ahead: MaxScanAhead,
    #[cfg(feature = "linter")]
    explicit_max_input: Option<MaxInputBytes>,
    explicit_max_scan_ahead: Option<MaxScanAhead>,
}

fn run(cli: ResolvedCli) -> Result<ExitCode> {
    let ctx = Context {
        common: CommonConfig::from_cli(&cli),
        max_input: cli.max_input(),
        max_scan_ahead: cli.scan_ahead(),
        #[cfg(feature = "linter")]
        explicit_max_input: cli.max_input_bytes,
        explicit_max_scan_ahead: cli.max_scan_ahead,
    };

    match cli.command {
        Some(Command::Parse {
            file,
            stats,
            limits,
        }) => {
            let input = InputSource::from_args(file, ctx.max_input)?;
            let cmd = commands::parse::ParseCommand::new(
                ctx.common,
                stats,
                limits.parse_limits(ctx.max_scan_ahead),
            );
            cmd.execute(&input)?;
            Ok(ExitCode::Success)
        }
        Some(Command::Format(args)) => run_format(ctx, args),
        Some(Command::Convert {
            to,
            file,
            pretty,
            write,
            limits,
        }) => {
            let input = InputSource::from_args(file, ctx.max_input)?;
            let output = OutputWriter::for_write(write.target(), input.file_path())?;
            let cmd = commands::convert::ConvertCommand::new(
                to,
                pretty,
                limits.parse_limits(ctx.max_scan_ahead),
            );
            cmd.execute(&input, &output)?;
            Ok(ExitCode::Success)
        }
        #[cfg(feature = "linter")]
        Some(Command::Lint(flags)) => run_lint(ctx, flags),
        None => {
            let mode =
                stdin_write_mode(EditIntent::Write(WriteTarget::Output(OutputTarget::Stdout)))?;
            FormatCommand::new(ctx.common, CommentPolicy::Reject)
                .run(&InputSource::from_stdin(ctx.max_input)?, &mode)
        }
    }
}

fn run_format(ctx: Context, args: FormatArgs) -> Result<ExitCode> {
    let comments = if args.strip_comments {
        CommentPolicy::Strip
    } else {
        CommentPolicy::Reject
    };
    let intent = EditIntent::resolve(args.dry_run, args.write.target());
    let common = ctx.common.with_formatter(
        FormatterConfig::new()
            .with_indent(args.indent)
            .with_width(args.width)
            .with_max_depth(args.max_depth)
            .with_max_documents(args.max_documents)
            .with_max_scan_ahead(ctx.max_scan_ahead),
    );

    match Target::resolve(args.paths, args.stdin_files, &args.batch)? {
        Target::Stdin => {
            let mode = stdin_write_mode(intent)?;
            FormatCommand::new(common, comments)
                .run(&InputSource::from_stdin(ctx.max_input)?, &mode)
        }
        Target::File(path) => {
            let input = InputSource::from_file(&path, ctx.max_input)?;
            let mode = WriteMode::new(intent, Some(&path))?;
            FormatCommand::new(common, comments).run(&input, &mode)
        }
        Target::Batch(target) => {
            let write = match intent {
                EditIntent::Preview => BatchWrite::DryRun,
                EditIntent::Write(WriteTarget::InPlace) => BatchWrite::InPlace,
                EditIntent::Write(WriteTarget::Output(_)) => {
                    anyhow::bail!("use -i to format files in-place or --dry-run to preview changes")
                }
            };
            let scan_ahead = ScanAheadPolicy::from(ctx.explicit_max_scan_ahead);
            commands::format_batch::execute_batch(
                &common,
                &target,
                write,
                comments,
                ctx.max_input,
                scan_ahead,
            )
        }
    }
}

#[cfg(feature = "linter")]
fn run_lint(ctx: Context, flags: LintFlags) -> Result<ExitCode> {
    let format = flags.format;
    let output = flags.output.target();
    let target = Target::resolve(flags.paths, flags.stdin_files, &flags.batch)
        .map_err(|err| commands::lint::report_unresolved(format, output.clone(), err))?;
    let args = commands::lint::LintArgs {
        config: flags.config.source(),
        max_line_length: flags.max_line_length,
        indent_size: flags.indent_size,
        format,
        allow_duplicate_keys: flags.allow_duplicate_keys,
        max_diagnostics: flags.max_diagnostics,
        max_input_bytes: ctx.explicit_max_input,
        max_scan_ahead: ctx.explicit_max_scan_ahead,
        limits: flags.limits,
        output,
    };
    let common = ctx.common;

    match target {
        Target::Stdin => {
            let cmd =
                commands::lint::LintCommand::build(common, args, &io::input::InputOrigin::Stdin)?;
            let input = InputSource::from_stdin(cmd.lint_config.max_input_bytes)
                .map_err(|err| cmd.report_unreadable(None, err))?;
            cmd.execute(&input)
        }
        Target::File(path) => {
            // Config first: an ignored file must not be read.
            let cmd = commands::lint::LintCommand::build(
                common,
                args,
                &io::input::InputOrigin::File(path.clone()),
            )?;
            if cmd.is_ignored(&path) {
                cmd.execute_ignored()
            } else {
                let input = InputSource::from_file(&path, cmd.lint_config.max_input_bytes)
                    .map_err(|err| cmd.report_unreadable(Some(&path), err))?;
                cmd.execute(&input)
            }
        }
        Target::Batch(mut target) => {
            // Config discovery is CWD-based, same as yamllint.
            let cmd = commands::lint::LintCommand::build(
                common.clone(),
                args,
                &io::input::InputOrigin::Stdin,
            )?;
            target.discovery.file_filter = cmd.file_filter.clone();
            if target.discovery.include == discovery::IncludePatterns::Default {
                target.discovery.include = discovery::IncludePatterns::DefaultWithYamllint;
            }
            commands::lint_batch::execute_lint_batch(
                &common,
                &target,
                &cmd.lint_config,
                format,
                cmd.scan_ahead,
                &cmd.output,
                cmd.max_diagnostics,
            )
        }
    }
}

/// Resolves the write mode for stdin input, which has no file to edit in place.
fn stdin_write_mode(intent: EditIntent) -> Result<WriteMode> {
    if intent == EditIntent::Write(WriteTarget::InPlace) {
        anyhow::bail!("--in-place (-i) requires a file argument");
    }
    WriteMode::new(intent, None)
}
