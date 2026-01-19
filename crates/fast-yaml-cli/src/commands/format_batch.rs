//! Batch format command execution.

use std::path::PathBuf;

use anyhow::{Context, Result};
use fast_yaml_core::emitter::EmitterConfig;
use fast_yaml_parallel::{BatchResult as ParallelBatchResult, FileProcessor};

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
}

impl BatchConfig {
    pub fn new(common: CommonConfig) -> Self {
        Self {
            common,
            discovery: DiscoveryConfig::new(),
            dry_run: false,
            in_place: false,
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

    // Extract paths from discovered files
    let file_paths: Vec<PathBuf> = files.iter().map(|f| f.path.clone()).collect();

    // Create emitter config
    let emitter_config = EmitterConfig::new()
        .with_indent(config.common.formatter.indent() as usize)
        .with_width(config.common.formatter.width());

    // Create processor with config from CLI settings
    let processor = FileProcessor::with_config(config.common.parallel.clone());

    // Process files based on mode
    let result = if config.dry_run {
        // Dry run: format but don't write, report what would change
        let formatted = processor.format_files(&file_paths, &emitter_config);
        convert_format_results_to_batch_result(formatted)
    } else if config.in_place {
        // In-place: format and write
        processor.format_in_place(&file_paths, &emitter_config)
    } else {
        // Default: just parse/validate
        processor.parse_files(&file_paths)
    };

    // Report results using BatchSummary event
    // Note: fast-yaml-parallel uses 'changed' instead of 'formatted'
    // and doesn't have 'skipped' (dry_run is CLI-specific)
    let skipped = if config.dry_run { result.changed } else { 0 };
    let formatted = if config.dry_run { 0 } else { result.changed };

    reporter.report(ReportEvent::BatchSummary {
        total: result.total,
        formatted,
        unchanged: result.success - result.changed,
        skipped,
        failed: result.failed,
        duration: result.duration,
    })?;

    // Report errors
    for (path, error) in &result.errors {
        reporter.report(ReportEvent::Error {
            path: Some(path),
            message: &error.to_string(),
        })?;
    }

    // Return appropriate exit code
    if result.failed > 0 {
        Ok(ExitCode::ParseError)
    } else {
        Ok(ExitCode::Success)
    }
}

/// Convert `format_files` results to `BatchResult` for dry-run reporting
fn convert_format_results_to_batch_result(
    results: Vec<(PathBuf, Result<String, fast_yaml_parallel::Error>)>,
) -> ParallelBatchResult {
    use fast_yaml_parallel::{FileOutcome, FileResult};
    use std::time::{Duration, Instant};

    let start = Instant::now();
    let mut file_results = Vec::with_capacity(results.len());

    for (path, result) in results {
        let outcome = match result {
            Ok(_formatted) => {
                // In dry-run mode, we report as "would change"
                // Use Changed to indicate file would be modified
                FileOutcome::Changed {
                    duration: Duration::ZERO,
                }
            }
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
