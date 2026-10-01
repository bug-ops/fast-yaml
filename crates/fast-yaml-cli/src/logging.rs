//! Debug logging of the `fy` process, controlled by `RUST_LOG`.
//!
//! Without `RUST_LOG` nothing is installed and no event is printed. With it, events of this
//! binary and of `fast-yaml-parallel` go to stderr, so stdout stays machine-readable.

use tracing_subscriber::EnvFilter;

use fast_yaml_cli::error;

/// Installs the stderr subscriber when `RUST_LOG` holds a valid filter.
///
/// An invalid filter is reported once on stderr and logging stays off.
pub fn init() {
    let Ok(spec) = std::env::var("RUST_LOG") else {
        return;
    };
    match EnvFilter::try_new(&spec) {
        Ok(filter) => {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_writer(std::io::stderr)
                .try_init();
        }
        Err(err) => error::stderr_line(format_args!("warning: ignoring invalid RUST_LOG: {err}")),
    }
}
