//! Batch format command execution.

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::batch::{
    BatchProcessor, BatchReporter, DiscoveryConfig, FileDiscovery, ProcessingConfig, ReporterConfig,
};
use crate::error::ExitCode;

/// Configuration for batch format execution.
#[allow(clippy::struct_excessive_bools)]
pub struct BatchFormatConfig {
    pub indent: u8,
    pub width: usize,
    pub in_place: bool,
    pub dry_run: bool,
    pub jobs: usize,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub recursive: bool,
    pub quiet: bool,
    pub verbose: bool,
    pub use_color: bool,
}

/// Execute batch formatting on multiple files.
pub fn execute_batch(
    config: &BatchFormatConfig,
    paths: &[PathBuf],
    stdin_files: bool,
) -> Result<ExitCode> {
    // Build discovery config
    let discovery_config = build_discovery_config(config);

    // Create file discovery
    let discovery =
        FileDiscovery::new(discovery_config).context("Failed to initialize file discovery")?;

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
        if !config.quiet {
            eprintln!("No YAML files found");
        }
        return Ok(ExitCode::Success);
    }

    // Build processing config
    let processing_config = ProcessingConfig::new()
        .with_indent(config.indent)
        .with_width(config.width)
        .with_in_place(config.in_place)
        .with_dry_run(config.dry_run)
        .with_workers(config.jobs)
        .with_verbose(config.verbose);

    // Process files
    let processor = BatchProcessor::new(processing_config);
    let result = processor.process(&files);

    // Report results
    let reporter_config = ReporterConfig::new(config.use_color, config.quiet, config.verbose);
    let reporter = BatchReporter::new(reporter_config);
    reporter.report(&result)?;

    // Return appropriate exit code
    if result.failed > 0 {
        Ok(ExitCode::ParseError)
    } else {
        Ok(ExitCode::Success)
    }
}

fn build_discovery_config(config: &BatchFormatConfig) -> DiscoveryConfig {
    let mut discovery = DiscoveryConfig::new();

    // Use custom include patterns if provided, otherwise use defaults
    if !config.include.is_empty() {
        discovery = discovery.with_include_patterns(config.include.clone());
    }

    // Apply exclude patterns
    if !config.exclude.is_empty() {
        discovery = discovery.with_exclude_patterns(config.exclude.clone());
    }

    // Set recursion depth
    if !config.recursive {
        discovery = discovery.with_max_depth(Some(1));
    }

    discovery
}
