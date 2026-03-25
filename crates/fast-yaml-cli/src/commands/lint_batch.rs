//! Batch lint command execution.

use std::path::PathBuf;

use anyhow::{Context, Result};
use fast_yaml_linter::{Diagnostic, Formatter, LintConfig, Linter, Severity, TextFormatter};
use rayon::prelude::*;

use crate::cli::LintFormat;
use crate::config::CommonConfig;
use crate::discovery::{DiscoveryConfig, FileDiscovery};
use crate::error::ExitCode;

/// Configuration for batch lint execution.
#[derive(Debug, Clone)]
pub struct LintBatchConfig {
    /// Common configuration
    pub common: CommonConfig,
    /// Discovery-specific configuration
    pub discovery: DiscoveryConfig,
    /// Lint configuration
    pub lint_config: LintConfig,
    /// Lint output format
    pub format: LintFormat,
}

impl LintBatchConfig {
    pub fn new(common: CommonConfig, lint_config: LintConfig, format: LintFormat) -> Self {
        Self {
            common,
            discovery: DiscoveryConfig::new(),
            lint_config,
            format,
        }
    }

    #[must_use]
    pub fn with_discovery(mut self, discovery: DiscoveryConfig) -> Self {
        self.discovery = discovery;
        self
    }
}

/// Execute batch linting on multiple files.
///
/// # Errors
///
/// Returns error if file discovery fails.
pub fn execute_lint_batch(config: &LintBatchConfig, paths: &[PathBuf]) -> Result<ExitCode> {
    let discovery = FileDiscovery::new(config.discovery.clone())
        .context("Failed to initialize file discovery")?;

    let files = discovery
        .discover(paths)
        .context("Failed to discover files")?;

    if files.is_empty() {
        if !config.common.output.is_quiet() {
            eprintln!("No YAML files found");
        }
        return Ok(ExitCode::Success);
    }

    let workers = config
        .common
        .parallel
        .workers()
        .unwrap_or_else(rayon::current_num_threads);

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .context("Failed to build thread pool")?;

    let lint_config = config.lint_config.clone();
    let format = config.format.clone();
    let use_color = config.common.output.use_color();
    let is_quiet = config.common.output.is_quiet();

    let file_paths: Vec<PathBuf> = files.iter().map(|f| f.path.clone()).collect();

    // Process files in parallel, collecting (path, content, diagnostics, has_errors) tuples.
    // Read/lint errors are printed to stderr directly; has_errors=true is set in that case.
    let results: Vec<(PathBuf, String, Vec<Diagnostic>, bool)> = pool.install(|| {
        file_paths
            .par_iter()
            .map(|path| {
                let content = match std::fs::read_to_string(path) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("error: failed to read '{}': {e}", path.display());
                        return (path.clone(), String::new(), vec![], true);
                    }
                };

                let linter = Linter::with_config(lint_config.clone());
                let diagnostics = match linter.lint(&content) {
                    Ok(d) => d,
                    Err(e) => {
                        eprintln!("error: '{}': {e}", path.display());
                        return (path.clone(), content, vec![], true);
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

    let any_errors = results.iter().any(|(_, _, _, has_errors)| *has_errors);

    match format {
        LintFormat::Text => {
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
        LintFormat::Json => {
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
    }

    if any_errors {
        Ok(ExitCode::LintErrors)
    } else {
        Ok(ExitCode::Success)
    }
}
