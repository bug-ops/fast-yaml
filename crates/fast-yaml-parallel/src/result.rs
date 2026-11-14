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
///
/// The counts are derived from what was recorded, so they cannot contradict each other:
/// [`total`](Self::total) is [`unchanged`](Self::unchanged) plus [`changed`](Self::changed) plus
/// [`failed`](Self::failed), and [`failed`](Self::failed) is the length of
/// [`errors`](Self::errors).
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use std::time::Duration;
/// use fast_yaml_parallel::{BatchResult, FileOutcome, FileResult};
///
/// let ok = Duration::from_millis(1);
/// let batch = BatchResult::from_results(
///     vec![
///         FileResult::new(PathBuf::from("a.yaml"), FileOutcome::Changed { duration: ok }),
///         FileResult::new(PathBuf::from("b.yaml"), FileOutcome::Unchanged { duration: ok }),
///     ],
///     Duration::from_secs(1),
/// );
/// assert_eq!((batch.total(), batch.succeeded(), batch.changed()), (2, 2, 1));
/// assert!(batch.is_success());
/// ```
#[derive(Debug, Default)]
pub struct BatchResult {
    unchanged: usize,
    changed: usize,
    duration: Duration,
    errors: Vec<(PathBuf, Error)>,
}

impl BatchResult {
    /// Creates a `BatchResult` from the outcomes of a batch that took `duration` in total.
    ///
    /// A [`FileOutcome::Success`] counts as unchanged. Errors keep the order of `results`.
    pub fn from_results(results: Vec<FileResult>, duration: Duration) -> Self {
        let mut batch = Self {
            duration,
            ..Self::default()
        };

        for result in results {
            match result.outcome {
                FileOutcome::Success { .. } | FileOutcome::Unchanged { .. } => {
                    batch.unchanged += 1;
                }
                FileOutcome::Changed { .. } => batch.changed += 1,
                FileOutcome::Error { error, .. } => batch.errors.push((result.path, error)),
            }
        }

        batch
    }

    /// Number of files processed, successfully or not.
    pub const fn total(&self) -> usize {
        self.succeeded() + self.errors.len()
    }

    /// Number of files processed without error.
    pub const fn succeeded(&self) -> usize {
        self.unchanged + self.changed
    }

    /// Number of files that were changed, or in a dry run would change.
    pub const fn changed(&self) -> usize {
        self.changed
    }

    /// Number of files that were processed without being changed.
    pub const fn unchanged(&self) -> usize {
        self.unchanged
    }

    /// Number of files that failed.
    pub const fn failed(&self) -> usize {
        self.errors.len()
    }

    /// Total processing duration of the batch.
    pub const fn duration(&self) -> Duration {
        self.duration
    }

    /// The failed files with their errors, in input order.
    pub fn errors(&self) -> &[(PathBuf, Error)] {
        &self.errors
    }

    /// Returns true if all files were processed successfully
    pub const fn is_success(&self) -> bool {
        self.errors.is_empty()
    }

    /// Calculates files processed per second
    #[expect(
        clippy::cast_precision_loss,
        reason = "a file count above 2^52 is not reachable"
    )]
    pub fn files_per_second(&self) -> f64 {
        let secs = self.duration.as_secs_f64();
        if secs > 0.0 {
            self.total() as f64 / secs
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
            error: Error::EmptyDocument {
                path: PathBuf::from("test"),
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
    fn test_file_outcome_zero_duration() {
        let outcome = FileOutcome::Success {
            duration: Duration::ZERO,
        };
        assert_eq!(outcome.duration(), Duration::ZERO);
        assert!(outcome.is_success());
    }

    fn file(name: &str, outcome: FileOutcome) -> FileResult {
        FileResult::new(PathBuf::from(name), outcome)
    }

    fn error_outcome(name: &str) -> FileOutcome {
        FileOutcome::Error {
            error: Error::EmptyDocument {
                path: PathBuf::from(name),
            },
            duration: Duration::from_millis(2),
        }
    }

    #[test]
    fn test_batch_result_from_results() {
        let ms = Duration::from_millis;
        let results = vec![
            file(
                "/test/file1.yaml",
                FileOutcome::Changed { duration: ms(10) },
            ),
            file(
                "/test/file2.yaml",
                FileOutcome::Unchanged { duration: ms(5) },
            ),
            file("/test/file3.yaml", FileOutcome::Success { duration: ms(3) }),
            file("/test/file4.yaml", error_outcome("error")),
        ];

        let batch = BatchResult::from_results(results, Duration::from_secs(7));
        assert_eq!(batch.total(), 4);
        assert_eq!(batch.succeeded(), 3);
        assert_eq!(batch.changed(), 1);
        assert_eq!(batch.unchanged(), 2);
        assert_eq!(batch.failed(), 1);
        assert_eq!(batch.errors().len(), 1);
        assert_eq!(batch.duration(), Duration::from_secs(7));
        assert!(!batch.is_success());
    }

    #[test]
    fn test_batch_result_default_is_empty_and_successful() {
        let batch = BatchResult::default();
        assert_eq!(batch.total(), 0);
        assert!(batch.is_success());
    }

    #[test]
    fn test_batch_result_files_per_second() {
        let results = (0..100)
            .map(|n| {
                file(
                    &format!("/{n}.yaml"),
                    FileOutcome::Changed {
                        duration: Duration::ZERO,
                    },
                )
            })
            .collect();
        let batch = BatchResult::from_results(results, Duration::from_secs(2));
        assert!((batch.files_per_second() - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_batch_result_files_per_second_zero_duration() {
        let results = vec![file(
            "/a.yaml",
            FileOutcome::Success {
                duration: Duration::ZERO,
            },
        )];
        let batch = BatchResult::from_results(results, Duration::ZERO);
        assert!((batch.files_per_second() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_batch_result_from_empty_vec() {
        let batch = BatchResult::from_results(vec![], Duration::ZERO);
        assert_eq!(batch.total(), 0);
        assert_eq!(batch.succeeded(), 0);
        assert_eq!(batch.changed(), 0);
        assert_eq!(batch.failed(), 0);
        assert!(batch.is_success());
        assert!(batch.errors().is_empty());
    }

    #[test]
    fn test_batch_result_all_failures_keep_input_order() {
        let results = vec![
            file("/a.yaml", error_outcome("error1")),
            file("/b.yaml", error_outcome("error2")),
        ];

        let batch = BatchResult::from_results(results, Duration::ZERO);
        assert_eq!(batch.total(), 2);
        assert_eq!(batch.succeeded(), 0);
        assert_eq!(batch.failed(), 2);
        assert!(!batch.is_success());
        let paths: Vec<_> = batch
            .errors()
            .iter()
            .map(|(path, _)| path.clone())
            .collect();
        assert_eq!(paths, [PathBuf::from("/a.yaml"), PathBuf::from("/b.yaml")]);
    }

    use proptest::prelude::*;

    fn outcome_of(kind: u8) -> FileOutcome {
        let duration = Duration::from_millis(1);
        match kind % 4 {
            0 => FileOutcome::Success { duration },
            1 => FileOutcome::Changed { duration },
            2 => FileOutcome::Unchanged { duration },
            _ => error_outcome("e"),
        }
    }

    proptest! {
        /// The counts of `from_results` always add up, whatever the outcomes.
        #[test]
        fn prop_from_results_counts_are_consistent(kinds in proptest::collection::vec(any::<u8>(), 0..200)) {
            let results: Vec<FileResult> = kinds
                .iter()
                .enumerate()
                .map(|(i, kind)| file(&format!("/{i}.yaml"), outcome_of(*kind)))
                .collect();
            let len = results.len();

            let batch = BatchResult::from_results(results, Duration::from_secs(1));

            prop_assert_eq!(batch.total(), len);
            prop_assert_eq!(batch.succeeded() + batch.failed(), batch.total());
            prop_assert_eq!(batch.changed() + batch.unchanged(), batch.succeeded());
            prop_assert_eq!(batch.failed(), batch.errors().len());
            prop_assert_eq!(batch.duration(), Duration::from_secs(1));
        }

        /// `files_per_second` is never negative or non-finite.
        #[test]
        fn prop_files_per_second_non_negative(
            total in 0usize..1000,
            duration_ms in 0u64..10000,
        ) {
            let results = (0..total)
                .map(|n| file(&format!("/{n}.yaml"), FileOutcome::Unchanged { duration: Duration::ZERO }))
                .collect();
            let batch = BatchResult::from_results(results, Duration::from_millis(duration_ms));

            let fps = batch.files_per_second();
            prop_assert!(fps >= 0.0);
            prop_assert!(fps.is_finite());
        }
    }
}
