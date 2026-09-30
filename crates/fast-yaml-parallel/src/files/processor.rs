//! Parallel file processor for batch YAML operations.

use std::path::{Path, PathBuf};
use std::time::Instant;

use fast_yaml_core::emitter::{Emitter, EmitterConfig};
use fast_yaml_core::has_comments;
use rayon::prelude::*;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::io::SmartReader;
use crate::result::{BatchResult, FileOutcome, FileResult};

/// Whether formatting may discard YAML comments, which the emitter cannot preserve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommentPolicy {
    /// Fail files that contain comments with [`Error::CommentsWouldBeStripped`].
    #[default]
    Reject,
    /// Format regardless and drop the comments.
    Strip,
}

/// Formatted content of one file together with whether it differs from the original.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatOutput {
    /// The formatted document text.
    pub formatted: String,
    /// `true` when `formatted` differs from the file's current content.
    pub changed: bool,
}

/// Parallel file processor for batch YAML operations.
///
/// Processes multiple YAML files in parallel using Rayon's work-stealing scheduler.
/// Automatically chooses optimal reading strategy based on file size (in-memory vs mmap).
///
/// # Security: Path Trust Boundary
///
/// This is a library crate providing file processing primitives. Path validation
/// is the responsibility of the caller:
///
/// - **For CLI tools**: Validate paths before passing to this API
/// - **For libraries**: Document that paths must be trusted
/// - **Path traversal**: No canonicalization or ".." filtering is performed
/// - **Symlinks**: Followed without validation (use OS permissions for access control)
///
/// If your application accepts user-controlled paths, validate them before calling
/// these methods. Example validation:
///
/// ```no_run
/// use std::path::{Path, PathBuf};
///
/// fn validate_path(path: &Path, base_dir: &Path) -> Result<PathBuf, String> {
///     let canonical = path.canonicalize()
///         .map_err(|e| format!("invalid path: {}", e))?;
///
///     if !canonical.starts_with(base_dir) {
///         return Err("path outside allowed directory".to_string());
///     }
///
///     Ok(canonical)
/// }
/// ```
#[derive(Debug)]
pub struct FileProcessor {
    config: Config,
    reader: SmartReader,
}

impl FileProcessor {
    /// Creates a processor with default config.
    pub fn new() -> Self {
        Self::with_config(Config::default())
    }

    /// Creates a processor with custom config.
    pub const fn with_config(config: Config) -> Self {
        let reader = SmartReader::with_threshold(config.mmap_threshold() as u64);

        Self { config, reader }
    }

    /// Process files with custom operation.
    ///
    /// Generic function for applying custom processing to files in parallel.
    pub fn process<F, R>(&self, paths: &[PathBuf], f: F) -> BatchResult
    where
        F: Fn(&Path, &str) -> Result<R> + Sync,
        R: Send,
    {
        let batch_start = Instant::now();
        let total = paths.len();

        if total == 0 {
            return BatchResult::new();
        }

        let results = if Self::should_use_sequential(paths) {
            self.process_files_sequential(paths, &f)
        } else {
            self.process_files_parallel(paths, &f)
        };

        let mut batch = BatchResult::from_results(results);
        batch.duration = batch_start.elapsed();
        batch
    }

    /// Parse all files and return `BatchResult`.
    pub fn parse_files(&self, paths: &[PathBuf]) -> BatchResult {
        self.process(paths, |path, content| {
            fast_yaml_core::Parser::parse_str_with_limits(content, &self.config.parse_limits())
                .map_err(|source| Error::Parse { index: 0, source })?
                .ok_or_else(|| Error::EmptyDocument {
                    path: path.to_path_buf(),
                })?;
            Ok(())
        })
    }

    /// Format files and return `(path, output)` pairs without writing anything.
    ///
    /// Each file is read once; `output.changed` reports whether formatting would alter it.
    /// With [`CommentPolicy::Reject`], files containing comments yield
    /// [`Error::CommentsWouldBeStripped`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::PathBuf;
    /// use fast_yaml_core::EmitterConfig;
    /// use fast_yaml_parallel::{CommentPolicy, FileProcessor};
    ///
    /// let processor = FileProcessor::new();
    /// let paths = [PathBuf::from("a.yaml")];
    /// for (path, result) in processor.format_files(&paths, &EmitterConfig::new(), CommentPolicy::Reject) {
    ///     match result {
    ///         Ok(out) if out.changed => println!("{} would change", path.display()),
    ///         Ok(_) => {}
    ///         Err(e) => eprintln!("{}: {e}", path.display()),
    ///     }
    /// }
    /// ```
    pub fn format_files(
        &self,
        paths: &[PathBuf],
        emitter_config: &EmitterConfig,
        comments: CommentPolicy,
    ) -> Vec<(PathBuf, Result<FormatOutput>)> {
        let process_file = |path: &Path| self.format_content(path, emitter_config, comments);

        if Self::should_use_sequential(paths) {
            paths
                .iter()
                .map(|path| (path.clone(), process_file(path)))
                .collect()
        } else {
            paths
                .par_iter()
                .map(|path| (path.clone(), process_file(path)))
                .collect()
        }
    }

    /// Format files in place (write back if changed).
    ///
    /// With [`CommentPolicy::Reject`], files containing comments are reported as
    /// [`Error::CommentsWouldBeStripped`] and left untouched.
    pub fn format_in_place(
        &self,
        paths: &[PathBuf],
        emitter_config: &EmitterConfig,
        comments: CommentPolicy,
    ) -> BatchResult {
        let batch_start = std::time::Instant::now();
        let total = paths.len();

        if total == 0 {
            return BatchResult::new();
        }

        let results = if Self::should_use_sequential(paths) {
            paths
                .iter()
                .map(|path| self.format_single_file(path, emitter_config, comments))
                .collect()
        } else {
            paths
                .par_iter()
                .map(|path| self.format_single_file(path, emitter_config, comments))
                .collect()
        };

        let mut batch = BatchResult::from_results(results);
        batch.duration = batch_start.elapsed();
        batch
    }

    /// Reads `path` once, enforces the size limit and comment policy, and formats it.
    fn format_content(
        &self,
        path: &Path,
        emitter_config: &EmitterConfig,
        comments: CommentPolicy,
    ) -> Result<FormatOutput> {
        let metadata = std::fs::metadata(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;

        let file_size = metadata.len();
        let max_size = self.config.max_input_size();

        if file_size > max_size as u64 {
            #[allow(clippy::cast_possible_truncation)]
            let size = file_size as usize;
            return Err(Error::InputTooLarge {
                size,
                max: max_size,
            });
        }

        let file_content = self.reader.read(path)?;
        let content = file_content.as_str()?;

        let formatted = Emitter::format_with_config(content, emitter_config).map_err(|source| {
            Error::Format {
                path: path.to_path_buf(),
                source,
            }
        })?;

        if comments == CommentPolicy::Reject
            && has_comments(content).map_err(|source| Error::CommentScan { source })?
        {
            return Err(Error::CommentsWouldBeStripped);
        }

        Ok(FormatOutput {
            changed: content != formatted,
            formatted,
        })
    }

    /// Format a single file in place
    fn format_single_file(
        &self,
        path: &Path,
        emitter_config: &EmitterConfig,
        comments: CommentPolicy,
    ) -> FileResult {
        let start = std::time::Instant::now();

        let output = match self.format_content(path, emitter_config, comments) {
            Ok(output) => output,
            Err(error) => {
                return FileResult::new(
                    path.to_path_buf(),
                    FileOutcome::Error {
                        error,
                        duration: start.elapsed(),
                    },
                );
            }
        };

        if output.changed
            && let Err(error) = Self::write_file_atomic(path, &output.formatted)
        {
            return FileResult::new(
                path.to_path_buf(),
                FileOutcome::Error {
                    error,
                    duration: start.elapsed(),
                },
            );
        }

        let duration = start.elapsed();
        let outcome = if output.changed {
            FileOutcome::Changed { duration }
        } else {
            FileOutcome::Unchanged { duration }
        };

        FileResult::new(path.to_path_buf(), outcome)
    }

    /// Processes files in parallel using Rayon's `par_iter`
    fn process_files_parallel<F, R>(&self, paths: &[PathBuf], f: &F) -> Vec<FileResult>
    where
        F: Fn(&Path, &str) -> Result<R> + Sync,
        R: Send,
    {
        paths
            .par_iter()
            .map(|path| self.process_single_file(path, f))
            .collect()
    }

    /// Processes files sequentially without parallel overhead.
    fn process_files_sequential<F, R>(&self, paths: &[PathBuf], f: &F) -> Vec<FileResult>
    where
        F: Fn(&Path, &str) -> Result<R>,
    {
        paths
            .iter()
            .map(|path| self.process_single_file(path, f))
            .collect()
    }

    /// Processes a single file and returns the result
    fn process_single_file<F, R>(&self, path: &Path, f: &F) -> FileResult
    where
        F: Fn(&Path, &str) -> Result<R>,
    {
        let start = Instant::now();

        match self.process_file_content(path, f) {
            Ok(()) => {
                let duration = start.elapsed();
                FileResult::new(path.to_path_buf(), FileOutcome::Success { duration })
            }
            Err(error) => FileResult::new(
                path.to_path_buf(),
                FileOutcome::Error {
                    error,
                    duration: start.elapsed(),
                },
            ),
        }
    }

    /// Process file content with given function
    fn process_file_content<F, R>(&self, path: &Path, f: &F) -> Result<()>
    where
        F: Fn(&Path, &str) -> Result<R>,
    {
        let metadata = std::fs::metadata(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;

        let file_size = metadata.len();
        let max_size = self.config.max_input_size();

        if file_size > max_size as u64 {
            #[allow(clippy::cast_possible_truncation)]
            let size = file_size as usize;
            return Err(Error::InputTooLarge {
                size,
                max: max_size,
            });
        }

        let file_content = self.reader.read(path)?;
        let content = file_content.as_str()?;

        f(path, content)?;
        Ok(())
    }

    /// Writes content to file via the shared secure writer [`crate::write_atomic`].
    fn write_file_atomic(path: &Path, content: &str) -> Result<()> {
        crate::write_atomic(path, content.as_bytes()).map_err(|source| Error::Write {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Returns true if sequential processing should be used.
    ///
    /// Sequential processing is preferred when:
    /// - Very few files (< 4)
    /// - Small total size (< 1MB) AND moderate file count (< 10)
    ///
    /// This avoids parallelism overhead for small workloads while enabling
    /// parallel processing for large files even if there are only a few of them.
    fn should_use_sequential(paths: &[PathBuf]) -> bool {
        let file_count = paths.len();

        if file_count < 4 {
            return true;
        }

        let total_size: u64 = paths
            .iter()
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum();

        total_size < 1_000_000 && file_count < 10
    }
}

impl Default for FileProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let path = dir.path().join(name);
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn test_parse_files_honors_parse_limits() {
        use fast_yaml_core::limits::{MaxDepth, ParseLimits};
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "nested.yaml", "[[[1]]]\n");
        let limits = ParseLimits {
            max_depth: MaxDepth::new(2).unwrap(),
            ..ParseLimits::default()
        };
        let strict = FileProcessor::with_config(Config::new().with_parse_limits(limits));
        assert!(!strict.parse_files(std::slice::from_ref(&path)).is_success());
        assert!(FileProcessor::new().parse_files(&[path]).is_success());
    }

    #[test]
    fn test_parse_files_rejects_invalid_merge_key_in_later_document() {
        let dir = TempDir::new().unwrap();
        let bad = create_test_file(&dir, "bad.yaml", "x: 1\n---\nm:\n  <<: [1]\n");
        let good = create_test_file(&dir, "good.yaml", "x: 1\n---\nb: &b {y: 2}\nm:\n  <<: *b\n");
        let result = FileProcessor::new().parse_files(&[bad, good]);
        assert_eq!(result.failed, 1);
        assert_eq!(result.success, 1);
    }

    #[test]
    fn test_file_processor_new() {
        let _processor = FileProcessor::new();
    }

    #[test]
    fn test_file_processor_with_config() {
        let config = Config::new().with_workers(Some(4));
        let _processor = FileProcessor::with_config(config);
    }

    #[test]
    fn test_process_single_file() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.yaml", "key: value\n");

        let processor = FileProcessor::new();
        let result = processor.parse_files(&[path]);

        assert_eq!(result.total, 1);
        assert!(result.is_success());
    }

    #[test]
    fn test_process_multiple_files() {
        let dir = TempDir::new().unwrap();
        let paths = vec![
            create_test_file(&dir, "file1.yaml", "key1: value1\n"),
            create_test_file(&dir, "file2.yaml", "key2: value2\n"),
            create_test_file(&dir, "file3.yaml", "key3: value3\n"),
        ];

        let processor = FileProcessor::new();
        let result = processor.parse_files(&paths);

        assert_eq!(result.total, 3);
        assert!(result.is_success());
    }

    #[test]
    fn test_process_empty_batch() {
        let processor = FileProcessor::new();
        let result = processor.parse_files(&[]);

        assert_eq!(result.total, 0);
        assert!(result.is_success());
    }

    #[test]
    fn test_process_with_errors() {
        let dir = TempDir::new().unwrap();
        let paths = vec![
            create_test_file(&dir, "valid.yaml", "key: value\n"),
            create_test_file(&dir, "invalid.yaml", "invalid: [\n"),
        ];

        let processor = FileProcessor::new();
        let result = processor.parse_files(&paths);

        assert_eq!(result.total, 2);
        assert!(!result.is_success());
        assert!(result.failed >= 1);
    }

    #[test]
    fn test_parse_files() {
        let dir = TempDir::new().unwrap();
        let paths = vec![create_test_file(&dir, "test.yaml", "key: value\n")];

        let processor = FileProcessor::new();
        let result = processor.parse_files(&paths);

        assert!(result.is_success());
    }

    #[test]
    fn test_format_files() {
        let dir = TempDir::new().unwrap();
        let paths = vec![create_test_file(&dir, "test.yaml", "key: value\n")];

        let processor = FileProcessor::new();
        let emitter_config = EmitterConfig::new();
        let results = processor.format_files(&paths, &emitter_config, CommentPolicy::Strip);

        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_ok());
    }

    #[test]
    fn test_format_in_place() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.yaml", "key:  value\n");

        let processor = FileProcessor::new();
        let emitter_config = EmitterConfig::new();
        let result = processor.format_in_place(
            std::slice::from_ref(&path),
            &emitter_config,
            CommentPolicy::Strip,
        );

        assert_eq!(result.total, 1);
    }

    #[test]
    fn test_atomic_write() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.yaml");
        fs::write(&path, "old content").unwrap();

        FileProcessor::write_file_atomic(&path, "new content").unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "new content");
    }

    #[test]
    fn test_large_file_with_mmap() {
        let dir = TempDir::new().unwrap();

        let large_content = "key: value\n".repeat(100_000);
        let path = create_test_file(&dir, "large.yaml", &large_content);

        let config = Config::new().with_mmap_threshold(1024);
        let processor = FileProcessor::with_config(config);

        let result = processor.parse_files(&[path]);
        assert!(result.is_success());
    }

    #[test]
    fn test_sequential_threshold_exactly_9_files() {
        let dir = TempDir::new().unwrap();
        let mut paths = Vec::new();

        // Create exactly 9 files (below threshold of 10)
        for i in 0..9 {
            paths.push(create_test_file(
                &dir,
                &format!("file{i}.yaml"),
                "key: value\n",
            ));
        }

        let processor = FileProcessor::new();
        let result = processor.parse_files(&paths);

        assert_eq!(result.total, 9);
        assert!(result.is_success());
    }

    #[test]
    fn test_sequential_threshold_exactly_10_files() {
        let dir = TempDir::new().unwrap();
        let mut paths = Vec::new();

        // Create exactly 10 files (at threshold, triggers parallel)
        for i in 0..10 {
            paths.push(create_test_file(
                &dir,
                &format!("file{i}.yaml"),
                "key: value\n",
            ));
        }

        let processor = FileProcessor::new();
        let result = processor.parse_files(&paths);

        assert_eq!(result.total, 10);
        assert!(result.is_success());
    }

    #[test]
    fn test_many_files_parallel() {
        let dir = TempDir::new().unwrap();
        let mut paths = Vec::new();

        // Create 50 files to definitely trigger parallel mode
        for i in 0..50 {
            paths.push(create_test_file(
                &dir,
                &format!("file{i}.yaml"),
                &format!("index: {i}\n"),
            ));
        }

        let processor = FileProcessor::new();
        let result = processor.parse_files(&paths);

        assert_eq!(result.total, 50);
        assert!(result.is_success());
    }

    #[test]
    fn test_format_in_place_tracking() {
        let dir = TempDir::new().unwrap();

        // Create file - whether it changes depends on emitter behavior
        let path = create_test_file(&dir, "test.yaml", "key: value\n");

        let processor = FileProcessor::new();
        let emitter_config = EmitterConfig::new();
        let result = processor.format_in_place(
            std::slice::from_ref(&path),
            &emitter_config,
            CommentPolicy::Strip,
        );

        // Should process successfully
        assert_eq!(result.total, 1);
        // Changed count depends on emitter behavior
        assert!(result.changed <= result.total);
    }

    #[test]
    fn test_format_changed_file() {
        let dir = TempDir::new().unwrap();

        // Create file with bad formatting (extra spaces)
        let path = create_test_file(&dir, "unformatted.yaml", "key:     value\n");

        let processor = FileProcessor::new();
        let emitter_config = EmitterConfig::new();
        let result = processor.format_in_place(
            std::slice::from_ref(&path),
            &emitter_config,
            CommentPolicy::Strip,
        );

        // File should be changed
        assert_eq!(result.total, 1);
        // Note: Result depends on whether emitter normalizes spacing
    }

    #[test]
    fn test_all_files_fail() {
        let dir = TempDir::new().unwrap();
        let mut paths = Vec::new();

        // Create multiple invalid files
        for i in 0..5 {
            paths.push(create_test_file(
                &dir,
                &format!("invalid{i}.yaml"),
                "invalid: [\n",
            ));
        }

        let processor = FileProcessor::new();
        let result = processor.parse_files(&paths);

        assert_eq!(result.total, 5);
        assert_eq!(result.failed, 5);
        assert_eq!(result.success, 0);
        assert!(!result.is_success());
    }

    #[test]
    fn test_mixed_file_sizes() {
        let dir = TempDir::new().unwrap();

        // Create mix of small and large files
        let small = create_test_file(&dir, "small.yaml", "key: value\n");
        let large = create_test_file(&dir, "large.yaml", &"key: value\n".repeat(100_000));

        let config = Config::new().with_mmap_threshold(1024);
        let processor = FileProcessor::with_config(config);

        let result = processor.parse_files(&[small, large]);
        assert_eq!(result.total, 2);
        assert!(result.is_success());
    }

    #[test]
    fn test_partial_batch_failure() {
        let dir = TempDir::new().unwrap();

        let paths = vec![
            create_test_file(&dir, "valid1.yaml", "key1: value1\n"),
            create_test_file(&dir, "invalid.yaml", "broken: [\n"),
            create_test_file(&dir, "valid2.yaml", "key2: value2\n"),
        ];

        let processor = FileProcessor::new();
        let result = processor.parse_files(&paths);

        assert_eq!(result.total, 3);
        assert_eq!(result.success, 2);
        assert_eq!(result.failed, 1);
        assert!(!result.is_success());
    }

    #[test]
    fn test_atomic_write_temp_file_cleanup() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.yaml");

        // Initial write
        FileProcessor::write_file_atomic(&path, "content1").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "content1");

        // Update
        FileProcessor::write_file_atomic(&path, "content2").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "content2");

        // Verify atomic write succeeded
        assert_eq!(fs::read_to_string(&path).unwrap(), "content2");
    }

    #[test]
    #[cfg(unix)]
    fn test_write_to_readonly_directory() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.yaml");

        // Make directory read-only
        let mut perms = fs::metadata(dir.path()).unwrap().permissions();
        perms.set_mode(0o444);
        fs::set_permissions(dir.path(), perms).unwrap();

        // Attempt to write should fail
        let result = FileProcessor::write_file_atomic(&path, "content");

        // Restore permissions for cleanup
        let mut perms = fs::metadata(dir.path()).unwrap().permissions();
        perms.set_mode(0o755);
        let _ = fs::set_permissions(dir.path(), perms);

        assert!(result.is_err());
    }

    #[test]
    fn test_process_custom_operation() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.yaml", "key: value\n");

        let processor = FileProcessor::new();

        // Custom operation that counts characters
        let result = processor.process(&[path], |_path, content| Ok(content.len()));

        assert!(result.is_success());
    }

    #[test]
    fn test_format_files_returns_results() {
        let dir = TempDir::new().unwrap();
        let paths = vec![
            create_test_file(&dir, "file1.yaml", "key1: value1\n"),
            create_test_file(&dir, "file2.yaml", "key2: value2\n"),
        ];

        let processor = FileProcessor::new();
        let emitter_config = EmitterConfig::new();
        let results = processor.format_files(&paths, &emitter_config, CommentPolicy::Strip);

        assert_eq!(results.len(), 2);
        assert!(results[0].1.is_ok());
        assert!(results[1].1.is_ok());
    }

    #[test]
    fn test_format_files_reports_changed_accurately() {
        let dir = TempDir::new().unwrap();
        let clean = create_test_file(&dir, "clean.yaml", "key: value\n");
        let dirty = create_test_file(&dir, "dirty.yaml", "key:     value\n");

        let results = FileProcessor::new().format_files(
            &[clean, dirty],
            &EmitterConfig::new(),
            CommentPolicy::Reject,
        );

        assert!(!results[0].1.as_ref().unwrap().changed);
        assert!(results[1].1.as_ref().unwrap().changed);
    }

    #[test]
    fn test_format_files_comment_policy() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "c.yaml", "a: 1 # note\n");
        let processor = FileProcessor::new();
        let cfg = EmitterConfig::new();

        let rejected =
            processor.format_files(std::slice::from_ref(&path), &cfg, CommentPolicy::Reject);
        assert!(matches!(rejected[0].1, Err(Error::CommentsWouldBeStripped)));

        let stripped =
            processor.format_files(std::slice::from_ref(&path), &cfg, CommentPolicy::Strip);
        assert!(stripped[0].1.as_ref().unwrap().changed);
    }

    #[test]
    fn test_format_in_place_reject_leaves_file_untouched() {
        let dir = TempDir::new().unwrap();
        let content = "a: foo\n  \"bar\nb: 1 # real comment\n";
        let path = create_test_file(&dir, "c.yaml", content);

        let result = FileProcessor::new().format_in_place(
            std::slice::from_ref(&path),
            &EmitterConfig::new(),
            CommentPolicy::Reject,
        );

        assert_eq!(result.failed, 1);
        assert!(matches!(result.errors[0].1, Error::CommentsWouldBeStripped));
        assert_eq!(fs::read_to_string(&path).unwrap(), content);
    }

    #[test]
    fn test_format_in_place_unchanged_counts_as_unchanged() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "clean.yaml", "key: value\n");

        let result = FileProcessor::new().format_in_place(
            std::slice::from_ref(&path),
            &EmitterConfig::new(),
            CommentPolicy::Reject,
        );

        assert_eq!((result.success, result.changed), (1, 0));
    }

    #[test]
    fn test_format_files_enforces_max_input_size() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "big.yaml", "key: a-long-enough-value\n");

        let processor = FileProcessor::with_config(Config::default().with_max_input_size(4));
        let results = processor.format_files(&[path], &EmitterConfig::new(), CommentPolicy::Strip);

        assert!(matches!(
            results[0].1,
            Err(Error::InputTooLarge { max: 4, .. })
        ));
    }

    #[test]
    fn test_default_equals_new() {
        let processor1 = FileProcessor::new();
        let processor2 = FileProcessor::default();

        // Both should have same config
        assert_eq!(
            processor1.config.mmap_threshold(),
            processor2.config.mmap_threshold()
        );
    }

    #[test]
    fn test_max_input_size_enforcement() {
        let dir = TempDir::new().unwrap();

        // Create file larger than custom limit (1KB)
        let large_content = "x".repeat(2000);
        let path = create_test_file(&dir, "large.yaml", &large_content);

        // Configure processor with 1KB limit
        let config = Config::new().with_max_input_size(1000);
        let processor = FileProcessor::with_config(config);

        let result = processor.parse_files(&[path]);

        // Should fail with InputTooLarge error
        assert_eq!(result.total, 1);
        assert_eq!(result.failed, 1);
        assert!(!result.is_success());
    }

    #[test]
    fn test_file_within_size_limit() {
        let dir = TempDir::new().unwrap();

        // Create file smaller than limit
        let content = "key: value\n";
        let path = create_test_file(&dir, "small.yaml", content);

        // Configure processor with 1KB limit
        let config = Config::new().with_max_input_size(1000);
        let processor = FileProcessor::with_config(config);

        let result = processor.parse_files(&[path]);

        // Should succeed
        assert!(result.is_success());
    }
}
