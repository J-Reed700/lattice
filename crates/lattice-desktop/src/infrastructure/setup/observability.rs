//! Tracing setup orchestration.
//!
//! Single entry point for initializing the application's tracing stack.
//! Delegates to `infrastructure::observability::tracing` for the actual
//! subscriber setup — this module only decides *which* mode to use.

/// Initialize the tracing stack for the application.
///
/// Decision logic:
/// 1. If OTEL is enabled → try OTEL tracing (OTLP + stdout + file).
/// 2. If OTEL init fails → fall back to regular tracing (stdout + file).
/// 3. If OTEL is disabled → use regular tracing directly.
///
/// Uses [`crate::infrastructure::observability::tracing::is_otel_enabled`]
/// as the single source of truth for the OTEL-enabled decision.
pub fn setup_tracing() {
    use crate::infrastructure::observability::tracing as otel;

    if otel::is_otel_enabled() {
        match otel::init_otel_tracing("lattice-desktop", None) {
            Ok(true) => {
                // OTEL subscriber is active — nothing more to do.
            }
            Ok(false) => {
                eprintln!(
                    "[lattice] OTEL check returned disabled, falling back to regular tracing"
                );
                otel::init_regular_tracing();
            }
            Err(e) => {
                eprintln!("[lattice] Failed to initialize OpenTelemetry: {e}");
                otel::init_regular_tracing();
            }
        }
    } else {
        otel::init_regular_tracing();
    }
}

#[cfg(test)]
mod tests {
    use tracing_subscriber::EnvFilter;

    #[test]
    fn test_env_filter_creation() {
        let _filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("lattice_desktop=info,lattice=info"));
    }
}
