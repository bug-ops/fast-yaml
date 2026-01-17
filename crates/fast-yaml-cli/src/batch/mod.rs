//! Batch file processing for fast-yaml CLI.
//!
//! This module provides efficient multi-file processing capabilities by eliminating
//! process spawn overhead and leveraging internal Rayon parallelism.

#![allow(dead_code, unused_imports)] // Module is not fully integrated yet

pub mod discovery;
pub mod error;

pub use discovery::{DiscoveredFile, DiscoveryConfig, DiscoveryOrigin, FileDiscovery};
pub use error::DiscoveryError;
