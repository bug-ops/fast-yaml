//! Batch lint command execution.
//!
//! Files are linted in bounded chunks and reported in file order as each chunk completes, so
//! peak memory holds the contents of one chunk instead of every file, and both output streams
//! are deterministic whatever the worker interleaving.

use std::io::{self, Write};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fast_yaml_linter::{Diagnostic, Formatter, LintConfig, Linter, Severity, TextFormatter};
use fast_yaml_parallel::{Error as ParallelError, FileContent, SmartReader};
use rayon::prelude::*;
use serde::ser::{SerializeSeq, Serializer};
use serde_json::ser::PrettyFormatter;

use crate::cli::LintFormat;
use crate::config::CommonConfig;
use crate::discovery::FileDiscovery;
use crate::error::{ExitCode, RaiseHint};
use crate::invocation::BatchTarget;

/// Files in flight per worker: a chunk of this many files per thread is linted, then reported.
const IN_FLIGHT_PER_WORKER: usize = 4;

/// Formats a read failure for stderr; `Io` already names the path, the other variants do not.
fn read_error_line(path: &Path, err: &ParallelError) -> String {
    let hint = RaiseHint::of(err).map_or_else(String::new, |h| format!(" ({h})"));
    match err {
        ParallelError::Io { .. } => format!("error: {err}{hint}"),
        _ => format!("error: '{}': {err}{hint}", path.display()),
    }
}

/// What linting one file produced, in the shape the output format needs.
///
/// The file content is dropped inside the worker: text output is rendered there while the
/// content is alive, and JSON output needs only the diagnostics.
enum FileOutcome {
    /// The file could not be read or parsed; the message goes to stderr.
    Failed { message: String },
    /// Text output, empty when the file has nothing to report.
    Text { has_errors: bool, rendered: String },
    /// Diagnostics for the JSON array.
    Json {
        has_errors: bool,
        diagnostics: Vec<Diagnostic>,
    },
}

struct FileReport {
    path: PathBuf,
    outcome: FileOutcome,
}

fn lint_one(
    path: &Path,
    reader: &SmartReader,
    lint_config: &LintConfig,
    format: LintFormat,
    is_quiet: bool,
    use_color: bool,
) -> FileReport {
    let outcome = lint_content(path, reader, lint_config, format, is_quiet, use_color)
        .unwrap_or_else(|message| FileOutcome::Failed { message });
    FileReport {
        path: path.to_path_buf(),
        outcome,
    }
}

fn lint_content(
    path: &Path,
    reader: &SmartReader,
    lint_config: &LintConfig,
    format: LintFormat,
    is_quiet: bool,
    use_color: bool,
) -> Result<FileOutcome, String> {
    let content = reader
        .read(path, lint_config.max_input_bytes)
        .and_then(FileContent::into_string)
        .map_err(|e| read_error_line(path, &e))?;

    let mut diagnostics = Linter::with_config(lint_config.clone())
        .lint(&content)
        .map_err(|e| {
            let hint = RaiseHint::of(&e).map_or_else(String::new, |h| format!(" ({h})"));
            format!("error: '{}': {e}{hint}", path.display())
        })?;
    if is_quiet {
        diagnostics.retain(|d| d.severity == Severity::Error);
    }
    let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);

    Ok(match format {
        LintFormat::Text => {
            let mut formatter = TextFormatter::new();
            formatter.use_color = use_color;
            let rendered = if diagnostics.is_empty() {
                String::new()
            } else {
                formatter.format(&diagnostics, &content)
            };
            FileOutcome::Text {
                has_errors,
                rendered,
            }
        }
        LintFormat::Json => FileOutcome::Json {
            has_errors,
            diagnostics,
        },
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

    let workers = target
        .workers
        .map_or_else(rayon::current_num_threads, NonZeroUsize::get);

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .context("Failed to build thread pool")?;

    let use_color = common.output.use_color();
    let is_quiet = common.output.is_quiet();

    let file_paths: Vec<PathBuf> = files.iter().map(|f| f.path.clone()).collect();

    let reader = SmartReader::with_threshold(u64::MAX);
    let batches = file_paths
        .chunks((workers * IN_FLIGHT_PER_WORKER).max(1))
        .map(|chunk| {
            pool.install(|| {
                chunk
                    .par_iter()
                    .map(|path| lint_one(path, &reader, lint_config, format, is_quiet, use_color))
                    .collect::<Vec<_>>()
            })
        });

    let stdout = io::stdout();
    let any_errors = emit(format, stdout.lock(), io::stderr().lock(), batches)?;

    if any_errors {
        Ok(ExitCode::LintErrors)
    } else {
        Ok(ExitCode::Success)
    }
}

/// Writes every batch of reports in file order and returns whether any file had an error.
///
/// Failures go to `err`, results to `out`; a write error stops the run.
fn emit<W: Write, E: Write>(
    format: LintFormat,
    out: W,
    err: E,
    batches: impl Iterator<Item = Vec<FileReport>>,
) -> Result<bool> {
    match format {
        LintFormat::Text => emit_text(out, err, batches),
        LintFormat::Json => emit_json(out, err, batches),
    }
}

fn emit_text<W: Write, E: Write>(
    mut out: W,
    mut err: E,
    batches: impl Iterator<Item = Vec<FileReport>>,
) -> Result<bool> {
    let mut any_errors = false;
    for batch in batches {
        for FileReport { path, outcome } in batch {
            match outcome {
                FileOutcome::Failed { message } => {
                    any_errors = true;
                    writeln!(err, "{message}").context("Failed to write to stderr")?;
                }
                FileOutcome::Text {
                    has_errors,
                    rendered,
                } => {
                    any_errors |= has_errors;
                    if !rendered.is_empty() {
                        writeln!(out, "{}:", path.display())
                            .and_then(|()| write!(out, "{rendered}"))
                            .context("Failed to write lint output")?;
                    }
                }
                FileOutcome::Json { .. } => {}
            }
        }
        out.flush().context("Failed to write lint output")?;
    }
    Ok(any_errors)
}

/// Streams one JSON array of all diagnostics, each with a `file` field, in the layout of
/// `serde_json::to_string_pretty` (`[]` when there are none) followed by a newline.
fn emit_json<W: Write, E: Write>(
    out: W,
    mut err: E,
    batches: impl Iterator<Item = Vec<FileReport>>,
) -> Result<bool> {
    let mut serializer = serde_json::Serializer::with_formatter(out, PrettyFormatter::new());
    let mut array = (&mut serializer)
        .serialize_seq(None)
        .context("Failed to write lint output")?;
    let mut any_errors = false;

    for batch in batches {
        for FileReport { path, outcome } in batch {
            match outcome {
                FileOutcome::Failed { message } => {
                    any_errors = true;
                    writeln!(err, "{message}").context("Failed to write to stderr")?;
                }
                FileOutcome::Json {
                    has_errors,
                    diagnostics,
                } => {
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
                FileOutcome::Text { .. } => {}
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

#[cfg(test)]
mod tests {
    use super::*;
    use fast_yaml_linter::{DiagnosticBuilder, Location, Span};

    fn diagnostic(line: usize, severity: Severity) -> Diagnostic {
        let span = Span::new(Location::new(line, 1, 0), Location::new(line, 2, 1));
        DiagnosticBuilder::new("test-rule", severity, "message", span)
            .with_suggestion("fix", span, Some("x".to_owned()))
            .build("a: 1\n")
    }

    fn json_report(path: &str, diagnostics: Vec<Diagnostic>) -> FileReport {
        let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);
        FileReport {
            path: PathBuf::from(path),
            outcome: FileOutcome::Json {
                has_errors,
                diagnostics,
            },
        }
    }

    fn failed(message: &str) -> FileReport {
        FileReport {
            path: PathBuf::from("bad.yaml"),
            outcome: FileOutcome::Failed {
                message: message.to_owned(),
            },
        }
    }

    fn json_of(batches: Vec<Vec<FileReport>>) -> (String, String, bool) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let any = emit(LintFormat::Json, &mut out, &mut err, batches.into_iter()).unwrap();
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
            let batch = |range: &[(&str, Vec<Diagnostic>)]| -> Vec<FileReport> {
                range
                    .iter()
                    .map(|(path, d)| json_report(path, d.clone()))
                    .collect()
            };
            // One batch and one batch per file must produce the same bytes
            let (single, _, _) = json_of(vec![batch(&reports)]);
            let per_file: Vec<Vec<FileReport>> = reports
                .iter()
                .map(|r| batch(std::slice::from_ref(r)))
                .collect();
            let (split, _, _) = json_of(per_file);
            assert_eq!(single, expected);
            assert_eq!(split, expected);
        }
    }

    #[test]
    fn empty_json_is_an_empty_array_with_a_newline() {
        assert_eq!(json_of(vec![]).0, "[]\n");
    }

    #[test]
    fn failures_go_to_stderr_in_order_and_count_as_errors() {
        let (out, err, any) = json_of(vec![vec![failed("first")], vec![failed("second")]]);
        assert_eq!(out, "[]\n");
        assert_eq!(err, "first\nsecond\n");
        assert!(any);
    }

    #[test]
    fn warnings_alone_are_not_errors() {
        let (_, _, any) = json_of(vec![vec![json_report(
            "a.yaml",
            vec![diagnostic(1, Severity::Warning)],
        )]]);
        assert!(!any);
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
        let batches = vec![vec![json_report(
            "a.yaml",
            vec![diagnostic(1, Severity::Error)],
        )]];
        let result = emit(
            LintFormat::Json,
            BrokenPipe,
            Vec::new(),
            batches.into_iter(),
        );
        assert!(result.is_err());
        assert!(emit(LintFormat::Json, BrokenPipe, Vec::new(), std::iter::empty()).is_err());
    }

    #[test]
    fn text_output_names_each_file_and_skips_clean_ones() {
        let reports = vec![
            FileReport {
                path: PathBuf::from("dirty.yaml"),
                outcome: FileOutcome::Text {
                    has_errors: true,
                    rendered: "details\n".to_owned(),
                },
            },
            FileReport {
                path: PathBuf::from("clean.yaml"),
                outcome: FileOutcome::Text {
                    has_errors: false,
                    rendered: String::new(),
                },
            },
            failed("boom"),
        ];
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let any = emit(
            LintFormat::Text,
            &mut out,
            &mut err,
            std::iter::once(reports),
        )
        .unwrap();
        assert!(any);
        assert_eq!(String::from_utf8(out).unwrap(), "dirty.yaml:\ndetails\n");
        assert_eq!(String::from_utf8(err).unwrap(), "boom\n");
    }

    #[test]
    fn text_write_errors_are_propagated() {
        let reports = vec![FileReport {
            path: PathBuf::from("dirty.yaml"),
            outcome: FileOutcome::Text {
                has_errors: true,
                rendered: "details\n".to_owned(),
            },
        }];
        let result = emit(
            LintFormat::Text,
            BrokenPipe,
            Vec::new(),
            std::iter::once(reports),
        );
        assert!(result.is_err());
    }
}
