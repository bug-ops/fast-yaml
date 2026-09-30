//! Report event types for CLI output.

use std::path::Path;
use std::time::Duration;

/// Events that can be reported during command execution.
#[derive(Debug, Clone)]
pub enum ReportEvent<'a> {
    /// Error occurred
    Error {
        /// Path where error occurred (optional)
        path: Option<&'a Path>,
        /// Error message
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
        /// Files that would change (dry-run mode)
        would_change: usize,
        /// Files that failed
        failed: usize,
        /// Total duration
        duration: Duration,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_report_event_creation() {
        let path = PathBuf::from("test.yaml");

        assert!(matches!(
            ReportEvent::Error {
                path: Some(&path),
                message: "Test error",
            },
            ReportEvent::Error { .. }
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
                would_change: 1,
                failed: 1,
                duration: Duration::from_secs(5),
            },
            ReportEvent::BatchSummary { .. }
        ));
    }
}
