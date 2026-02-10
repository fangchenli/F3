#![feature(new_range_api)]
use mimalloc::MiMalloc;

pub mod common;
mod compression;
pub mod counter;
pub mod file;
pub mod io;
pub mod options;
pub mod reader;
pub mod writer;

pub mod context;
pub mod decoder;
mod dict;
pub(crate) mod encoder;

/// Initialize tracing subscriber for structured logging.
///
/// This function sets up the tracing subscriber with environment-based filtering
/// using the RUST_LOG environment variable. It's safe to call multiple times -
/// subsequent calls will be ignored.
///
/// # Examples
///
/// ```no_run
/// # use fff_poc::init_tracing;
/// // Initialize with default settings (reads RUST_LOG env var)
/// init_tracing();
///
/// // Or set RUST_LOG before calling:
/// // RUST_LOG=debug cargo test
/// // RUST_LOG=fff_poc=trace,fff_core=debug cargo run
/// ```
pub fn init_tracing() {
    use std::sync::Once;
    use tracing_subscriber::{fmt, EnvFilter};

    static INIT: Once = Once::new();

    INIT.call_once(|| {
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

        fmt()
            .with_env_filter(filter)
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .init();
    });
}

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;
