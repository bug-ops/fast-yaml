//! Batch lint command execution.

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fast_yaml_linter::formatter::{FileReport, input_error_diagnostic, syntax_diagnostic};
use fast_yaml_linter::{Diagnostic, Formatter, LintConfig, Linter, Severity, TextFormatter};
use fast_yaml_parallel::{Error as ParallelError, FileContent, SmartReader};
use rayon::prelude::*;

use crate::cli::{LintFormat, LintOutput};
use crate::commands::lint::report_source;
use crate::config::CommonConfig;
use crate::discovery::FileDiscovery;
use crate::error::{ExitCode, RaiseHint};
use crate::invocation::BatchTarget;

/// Formats a read failure for stderr; `Io` already names the path, the other variants do not.
fn read_error_line(path: &Path, err: &ParallelError) -> String {
    let hint = RaiseHint::of(err).map_or_else(String::new, |h| format!(" ({h})"));
    match err {
        ParallelError::Io { .. } => format!("error: {err}{hint}"),
        _ => format!("error: '{}': {err}{hint}", path.display()),
    }
}

/// Execute batch linting on multiple files.
///
/// # Errors
///
/// Returns error if file discovery fails.
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

    // Process files in parallel, collecting (path, content, diagnostics, has_errors) tuples.
    // Read/lint errors are printed to stderr directly; has_errors=true is set in that case.
    let reports = format.report().is_some();
    let reader = SmartReader::with_threshold(u64::MAX);
    let mut results: Vec<(PathBuf, String, Vec<Diagnostic>, bool)> = pool.install(|| {
        file_paths
            .par_iter()
            .map(|path| {
                let content = match reader
                    .read(path, lint_config.max_input_bytes)
                    .and_then(FileContent::into_string)
                {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("{}", read_error_line(path, &e));
                        let diagnostics = if reports {
                            vec![input_error_diagnostic(e.to_string())]
                        } else {
                            vec![]
                        };
                        return (path.clone(), String::new(), diagnostics, true);
                    }
                };

                let linter = Linter::with_config(lint_config.clone());
                let diagnostics = match linter.lint(&content) {
                    Ok(d) => d,
                    Err(e) => {
                        let hint =
                            RaiseHint::of(&e).map_or_else(String::new, |h| format!(" ({h})"));
                        eprintln!("error: '{}': {e}{hint}", path.display());
                        let diagnostics = if reports {
                            vec![syntax_diagnostic(&e, &content)]
                        } else {
                            vec![]
                        };
                        return (path.clone(), content, diagnostics, true);
                    }
                };

                let filtered: Vec<_> = if is_quiet {
                    diagnostics
                        .into_iter()
                        .filter(|d| d.severity == Severity::Error)
                        .collect()
                } else {
                    diagnostics
                };

                let has_errors = filtered.iter().any(|d| d.severity == Severity::Error);
                (path.clone(), content, filtered, has_errors)
            })
            .collect()
    });

    if reports {
        results.sort_by(|a, b| a.0.cmp(&b.0));
    }

    let mut any_errors = results.iter().any(|(_, _, _, has_errors)| *has_errors);

    match format.output() {
        LintOutput::Text => {
            for (path, content, diagnostics, _) in &results {
                if diagnostics.is_empty() {
                    continue;
                }
                let mut formatter = TextFormatter::new();
                formatter.use_color = use_color;
                let output = formatter.format(diagnostics, content);
                if !output.is_empty() {
                    println!("{}:", path.display());
                    print!("{output}");
                }
            }
        }
        LintOutput::Json => {
            // Collect all diagnostics into a single JSON array with a `file` field.
            let all: Vec<serde_json::Value> = results
                .iter()
                .flat_map(|(path, _, diagnostics, _)| {
                    let file = path.display().to_string();
                    diagnostics.iter().map(move |d| {
                        let mut v = serde_json::to_value(d).unwrap_or(serde_json::Value::Null);
                        if let serde_json::Value::Object(ref mut map) = v {
                            map.insert("file".to_string(), serde_json::Value::String(file.clone()));
                        }
                        v
                    })
                })
                .collect();
            let json = serde_json::to_string_pretty(&all).unwrap_or_else(|_| "[]".to_string());
            println!("{json}");
        }
        LintOutput::Report(report_format) => {
            let mut resolved = Vec::with_capacity(results.len());
            for (path, _, diagnostics, _) in &results {
                match report_source(Some(path)) {
                    Ok(source) => resolved.push((source, diagnostics)),
                    Err(err) => {
                        eprintln!("error: {err:#}");
                        any_errors = true;
                    }
                }
            }
            let files: Vec<FileReport<'_>> = resolved
                .iter()
                .map(|(source, diagnostics)| FileReport {
                    source,
                    diagnostics,
                })
                .collect();
            print!("{}", report_format.render(&files));
        }
    }

    if any_errors {
        Ok(ExitCode::LintErrors)
    } else {
        Ok(ExitCode::Success)
    }
}
