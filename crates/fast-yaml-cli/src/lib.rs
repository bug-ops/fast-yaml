//! Fast YAML CLI library components.
//!
//! This library exposes internal modules for testing and benchmarking.
//! It is not intended for public consumption - use the `fy` binary instead.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]
#![cfg_attr(not(test), deny(clippy::expect_used))]
#![cfg_attr(not(test), deny(clippy::panic))]

pub mod discovery;
/// Error types and exit codes for CLI operations
pub mod error;
