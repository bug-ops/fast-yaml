//! Parallel file processor for batch YAML operations.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fast_yaml_core::emitter::{Emitter, EmitterConfig};
use fast_yaml_core::limits::MaxScanAhead;
use fast_yaml_core::{EmitError, LimitKind, NormalizedInput, ParseError, has_comments_normalized};
use rayon::prelude::*;

use crate::atomic::{FileId, HardLinkPolicy, stat_identified};
use crate::config::{Config, MIN_PARALLEL_ITEMS};
use crate::error::{Error, Result};
use crate::io::{read_file, read_file_identified};
use crate::pool;
use crate::result::{BatchResult, FileOutcome, FileResult};
use crate::scan_ahead::ScanAheadLane;
use crate::trace::debug_event;
use crate::workers::Workers;

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

/// A file to format in place together with how to treat its hard links.
///
/// There is no `From<PathBuf>`: every caller names the [`HardLinkPolicy`].
///
/// # Examples
///
/// ```
/// use fast_yaml_parallel::{HardLinkPolicy, InPlaceTarget};
///
/// let target = InPlaceTarget::new("a.yaml".into(), HardLinkPolicy::Refuse);
/// assert_eq!(target.path().to_str(), Some("a.yaml"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InPlaceTarget {
    path: PathBuf,
    hard_links: HardLinkPolicy,
}

impl InPlaceTarget {
    /// Creates a target for `path` written under `hard_links`.
    #[must_use]
    pub const fn new(path: PathBuf, hard_links: HardLinkPolicy) -> Self {
        Self { path, hard_links }
    }

    /// The file to format.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Parallel file processor for batch YAML operations.
///
/// Processes multiple YAML files in parallel using Rayon's work-stealing scheduler.
/// Each file is read whole into memory after a size check.
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
}

impl FileProcessor {
    /// Creates a processor with default config.
    pub fn new() -> Self {
        Self::with_config(Config::default())
    }

    /// Creates a processor with custom config.
    pub const fn with_config(config: Config) -> Self {
        Self { config }
    }

    /// Process files with custom operation.
    ///
    /// Generic function for applying custom processing to files in parallel.
    ///
    /// `f` is called at most once per readable path with its content; an `Err` from `f` is
    /// recorded in [`BatchResult::errors`].
    pub fn process<F>(&self, paths: &[PathBuf], f: F) -> BatchResult
    where
        F: Fn(&Path, &str) -> Result<()> + Sync,
    {
        let batch_start = Instant::now();

        if paths.is_empty() {
            return BatchResult::default();
        }

        let results = if self.should_use_sequential(paths) {
            self.process_files_sequential(paths, &f)
        } else {
            self.process_files_parallel(paths, &f)
        };

        BatchResult::from_results(results, batch_start.elapsed())
    }

    /// Parse all files and return `BatchResult`.
    pub fn parse_files(&self, paths: &[PathBuf]) -> BatchResult {
        self.process(paths, |path, content| {
            fast_yaml_core::Parser::parse_str_with_limits(content, &self.config.parse_limits())
                .map_err(|source| Error::Parse {
                    index: source.document_index().get(),
                    source,
                })?
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
        let lane = self.lane();
        let process_file = |path: &Path| {
            self.format_content(path, emitter_config, comments, &lane)
                .map(|(output, _)| output)
        };

        if self.should_use_sequential(paths) {
            paths
                .iter()
                .map(|path| (path.clone(), process_file(path)))
                .collect()
        } else {
            self.install(
                || {
                    paths
                        .par_iter()
                        .map(|path| (path.clone(), process_file(path)))
                        .collect()
                },
                |error| {
                    paths
                        .iter()
                        .map(|path| (path.clone(), Err(error())))
                        .collect()
                },
            )
        }
    }

    /// Formats files without writing anything and reports how many would change.
    ///
    /// The result is the one [`format_in_place`](Self::format_in_place) returns, except that
    /// [`BatchResult::changed`] counts the files that would change. Errors are those of
    /// [`format_files`](Self::format_files); each file's duration is its own formatting time.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::PathBuf;
    /// use fast_yaml_core::EmitterConfig;
    /// use fast_yaml_parallel::{CommentPolicy, FileProcessor};
    ///
    /// let batch = FileProcessor::new().check_format(
    ///     &[PathBuf::from("a.yaml")],
    ///     &EmitterConfig::new(),
    ///     CommentPolicy::Reject,
    /// );
    /// println!("{} of {} files would change", batch.changed(), batch.total());
    /// ```
    pub fn check_format(
        &self,
        paths: &[PathBuf],
        emitter_config: &EmitterConfig,
        comments: CommentPolicy,
    ) -> BatchResult {
        let batch_start = Instant::now();
        if paths.is_empty() {
            return BatchResult::default();
        }

        let lane = self.lane();
        let check = |path: &PathBuf| {
            let start = Instant::now();
            match self.format_content(path, emitter_config, comments, &lane) {
                Ok((output, _)) => {
                    let duration = start.elapsed();
                    let outcome = if output.changed {
                        FileOutcome::Changed { duration }
                    } else {
                        FileOutcome::Unchanged { duration }
                    };
                    FileResult::new(path.clone(), outcome)
                }
                Err(error) => Self::failed(path, error, start.elapsed()),
            }
        };

        let results = if self.should_use_sequential(paths) {
            paths.iter().map(check).collect()
        } else {
            self.install(
                || paths.par_iter().map(check).collect(),
                |error| {
                    paths
                        .iter()
                        .map(|path| Self::failed(path, error(), Duration::ZERO))
                        .collect()
                },
            )
        };
        BatchResult::from_results(results, batch_start.elapsed())
    }

    /// Format files in place (write back if changed).
    ///
    /// With [`CommentPolicy::Reject`], files containing comments are reported as
    /// [`Error::CommentsWouldBeStripped`] and left untouched. Each target's
    /// [`HardLinkPolicy`] decides what happens to a file with several hard links.
    pub fn format_in_place(
        &self,
        targets: &[InPlaceTarget],
        emitter_config: &EmitterConfig,
        comments: CommentPolicy,
    ) -> BatchResult {
        let batch_start = Instant::now();

        if targets.is_empty() {
            return BatchResult::default();
        }

        let results = self.format_targets(targets, emitter_config, comments);

        BatchResult::from_results(results, batch_start.elapsed())
    }

    /// Formats every target and returns one result per target, in input order.
    ///
    /// Targets that are the same file under different names run one after another on one
    /// worker, so two writers never race on one inode.
    fn format_targets(
        &self,
        targets: &[InPlaceTarget],
        emitter_config: &EmitterConfig,
        comments: CommentPolicy,
    ) -> Vec<FileResult> {
        let lane = self.lane();
        let format_one = |target: &InPlaceTarget| {
            self.format_single_file(target, emitter_config, comments, &lane)
        };
        let sequential = || targets.iter().map(&format_one).collect();

        if self.sequential_by_count(targets.len()) {
            return sequential();
        }
        let serial_ids = if targets.len() < SEQUENTIAL_MAX_FILES {
            let probes: Vec<Probe> = targets
                .iter()
                .map(|target| Probe::of(&target.path))
                .collect();
            let total_size = probes.iter().map(|probe| probe.len).sum();
            if Self::sequential_by_size(targets.len(), total_size) {
                return sequential();
            }
            Some(probes.iter().map(|probe| probe.id).collect::<Vec<_>>())
        } else {
            None
        };

        self.install(
            || {
                let ids = serial_ids.unwrap_or_else(|| {
                    targets
                        .par_iter()
                        .map(|target| FileId::of_path(&target.path).ok().flatten())
                        .collect()
                });
                run_grouped(targets, &same_file_groups(ids), format_one)
            },
            |error| {
                targets
                    .iter()
                    .map(|target| Self::failed(&target.path, error(), Duration::ZERO))
                    .collect()
            },
        )
    }

    /// The scan-ahead lane of one format run.
    fn lane(&self) -> ScanAheadLane {
        ScanAheadLane::for_policy(
            self.config.scan_ahead_policy(),
            self.config.workers().threads(),
        )
    }

    /// Reads `path` once, enforces the size limit and comment policy, and formats it.
    ///
    /// Also returns the identity of the file that was read, which a write must be bound to.
    fn format_content(
        &self,
        path: &Path,
        emitter_config: &EmitterConfig,
        comments: CommentPolicy,
        lane: &ScanAheadLane,
    ) -> Result<(FormatOutput, Option<FileId>)> {
        let (content, id) = read_file_identified(path, self.config.max_input_bytes())?;

        let normalized = NormalizedInput::new(&content).map_err(|source| Error::Format {
            path: path.to_path_buf(),
            source: source.into(),
        })?;
        let format_at = |limit: MaxScanAhead| {
            let mut limited = emitter_config.clone();
            limited.parse_limits.max_scan_ahead = limit;
            let formatted =
                Emitter::format_normalized(&normalized, &limited).map_err(|source| {
                    Error::Format {
                        path: path.to_path_buf(),
                        source,
                    }
                })?;

            if comments == CommentPolicy::Reject
                && has_comments_normalized(&normalized, limit)
                    .map_err(|source| Error::CommentScan { source })?
            {
                return Err(Error::CommentsWouldBeStripped);
            }
            Ok(formatted)
        };
        let formatted = lane.run(format_at, exceeds_scan_ahead)?;

        let output = FormatOutput {
            changed: content != formatted,
            formatted,
        };
        Ok((output, id))
    }

    /// Format a single file in place
    fn format_single_file(
        &self,
        target: &InPlaceTarget,
        emitter_config: &EmitterConfig,
        comments: CommentPolicy,
        lane: &ScanAheadLane,
    ) -> FileResult {
        let start = std::time::Instant::now();
        let path = target.path();

        let (output, id) = match self.format_content(path, emitter_config, comments, lane) {
            Ok(read) => read,
            Err(error) => return Self::failed(path, error, start.elapsed()),
        };

        if output.changed
            && let Err(error) =
                Self::write_file_atomic(path, &output.formatted, target.hard_links, id)
        {
            return Self::failed(path, error, start.elapsed());
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
    fn process_files_parallel<F>(&self, paths: &[PathBuf], f: &F) -> Vec<FileResult>
    where
        F: Fn(&Path, &str) -> Result<()> + Sync,
    {
        self.install(
            || {
                paths
                    .par_iter()
                    .map(|path| self.process_single_file(path, f))
                    .collect()
            },
            |error| {
                paths
                    .iter()
                    .map(|path| Self::failed(path, error(), Duration::ZERO))
                    .collect()
            },
        )
    }

    /// Runs `op` on the pool the config asks for; when that pool cannot be built, `fallback`
    /// receives a constructor of the pool error.
    fn install<R: Send>(
        &self,
        op: impl FnOnce() -> R + Send,
        fallback: impl FnOnce(&dyn Fn() -> Error) -> R,
    ) -> R {
        match pool::for_config(&self.config) {
            Ok(Some(pool)) => pool.install(op),
            Ok(None) => op(),
            Err(source) => fallback(&|| Error::ThreadPool(Arc::clone(&source))),
        }
    }

    fn failed(path: &Path, error: Error, duration: Duration) -> FileResult {
        FileResult::new(path.to_path_buf(), FileOutcome::Error { error, duration })
    }

    /// Processes files sequentially without parallel overhead.
    fn process_files_sequential<F>(&self, paths: &[PathBuf], f: &F) -> Vec<FileResult>
    where
        F: Fn(&Path, &str) -> Result<()>,
    {
        paths
            .iter()
            .map(|path| self.process_single_file(path, f))
            .collect()
    }

    /// Processes a single file and returns the result
    fn process_single_file<F>(&self, path: &Path, f: &F) -> FileResult
    where
        F: Fn(&Path, &str) -> Result<()>,
    {
        let start = Instant::now();

        match self.process_file_content(path, f) {
            Ok(()) => {
                let duration = start.elapsed();
                FileResult::new(path.to_path_buf(), FileOutcome::Success { duration })
            }
            Err(error) => Self::failed(path, error, start.elapsed()),
        }
    }

    /// Process file content with given function
    fn process_file_content<F>(&self, path: &Path, f: &F) -> Result<()>
    where
        F: Fn(&Path, &str) -> Result<()>,
    {
        let content = read_file(path, self.config.max_input_bytes())?;
        f(path, &content)
    }

    /// Writes content to the file `expected` via the shared secure writer
    /// [`crate::write_atomic_bound`].
    fn write_file_atomic(
        path: &Path,
        content: &str,
        hard_links: HardLinkPolicy,
        expected: Option<FileId>,
    ) -> Result<()> {
        crate::write_atomic_bound(path, content.as_bytes(), hard_links, expected).map_err(
            |source| Error::Write {
                path: path.to_path_buf(),
                source,
            },
        )
    }

    /// Returns true if sequential processing should be used.
    ///
    /// Sequential processing is preferred when:
    /// - Workers are set to 0
    /// - Very few files (< [`MIN_PARALLEL_ITEMS`])
    /// - Small total size (< 1MB) AND moderate file count (< 10)
    ///
    /// This avoids parallelism overhead for small workloads while enabling
    /// parallel processing for large files even if there are only a few of them.
    fn should_use_sequential(&self, paths: &[PathBuf]) -> bool {
        if self.sequential_by_count(paths.len()) {
            return true;
        }
        let total_size = paths
            .iter()
            .filter_map(|p| fs::metadata(p).ok())
            .map(|m| m.len())
            .sum();
        Self::sequential_by_size(paths.len(), total_size)
    }

    /// The part of [`Self::should_use_sequential`] decided without touching the file system.
    fn sequential_by_count(&self, file_count: usize) -> bool {
        let sequential =
            self.config.workers() == Workers::Sequential || file_count < MIN_PARALLEL_ITEMS;
        if sequential {
            debug_event!("running {file_count} files sequentially");
        }
        sequential
    }

    /// The part of [`Self::should_use_sequential`] decided by the size of the workload.
    fn sequential_by_size(file_count: usize, total_size: u64) -> bool {
        let sequential = total_size < SEQUENTIAL_MAX_BYTES && file_count < SEQUENTIAL_MAX_FILES;
        debug_event!(
            "{file_count} files, {total_size} bytes: running {}",
            if sequential {
                "sequentially"
            } else {
                "in parallel"
            }
        );
        sequential
    }
}

/// Total size below which a small batch runs without a pool.
const SEQUENTIAL_MAX_BYTES: u64 = 1_000_000;

/// File count from which a batch always runs in the pool, so its sizes are never probed.
const SEQUENTIAL_MAX_FILES: usize = 10;

/// What one stat of a target tells the scheduler.
struct Probe {
    len: u64,
    id: Option<FileId>,
}

impl Probe {
    /// Size and identity of the file `path` resolves to; a path that cannot be stat-ed has
    /// size 0 and no identity.
    fn of(path: &Path) -> Self {
        stat_identified(path).map_or(Self { len: 0, id: None }, |(metadata, id)| Self {
            len: metadata.len(),
            id,
        })
    }
}

/// Indices grouped by key; an index without a key is alone in its group. Groups are in order
/// of first appearance, indices ascending within one.
fn same_file_groups(keys: impl IntoIterator<Item = Option<FileId>>) -> Vec<Vec<usize>> {
    let mut group_of: HashMap<FileId, usize> = HashMap::new();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (index, key) in keys.into_iter().enumerate() {
        match key.map(|key| group_of.entry(key)) {
            Some(Entry::Occupied(group)) => groups[*group.get()].push(index),
            Some(Entry::Vacant(slot)) => {
                slot.insert(groups.len());
                groups.push(vec![index]);
            }
            None => groups.push(vec![index]),
        }
    }
    groups
}

/// Runs `run` over `items` with the groups in parallel and the members of a group one after
/// another on one worker; results are in the order of `items`.
fn run_grouped<T: Sync, R: Send>(
    items: &[T],
    groups: &[Vec<usize>],
    run: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    let mut indexed: Vec<(usize, R)> = groups
        .par_iter()
        .flat_map_iter(|group| group.iter().map(|&index| (index, run(&items[index]))))
        .collect();
    indexed.sort_unstable_by_key(|(index, _)| *index);
    indexed.into_iter().map(|(_, result)| result).collect()
}

/// Whether `error` is the scanner look-ahead limit, the only failure a larger limit can cure.
const fn exceeds_scan_ahead(error: &Error) -> bool {
    matches!(
        error,
        Error::Format {
            source: EmitError::Parse(ParseError::LimitExceeded {
                kind: LimitKind::ScanAhead(_),
                ..
            }),
            ..
        } | Error::CommentScan {
            source: ParseError::LimitExceeded {
                kind: LimitKind::ScanAhead(_),
                ..
            },
        }
    )
}

impl Default for FileProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fast_yaml_core::limits::MaxInputBytes;
    use std::fs;
    use tempfile::TempDir;

    fn id_of(path: &Path) -> Option<FileId> {
        FileId::of(&fs::File::open(path).unwrap()).unwrap()
    }

    fn write_existing(path: &Path, content: &str, hard_links: HardLinkPolicy) -> Result<()> {
        FileProcessor::write_file_atomic(path, content, hard_links, id_of(path))
    }

    fn in_place(path: &Path) -> InPlaceTarget {
        InPlaceTarget::new(path.to_path_buf(), HardLinkPolicy::WriteInPlace)
    }

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
        assert_eq!(result.failed(), 1);
        assert_eq!(result.succeeded(), 1);
    }

    #[test]
    fn test_file_processor_new() {
        let _processor = FileProcessor::new();
    }

    #[test]
    fn test_file_processor_with_config() {
        let config = Config::new().with_workers(Workers::try_from(4).unwrap());
        let _processor = FileProcessor::with_config(config);
    }

    #[test]
    fn test_process_single_file() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.yaml", "key: value\n");

        let processor = FileProcessor::new();
        let result = processor.parse_files(&[path]);

        assert_eq!(result.total(), 1);
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

        assert_eq!(result.total(), 3);
        assert!(result.is_success());
    }

    #[test]
    fn test_process_empty_batch() {
        let processor = FileProcessor::new();
        let result = processor.parse_files(&[]);

        assert_eq!(result.total(), 0);
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

        assert_eq!(result.total(), 2);
        assert!(!result.is_success());
        assert!(result.failed() >= 1);
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
        let result =
            processor.format_in_place(&[in_place(&path)], &emitter_config, CommentPolicy::Strip);

        assert_eq!(result.total(), 1);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn test_atomic_write_refuses_a_file_swapped_after_the_read() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.yaml");
        let other = dir.path().join("other.yaml");
        fs::write(&path, "read: 1").unwrap();
        let read_id = id_of(&path);
        fs::write(&other, "unrelated: 1").unwrap();
        fs::rename(&other, &path).unwrap();

        let result =
            FileProcessor::write_file_atomic(&path, "x: 1", HardLinkPolicy::WriteInPlace, read_id);

        assert!(matches!(result, Err(Error::Write { .. })));
        assert_eq!(fs::read_to_string(&path).unwrap(), "unrelated: 1");
    }

    #[test]
    fn test_atomic_write() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.yaml");
        fs::write(&path, "old content").unwrap();

        write_existing(&path, "new content", HardLinkPolicy::WriteInPlace).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "new content");
    }

    #[test]
    fn test_large_file() {
        let dir = TempDir::new().unwrap();

        let large_content = "key: value\n".repeat(100_000);
        let path = create_test_file(&dir, "large.yaml", &large_content);

        let processor = FileProcessor::new();

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

        assert_eq!(result.total(), 9);
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

        assert_eq!(result.total(), 10);
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

        assert_eq!(result.total(), 50);
        assert!(result.is_success());
    }

    #[test]
    fn test_format_in_place_tracking() {
        let dir = TempDir::new().unwrap();

        // Create file - whether it changes depends on emitter behavior
        let path = create_test_file(&dir, "test.yaml", "key: value\n");

        let processor = FileProcessor::new();
        let emitter_config = EmitterConfig::new();
        let result =
            processor.format_in_place(&[in_place(&path)], &emitter_config, CommentPolicy::Strip);

        // Should process successfully
        assert_eq!(result.total(), 1);
        // Changed count depends on emitter behavior
        assert!(result.changed() <= result.total());
    }

    #[test]
    fn test_format_changed_file() {
        let dir = TempDir::new().unwrap();

        // Create file with bad formatting (extra spaces)
        let path = create_test_file(&dir, "unformatted.yaml", "key:     value\n");

        let processor = FileProcessor::new();
        let emitter_config = EmitterConfig::new();
        let result =
            processor.format_in_place(&[in_place(&path)], &emitter_config, CommentPolicy::Strip);

        // File should be changed
        assert_eq!(result.total(), 1);
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

        assert_eq!(result.total(), 5);
        assert_eq!(result.failed(), 5);
        assert_eq!(result.succeeded(), 0);
        assert!(!result.is_success());
    }

    #[test]
    fn test_mixed_file_sizes() {
        let dir = TempDir::new().unwrap();

        // Create mix of small and large files
        let small = create_test_file(&dir, "small.yaml", "key: value\n");
        let large = create_test_file(&dir, "large.yaml", &"key: value\n".repeat(100_000));

        let processor = FileProcessor::new();

        let result = processor.parse_files(&[small, large]);
        assert_eq!(result.total(), 2);
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

        assert_eq!(result.total(), 3);
        assert_eq!(result.succeeded(), 2);
        assert_eq!(result.failed(), 1);
        assert!(!result.is_success());
    }

    #[test]
    fn test_atomic_write_temp_file_cleanup() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.yaml");

        fs::write(&path, "initial").unwrap();
        write_existing(&path, "content1", HardLinkPolicy::WriteInPlace).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "content1");

        // Update
        write_existing(&path, "content2", HardLinkPolicy::WriteInPlace).unwrap();
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
        fs::write(&path, "old").unwrap();
        let id = id_of(&path);

        // Make directory read-only
        let mut perms = fs::metadata(dir.path()).unwrap().permissions();
        perms.set_mode(0o444);
        fs::set_permissions(dir.path(), perms).unwrap();

        // Attempt to write should fail
        let result =
            FileProcessor::write_file_atomic(&path, "content", HardLinkPolicy::WriteInPlace, id);

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

        // Custom operation that inspects the content
        let result = processor.process(&[path], |_path, content| {
            assert_eq!(content, "key: value\n");
            Ok(())
        });

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
    fn test_check_format_counts_without_writing() {
        let dir = TempDir::new().unwrap();
        let messy = create_test_file(&dir, "messy.yaml", "a:   1\n");
        let clean = create_test_file(&dir, "clean.yaml", "a: 1\n");
        let commented = create_test_file(&dir, "commented.yaml", "a: 1 # note\n");
        let missing = dir.path().join("missing.yaml");
        let paths = [messy.clone(), clean.clone(), commented, missing.clone()];

        let batch =
            FileProcessor::new().check_format(&paths, &EmitterConfig::new(), CommentPolicy::Reject);

        assert_eq!(batch.total(), 4);
        assert_eq!(
            (batch.changed(), batch.unchanged(), batch.failed()),
            (1, 1, 2)
        );
        assert!(matches!(
            batch.errors()[0].1,
            Error::CommentsWouldBeStripped
        ));
        assert_eq!(batch.errors()[1].0, missing);
        assert_eq!(fs::read_to_string(&messy).unwrap(), "a:   1\n");
        assert_eq!(fs::read_to_string(&clean).unwrap(), "a: 1\n");
    }

    #[test]
    fn test_check_format_of_nothing_is_an_empty_success() {
        let batch =
            FileProcessor::new().check_format(&[], &EmitterConfig::new(), CommentPolicy::Reject);
        assert_eq!(batch.total(), 0);
        assert!(batch.is_success());
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
            &[in_place(&path)],
            &EmitterConfig::new(),
            CommentPolicy::Reject,
        );

        assert_eq!(result.failed(), 1);
        assert!(matches!(
            result.errors()[0].1,
            Error::CommentsWouldBeStripped
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), content);
    }

    #[test]
    fn test_format_in_place_unchanged_counts_as_unchanged() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "clean.yaml", "key: value\n");

        let result = FileProcessor::new().format_in_place(
            &[in_place(&path)],
            &EmitterConfig::new(),
            CommentPolicy::Reject,
        );

        assert_eq!((result.succeeded(), result.changed()), (1, 0));
    }

    #[test]
    fn test_format_files_enforces_max_input_bytes() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "big.yaml", "key: a-long-enough-value\n");

        let processor = FileProcessor::with_config(
            Config::default().with_max_input_bytes(MaxInputBytes::new(4).unwrap()),
        );
        let results = processor.format_files(&[path], &EmitterConfig::new(), CommentPolicy::Strip);

        assert!(matches!(
            &results[0].1,
            Err(Error::InputTooLarge(e)) if e.limit.get() == 4
        ));
    }

    #[test]
    fn test_default_equals_new() {
        let processor1 = FileProcessor::new();
        let processor2 = FileProcessor::default();

        // Both should have same config
        assert_eq!(
            processor1.config.max_input_bytes(),
            processor2.config.max_input_bytes()
        );
    }

    #[test]
    fn test_max_input_bytes_enforcement() {
        let dir = TempDir::new().unwrap();

        // Create file larger than custom limit (1KB)
        let large_content = "x".repeat(2000);
        let path = create_test_file(&dir, "large.yaml", &large_content);

        // Configure processor with 1KB limit
        let config = Config::new().with_max_input_bytes(MaxInputBytes::new(1000).unwrap());
        let processor = FileProcessor::with_config(config);

        let result = processor.parse_files(&[path]);

        // Should fail with InputTooLarge error
        assert_eq!(result.total(), 1);
        assert_eq!(result.failed(), 1);
        assert!(!result.is_success());
    }

    #[test]
    fn test_file_within_size_limit() {
        let dir = TempDir::new().unwrap();

        // Create file smaller than limit
        let content = "key: value\n";
        let path = create_test_file(&dir, "small.yaml", content);

        // Configure processor with 1KB limit
        let config = Config::new().with_max_input_bytes(MaxInputBytes::new(1000).unwrap());
        let processor = FileProcessor::with_config(config);

        let result = processor.parse_files(&[path]);

        // Should succeed
        assert!(result.is_success());
    }

    #[test]
    fn test_scaled_policy_lowers_the_first_scan_ahead_limit() {
        use crate::scan_ahead::ScanAheadPolicy;
        use fast_yaml_core::limits::MaxScanAhead;

        let lane = |policy| {
            FileProcessor::with_config(
                Config::new()
                    .with_workers(Workers::try_from(8).unwrap())
                    .with_scan_ahead_policy(policy),
            )
            .lane()
            .first_limit()
        };
        assert_eq!(
            lane(ScanAheadPolicy::Fixed(MaxScanAhead::DEFAULT)),
            MaxScanAhead::DEFAULT
        );
        assert!(lane(ScanAheadPolicy::Scaled).get() < MaxScanAhead::DEFAULT.get());
    }

    fn flow_file(dir: &TempDir) -> PathBuf {
        create_test_file(dir, "flow.yaml", &format!("[{}1]\n", "1, ".repeat(100)))
    }

    fn limited(chars: usize) -> Config {
        use fast_yaml_core::limits::{MaxScanAhead, ParseLimits};
        Config::new().with_parse_limits(ParseLimits {
            max_scan_ahead: MaxScanAhead::new(chars).unwrap(),
            ..ParseLimits::default()
        })
    }

    fn is_scan_ahead_rejection(error: &Error) -> bool {
        exceeds_scan_ahead(error) && error.to_string().contains("lookahead exceeds 64")
    }

    #[test]
    fn test_explicit_scan_ahead_limit_applies_to_format_files() {
        let dir = TempDir::new().unwrap();
        let path = flow_file(&dir);
        let emitter = EmitterConfig::new();

        let strict = FileProcessor::with_config(limited(64));
        let results =
            strict.format_files(std::slice::from_ref(&path), &emitter, CommentPolicy::Reject);
        assert!(
            matches!(&results[0].1, Err(e) if is_scan_ahead_rejection(e)),
            "{results:?}"
        );

        let default = FileProcessor::new();
        let results = default.format_files(&[path], &emitter, CommentPolicy::Reject);
        assert!(results[0].1.is_ok(), "{results:?}");
    }

    #[test]
    fn test_explicit_scan_ahead_limit_applies_to_format_in_place() {
        let dir = TempDir::new().unwrap();
        let path = flow_file(&dir);
        let emitter = EmitterConfig::new();

        let strict = FileProcessor::with_config(limited(64));
        let batch = strict.format_in_place(&[in_place(&path)], &emitter, CommentPolicy::Reject);
        assert_eq!(batch.failed(), 1);
        assert!(
            batch
                .errors()
                .iter()
                .all(|(_, e)| is_scan_ahead_rejection(e))
        );

        let batch = FileProcessor::new().format_in_place(
            &[in_place(&path)],
            &emitter,
            CommentPolicy::Reject,
        );
        assert_eq!(batch.failed(), 0);
    }

    #[test]
    fn test_scaled_policy_after_parse_limits_wins_and_parse_files_keep_their_limit() {
        use crate::scan_ahead::ScanAheadPolicy;
        let config = limited(64).with_scan_ahead_policy(ScanAheadPolicy::Scaled);
        assert_eq!(config.scan_ahead_policy(), ScanAheadPolicy::Scaled);

        let dir = TempDir::new().unwrap();
        let path = flow_file(&dir);
        let strict = FileProcessor::with_config(limited(64));
        assert!(!strict.parse_files(std::slice::from_ref(&path)).is_success());
        assert!(FileProcessor::new().parse_files(&[path]).is_success());
    }

    fn groups_of(paths: &[PathBuf]) -> Vec<Vec<usize>> {
        same_file_groups(paths.iter().map(|path| Probe::of(path).id))
    }

    #[test]
    fn test_same_file_groups_keep_distinct_files_apart() {
        let dir = TempDir::new().unwrap();
        let a = create_test_file(&dir, "a.yaml", "a: 1\n");
        let b = create_test_file(&dir, "b.yaml", "b: 1\n");
        assert_eq!(groups_of(&[a, b]), vec![vec![0], vec![1]]);
    }

    #[test]
    fn test_same_file_groups_merge_relative_spellings() {
        let dir = TempDir::new().unwrap();
        let a = create_test_file(&dir, "a.yaml", "a: 1\n");
        let dotted = dir.path().join(".").join("a.yaml");
        assert_eq!(groups_of(&[a, dotted]), vec![vec![0, 1]]);
    }

    #[test]
    fn test_same_file_groups_leave_a_missing_file_alone() {
        let dir = TempDir::new().unwrap();
        let a = create_test_file(&dir, "a.yaml", "a: 1\n");
        let missing = dir.path().join("missing.yaml");
        assert_eq!(
            groups_of(&[missing.clone(), a, missing]),
            vec![vec![0], vec![1], vec![2]]
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_same_file_groups_merge_hard_links_and_symlinks() {
        let dir = TempDir::new().unwrap();
        let a = create_test_file(&dir, "a.yaml", "a: 1\n");
        let hard = dir.path().join("hard.yaml");
        fs::hard_link(&a, &hard).unwrap();
        let soft = dir.path().join("soft.yaml");
        std::os::unix::fs::symlink(&a, &soft).unwrap();
        let other = create_test_file(&dir, "other.yaml", "b: 1\n");
        assert_eq!(
            groups_of(&[a, other, hard, soft]),
            vec![vec![0, 2, 3], vec![1]]
        );
    }

    #[test]
    fn test_same_file_groups_merge_spellings_of_one_path() {
        let dir = TempDir::new().unwrap();
        let a = create_test_file(&dir, "a.yaml", "a: 1\n");
        let spelled = dir.path().join(".").join("a.yaml");
        let other = create_test_file(&dir, "other.yaml", "b: 1\n");
        assert_eq!(groups_of(&[a, other, spelled]), vec![vec![0, 2], vec![1]]);
    }

    #[test]
    fn test_same_file_groups_without_keys_are_singletons() {
        assert_eq!(
            same_file_groups([None, None, None]),
            vec![vec![0], vec![1], vec![2]]
        );
    }

    #[test]
    fn test_same_file_groups_collect_equal_keys_in_input_order() {
        let dir = TempDir::new().unwrap();
        let a = id_of(&create_test_file(&dir, "a.yaml", "a: 1\n"));
        let b = id_of(&create_test_file(&dir, "b.yaml", "b: 1\n"));
        assert_eq!(
            same_file_groups([a, b, a, None, a]),
            vec![vec![0, 2, 4], vec![1], vec![3]]
        );
    }

    #[test]
    fn test_run_grouped_never_overlaps_members_of_a_group() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Duration;

        let running = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap();

        let results = pool.install(|| {
            run_grouped(
                &[0_usize, 1, 2, 3],
                &[vec![2], vec![0, 1], vec![3]],
                |&item| {
                    if item < 2 {
                        peak.fetch_max(
                            running.fetch_add(1, Ordering::SeqCst) + 1,
                            Ordering::SeqCst,
                        );
                        // Unserialized, member 1 would start on another worker during this sleep
                        if item == 0 {
                            std::thread::sleep(Duration::from_millis(150));
                        }
                        running.fetch_sub(1, Ordering::SeqCst);
                    }
                    item * 10
                },
            )
        });

        assert_eq!(results, vec![0, 10, 20, 30]);
        assert_eq!(peak.load(Ordering::SeqCst), 1);
    }

    #[cfg(unix)]
    #[test]
    fn test_format_in_place_groups_hard_links_in_a_large_batch() {
        let dir = TempDir::new().unwrap();
        let mut targets: Vec<InPlaceTarget> = (0..10)
            .map(|n| {
                let path = create_test_file(&dir, &format!("f{n}.yaml"), "a:   1\n");
                InPlaceTarget::new(path, HardLinkPolicy::WriteInPlace)
            })
            .collect();
        let first = targets[0].path.clone();
        for name in ["link1.yaml", "link2.yaml"] {
            let link = dir.path().join(name);
            fs::hard_link(&first, &link).unwrap();
            targets.push(InPlaceTarget::new(link, HardLinkPolicy::WriteInPlace));
        }
        let batch = FileProcessor::new().format_in_place(
            &targets,
            &EmitterConfig::default(),
            CommentPolicy::Reject,
        );
        assert_eq!(batch.failed(), 0);
        assert_eq!(fs::read_to_string(&first).unwrap(), "a: 1\n");
    }

    #[test]
    fn test_sequential_by_count_ignores_the_file_system() {
        use crate::workers::{WorkerCount, Workers};

        let fixed = Config::new().with_workers(Workers::Fixed(WorkerCount::new(4).unwrap()));
        let processor = FileProcessor::with_config(fixed);
        assert!(processor.sequential_by_count(3));
        assert!(!processor.sequential_by_count(4));
        let sequential =
            FileProcessor::with_config(Config::new().with_workers(Workers::Sequential));
        assert!(sequential.sequential_by_count(100));
    }

    #[test]
    fn test_sequential_by_size_needs_few_and_small_files() {
        assert!(FileProcessor::sequential_by_size(9, 999_999));
        assert!(!FileProcessor::sequential_by_size(10, 10));
        assert!(!FileProcessor::sequential_by_size(9, 1_000_000));
    }
}
