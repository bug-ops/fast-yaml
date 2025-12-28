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

use anyhow::Result;
use clap::Parser;

mod cli;
mod commands;
mod error;
mod io;

use cli::{Cli, Command};
use error::{format_error, ExitCode};
use io::{InputSource, OutputWriter};

fn main() {
    let exit_code = match run() {
        Ok(code) => code,
        Err(err) => {
            let use_color = should_use_color();
            eprintln!("{}", format_error(&err, use_color));
            ExitCode::ParseError
        }
    };

    std::process::exit(exit_code.as_i32());
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();

    // Determine color usage
    let use_color = !cli.no_color && should_use_color();

    // Read input
    let input = InputSource::from_args(cli.file.clone())?;

    // Create output writer
    let output = OutputWriter::from_args(cli.output.clone(), cli.in_place, input.file_path())?;

    // Execute command
    let exit_code = match cli.command {
        Some(Command::Parse { stats }) => {
            let cmd = commands::parse::ParseCommand::new(stats, use_color, cli.quiet);
            cmd.execute(&input)?;
            ExitCode::Success
        }
        Some(Command::Format { indent, width }) => {
            let cmd = commands::format::FormatCommand::new(indent, width);
            cmd.execute(&input, &output)?;
            ExitCode::Success
        }
        Some(Command::Convert { to, pretty }) => {
            let cmd = commands::convert::ConvertCommand::new(to, pretty);
            cmd.execute(&input, &output)?;
            ExitCode::Success
        }
        #[cfg(feature = "linter")]
        Some(Command::Lint {
            max_line_length: _,
            indent_size: _,
            format: _,
        }) => {
            // Lint command placeholder - will be implemented in Phase 2
            eprintln!("Lint command not yet implemented");
            ExitCode::Success
        }
        None => {
            // Default: parse and format (passthrough)
            let cmd = commands::format::FormatCommand::new(2, 80);
            cmd.execute(&input, &output)?;
            ExitCode::Success
        }
    };

    Ok(exit_code)
}

/// Determine if colored output should be used
fn should_use_color() -> bool {
    // Respect NO_COLOR environment variable
    if std::env::var("NO_COLOR").is_ok() {
        return false;
    }

    #[cfg(feature = "colors")]
    {
        use is_terminal::IsTerminal;
        // Check if stdout is a terminal
        std::io::stdout().is_terminal()
    }

    #[cfg(not(feature = "colors"))]
    false
}
