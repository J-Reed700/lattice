//! OpenTelemetry and structured tracing initialization.
//!
//! Provides two tracing modes:
//! - **OTEL mode**: Exports spans via OTLP + stdout fmt + rotating file log
//! - **Regular mode**: stdout fmt + rotating file log (no OTLP)
//!
//! File logs are written to `<data_dir>/lattice/logs/` with daily rotation.

use opentelemetry::{global, trace::TracerProvider, KeyValue};
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::{trace::SdkTracerProvider, Resource};
use std::sync::OnceLock;
use tracing_appender::rolling;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Global handle to the OTEL tracer provider, used for graceful shutdown.
static TRACER_PROVIDER: OnceLock<SdkTracerProvider> = OnceLock::new();

// ============================================================================
// Public API
// ============================================================================

/// Check whether OpenTelemetry export is enabled via environment.
///
/// Returns `true` if either:
/// - `OTEL_ENABLED=true`
/// - `OTEL_TRACES_EXPORTER` contains `"otlp"`
///
/// This is the **single source of truth** for the OTEL-enabled decision.
pub fn is_otel_enabled() -> bool {
    let explicit = std::env::var("OTEL_ENABLED")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .unwrap_or(false);

    if explicit {
        return true;
    }

    // Also honour the standard OTEL env var
    let traces_exporter = std::env::var("OTEL_TRACES_EXPORTER")
        .unwrap_or_default()
        .to_lowercase();

    traces_exporter.split(',').any(|v| v.trim() == "otlp")
}

/// Initialize OpenTelemetry tracing with OTLP export, stdout, and file logging.
///
/// Returns `Ok(true)` if OTEL was initialized, `Ok(false)` if OTEL is disabled
/// (caller should set up regular tracing as fallback).
pub fn init_otel_tracing(
    service_name: &str,
    otlp_endpoint: Option<String>,
) -> Result<bool, Box<dyn std::error::Error>> {
    if !is_otel_enabled() {
        eprintln!("[lattice] OpenTelemetry disabled (OTEL_ENABLED not set to true)");
        return Ok(false);
    }

    let protocol = std::env::var("OTEL_EXPORTER_OTLP_PROTOCOL")
        .unwrap_or_else(|_| "http/protobuf".to_string())
        .to_lowercase();

    let default_endpoint = if protocol == "grpc" {
        "http://localhost:4317".to_string()
    } else {
        "http://localhost:4318".to_string()
    };

    let mut endpoint = otlp_endpoint
        .or_else(|| std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok())
        .unwrap_or(default_endpoint);

    if protocol != "grpc" && !endpoint.ends_with("/v1/traces") {
        endpoint = format!("{}/v1/traces", endpoint.trim_end_matches('/'));
    }

    eprintln!(
        "[lattice] Initializing OpenTelemetry — protocol: {}, endpoint: {}",
        protocol, endpoint
    );

    // Build OTLP span exporter
    let exporter = if protocol == "grpc" {
        SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint.clone())
            .build()?
    } else {
        SpanExporter::builder()
            .with_http()
            .with_endpoint(endpoint.clone())
            .build()?
    };

    let provider = SdkTracerProvider::builder()
        .with_resource(
            Resource::builder()
                .with_service_name(service_name.to_string())
                .with_attributes([KeyValue::new("service.version", env!("CARGO_PKG_VERSION"))])
                .build(),
        )
        .with_batch_exporter(exporter)
        .build();

    global::set_tracer_provider(provider.clone());

    // Store provider for graceful shutdown (flush pending spans)
    let _ = TRACER_PROVIDER.set(provider.clone());

    let tracer = provider.tracer("lattice-desktop");
    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_file(true)
        .with_line_number(true);

    // Rotating file log
    let (file_writer, _guard) = create_file_writer();
    std::mem::forget(_guard); // Keep writer alive for process lifetime
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(file_writer)
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_ansi(false);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(telemetry)
        .with(fmt_layer)
        .with(file_layer)
        .try_init()?;

    tracing::info!("OpenTelemetry initialized (endpoint: {})", endpoint);
    tracing::info_span!("otel.startup").in_scope(|| {
        tracing::info!("OpenTelemetry startup span emitted");
    });

    Ok(true)
}

/// Initialize regular tracing (stdout + file, no OTEL).
///
/// Called as fallback when OTEL is disabled or fails to initialize.
pub fn init_regular_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,lattice=debug,lattice_desktop=debug"));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true);

    // Rotating file log
    let (file_writer, _guard) = create_file_writer();
    std::mem::forget(_guard); // Keep writer alive for process lifetime
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(file_writer)
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_ansi(false);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .with(file_layer)
        .init();

    tracing::info!("Regular tracing initialized (no OTEL)");
    eprintln!("[lattice] Regular tracing initialized successfully");
}

/// Flush and shut down the OpenTelemetry tracer provider.
///
/// **Must** be called during graceful shutdown to ensure the final batch of
/// spans is exported. Without this, spans from the last few seconds of
/// execution are silently lost.
pub fn shutdown_tracing() {
    tracing::info!("Shutting down tracing");

    if let Some(provider) = TRACER_PROVIDER.get() {
        // Flush any pending spans before the process exits.
        if let Err(e) = provider.shutdown() {
            eprintln!("[lattice] Error shutting down OTEL tracer provider: {e}");
        } else {
            tracing::info!("OpenTelemetry tracer provider shut down successfully");
        }
    }
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Create a non-blocking file writer with daily rotation.
///
/// Logs are written to `<data_local_dir>/lattice/logs/lattice.log`.
/// If the directory cannot be determined, falls back to the system temp dir.
fn create_file_writer() -> (
    tracing_appender::non_blocking::NonBlocking,
    tracing_appender::non_blocking::WorkerGuard,
) {
    let log_dir = log_directory();

    // Ensure the directory exists (best-effort)
    let _ = std::fs::create_dir_all(&log_dir);

    eprintln!("[lattice] Log files: {}", log_dir.display());

    let file_appender = rolling::daily(&log_dir, "lattice.log");
    tracing_appender::non_blocking(file_appender)
}

/// Determine the log directory path.
///
/// Uses the platform-appropriate local data directory:
/// - macOS: `~/Library/Application Support/lattice/logs`
/// - Linux: `~/.local/share/lattice/logs`
/// - Windows: `{FOLDERID_LocalAppData}\lattice\logs`
fn log_directory() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("lattice")
        .join("logs")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_directory_is_reasonable() {
        let dir = log_directory();
        assert!(
            dir.to_string_lossy().contains("lattice"),
            "Log directory should contain 'lattice': {:?}",
            dir
        );
    }

    #[test]
    fn test_log_directory_ends_with_logs() {
        let dir = log_directory();
        assert!(
            dir.ends_with("lattice/logs"),
            "Log directory should end with lattice/logs: {:?}",
            dir
        );
    }
}
