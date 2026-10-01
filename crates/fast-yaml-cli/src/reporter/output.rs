//! Reporter implementation for unified CLI output.

use super::events::{BatchStats, ReportEvent};
use crate::config::OutputConfig;
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

/// Formats a count with a correctly pluralized "file" noun.
fn file_count(total: usize) -> String {
    if total == 1 {
        "1 file".to_string()
    } else {
        format!("{total} files")
    }
}

/// Universal reporter that handles all CLI output.
///
/// Centralizes output formatting and color handling across all commands.
pub struct Reporter {
    config: OutputConfig,
    stdout: io::Stdout,
    stderr: io::Stderr,
    start_time: Option<Instant>,
}

impl Reporter {
    /// Creates a new reporter with the given output configuration.
    #[must_use]
    pub fn new(config: OutputConfig) -> Self {
        Self {
            config,
            stdout: io::stdout(),
            stderr: io::stderr(),
            start_time: None,
        }
    }

    /// Starts timing an operation.
    ///
    /// Call this at the beginning of an operation to enable timing reports.
    pub fn start_timing(&mut self) {
        self.start_time = Some(Instant::now());
    }

    /// Returns elapsed time since `start_timing()` was called.
    #[must_use]
    pub fn elapsed(&self) -> Option<Duration> {
        self.start_time.map(|start| start.elapsed())
    }

    /// Reports an event.
    ///
    /// Events are filtered based on the output configuration
    /// (quiet mode, verbose mode, etc.).
    ///
    /// # Errors
    ///
    /// Returns an error if writing to stderr fails.
    pub fn report(&self, event: ReportEvent<'_>) -> io::Result<()> {
        match self.report_event(event) {
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
            other => other,
        }
    }

    fn report_event(&self, event: ReportEvent<'_>) -> io::Result<()> {
        match event {
            ReportEvent::Error { path, message } => {
                self.write_error(path, message)?;
            }
            ReportEvent::Success { message } => {
                if !self.config.is_quiet() {
                    self.write_success(message)?;
                }
            }
            ReportEvent::Timing {
                operation,
                duration,
            } => {
                if self.config.show_timing() && !self.config.is_quiet() {
                    self.write_timing(operation, duration)?;
                }
            }
            ReportEvent::BatchSummary(stats) => {
                if !self.config.is_quiet() || stats.failed > 0 {
                    self.write_batch_summary(stats)?;
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::uninlined_format_args)]
    fn write_error(&self, path: Option<&Path>, message: &str) -> io::Result<()> {
        let mut lock = self.stderr.lock();
        #[cfg(feature = "colors")]
        if self.config.use_color() {
            use colored::Colorize;
            if let Some(p) = path {
                return writeln!(
                    lock,
                    "{} {}: {}",
                    "error:".red().bold(),
                    p.display(),
                    message
                );
            }
            return writeln!(lock, "{} {}", "error:".red().bold(), message);
        }
        #[cfg(not(feature = "colors"))]
        {
            let _ = self.config.use_color();
        }
        if let Some(p) = path {
            writeln!(lock, "error: {}: {}", p.display(), message)
        } else {
            writeln!(lock, "error: {}", message)
        }
    }

    #[allow(clippy::uninlined_format_args)]
    fn write_success(&self, message: &str) -> io::Result<()> {
        let mut lock = self.stdout.lock();
        #[cfg(feature = "colors")]
        if self.config.use_color() {
            use colored::Colorize;
            return writeln!(lock, "{} {}", "✓".green().bold(), message);
        }
        #[cfg(not(feature = "colors"))]
        {
            let _ = self.config.use_color();
        }
        writeln!(lock, "✓ {}", message)
    }

    fn write_timing(&self, operation: &str, duration: Duration) -> io::Result<()> {
        let mut lock = self.stderr.lock();
        #[cfg(feature = "colors")]
        if self.config.use_color() {
            use colored::Colorize;
            return writeln!(
                lock,
                "{} {} in {:.2}ms",
                "⏱".cyan(),
                operation,
                duration.as_secs_f64() * 1000.0
            );
        }
        #[cfg(not(feature = "colors"))]
        {
            let _ = self.config.use_color();
        }
        writeln!(
            lock,
            "⏱  {} in {:.2}ms",
            operation,
            duration.as_secs_f64() * 1000.0
        )
    }

    #[allow(clippy::uninlined_format_args)]
    fn write_batch_summary(&self, stats: BatchStats) -> io::Result<()> {
        let BatchStats {
            total,
            formatted,
            unchanged,
            would_change,
            failed,
            duration,
        } = stats;
        let mut lock = self.stderr.lock();

        #[cfg(feature = "colors")]
        if self.config.use_color() {
            use colored::Colorize;
            writeln!(lock)?;
            writeln!(
                lock,
                "{} {} in {:.2}ms",
                "Completed:".bold(),
                file_count(total),
                duration.as_secs_f64() * 1000.0
            )?;
            if formatted > 0 {
                writeln!(lock, "  {} formatted", formatted.to_string().green())?;
            }
            if unchanged > 0 {
                writeln!(lock, "  {} unchanged", unchanged.to_string().cyan())?;
            }
            if would_change > 0 {
                writeln!(lock, "  {} would change", would_change.to_string().yellow())?;
            }
            if failed > 0 {
                writeln!(lock, "  {} failed", failed.to_string().red())?;
            }
            return Ok(());
        }
        #[cfg(not(feature = "colors"))]
        {
            let _ = self.config.use_color();
        }

        writeln!(lock)?;
        writeln!(
            lock,
            "Completed: {} in {:.2}ms",
            file_count(total),
            duration.as_secs_f64() * 1000.0
        )?;
        if formatted > 0 {
            writeln!(lock, "  {} formatted", formatted)?;
        }
        if unchanged > 0 {
            writeln!(lock, "  {} unchanged", unchanged)?;
        }
        if would_change > 0 {
            writeln!(lock, "  {} would change", would_change)?;
        }
        if failed > 0 {
            writeln!(lock, "  {} failed", failed)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Verbosity;
    use std::path::PathBuf;

    #[test]
    fn test_new_reporter() {
        let config = OutputConfig::new();
        let _reporter = Reporter::new(config);
    }

    #[test]
    fn test_start_timing() {
        let config = OutputConfig::new();
        let mut reporter = Reporter::new(config);
        reporter.start_timing();
        assert!(reporter.elapsed().is_some());
    }

    #[test]
    fn test_elapsed_without_start() {
        let config = OutputConfig::new();
        let reporter = Reporter::new(config);
        assert!(reporter.elapsed().is_none());
    }

    #[test]
    fn test_report_error_always_shown() {
        let config = OutputConfig::new().with_verbosity(Verbosity::Quiet);
        let reporter = Reporter::new(config);
        let path = PathBuf::from("test.yaml");

        let result = reporter.report(ReportEvent::Error {
            path: Some(&path),
            message: "Test error",
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_file_count_pluralization() {
        assert_eq!(file_count(0), "0 files");
        assert_eq!(file_count(1), "1 file");
        assert_eq!(file_count(2), "2 files");
    }

    #[test]
    fn test_report_timing_verbose_mode() {
        let config = OutputConfig::new().with_verbosity(Verbosity::Verbose);
        let reporter = Reporter::new(config);

        let result = reporter.report(ReportEvent::Timing {
            operation: "parse",
            duration: Duration::from_secs(1),
        });
        assert!(result.is_ok());
    }
}
