//! Batch format command execution.

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::batch::{BatchProcessor, DiscoveryConfig, FileDiscovery, ProcessingConfig};
use crate::config::CommonConfig;
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

    // Build processing config from common config
    let processing_config = ProcessingConfig::new()
        .with_indent(config.common.formatter.indent())
        .with_width(config.common.formatter.width())
        .with_in_place(config.in_place)
        .with_dry_run(config.dry_run)
        .with_workers(config.common.parallel.workers())
        .with_mmap_threshold(config.common.parallel.mmap_threshold())
        .with_verbose(config.common.output.is_verbose());

    // Process files
    let processor = BatchProcessor::new(processing_config);
    let result = processor.process(&files);

    // Report results using BatchSummary event
    reporter.report(ReportEvent::BatchSummary {
        total: result.total,
        formatted: result.formatted,
        unchanged: result.unchanged,
        skipped: result.skipped,
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
