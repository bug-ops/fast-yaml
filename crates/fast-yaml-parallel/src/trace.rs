//! Debug events of batch processing, compiled out unless the `tracing` feature is on.
//!
//! Events use the format-string form only, so the disabled macro can still borrow and
//! type-check its arguments and a feature combination never leaves a binding unused.

#![allow(clippy::redundant_pub_crate)]

/// Emits a `tracing` debug event.
#[cfg(feature = "tracing")]
macro_rules! debug_event {
    ($($arg:tt)+) => {
        ::tracing::debug!($($arg)+)
    };
}

/// Type-checks the arguments of a debug event that is compiled out.
#[cfg(not(feature = "tracing"))]
macro_rules! debug_event {
    ($($arg:tt)+) => {{
        let _ = ::core::format_args!($($arg)+);
    }};
}

pub(crate) use debug_event;
