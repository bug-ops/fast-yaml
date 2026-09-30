//! Report event types for CLI output.

use std::path::Path;
use std::time::Duration;

/// Outcome counts of a batch run.
#[derive(Debug, Clone, Copy)]
pub struct BatchStats {
    /// Total files processed
    pub total: usize,
    /// Files that were formatted
    pub formatted: usize,
    /// Files that were unchanged
    pub unchanged: usize,
    /// Files that would change (dry-run mode)
    pub would_change: usize,
    /// Files that failed
    pub failed: usize,
    /// Total duration
    pub duration: Duration,
}

/// Events that can be reported during command execution.
#[derive(Debug, Clone, Copy)]
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
    BatchSummary(BatchStats),
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
            ReportEvent::BatchSummary(BatchStats {
                total: 10,
                formatted: 5,
                unchanged: 3,
                would_change: 1,
                failed: 1,
                duration: Duration::from_secs(5),
            }),
            ReportEvent::BatchSummary(_)
        ));
    }
}
