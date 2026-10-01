//! Batch lint command execution.
//!
//! Files are linted by the pool's workers and reported in file order through a bounded window:
//! at most `window` files are started but not yet reported, so a file's content lives only while
//! a worker lints it and finished results wait in a small reorder buffer. A slow file delays only
//! the reports behind it, not the workers, and both output streams are deterministic whatever
//! the worker interleaving.

use std::collections::HashMap;
use std::fmt;
use std::io::{self, Write};
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

use anyhow::{Context, Result};
use fast_yaml_linter::formatter::{
    FileReport as ReportedFile, ReportFormat, input_error_diagnostic, syntax_diagnostic,
};
use fast_yaml_linter::{
    Diagnostic, Formatter, LintConfig, LintError, Linter, Severity, TextFormatter,
};
use fast_yaml_parallel::{Error as ParallelError, read_file, shared_pool};
use rayon::{Scope, ThreadPool};
use serde::ser::{SerializeSeq, Serializer};
use serde_json::ser::PrettyFormatter;

use crate::cli::{LintFormat, LintOutput};
use crate::commands::lint::report_source;
use crate::config::CommonConfig;
use crate::discovery::FileDiscovery;
use crate::error::{ExitCode, RaiseHint};
use crate::invocation::BatchTarget;

/// Files that may be started but not yet reported, per worker. Finished files hold only their
/// diagnostics, so the window can be wide enough to keep workers busy behind one slow file.
const WINDOW_PER_WORKER: usize = 16;

/// Why a file could not be linted; the file is reported on stderr and counts as an error.
#[derive(Debug)]
enum FileFailure {
    Read {
        path: PathBuf,
        source: ParallelError,
    },
    Lint {
        path: PathBuf,
        source: LintError,
    },
}

impl FileFailure {
    fn hint(&self) -> Option<RaiseHint> {
        match self {
            Self::Read { source, .. } => RaiseHint::of(source),
            Self::Lint { source, .. } => RaiseHint::of(source),
        }
    }
}

impl fmt::Display for FileFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // `Io` already names the path, the other variants do not
            Self::Read {
                source: source @ ParallelError::Io { .. },
                ..
            } => write!(f, "error: {source}")?,
            Self::Read { path, source } => write!(f, "error: '{}': {source}", path.display())?,
            Self::Lint { path, source } => write!(f, "error: '{}': {source}", path.display())?,
        }
        self.hint().map_or(Ok(()), |hint| write!(f, " ({hint})"))
    }
}

/// A linted file: whether it has errors, and what its output format keeps of the diagnostics.
///
/// The file content is dropped inside the worker; text output is rendered there while the
/// content is alive.
struct Linted<P> {
    has_errors: bool,
    payload: P,
}

/// A file that could not be linted, with what its output format keeps of the failure.
struct Failed<X> {
    failure: FileFailure,
    salvage: X,
}

struct FileReport<P, X = ()> {
    path: PathBuf,
    outcome: Result<Linted<P>, Failed<X>>,
}

/// An output format: what a worker keeps of a file's diagnostics, and how the reports are
/// written. One type per format, so a report can only reach the emitter of its own format.
trait OutputFormat: Sync {
    type Payload: Send;
    /// What the format keeps of a failure besides its message on stderr.
    type Salvage: Send;

    /// Reduces the diagnostics of a file with this `content` to what the emitter needs.
    fn payload(&self, diagnostics: Vec<Diagnostic>, content: &str) -> Self::Payload;

    /// Reduces a failure to what the emitter needs; `content` is known when linting failed
    /// after reading.
    fn salvage(&self, failure: &FileFailure, content: Option<&str>) -> Self::Salvage;

    /// Writes the reports in file order, failures to `err` and results to `out`, and returns
    /// whether any file had an error. A write error stops the run.
    fn emit<W: Write, E: Write>(
        &self,
        out: W,
        err: E,
        reports: impl Iterator<Item = FileReport<Self::Payload, Self::Salvage>>,
    ) -> Result<bool>;
}

/// Human-readable output, rendered by the worker.
struct TextOutput {
    use_color: bool,
}

/// One JSON array of all diagnostics.
struct JsonOutput;

/// A CI report: github annotations, SARIF or parsable lines, listed by absolute path.
struct ReportOutput {
    format: ReportFormat,
}

fn lint_one<F: OutputFormat>(
    path: &Path,
    linter: &Linter,
    format: &F,
    is_quiet: bool,
) -> FileReport<F::Payload, F::Salvage> {
    FileReport {
        path: path.to_path_buf(),
        outcome: lint_content(path, linter, format, is_quiet),
    }
}

fn lint_content<F: OutputFormat>(
    path: &Path,
    linter: &Linter,
    format: &F,
    is_quiet: bool,
) -> Result<Linted<F::Payload>, Failed<F::Salvage>> {
    let failed = |failure: FileFailure, content: Option<&str>| Failed {
        salvage: format.salvage(&failure, content),
        failure,
    };
    let content = read_file(path, linter.config().max_input_bytes).map_err(|source| {
        failed(
            FileFailure::Read {
                path: path.to_path_buf(),
                source,
            },
            None,
        )
    })?;

    let mut diagnostics = linter.lint(&content).map_err(|source| {
        failed(
            FileFailure::Lint {
                path: path.to_path_buf(),
                source,
            },
            Some(&content),
        )
    })?;
    if is_quiet {
        diagnostics.retain(|d| d.severity == Severity::Error);
    }
    Ok(Linted {
        has_errors: diagnostics.iter().any(|d| d.severity == Severity::Error),
        payload: format.payload(diagnostics, &content),
    })
}

/// Execute batch linting on multiple files.
///
/// # Errors
///
/// Returns error if file discovery fails or the output cannot be written.
pub fn execute_lint_batch(
    common: &CommonConfig,
    target: &BatchTarget,
    lint_config: &LintConfig,
    format: LintFormat,
) -> Result<ExitCode> {
    let discovery = FileDiscovery::new(target.discovery.clone())
        .context("Failed to initialize file discovery")?;

    let files = discovery
        .discover_source(&target.source)
        .context("Failed to discover files")?;

    let workers = target.workers.unwrap_or_else(|| {
        NonZeroUsize::new(rayon::current_num_threads()).unwrap_or(NonZeroUsize::MIN)
    });
    let pool = shared_pool(workers).context("Failed to build thread pool")?;

    let file_paths: Vec<PathBuf> = files.iter().map(|f| f.path.clone()).collect();
    let linter = Linter::with_config(lint_config.clone());
    let is_quiet = common.output.is_quiet();

    let any_errors = match format.output() {
        LintOutput::Text => run_batch(
            &pool,
            &file_paths,
            &linter,
            &TextOutput {
                use_color: common.output.use_color(),
            },
            is_quiet,
        ),
        LintOutput::Json => run_batch(&pool, &file_paths, &linter, &JsonOutput, is_quiet),
        LintOutput::Report(format) => run_batch(
            &pool,
            &file_paths,
            &linter,
            &ReportOutput { format },
            is_quiet,
        ),
    }?;

    if any_errors {
        Ok(ExitCode::LintErrors)
    } else {
        Ok(ExitCode::Success)
    }
}

/// Lints `file_paths` on the pool and writes the reports of `format` to stdout and stderr.
fn run_batch<F: OutputFormat>(
    pool: &ThreadPool,
    file_paths: &[PathBuf],
    linter: &Linter,
    format: &F,
    is_quiet: bool,
) -> Result<bool> {
    let window = pool
        .current_num_threads()
        .saturating_mul(WINDOW_PER_WORKER)
        .max(1);
    let lint_nth = |index: usize| lint_one(&file_paths[index], linter, format, is_quiet);

    let stdout = io::stdout();
    run_ordered(pool, file_paths.len(), window, &lint_nth, |reports| {
        format.emit(stdout.lock(), io::stderr().lock(), reports)
    })
}

/// Runs `work(0..count)` on the pool and hands the results to `consume` in index order.
///
/// At most `window` indices are started but not yet consumed. The calling thread consumes, so
/// `consume` may block on output without stalling the workers, and it stops the remaining
/// work when it returns early.
fn run_ordered<T: Send, R>(
    pool: &ThreadPool,
    count: usize,
    window: usize,
    work: &(impl Fn(usize) -> T + Sync),
    consume: impl FnOnce(&mut dyn Iterator<Item = T>) -> R,
) -> R {
    let cancelled = AtomicBool::new(false);
    pool.in_place_scope(|scope| {
        // Dropped before the scope joins its jobs, on return and on unwind (a re-raised panic),
        // so the queued work is skipped instead of run
        let _cancel = CancelOnDrop(&cancelled);
        let (sender, receiver) = mpsc::channel::<(usize, std::thread::Result<T>)>();
        let mut ordered = Ordered {
            scope,
            sender,
            receiver,
            work,
            cancelled: &cancelled,
            count,
            window,
            launched: 0,
            consumed: 0,
            finished: HashMap::new(),
        };
        consume(&mut ordered)
    })
}

/// Sets the flag when dropped.
struct CancelOnDrop<'a>(&'a AtomicBool);

impl Drop for CancelOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Iterator over the results of a [`run_ordered`] call, in index order.
struct Ordered<'a, 'scope, T, W> {
    scope: &'a Scope<'scope>,
    sender: Sender<(usize, std::thread::Result<T>)>,
    receiver: Receiver<(usize, std::thread::Result<T>)>,
    work: &'scope W,
    cancelled: &'scope AtomicBool,
    count: usize,
    window: usize,
    launched: usize,
    consumed: usize,
    finished: HashMap<usize, std::thread::Result<T>>,
}

impl<'scope, T: Send + 'scope, W: Fn(usize) -> T + Sync> Ordered<'_, 'scope, T, W> {
    /// Starts work until the window of unconsumed indices is full.
    fn top_up(&mut self) {
        while self.launched < self.count && self.launched - self.consumed < self.window {
            let index = self.launched;
            self.launched += 1;
            let sender = self.sender.clone();
            let (work, cancelled) = (self.work, self.cancelled);
            self.scope.spawn(move |_| {
                if !cancelled.load(Ordering::Relaxed) {
                    // A panic is carried to the consumer, which re-raises it at this index;
                    // the sender is never dropped before then, so waiting would hang
                    let outcome = catch_unwind(AssertUnwindSafe(|| work(index)));
                    // The consumer may be gone after an early return
                    let _ = sender.send((index, outcome));
                }
            });
        }
    }
}

impl<'scope, T: Send + 'scope, W: Fn(usize) -> T + Sync> Iterator for Ordered<'_, 'scope, T, W> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        if self.consumed == self.count {
            return None;
        }
        self.top_up();
        let item = loop {
            if let Some(item) = self.finished.remove(&self.consumed) {
                break item;
            }
            let (index, item) = self.receiver.recv().ok()?;
            self.finished.insert(index, item);
        };
        self.consumed += 1;
        match item {
            Ok(item) => {
                self.top_up();
                Some(item)
            }
            Err(payload) => resume_unwind(payload),
        }
    }
}

impl OutputFormat for TextOutput {
    type Payload = String;
    type Salvage = ();

    fn salvage(&self, _failure: &FileFailure, _content: Option<&str>) {}

    fn payload(&self, diagnostics: Vec<Diagnostic>, content: &str) -> String {
        if diagnostics.is_empty() {
            return String::new();
        }
        let mut formatter = TextFormatter::new();
        formatter.use_color = self.use_color;
        formatter.format(&diagnostics, content)
    }

    fn emit<W: Write, E: Write>(
        &self,
        mut out: W,
        mut err: E,
        reports: impl Iterator<Item = FileReport<String>>,
    ) -> Result<bool> {
        let mut any_errors = false;
        for FileReport { path, outcome } in reports {
            match outcome {
                Err(Failed { failure, .. }) => {
                    any_errors = true;
                    writeln!(err, "{failure}").context("Failed to write to stderr")?;
                }
                Ok(Linted {
                    has_errors,
                    payload: rendered,
                }) => {
                    any_errors |= has_errors;
                    if !rendered.is_empty() {
                        writeln!(out, "{}:", path.display())
                            .and_then(|()| write!(out, "{rendered}"))
                            .and_then(|()| out.flush())
                            .context("Failed to write lint output")?;
                    }
                }
            }
        }
        Ok(any_errors)
    }
}

impl OutputFormat for ReportOutput {
    type Payload = Vec<Diagnostic>;
    /// The `syntax` or input-error diagnostic that stands for the file in the report.
    type Salvage = Diagnostic;

    fn payload(&self, diagnostics: Vec<Diagnostic>, _content: &str) -> Vec<Diagnostic> {
        diagnostics
    }

    fn salvage(&self, failure: &FileFailure, content: Option<&str>) -> Diagnostic {
        match failure {
            FileFailure::Read { source, .. } => input_error_diagnostic(source.to_string()),
            FileFailure::Lint { source, .. } => syntax_diagnostic(source, content.unwrap_or("")),
        }
    }

    /// Renders one report of all files, sorted by path (the formats list files in path order,
    /// so the reports are collected before anything is written).
    fn emit<W: Write, E: Write>(
        &self,
        mut out: W,
        mut err: E,
        reports: impl Iterator<Item = FileReport<Vec<Diagnostic>, Diagnostic>>,
    ) -> Result<bool> {
        let mut any_errors = false;
        let mut files: Vec<(PathBuf, Vec<Diagnostic>)> = Vec::new();
        for FileReport { path, outcome } in reports {
            match outcome {
                Err(Failed { failure, salvage }) => {
                    any_errors = true;
                    writeln!(err, "{failure}").context("Failed to write to stderr")?;
                    files.push((path, vec![salvage]));
                }
                Ok(Linted {
                    has_errors,
                    payload,
                }) => {
                    any_errors |= has_errors;
                    files.push((path, payload));
                }
            }
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));

        let mut resolved = Vec::with_capacity(files.len());
        for (path, diagnostics) in &files {
            match report_source(Some(path)) {
                Ok(source) => resolved.push((source, diagnostics)),
                Err(error) => {
                    any_errors = true;
                    writeln!(err, "error: {error:#}").context("Failed to write to stderr")?;
                }
            }
        }
        let reports: Vec<ReportedFile<'_>> = resolved
            .iter()
            .map(|(source, diagnostics)| ReportedFile {
                source,
                diagnostics,
            })
            .collect();
        write!(out, "{}", self.format.render(&reports))
            .and_then(|()| out.flush())
            .context("Failed to write lint output")?;
        Ok(any_errors)
    }
}

impl OutputFormat for JsonOutput {
    type Payload = Vec<Diagnostic>;
    type Salvage = ();

    fn salvage(&self, _failure: &FileFailure, _content: Option<&str>) {}

    fn payload(&self, diagnostics: Vec<Diagnostic>, _content: &str) -> Vec<Diagnostic> {
        diagnostics
    }

    /// Streams the array in the layout of `serde_json::to_string_pretty` (`[]` when there are
    /// no diagnostics), each diagnostic with a `file` field, followed by a newline.
    fn emit<W: Write, E: Write>(
        &self,
        out: W,
        mut err: E,
        reports: impl Iterator<Item = FileReport<Vec<Diagnostic>>>,
    ) -> Result<bool> {
        let mut serializer = serde_json::Serializer::with_formatter(out, PrettyFormatter::new());
        let mut array = (&mut serializer)
            .serialize_seq(None)
            .context("Failed to write lint output")?;
        let mut any_errors = false;

        for FileReport { path, outcome } in reports {
            match outcome {
                Err(Failed { failure, .. }) => {
                    any_errors = true;
                    writeln!(err, "{failure}").context("Failed to write to stderr")?;
                }
                Ok(Linted {
                    has_errors,
                    payload: diagnostics,
                }) => {
                    any_errors |= has_errors;
                    let file = path.display().to_string();
                    for diagnostic in &diagnostics {
                        let mut value = serde_json::to_value(diagnostic)
                            .context("Failed to serialize a diagnostic")?;
                        if let serde_json::Value::Object(map) = &mut value {
                            map.insert("file".to_owned(), serde_json::Value::String(file.clone()));
                        }
                        array
                            .serialize_element(&value)
                            .context("Failed to write lint output")?;
                    }
                }
            }
        }

        array.end().context("Failed to write lint output")?;
        let mut out = serializer.into_inner();
        writeln!(out)
            .and_then(|()| out.flush())
            .context("Failed to write lint output")?;
        Ok(any_errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fast_yaml_linter::{DiagnosticBuilder, Location, Span};
    use std::sync::atomic::AtomicUsize;

    fn diagnostic(line: usize, severity: Severity) -> Diagnostic {
        let span = Span::new(Location::new(line, 1, 0), Location::new(line, 2, 1));
        DiagnosticBuilder::new("test-rule", severity, "message", span)
            .with_suggestion("fix", span, Some("x".to_owned()))
            .build("a: 1\n")
    }

    fn linted<P>(path: &str, payload: P, has_errors: bool) -> FileReport<P> {
        FileReport {
            path: PathBuf::from(path),
            outcome: Ok(Linted {
                has_errors,
                payload,
            }),
        }
    }

    fn json_report(path: &str, diagnostics: Vec<Diagnostic>) -> FileReport<Vec<Diagnostic>> {
        let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);
        linted(path, diagnostics, has_errors)
    }

    fn failed<P>(path: &str) -> FileReport<P> {
        FileReport {
            path: PathBuf::from(path),
            outcome: Err(Failed {
                failure: FileFailure::Lint {
                    path: PathBuf::from(path),
                    source: Linter::with_all_rules().lint("a: [").unwrap_err(),
                },
                salvage: (),
            }),
        }
    }

    fn json_of(reports: Vec<FileReport<Vec<Diagnostic>>>) -> (String, String, bool) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let any = JsonOutput
            .emit(&mut out, &mut err, reports.into_iter())
            .unwrap();
        (
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
            any,
        )
    }

    /// The array the previous implementation built before printing it in one piece.
    fn buffered(reports: &[(&str, Vec<Diagnostic>)]) -> String {
        let all: Vec<serde_json::Value> = reports
            .iter()
            .flat_map(|(path, diagnostics)| {
                diagnostics.iter().map(move |d| {
                    let mut value = serde_json::to_value(d).unwrap();
                    if let serde_json::Value::Object(map) = &mut value {
                        map.insert("file".into(), serde_json::Value::String((*path).into()));
                    }
                    value
                })
            })
            .collect();
        format!("{}\n", serde_json::to_string_pretty(&all).unwrap())
    }

    #[test]
    fn json_is_byte_identical_to_the_buffered_array_for_zero_one_and_many() {
        let none: Vec<(&str, Vec<Diagnostic>)> = vec![];
        let one = vec![("a.yaml", vec![diagnostic(1, Severity::Error)])];
        let many = vec![
            (
                "a.yaml",
                vec![
                    diagnostic(1, Severity::Error),
                    diagnostic(2, Severity::Warning),
                ],
            ),
            ("b.yaml", vec![]),
            ("c.yaml", vec![diagnostic(3, Severity::Info)]),
        ];
        for reports in [none, one, many] {
            let expected = buffered(&reports);
            let streamed = reports
                .iter()
                .map(|(path, d)| json_report(path, d.clone()))
                .collect();
            assert_eq!(json_of(streamed).0, expected);
        }
    }

    #[test]
    fn empty_json_is_an_empty_array_with_a_newline() {
        assert_eq!(json_of(vec![]).0, "[]\n");
    }

    #[test]
    fn failures_go_to_stderr_in_order_and_count_as_errors() {
        let (out, err, any) = json_of(vec![failed("first.yaml"), failed("second.yaml")]);
        assert_eq!(out, "[]\n");
        let lines: Vec<&str> = err.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("error: 'first.yaml': "), "{err}");
        assert!(lines[1].starts_with("error: 'second.yaml': "), "{err}");
        assert!(any);
    }

    #[test]
    fn warnings_alone_are_not_errors() {
        let (_, _, any) = json_of(vec![json_report(
            "a.yaml",
            vec![diagnostic(1, Severity::Warning)],
        )]);
        assert!(!any);
    }

    #[test]
    fn a_limit_failure_carries_its_raise_hint_once() {
        let limits = fast_yaml_core::limits::ParseLimits {
            max_depth: fast_yaml_core::limits::MaxDepth::new(1).unwrap(),
            ..fast_yaml_core::limits::ParseLimits::default()
        };
        let linter = Linter::with_config(LintConfig::new().with_parse_limits(limits));
        let failure = FileFailure::Lint {
            path: PathBuf::from("deep.yaml"),
            source: linter.lint("a: [[1]]\n").unwrap_err(),
        };
        let text = failure.to_string();
        assert!(text.starts_with("error: 'deep.yaml': "), "{text}");
        assert_eq!(text.matches("raise with --max-depth").count(), 1, "{text}");
    }

    struct BrokenPipe;

    impl Write for BrokenPipe {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
    }

    #[test]
    fn json_write_errors_are_propagated() {
        let reports = vec![json_report("a.yaml", vec![diagnostic(1, Severity::Error)])];
        assert!(
            JsonOutput
                .emit(BrokenPipe, Vec::new(), reports.into_iter())
                .is_err()
        );
        assert!(
            JsonOutput
                .emit(BrokenPipe, Vec::new(), std::iter::empty())
                .is_err()
        );
    }

    #[test]
    fn text_output_names_each_file_and_skips_clean_ones() {
        let reports = vec![
            linted("dirty.yaml", "details\n".to_owned(), true),
            linted("clean.yaml", String::new(), false),
            failed("bad.yaml"),
        ];
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let any = TextOutput { use_color: false }
            .emit(&mut out, &mut err, reports.into_iter())
            .unwrap();
        assert!(any);
        assert_eq!(String::from_utf8(out).unwrap(), "dirty.yaml:\ndetails\n");
        assert!(
            String::from_utf8(err)
                .unwrap()
                .starts_with("error: 'bad.yaml': ")
        );
    }

    #[test]
    fn text_write_errors_are_propagated() {
        let reports = vec![linted("dirty.yaml", "details\n".to_owned(), true)];
        let result =
            TextOutput { use_color: false }.emit(BrokenPipe, Vec::new(), reports.into_iter());
        assert!(result.is_err());
    }

    fn pool(threads: usize) -> ThreadPool {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
    }

    #[test]
    fn run_ordered_yields_results_in_index_order_whatever_the_finish_order() {
        let started = AtomicUsize::new(0);
        let work = |index: usize| {
            started.fetch_add(1, Ordering::SeqCst);
            // Early indices finish last
            std::thread::sleep(std::time::Duration::from_millis(
                (40 - index.min(40)) as u64,
            ));
            index
        };
        let all = run_ordered(&pool(4), 40, 8, &work, |items| items.collect::<Vec<_>>());
        assert_eq!(all, (0..40).collect::<Vec<_>>());
        assert_eq!(started.load(Ordering::SeqCst), 40);
    }

    #[test]
    fn run_ordered_never_has_more_than_a_window_in_flight() {
        let (consumed, max_ahead) = (AtomicUsize::new(0), AtomicUsize::new(0));
        let started = AtomicUsize::new(0);
        let work = |_: usize| {
            let ahead =
                started.fetch_add(1, Ordering::SeqCst) + 1 - consumed.load(Ordering::SeqCst);
            max_ahead.fetch_max(ahead, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        run_ordered(&pool(4), 200, 6, &work, |items| {
            for () in items {
                consumed.fetch_add(1, Ordering::SeqCst);
            }
        });
        // The consumer counts an item after `next` has already topped the window up
        assert!(max_ahead.load(Ordering::SeqCst) <= 6 + 1);
    }

    #[test]
    fn run_ordered_handles_no_work_and_an_early_stop() {
        let work = |index: usize| index;
        assert!(run_ordered(&pool(2), 0, 4, &work, |items| items.next()).is_none());
        let first = run_ordered(&pool(2), 1000, 4, &work, |items| items.next());
        assert_eq!(first, Some(0));
    }

    #[test]
    fn run_ordered_re_raises_a_worker_panic_instead_of_hanging() {
        let (done, finished) = mpsc::channel();
        std::thread::spawn(move || {
            let work = |index: usize| {
                assert!(index != 3, "worker failed");
                index
            };
            let result = catch_unwind(AssertUnwindSafe(|| {
                run_ordered(&pool(2), 20, 4, &work, |items| items.collect::<Vec<_>>())
            }));
            let _ = done.send(result.is_err());
        });
        let panicked = finished
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("run_ordered hung after a worker panic");
        assert!(panicked);
    }

    #[test]
    fn run_ordered_skips_queued_work_when_the_consumer_stops_or_panics() {
        for panics in [false, true] {
            let executed = AtomicUsize::new(0);
            let work = |index: usize| {
                executed.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(10));
                index
            };
            let stop = |items: &mut dyn Iterator<Item = usize>| {
                assert_eq!(items.next(), Some(0));
                assert!(!panics, "consumer failed");
            };
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                run_ordered(&pool(1), 1000, 64, &work, stop);
            }));
            assert_eq!(outcome.is_err(), panics);
            assert!(executed.load(Ordering::SeqCst) < 50, "{panics}");
        }
    }
}
