//! Batch format command execution.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use fast_yaml_core::emitter::EmitterConfig;
use fast_yaml_parallel::{
    BatchResult as ParallelBatchResult, CommentPolicy, FileProcessor, FormatOutput,
};

use crate::commands::format::error_message;
use crate::config::CommonConfig;
use crate::discovery::{DiscoveryConfig, FileDiscovery};
use crate::error::ExitCode;
use crate::reporter::{ReportEvent, Reporter};

/// Configuration for batch format execution using composed configs.
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Common configuration (formatter, output, parallel settings)
    pub common: CommonConfig,
    /// Discovery-specific configuration
    pub discovery: DiscoveryConfig,
    /// Batch-specific settings
    pub dry_run: bool,
    pub in_place: bool,
    /// Allow formatting files that contain comments (comments are dropped)
    pub strip_comments: bool,
}

impl BatchConfig {
    pub fn new(common: CommonConfig) -> Self {
        Self {
            common,
            discovery: DiscoveryConfig::new(),
            dry_run: false,
            in_place: false,
            strip_comments: false,
        }
    }

    #[must_use]
    pub fn with_discovery(mut self, discovery: DiscoveryConfig) -> Self {
        self.discovery = discovery;
        self
    }

    #[must_use]
    pub const fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    #[must_use]
    pub const fn with_strip_comments(mut self, strip_comments: bool) -> Self {
        self.strip_comments = strip_comments;
        self
    }

    #[must_use]
    pub const fn with_in_place(mut self, in_place: bool) -> Self {
        self.in_place = in_place;
        self
    }
}

/// Execute batch formatting on multiple files.
pub fn execute_batch(
    config: &BatchConfig,
    paths: &[PathBuf],
    stdin_files: bool,
) -> Result<ExitCode> {
    // Create file discovery
    let discovery = FileDiscovery::new(config.discovery.clone())
        .context("Failed to initialize file discovery")?;

    // Discover files
    let files = if stdin_files {
        discovery
            .discover_from_stdin()
            .context("Failed to read file list from stdin")?
    } else {
        discovery
            .discover(paths)
            .context("Failed to discover files")?
    };

    // Handle empty result
    if files.is_empty() {
        if !config.common.output.is_quiet() {
            eprintln!("No YAML files found");
        }
        return Ok(ExitCode::Success);
    }

    // Create reporter
    let reporter = Reporter::new(config.common.output.clone());

    let file_paths: Vec<PathBuf> = files.iter().map(|f| f.path.clone()).collect();

    let emitter_config = EmitterConfig::new()
        .with_indent(config.common.formatter.indent() as usize)
        .with_width(config.common.formatter.width());

    let comments = if config.strip_comments {
        CommentPolicy::Strip
    } else {
        CommentPolicy::Reject
    };

    let processor = FileProcessor::with_config(config.common.parallel.clone());

    let result = if config.dry_run {
        let formatted = processor.format_files(&file_paths, &emitter_config, comments);
        convert_format_results_to_batch_result(formatted)
    } else if config.in_place {
        processor.format_in_place(&file_paths, &emitter_config, comments)
    } else {
        bail!("use -i to format files in-place or --dry-run to preview changes");
    };

    // In dry-run mode, 'changed' means "would change"; in in-place mode it means "formatted".
    let would_change = if config.dry_run { result.changed } else { 0 };
    let formatted = if config.dry_run { 0 } else { result.changed };

    reporter.report(ReportEvent::BatchSummary {
        total: result.total,
        formatted,
        unchanged: result.success - result.changed,
        would_change,
        failed: result.failed,
        duration: result.duration,
    })?;

    // Report errors
    for (path, error) in &result.errors {
        reporter.report(ReportEvent::Error {
            path: Some(path),
            message: &error_message(error),
        })?;
    }

    // Return appropriate exit code
    Ok(if result.failed > 0 {
        ExitCode::ParseError
    } else if would_change > 0 {
        ExitCode::WouldChange
    } else {
        ExitCode::Success
    })
}

/// Convert `format_files` results to `BatchResult` for dry-run reporting
fn convert_format_results_to_batch_result(
    results: Vec<(PathBuf, Result<FormatOutput, fast_yaml_parallel::Error>)>,
) -> ParallelBatchResult {
    use fast_yaml_parallel::{FileOutcome, FileResult};
    use std::time::{Duration, Instant};

    let start = Instant::now();
    let mut file_results = Vec::with_capacity(results.len());

    for (path, result) in results {
        let outcome = match result {
            Ok(output) if output.changed => FileOutcome::Changed {
                duration: Duration::ZERO,
            },
            Ok(_) => FileOutcome::Unchanged {
                duration: Duration::ZERO,
            },
            Err(error) => FileOutcome::Error {
                error,
                duration: Duration::ZERO,
            },
        };
        file_results.push(FileResult::new(path, outcome));
    }

    let mut batch = ParallelBatchResult::from_results(file_results);
    batch.duration = start.elapsed();
    batch
}
