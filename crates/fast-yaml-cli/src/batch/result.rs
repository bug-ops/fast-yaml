//! Result types for batch file processing.

use std::path::PathBuf;
use std::time::Duration;

use super::error::ProcessingError;

/// Outcome of processing a single file.
#[derive(Debug)]
pub enum FileOutcome {
    /// File formatted successfully and content changed
    Formatted {
        /// True if content changed
        changed: bool,
        /// Processing duration
        duration: Duration,
    },
    /// File unchanged (already formatted)
    Unchanged {
        /// Processing duration
        duration: Duration,
    },
    /// Skipped (dry-run mode, would change)
    Skipped {
        /// Processing duration
        duration: Duration,
    },
    /// Processing failed
    Failed {
        /// The error that occurred
        error: ProcessingError,
        /// Processing duration
        duration: Duration,
    },
}

impl FileOutcome {
    /// Returns true if the file was successfully processed
    pub const fn is_success(&self) -> bool {
        !matches!(self, Self::Failed { .. })
    }

    /// Returns the processing duration
    pub const fn duration(&self) -> Duration {
        match self {
            Self::Formatted { duration, .. }
            | Self::Unchanged { duration }
            | Self::Skipped { duration }
            | Self::Failed { duration, .. } => *duration,
        }
    }

    /// Returns true if the file was changed
    pub const fn was_changed(&self) -> bool {
        matches!(self, Self::Formatted { changed: true, .. })
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
    /// Number of files formatted (changed)
    pub formatted: usize,
    /// Number of files unchanged (already formatted)
    pub unchanged: usize,
    /// Number of files skipped (dry-run mode)
    pub skipped: usize,
    /// Number of files that failed processing
    pub failed: usize,
    /// Total processing duration
    pub duration: Duration,
    /// List of errors with file paths
    pub errors: Vec<(PathBuf, ProcessingError)>,
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
        let mut formatted = 0;
        let mut unchanged = 0;
        let mut skipped = 0;
        let mut failed = 0;
        // Pre-allocate for worst case: all files failed
        let mut errors = Vec::with_capacity(total);

        for result in results {
            match result.outcome {
                FileOutcome::Formatted { changed, .. } => {
                    if changed {
                        formatted += 1;
                    } else {
                        unchanged += 1;
                    }
                }
                FileOutcome::Unchanged { .. } => unchanged += 1,
                FileOutcome::Skipped { .. } => skipped += 1,
                FileOutcome::Failed { error, .. } => {
                    failed += 1;
                    errors.push((result.path, error));
                }
            }
        }

        let duration = start.elapsed();

        Self {
            total,
            formatted,
            unchanged,
            skipped,
            failed,
            duration,
            errors,
        }
    }

    /// Returns true if all files were processed successfully
    pub const fn is_success(&self) -> bool {
        self.failed == 0
    }

    /// Returns the number of successfully processed files
    pub const fn success_count(&self) -> usize {
        self.formatted + self.unchanged + self.skipped
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

    /// Adds a file result to this batch result
    pub fn add_result(&mut self, result: FileResult) {
        self.total += 1;
        match result.outcome {
            FileOutcome::Formatted { changed, .. } => {
                if changed {
                    self.formatted += 1;
                } else {
                    self.unchanged += 1;
                }
            }
            FileOutcome::Unchanged { .. } => self.unchanged += 1,
            FileOutcome::Skipped { .. } => self.skipped += 1,
            FileOutcome::Failed { error, .. } => {
                self.failed += 1;
                self.errors.push((result.path, error));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_outcome_is_success() {
        let success = FileOutcome::Formatted {
            changed: true,
            duration: Duration::from_millis(10),
        };
        assert!(success.is_success());

        let failed = FileOutcome::Failed {
            error: ProcessingError::ParseError("test".to_string()),
            duration: Duration::from_millis(5),
        };
        assert!(!failed.is_success());
    }

    #[test]
    fn test_file_outcome_duration() {
        let outcome = FileOutcome::Formatted {
            changed: true,
            duration: Duration::from_millis(123),
        };
        assert_eq!(outcome.duration(), Duration::from_millis(123));
    }

    #[test]
    fn test_file_outcome_was_changed() {
        let changed = FileOutcome::Formatted {
            changed: true,
            duration: Duration::from_millis(10),
        };
        assert!(changed.was_changed());

        let unchanged = FileOutcome::Formatted {
            changed: false,
            duration: Duration::from_millis(10),
        };
        assert!(!unchanged.was_changed());

        let skipped = FileOutcome::Skipped {
            duration: Duration::from_millis(5),
        };
        assert!(!skipped.was_changed());
    }

    #[test]
    fn test_file_result_new() {
        let path = PathBuf::from("/test/file.yaml");
        let outcome = FileOutcome::Formatted {
            changed: true,
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
                FileOutcome::Formatted {
                    changed: true,
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
                FileOutcome::Skipped {
                    duration: Duration::from_millis(3),
                },
            ),
            FileResult::new(
                PathBuf::from("/test/file4.yaml"),
                FileOutcome::Failed {
                    error: ProcessingError::ParseError("error".to_string()),
                    duration: Duration::from_millis(2),
                },
            ),
        ];

        let batch = BatchResult::from_results(results);
        assert_eq!(batch.total, 4);
        assert_eq!(batch.formatted, 1);
        assert_eq!(batch.unchanged, 1);
        assert_eq!(batch.skipped, 1);
        assert_eq!(batch.failed, 1);
        assert_eq!(batch.errors.len(), 1);
        assert!(!batch.is_success());
    }

    #[test]
    fn test_batch_result_is_success() {
        let mut batch = BatchResult::new();
        assert!(batch.is_success());

        batch.add_result(FileResult::new(
            PathBuf::from("/test/file.yaml"),
            FileOutcome::Formatted {
                changed: true,
                duration: Duration::from_millis(10),
            },
        ));
        assert!(batch.is_success());

        batch.add_result(FileResult::new(
            PathBuf::from("/test/file2.yaml"),
            FileOutcome::Failed {
                error: ProcessingError::ParseError("error".to_string()),
                duration: Duration::from_millis(5),
            },
        ));
        assert!(!batch.is_success());
    }

    #[test]
    fn test_batch_result_success_count() {
        let batch = BatchResult {
            total: 10,
            formatted: 5,
            unchanged: 3,
            skipped: 1,
            failed: 1,
            duration: Duration::from_secs(1),
            errors: vec![],
        };
        assert_eq!(batch.success_count(), 9);
    }

    #[test]
    fn test_batch_result_files_per_second() {
        let batch = BatchResult {
            total: 100,
            formatted: 50,
            unchanged: 50,
            skipped: 0,
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
            formatted: 100,
            unchanged: 0,
            skipped: 0,
            failed: 0,
            duration: Duration::from_secs(0),
            errors: vec![],
        };
        assert!((batch.files_per_second() - 0.0).abs() < f64::EPSILON);
    }

    // Property-based tests using proptest
    use proptest::prelude::*;

    proptest! {
        /// Property: BatchResult.total == success_count() + failed
        #[test]
        fn prop_batch_result_total_invariant(
            formatted in 0usize..1000,
            unchanged in 0usize..1000,
            skipped in 0usize..1000,
            failed in 0usize..1000,
        ) {
            let total = formatted + unchanged + skipped + failed;
            let batch = BatchResult {
                total,
                formatted,
                unchanged,
                skipped,
                failed,
                duration: Duration::from_secs(1),
                errors: vec![],
            };

            prop_assert_eq!(batch.total, batch.success_count() + batch.failed);
        }

        /// Property: success_count() is always >= 0 and <= total
        #[test]
        fn prop_success_count_bounds(
            total in 0usize..1000,
            failed in 0usize..1000,
        ) {
            let success = if total > failed { total - failed } else { 0 };
            let batch = BatchResult {
                total,
                formatted: success / 2,
                unchanged: success - success / 2,
                skipped: 0,
                failed,
                duration: Duration::from_secs(1),
                errors: vec![],
            };

            let success_count = batch.success_count();
            prop_assert!(success_count <= batch.total);
        }

        /// Property: Adding results maintains total count invariant
        #[test]
        fn prop_add_result_maintains_invariant(
            initial_count in 0usize..100,
            add_success in proptest::bool::ANY,
        ) {
            let mut batch = BatchResult::new();

            // Add initial results
            for i in 0..initial_count {
                let outcome = if add_success {
                    FileOutcome::Unchanged {
                        duration: Duration::from_millis(1),
                    }
                } else {
                    FileOutcome::Failed {
                        error: ProcessingError::ParseError(format!("error {i}")),
                        duration: Duration::from_millis(1),
                    }
                };
                batch.add_result(FileResult::new(PathBuf::from(format!("/file{i}.yaml")), outcome));
            }

            // Verify invariant
            prop_assert_eq!(batch.total, batch.success_count() + batch.failed);
        }

        /// Property: files_per_second is always >= 0.0
        #[test]
        fn prop_files_per_second_non_negative(
            total in 0usize..1000,
            duration_ms in 0u64..10000,
        ) {
            let batch = BatchResult {
                total,
                formatted: total,
                unchanged: 0,
                skipped: 0,
                failed: 0,
                duration: Duration::from_millis(duration_ms),
                errors: vec![],
            };

            let fps = batch.files_per_second();
            prop_assert!(fps >= 0.0);
            prop_assert!(fps.is_finite());
        }
    }
}
