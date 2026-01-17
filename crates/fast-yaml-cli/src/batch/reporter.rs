//! Result reporting for batch file processing.
//!
//! This module provides formatted output for batch processing results,
//! with optional color support and quiet/verbose modes.

use std::io::{self, Write};
use std::path::PathBuf;

use super::error::ProcessingError;
use super::result::BatchResult;

/// Configuration for the batch result reporter.
#[derive(Debug, Clone, Default)]
pub struct ReporterConfig {
    use_color: bool,
    quiet: bool,
    verbose: bool,
}

impl ReporterConfig {
    /// Creates a new `ReporterConfig` with the specified flags.
    pub const fn new(use_color: bool, quiet: bool, verbose: bool) -> Self {
        Self {
            use_color,
            quiet,
            verbose,
        }
    }

    /// Returns whether color output is enabled.
    pub const fn use_color(&self) -> bool {
        self.use_color
    }

    /// Returns whether quiet mode is enabled.
    pub const fn is_quiet(&self) -> bool {
        self.quiet
    }

    /// Returns whether verbose mode is enabled.
    pub const fn is_verbose(&self) -> bool {
        self.verbose
    }
}

/// Reports batch processing results to stderr.
pub struct BatchReporter {
    config: ReporterConfig,
}

impl BatchReporter {
    /// Creates a new `BatchReporter` with the given configuration.
    pub const fn new(config: ReporterConfig) -> Self {
        Self { config }
    }

    /// Creates a new `BatchReporter` from individual flags.
    pub const fn from_flags(use_color: bool, quiet: bool, verbose: bool) -> Self {
        Self::new(ReporterConfig::new(use_color, quiet, verbose))
    }

    /// Reports batch results to stderr.
    ///
    /// In quiet mode, output is suppressed unless there are errors.
    pub fn report(&self, result: &BatchResult) -> io::Result<()> {
        // Quiet mode: skip output if no errors
        if self.config.is_quiet() && result.errors.is_empty() {
            return Ok(());
        }

        // Lock stderr once for all output
        let mut stderr = io::stderr().lock();

        // Write errors first
        if !result.errors.is_empty() {
            self.write_errors(&mut stderr, &result.errors)?;
        }

        // Write summary unless in quiet mode (errors already printed)
        if !self.config.is_quiet() {
            self.write_summary(&mut stderr, result)?;
        }

        Ok(())
    }

    /// Writes error messages to the output.
    fn write_errors<W: Write>(
        &self,
        w: &mut W,
        errors: &[(PathBuf, ProcessingError)],
    ) -> io::Result<()> {
        for (path, error) in errors {
            #[cfg(feature = "colors")]
            if self.config.use_color() {
                use colored::Colorize;
                writeln!(w, "{} {}: {}", "error:".red().bold(), path.display(), error)?;
                continue;
            }

            writeln!(w, "error: {}: {}", path.display(), error)?;
        }
        Ok(())
    }

    /// Writes the summary line to the output.
    fn write_summary<W: Write>(&self, w: &mut W, result: &BatchResult) -> io::Result<()> {
        let duration_ms = result.duration.as_millis();

        #[cfg(feature = "colors")]
        if self.config.use_color() {
            return Self::write_summary_colored(w, result, duration_ms);
        }

        Self::write_summary_plain(w, result, duration_ms)
    }

    /// Writes a plain (no color) summary line.
    fn write_summary_plain<W: Write>(
        w: &mut W,
        result: &BatchResult,
        duration_ms: u128,
    ) -> io::Result<()> {
        // Start with status and total
        if result.failed > 0 {
            write!(w, "Completed with errors: {} files", result.total)?;
        } else {
            write!(w, "Completed: {} files", result.total)?;
        }

        // Append non-zero counts
        if result.formatted > 0 {
            write!(w, ", {} formatted", result.formatted)?;
        }
        if result.unchanged > 0 {
            write!(w, ", {} unchanged", result.unchanged)?;
        }
        if result.skipped > 0 {
            write!(w, ", {} skipped", result.skipped)?;
        }
        if result.failed > 0 {
            write!(w, ", {} failed", result.failed)?;
        }

        // Append duration
        writeln!(w, " in {duration_ms}ms")?;
        Ok(())
    }

    /// Writes a colored summary line.
    #[cfg(feature = "colors")]
    fn write_summary_colored<W: Write>(
        w: &mut W,
        result: &BatchResult,
        duration_ms: u128,
    ) -> io::Result<()> {
        use colored::Colorize;

        // Start with status and total
        if result.failed > 0 {
            write!(
                w,
                "{} {} files",
                "Completed with errors:".yellow().bold(),
                result.total
            )?;
        } else {
            write!(w, "{} {} files", "Completed:".green().bold(), result.total)?;
        }

        // Append non-zero counts
        if result.formatted > 0 {
            write!(w, ", {} formatted", result.formatted.to_string().green())?;
        }
        if result.unchanged > 0 {
            write!(w, ", {} unchanged", result.unchanged)?;
        }
        if result.skipped > 0 {
            write!(w, ", {} skipped", result.skipped)?;
        }
        if result.failed > 0 {
            write!(w, ", {} failed", result.failed.to_string().red())?;
        }

        // Append duration
        writeln!(w, " in {duration_ms}ms")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Helper function to capture output to a buffer.
    fn capture_output<F>(f: F) -> String
    where
        F: FnOnce(&mut Vec<u8>) -> io::Result<()>,
    {
        let mut buffer = Vec::new();
        f(&mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
    }

    #[test]
    fn test_reporter_config_new() {
        let config = ReporterConfig::new(true, false, true);
        assert!(config.use_color());
        assert!(!config.is_quiet());
        assert!(config.is_verbose());
    }

    #[test]
    fn test_reporter_config_default() {
        let config = ReporterConfig::default();
        assert!(!config.use_color());
        assert!(!config.is_quiet());
        assert!(!config.is_verbose());
    }

    #[test]
    fn test_report_quiet_no_errors() {
        let reporter = BatchReporter::from_flags(false, true, false);
        let result = BatchResult {
            total: 50,
            formatted: 12,
            unchanged: 38,
            skipped: 0,
            failed: 0,
            duration: Duration::from_millis(45),
            errors: vec![],
        };

        let output = capture_output(|_buf| reporter.report(&result));
        assert_eq!(output, "");
    }

    #[test]
    fn test_report_quiet_with_errors() {
        let reporter = BatchReporter::from_flags(false, true, false);
        let result = BatchResult {
            total: 50,
            formatted: 12,
            unchanged: 36,
            skipped: 0,
            failed: 2,
            duration: Duration::from_millis(67),
            errors: vec![
                (
                    PathBuf::from("config/invalid.yaml"),
                    ProcessingError::ParseError(
                        "expected ',' or ']' at line 5 column 1".to_string(),
                    ),
                ),
                (
                    PathBuf::from("data/broken.yaml"),
                    ProcessingError::ReadError(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "permission denied",
                    )),
                ),
            ],
        };

        let output = capture_output(|buf| {
            let mut stderr = buf;
            reporter.write_errors(&mut stderr, &result.errors)
        });
        assert!(output.contains("error: config/invalid.yaml:"));
        assert!(output.contains("expected ',' or ']' at line 5 column 1"));
        assert!(output.contains("error: data/broken.yaml:"));
        assert!(output.contains("permission denied"));
    }

    #[test]
    fn test_write_summary_plain_all_counts() {
        let result = BatchResult {
            total: 100,
            formatted: 25,
            unchanged: 50,
            skipped: 15,
            failed: 10,
            duration: Duration::from_millis(123),
            errors: vec![],
        };

        let output = capture_output(|buf| {
            let duration_ms = result.duration.as_millis();
            BatchReporter::write_summary_plain(buf, &result, duration_ms)
        });

        assert!(output.contains("Completed with errors: 100 files"));
        assert!(output.contains("25 formatted"));
        assert!(output.contains("50 unchanged"));
        assert!(output.contains("15 skipped"));
        assert!(output.contains("10 failed"));
        assert!(output.contains("in 123ms"));
    }

    #[test]
    fn test_write_summary_plain_zero_counts_omitted() {
        let result = BatchResult {
            total: 50,
            formatted: 12,
            unchanged: 38,
            skipped: 0,
            failed: 0,
            duration: Duration::from_millis(45),
            errors: vec![],
        };

        let output = capture_output(|buf| {
            let duration_ms = result.duration.as_millis();
            BatchReporter::write_summary_plain(buf, &result, duration_ms)
        });

        assert!(output.contains("Completed: 50 files"));
        assert!(output.contains("12 formatted"));
        assert!(output.contains("38 unchanged"));
        assert!(!output.contains("0 skipped"));
        assert!(!output.contains("0 failed"));
        assert!(output.contains("in 45ms"));
    }

    #[test]
    fn test_write_errors_format() {
        let reporter = BatchReporter::from_flags(false, false, false);
        let errors = vec![
            (
                PathBuf::from("test/file.yaml"),
                ProcessingError::ParseError("syntax error".to_string()),
            ),
            (
                PathBuf::from("data/doc.yaml"),
                ProcessingError::ReadError(io::Error::new(
                    io::ErrorKind::NotFound,
                    "file not found",
                )),
            ),
        ];

        let output = capture_output(|buf| reporter.write_errors(buf, &errors));

        assert!(output.contains("error: test/file.yaml: failed to parse YAML: syntax error"));
        assert!(output.contains("error: data/doc.yaml: failed to read file: file not found"));
    }

    #[test]
    #[cfg(feature = "colors")]
    fn test_write_summary_colored() {
        use colored::control;
        // Force colored output for testing
        control::set_override(true);

        let result = BatchResult {
            total: 50,
            formatted: 12,
            unchanged: 38,
            skipped: 0,
            failed: 0,
            duration: Duration::from_millis(45),
            errors: vec![],
        };

        let output = capture_output(|buf| {
            let duration_ms = result.duration.as_millis();
            BatchReporter::write_summary_colored(buf, &result, duration_ms)
        });

        // Restore default color behavior
        control::unset_override();

        // Check that colored output contains ANSI codes
        assert!(output.contains("\x1b["));
        assert!(output.contains("Completed:"));
        assert!(output.contains("50 files"));
        assert!(output.contains("formatted"));
        assert!(output.contains("38 unchanged"));
        assert!(output.contains("in 45ms"));
    }

    #[test]
    #[cfg(feature = "colors")]
    fn test_write_summary_colored_with_errors() {
        use colored::control;
        // Force colored output for testing
        control::set_override(true);

        let result = BatchResult {
            total: 50,
            formatted: 12,
            unchanged: 36,
            skipped: 0,
            failed: 2,
            duration: Duration::from_millis(67),
            errors: vec![],
        };

        let output = capture_output(|buf| {
            let duration_ms = result.duration.as_millis();
            BatchReporter::write_summary_colored(buf, &result, duration_ms)
        });

        // Restore default color behavior
        control::unset_override();

        // Check that colored output contains ANSI codes
        assert!(output.contains("\x1b["));
        assert!(output.contains("Completed with errors:"));
        assert!(output.contains("50 files"));
        assert!(output.contains("formatted"));
        assert!(output.contains("failed"));
    }

    #[test]
    fn test_write_summary_dry_run() {
        let result = BatchResult {
            total: 50,
            formatted: 0,
            unchanged: 38,
            skipped: 12,
            failed: 0,
            duration: Duration::from_millis(34),
            errors: vec![],
        };

        let output = capture_output(|buf| {
            let duration_ms = result.duration.as_millis();
            BatchReporter::write_summary_plain(buf, &result, duration_ms)
        });

        assert!(output.contains("Completed: 50 files"));
        assert!(output.contains("38 unchanged"));
        assert!(output.contains("12 skipped"));
        assert!(!output.contains("formatted"));
        assert!(!output.contains("failed"));
        assert!(output.contains("in 34ms"));
    }

    #[test]
    fn test_write_summary_only_unchanged() {
        let result = BatchResult {
            total: 50,
            formatted: 0,
            unchanged: 50,
            skipped: 0,
            failed: 0,
            duration: Duration::from_millis(23),
            errors: vec![],
        };

        let output = capture_output(|buf| {
            let duration_ms = result.duration.as_millis();
            BatchReporter::write_summary_plain(buf, &result, duration_ms)
        });

        assert!(output.contains("Completed: 50 files"));
        assert!(output.contains("50 unchanged"));
        assert!(!output.contains("formatted"));
        assert!(!output.contains("skipped"));
        assert!(!output.contains("failed"));
        assert!(output.contains("in 23ms"));
    }
}
