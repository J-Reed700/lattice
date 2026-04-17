pub mod errors;
pub mod metrics;
// Vertical-slice migration (metrics): adapter lives in features/metrics/adapter.rs.
// (Core `metrics` service stays here — it's shared observability infrastructure.)
#[path = "../../features/metrics/adapter.rs"]
pub mod metrics_adapter;
pub mod tracing;

pub use errors::track_error;
pub use metrics::{Metrics, MetricsSnapshot};
