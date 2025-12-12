//! fast-yaml-ffi: FFI utilities for fast-yaml bindings.
//!
//! This crate provides generic traits and utilities for converting between
//! Rust types and foreign types (e.g., Python objects via `PyO3`).
//!
//! # Core Traits
//!
//! - `ToFfi<T>`: Convert Rust types to foreign types
//! - `FromFfi<T>`: Convert foreign types to Rust types
//!
//! # Examples
//!
//! ```ignore
//! use fast_yaml_ffi::{ToFfi, FromFfi};
//!
//! // Convert Rust value to Python object
//! let py_obj = rust_value.to_ffi()?;
//!
//! // Convert Python object to Rust value
//! let rust_value = RustType::from_ffi(&py_obj)?;
//! ```

/// FFI conversion traits for type conversion across language boundaries.
pub mod conversion;
/// Error types for FFI operations.
pub mod error;

pub use conversion::{FromFfi, ToFfi};
pub use error::{FfiError, FfiResult};
