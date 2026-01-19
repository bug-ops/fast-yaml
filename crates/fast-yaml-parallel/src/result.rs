//! Result types for batch file processing.

use std::path::PathBuf;
use std::time::Duration;

use crate::error::Error;

/// Outcome of processing a single file.
#[derive(Debug)]
pub enum FileOutcome {
    /// File processed successfully (content may or may not have changed)
    Success {
        /// Processing duration
        duration: Duration,
    },
    /// File formatted and content changed
    Changed {
        /// Processing duration
        duration: Duration,
    },
    /// File unchanged (already formatted)
    Unchanged {
        /// Processing duration
        duration: Duration,
    },
    /// Processing failed
    Error {
        /// The error that occurred
        error: Error,
        /// Processing duration
        duration: Duration,
    },
}

impl FileOutcome {
    /// Returns true if the file was successfully processed
    pub const fn is_success(&self) -> bool {
        !matches!(self, Self::Error { .. })
    }

    /// Returns the processing duration
    pub const fn duration(&self) -> Duration {
        match self {
            Self::Success { duration }
            | Self::Changed { duration }
            | Self::Unchanged { duration }
            | Self::Error { duration, .. } => *duration,
        }
    }

    /// Returns true if the file was changed
    pub const fn was_changed(&self) -> bool {
        matches!(self, Self::Changed { .. })
    }
}

/// Result for a single file with path context.
#[derive(Debug)]
pub struct FileResult {
    /// Path to the processed file
    pub path: PathBuf,
    /// Processing outcome
    pub outcome: FileOutcome,
}

impl FileResult {
    /// Creates a new `FileResult`
    pub const fn new(path: PathBuf, outcome: FileOutcome) -> Self {
        Self { path, outcome }
    }

    /// Returns true if processing was successful
    pub const fn is_success(&self) -> bool {
        self.outcome.is_success()
    }
}

/// Aggregated results from batch processing.
#[derive(Debug, Default)]
pub struct BatchResult {
    /// Total number of files processed
    pub total: usize,
    /// Number of files successfully processed
    pub success: usize,
    /// Number of files changed
    pub changed: usize,
    /// Number of files that failed processing
    pub failed: usize,
    /// Total processing duration
    pub duration: Duration,
    /// List of errors with file paths
    pub errors: Vec<(PathBuf, Error)>,
}

impl BatchResult {
    /// Creates a new empty `BatchResult`
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a `BatchResult` from a list of `FileResult`s
    pub fn from_results(results: Vec<FileResult>) -> Self {
        let start = std::time::Instant::now();
        let total = results.len();
        let mut success = 0;
        let mut changed = 0;
        let mut failed = 0;
        let mut errors = Vec::with_capacity(total);

        for result in results {
            match result.outcome {
                FileOutcome::Success { .. } | FileOutcome::Unchanged { .. } => {
                    success += 1;
                }
                FileOutcome::Changed { .. } => {
                    success += 1;
                    changed += 1;
                }
                FileOutcome::Error { error, .. } => {
                    failed += 1;
                    errors.push((result.path, error));
                }
            }
        }

        let duration = start.elapsed();

        Self {
            total,
            success,
            changed,
            failed,
            duration,
            errors,
        }
    }

    /// Returns true if all files were processed successfully
    pub const fn is_success(&self) -> bool {
        self.failed == 0
    }

    /// Calculates files processed per second
    #[allow(clippy::cast_precision_loss)]
    pub fn files_per_second(&self) -> f64 {
        let secs = self.duration.as_secs_f64();
        if secs > 0.0 {
            self.total as f64 / secs
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_outcome_is_success() {
        let success = FileOutcome::Success {
            duration: Duration::from_millis(10),
        };
        assert!(success.is_success());

        let changed = FileOutcome::Changed {
            duration: Duration::from_millis(10),
        };
        assert!(changed.is_success());

        let unchanged = FileOutcome::Unchanged {
            duration: Duration::from_millis(10),
        };
        assert!(unchanged.is_success());

        let failed = FileOutcome::Error {
            error: Error::Format {
                message: "test".to_string(),
            },
            duration: Duration::from_millis(5),
        };
        assert!(!failed.is_success());
    }

    #[test]
    fn test_file_outcome_duration() {
        let outcome = FileOutcome::Changed {
            duration: Duration::from_millis(123),
        };
        assert_eq!(outcome.duration(), Duration::from_millis(123));
    }

    #[test]
    fn test_file_outcome_was_changed() {
        let changed = FileOutcome::Changed {
            duration: Duration::from_millis(10),
        };
        assert!(changed.was_changed());

        let unchanged = FileOutcome::Unchanged {
            duration: Duration::from_millis(10),
        };
        assert!(!unchanged.was_changed());

        let success = FileOutcome::Success {
            duration: Duration::from_millis(5),
        };
        assert!(!success.was_changed());
    }

    #[test]
    fn test_file_result_new() {
        let path = PathBuf::from("/test/file.yaml");
        let outcome = FileOutcome::Changed {
            duration: Duration::from_millis(10),
        };
        let result = FileResult::new(path.clone(), outcome);
        assert_eq!(result.path, path);
        assert!(result.is_success());
    }

    #[test]
    fn test_batch_result_from_results() {
        let results = vec![
            FileResult::new(
                PathBuf::from("/test/file1.yaml"),
                FileOutcome::Changed {
                    duration: Duration::from_millis(10),
                },
            ),
            FileResult::new(
                PathBuf::from("/test/file2.yaml"),
                FileOutcome::Unchanged {
                    duration: Duration::from_millis(5),
                },
            ),
            FileResult::new(
                PathBuf::from("/test/file3.yaml"),
                FileOutcome::Success {
                    duration: Duration::from_millis(3),
                },
            ),
            FileResult::new(
                PathBuf::from("/test/file4.yaml"),
                FileOutcome::Error {
                    error: Error::Format {
                        message: "error".to_string(),
                    },
                    duration: Duration::from_millis(2),
                },
            ),
        ];

        let batch = BatchResult::from_results(results);
        assert_eq!(batch.total, 4);
        assert_eq!(batch.success, 3);
        assert_eq!(batch.changed, 1);
        assert_eq!(batch.failed, 1);
        assert_eq!(batch.errors.len(), 1);
        assert!(!batch.is_success());
    }

    #[test]
    fn test_batch_result_is_success() {
        let mut batch = BatchResult::new();
        assert!(batch.is_success());

        batch.failed = 1;
        assert!(!batch.is_success());
    }

    #[test]
    fn test_batch_result_files_per_second() {
        let batch = BatchResult {
            total: 100,
            success: 100,
            changed: 50,
            failed: 0,
            duration: Duration::from_secs(2),
            errors: vec![],
        };
        assert!((batch.files_per_second() - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_batch_result_files_per_second_zero_duration() {
        let batch = BatchResult {
            total: 100,
            success: 100,
            changed: 0,
            failed: 0,
            duration: Duration::from_secs(0),
            errors: vec![],
        };
        assert!((batch.files_per_second() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_batch_result_from_empty_vec() {
        let batch = BatchResult::from_results(vec![]);
        assert_eq!(batch.total, 0);
        assert_eq!(batch.success, 0);
        assert_eq!(batch.changed, 0);
        assert_eq!(batch.failed, 0);
        assert!(batch.is_success());
        assert_eq!(batch.errors.len(), 0);
    }

    #[test]
    fn test_batch_result_all_success() {
        let results = vec![
            FileResult::new(
                PathBuf::from("/a.yaml"),
                FileOutcome::Success {
                    duration: Duration::from_millis(1),
                },
            ),
            FileResult::new(
                PathBuf::from("/b.yaml"),
                FileOutcome::Success {
                    duration: Duration::from_millis(2),
                },
            ),
        ];

        let batch = BatchResult::from_results(results);
        assert_eq!(batch.total, 2);
        assert_eq!(batch.success, 2);
        assert_eq!(batch.changed, 0);
        assert_eq!(batch.failed, 0);
        assert!(batch.is_success());
    }

    #[test]
    fn test_batch_result_all_failures() {
        let results = vec![
            FileResult::new(
                PathBuf::from("/a.yaml"),
                FileOutcome::Error {
                    error: Error::Format {
                        message: "error1".to_string(),
                    },
                    duration: Duration::from_millis(1),
                },
            ),
            FileResult::new(
                PathBuf::from("/b.yaml"),
                FileOutcome::Error {
                    error: Error::Format {
                        message: "error2".to_string(),
                    },
                    duration: Duration::from_millis(2),
                },
            ),
        ];

        let batch = BatchResult::from_results(results);
        assert_eq!(batch.total, 2);
        assert_eq!(batch.success, 0);
        assert_eq!(batch.failed, 2);
        assert!(!batch.is_success());
        assert_eq!(batch.errors.len(), 2);
    }

    #[test]
    fn test_batch_result_all_changed() {
        let results = vec![
            FileResult::new(
                PathBuf::from("/a.yaml"),
                FileOutcome::Changed {
                    duration: Duration::from_millis(1),
                },
            ),
            FileResult::new(
                PathBuf::from("/b.yaml"),
                FileOutcome::Changed {
                    duration: Duration::from_millis(2),
                },
            ),
        ];

        let batch = BatchResult::from_results(results);
        assert_eq!(batch.total, 2);
        assert_eq!(batch.success, 2);
        assert_eq!(batch.changed, 2);
        assert_eq!(batch.failed, 0);
        assert!(batch.is_success());
    }

    #[test]
    fn test_file_outcome_zero_duration() {
        let outcome = FileOutcome::Success {
            duration: Duration::ZERO,
        };
        assert_eq!(outcome.duration(), Duration::ZERO);
        assert!(outcome.is_success());
    }

    #[test]
    fn test_batch_result_files_per_second_nanoseconds() {
        let batch = BatchResult {
            total: 1000,
            success: 1000,
            changed: 0,
            failed: 0,
            duration: Duration::from_nanos(1),
            errors: vec![],
        };

        let fps = batch.files_per_second();
        assert!(fps > 0.0);
        assert!(fps.is_finite());
    }

    // Property-based tests using proptest
    use proptest::prelude::*;

    proptest! {
        /// Property: BatchResult.total == success + failed
        #[test]
        fn prop_batch_total_invariant(
            success in 0usize..1000,
            failed in 0usize..1000,
        ) {
            let total = success + failed;
            let batch = BatchResult {
                total,
                success,
                changed: 0,
                failed,
                duration: Duration::from_secs(1),
                errors: vec![],
            };

            prop_assert_eq!(batch.total, batch.success + batch.failed);
        }

        /// Property: success_count is always >= 0 and <= total
        #[test]
        fn prop_success_count_bounds(
            total in 0usize..1000,
            failed in 0usize..1000,
        ) {
            let success = total.saturating_sub(failed);
            let batch = BatchResult {
                total,
                success,
                changed: 0,
                failed,
                duration: Duration::from_secs(1),
                errors: vec![],
            };

            prop_assert!(batch.success <= batch.total);
        }

        /// Property: files_per_second is always >= 0.0
        #[test]
        fn prop_files_per_second_non_negative(
            total in 0usize..1000,
            duration_ms in 0u64..10000,
        ) {
            let batch = BatchResult {
                total,
                success: total,
                changed: 0,
                failed: 0,
                duration: Duration::from_millis(duration_ms),
                errors: vec![],
            };

            let fps = batch.files_per_second();
            prop_assert!(fps >= 0.0);
            prop_assert!(fps.is_finite());
        }

        /// Property: changed <= success
        #[test]
        fn prop_changed_le_success(
            total in 0usize..1000,
            success in 0usize..1000,
            changed in 0usize..1000,
        ) {
            let success = success.min(total);
            let changed = changed.min(success);
            let batch = BatchResult {
                total,
                success,
                changed,
                failed: total.saturating_sub(success),
                duration: Duration::from_secs(1),
                errors: vec![],
            };

            prop_assert!(batch.changed <= batch.success);
        }

        /// Property: errors.len() == failed
        #[test]
        fn prop_errors_len_eq_failed(
            success in 0usize..100,
            failed in 0usize..100,
        ) {
            let total = success + failed;
            let mut errors = Vec::with_capacity(failed);
            for i in 0..failed {
                errors.push((
                    PathBuf::from(format!("/file{i}.yaml")),
                    Error::Format { message: format!("error{i}") }
                ));
            }

            let batch = BatchResult {
                total,
                success,
                changed: 0,
                failed,
                duration: Duration::from_secs(1),
                errors,
            };

            prop_assert_eq!(batch.errors.len(), batch.failed);
        }
    }
}
