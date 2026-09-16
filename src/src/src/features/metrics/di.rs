//! Metrics feature dependency injection.

use std::sync::Arc;

use crate::application::ports::MetricsPort;
use crate::features::metrics::adapter::MetricsAdapter;
use crate::features::metrics::use_cases::GetMetricsUseCase;
use crate::infrastructure::observability::metrics::Metrics;
use crate::interfaces::di::Container;

#[derive(Clone)]
pub struct MetricsDi {
    pub metrics: Arc<dyn MetricsPort>,
    pub metrics_service: Arc<Metrics>,
    pub get_metrics_use_case: Arc<GetMetricsUseCase>,
}

pub fn build() -> MetricsDi {
    let metrics_service = Arc::new(Metrics::new());
    let metrics =
        Arc::new(MetricsAdapter::new(metrics_service.as_ref().clone())) as Arc<dyn MetricsPort>;

    MetricsDi {
        get_metrics_use_case: Arc::new(GetMetricsUseCase::new(metrics.clone())),
        metrics,
        metrics_service,
    }
}

/// Metrics' registrar surface on `Container`.
impl Container {
    // Metrics (from SystemModule)
    pub fn get_metrics_use_case(&self) -> Arc<GetMetricsUseCase> {
        Arc::clone(self.system.get_metrics_use_case())
    }

    /// Get metrics service (for legacy commands - Note: Metrics is concrete type, not in modules)
    pub fn metrics(&self) -> Arc<Metrics> {
        Arc::clone(self.system.metrics_service())
    }
}
