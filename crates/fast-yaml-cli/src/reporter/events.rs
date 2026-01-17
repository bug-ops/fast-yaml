//! Report event types for CLI output.

use std::path::Path;
use std::time::Duration;

/// Events that can be reported during command execution.
#[derive(Debug, Clone)]
pub enum ReportEvent<'a> {
    /// Progress update: current/total, path
    Progress {
        /// Current file number
        current: usize,
        /// Total number of files
        total: usize,
        /// Path being processed
        path: &'a Path,
    },
    /// File processing result
    FileResult {
        /// Path that was processed
        path: &'a Path,
        /// Processing outcome
        outcome: FileOutcome,
        /// Time taken to process
        duration: Duration,
    },
    /// Error occurred
    Error {
        /// Path where error occurred (optional)
        path: Option<&'a Path>,
        /// Error message
        message: &'a str,
    },
    /// Warning message
    Warning {
        /// Warning message
        message: &'a str,
    },
    /// Informational message
    Info {
        /// Info message
        message: &'a str,
    },
    /// Success message
    Success {
        /// Success message
        message: &'a str,
    },
    /// Timing information
    Timing {
        /// Operation name
        operation: &'a str,
        /// Duration of operation
        duration: Duration,
    },
    /// Batch summary
    BatchSummary {
        /// Total files processed
        total: usize,
        /// Files that were formatted
        formatted: usize,
        /// Files that were unchanged
        unchanged: usize,
        /// Files that were skipped
        skipped: usize,
        /// Files that failed
        failed: usize,
        /// Total duration
        duration: Duration,
    },
}

/// File processing outcome (compatible with existing batch result).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileOutcome {
    /// File was successfully formatted
    Formatted,
    /// File was already correctly formatted
    Unchanged,
    /// File was skipped (e.g., not a YAML file)
    Skipped,
    /// Processing failed
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_file_outcome_equality() {
        assert_eq!(FileOutcome::Formatted, FileOutcome::Formatted);
        assert_ne!(FileOutcome::Formatted, FileOutcome::Unchanged);
    }

    #[test]
    fn test_report_event_creation() {
        let path = PathBuf::from("test.yaml");

        // Test creating different event types
        assert!(matches!(
            ReportEvent::Progress {
                current: 1,
                total: 10,
                path: &path,
            },
            ReportEvent::Progress { .. }
        ));

        assert!(matches!(
            ReportEvent::FileResult {
                path: &path,
                outcome: FileOutcome::Formatted,
                duration: Duration::from_millis(100),
            },
            ReportEvent::FileResult { .. }
        ));

        assert!(matches!(
            ReportEvent::Error {
                path: Some(&path),
                message: "Test error",
            },
            ReportEvent::Error { .. }
        ));

        assert!(matches!(
            ReportEvent::Warning {
                message: "Test warning",
            },
            ReportEvent::Warning { .. }
        ));

        assert!(matches!(
            ReportEvent::Info {
                message: "Test info",
            },
            ReportEvent::Info { .. }
        ));

        assert!(matches!(
            ReportEvent::Success {
                message: "Test success",
            },
            ReportEvent::Success { .. }
        ));

        assert!(matches!(
            ReportEvent::Timing {
                operation: "parse",
                duration: Duration::from_secs(1),
            },
            ReportEvent::Timing { .. }
        ));

        assert!(matches!(
            ReportEvent::BatchSummary {
                total: 10,
                formatted: 5,
                unchanged: 3,
                skipped: 1,
                failed: 1,
                duration: Duration::from_secs(5),
            },
            ReportEvent::BatchSummary { .. }
        ));
    }
}
