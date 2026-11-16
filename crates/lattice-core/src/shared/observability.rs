//! The app's log format and filter, shared by the desktop's startup and the test processes.

use tracing_subscriber::{fmt::MakeWriter, registry::LookupSpan, EnvFilter, Layer};

/// `tokenizers` warns once per added token when it loads the Qwen3 tokenizer
/// (IDs it expected but the file leaves implicit) — two dozen lines on every
/// startup for a tokenizer that loads and works.
pub const DEFAULT_FILTER: &str = "info,tokenizers=error";

/// The filter from `RUST_LOG`, else `default`.
pub fn env_filter(default: &str) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default))
}

/// The file log's line format.
pub fn file_log_layer<S, W>(writer: W) -> impl Layer<S>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    W: for<'w> MakeWriter<'w> + 'static,
{
    tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_ansi(false)
}
