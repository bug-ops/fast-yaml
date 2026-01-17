//! Universal output/reporting system for CLI commands.
//!
//! This module provides a unified interface for reporting events,
//! progress, and results across all commands, centralizing color
//! handling and output formatting.

mod events;
mod output;

pub use events::ReportEvent;
pub use output::Reporter;
