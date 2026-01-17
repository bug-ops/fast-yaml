//! Parallel file processor for batch YAML formatting.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use fast_yaml_core::emitter::{Emitter, EmitterConfig};
use rayon::prelude::*;

use super::config::ProcessingConfig;
use super::discovery::DiscoveredFile;
use super::error::ProcessingError;
use super::reader::SmartFileReader;
use super::result::{BatchResult, FileOutcome, FileResult};

/// Parallel file processor for batch YAML formatting.
///
/// Processes multiple YAML files in parallel using Rayon's work-stealing scheduler.
/// Automatically chooses optimal reading strategy based on file size (in-memory vs mmap).
pub struct BatchProcessor {
    config: ProcessingConfig,
    emitter_config: EmitterConfig,
    reader: SmartFileReader,
}

impl BatchProcessor {
    /// Creates a new `BatchProcessor` with the given configuration
    pub fn new(config: ProcessingConfig) -> Self {
        let emitter_config = EmitterConfig::new()
            .with_indent(config.indent as usize)
            .with_width(config.width);

        let reader = SmartFileReader::with_threshold(config.mmap_threshold as u64);

        Self {
            config,
            emitter_config,
            reader,
        }
    }

    /// Processes a batch of discovered files in parallel.
    ///
    /// Returns aggregated results with success/failure counts and error details.
    /// Continues processing all files even if some fail.
    pub fn process(&self, files: &[DiscoveredFile]) -> BatchResult {
        let batch_start = Instant::now();
        let total = files.len();

        if total == 0 {
            return BatchResult::new();
        }

        let results = if self.should_use_custom_pool() {
            self.process_with_custom_pool(files)
        } else {
            self.process_with_default_pool(files)
        };

        let mut batch = BatchResult::from_results(results);
        batch.duration = batch_start.elapsed();
        batch
    }

    /// Processes files using a custom thread pool with configured worker count
    fn process_with_custom_pool(&self, files: &[DiscoveredFile]) -> Vec<FileResult> {
        rayon::ThreadPoolBuilder::new()
            .num_threads(self.config.effective_workers())
            .build()
            .map_or_else(
                |_| {
                    // Fallback to default pool if custom pool creation fails
                    self.process_files_parallel(files)
                },
                |pool| pool.install(|| self.process_files_parallel(files)),
            )
    }

    /// Processes files using the default Rayon thread pool.
    ///
    /// For very small batches (<10 files), uses sequential processing
    /// to avoid Rayon thread pool overhead.
    fn process_with_default_pool(&self, files: &[DiscoveredFile]) -> Vec<FileResult> {
        // Sequential path for tiny batches - avoids Rayon overhead
        if files.len() < 10 {
            self.process_files_sequential(files)
        } else {
            self.process_files_parallel(files)
        }
    }

    /// Processes files in parallel using Rayon's `par_iter`
    fn process_files_parallel(&self, files: &[DiscoveredFile]) -> Vec<FileResult> {
        let processed = AtomicUsize::new(0);
        let total = files.len();

        files
            .par_iter()
            .map(|file| {
                let result = self.process_single_file(&file.path);

                if self.config.verbose {
                    let n = processed.fetch_add(1, Ordering::Relaxed) + 1;
                    // Pre-format string to reduce time holding stderr lock
                    let msg = format!("[{n}/{total}] {}", file.path.display());
                    eprintln!("{msg}");
                }

                result
            })
            .collect()
    }

    /// Processes files sequentially without parallel overhead.
    ///
    /// Used for small batches (<10 files) where Rayon thread pool overhead
    /// would exceed the benefit of parallelism.
    fn process_files_sequential(&self, files: &[DiscoveredFile]) -> Vec<FileResult> {
        let total = files.len();

        files
            .iter()
            .enumerate()
            .map(|(i, file)| {
                let result = self.process_single_file(&file.path);

                if self.config.verbose {
                    eprintln!("[{}/{}] {}", i + 1, total, file.path.display());
                }

                result
            })
            .collect()
    }

    /// Processes a single file and returns the result
    fn process_single_file(&self, path: &Path) -> FileResult {
        let start = Instant::now();

        match self.format_file(path) {
            Ok(changed) => {
                let duration = start.elapsed();
                let outcome = if self.config.dry_run && changed {
                    FileOutcome::Skipped { duration }
                } else if changed {
                    FileOutcome::Formatted {
                        changed: true,
                        duration,
                    }
                } else {
                    FileOutcome::Unchanged { duration }
                };
                FileResult::new(path.to_path_buf(), outcome)
            }
            Err(error) => FileResult::new(
                path.to_path_buf(),
                FileOutcome::Failed {
                    error,
                    duration: start.elapsed(),
                },
            ),
        }
    }

    /// Formats a single file and returns whether it changed.
    ///
    /// Reads the file, formats it, compares with original, and writes if changed
    /// (unless in dry-run mode).
    fn format_file(&self, path: &Path) -> Result<bool, ProcessingError> {
        let file_content = self.reader.read(path)?;
        let original = file_content.as_str()?;

        let formatted = Emitter::format_with_config(original, &self.emitter_config)
            .map_err(|e| ProcessingError::FormatError(format!("{}: {}", path.display(), e)))?;

        let changed = original != formatted;

        if changed && self.config.in_place && !self.config.dry_run {
            Self::write_file_atomic(path, &formatted)?;
        }

        Ok(changed)
    }

    /// Writes content to file atomically using temp file + rename.
    ///
    /// This ensures that concurrent reads will either see the old content
    /// or the new content, never a partial write.
    fn write_file_atomic(path: &Path, content: &str) -> Result<(), ProcessingError> {
        let temp_path = path.with_extension("tmp");

        std::fs::write(&temp_path, content).map_err(ProcessingError::WriteError)?;

        std::fs::rename(&temp_path, path).map_err(ProcessingError::WriteError)?;

        Ok(())
    }

    /// Returns true if a custom thread pool should be used
    const fn should_use_custom_pool(&self) -> bool {
        self.config.workers != 0
    }
}

/// Convenience function for batch processing with default configuration
pub fn process_batch(files: &[DiscoveredFile], config: ProcessingConfig) -> BatchResult {
    let processor = BatchProcessor::new(config);
    processor.process(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_file(dir: &TempDir, name: &str, content: &str) -> DiscoveredFile {
        let path = dir.path().join(name);
        fs::write(&path, content).unwrap();
        DiscoveredFile {
            path,
            origin: super::super::discovery::DiscoveryOrigin::DirectPath,
        }
    }

    #[test]
    fn test_process_single_file_success() {
        let dir = TempDir::new().unwrap();
        let file = create_test_file(&dir, "test.yaml", "key: value\n");

        let config = ProcessingConfig::new();
        let processor = BatchProcessor::new(config);

        let result = processor.process_single_file(&file.path);
        assert!(result.is_success());
    }

    #[test]
    fn test_process_single_file_no_write_when_not_in_place() {
        let dir = TempDir::new().unwrap();
        let file = create_test_file(&dir, "test.yaml", "key: value\n");

        let config = ProcessingConfig::new().with_in_place(false);
        let processor = BatchProcessor::new(config);

        let original_content = fs::read_to_string(&file.path).unwrap();
        let result = processor.process_single_file(&file.path);

        // File should not be modified when in_place is false
        let after_content = fs::read_to_string(&file.path).unwrap();
        assert_eq!(original_content, after_content);
        assert!(result.is_success());
    }

    #[test]
    fn test_process_single_file_dry_run() {
        let dir = TempDir::new().unwrap();
        let file = create_test_file(&dir, "test.yaml", "key:value\n");

        let config = ProcessingConfig::new()
            .with_dry_run(true)
            .with_in_place(true);
        let processor = BatchProcessor::new(config);

        let original_content = fs::read_to_string(&file.path).unwrap();
        let _ = processor.process_single_file(&file.path);

        let after_content = fs::read_to_string(&file.path).unwrap();
        assert_eq!(original_content, after_content);
    }

    #[test]
    fn test_process_single_file_in_place() {
        let dir = TempDir::new().unwrap();
        let file = create_test_file(&dir, "test.yaml", "key:  value\n");

        let config = ProcessingConfig::new().with_in_place(true);
        let processor = BatchProcessor::new(config);

        let original_content = fs::read_to_string(&file.path).unwrap();
        let result = processor.process_single_file(&file.path);

        let after_content = fs::read_to_string(&file.path).unwrap();

        if result.outcome.was_changed() {
            assert_ne!(original_content, after_content);
        }
    }

    #[test]
    fn test_batch_result_aggregation() {
        let dir = TempDir::new().unwrap();

        let files = vec![
            create_test_file(&dir, "file1.yaml", "key: value\n"),
            create_test_file(&dir, "file2.yaml", "list:\n  - item\n"),
            create_test_file(&dir, "file3.yaml", "valid: yaml\n"),
        ];

        let config = ProcessingConfig::new();
        let processor = BatchProcessor::new(config);

        let result = processor.process(&files);

        assert_eq!(result.total, 3);
        assert_eq!(result.success_count(), 3);
        assert!(result.is_success());
    }

    #[test]
    fn test_effective_workers_default() {
        let config = ProcessingConfig::new();
        let processor = BatchProcessor::new(config);

        assert!(!processor.should_use_custom_pool());
    }

    #[test]
    fn test_effective_workers_custom() {
        let config = ProcessingConfig::new().with_workers(4);
        let processor = BatchProcessor::new(config);

        assert!(processor.should_use_custom_pool());
        assert_eq!(processor.config.effective_workers(), 4);
    }

    #[test]
    fn test_process_empty_batch() {
        let config = ProcessingConfig::new();
        let processor = BatchProcessor::new(config);

        let result = processor.process(&[]);

        assert_eq!(result.total, 0);
        assert!(result.is_success());
    }

    #[test]
    fn test_process_batch_with_error() {
        let dir = TempDir::new().unwrap();

        let mut files = vec![
            create_test_file(&dir, "file1.yaml", "key: value\n"),
            create_test_file(&dir, "file2.yaml", "invalid: [\n"),
        ];

        let nonexistent = dir.path().join("nonexistent.yaml");
        files.push(DiscoveredFile {
            path: nonexistent,
            origin: super::super::discovery::DiscoveryOrigin::DirectPath,
        });

        let config = ProcessingConfig::new();
        let processor = BatchProcessor::new(config);

        let result = processor.process(&files);

        assert_eq!(result.total, 3);
        assert!(result.failed >= 1);
        assert!(!result.is_success());
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_format_file_parse_error() {
        let dir = TempDir::new().unwrap();
        let file = create_test_file(&dir, "invalid.yaml", "invalid: [\n");

        let config = ProcessingConfig::new();
        let processor = BatchProcessor::new(config);

        let result = processor.format_file(&file.path);
        assert!(result.is_err());
    }

    #[test]
    fn test_atomic_write() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.yaml");
        fs::write(&path, "old content").unwrap();

        BatchProcessor::write_file_atomic(&path, "new content").unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "new content");

        let temp_path = path.with_extension("tmp");
        assert!(!temp_path.exists());
    }

    #[test]
    fn test_process_batch_convenience_function() {
        let dir = TempDir::new().unwrap();

        let files = vec![create_test_file(&dir, "file1.yaml", "key: value\n")];

        let config = ProcessingConfig::new();
        let result = process_batch(&files, config);

        assert_eq!(result.total, 1);
        assert!(result.is_success());
    }

    #[test]
    fn test_large_file_processing() {
        let dir = TempDir::new().unwrap();

        let large_content = "key: value\n".repeat(100_000);
        let file = create_test_file(&dir, "large.yaml", &large_content);

        let config = ProcessingConfig::new().with_mmap_threshold(1024);
        let processor = BatchProcessor::new(config);

        let result = processor.process_single_file(&file.path);
        assert!(result.is_success());
    }
}
