//! Metrics feature dependency injection.

use std::sync::Arc;

use crate::application::ports::MetricsPort;
use crate::features::metrics::adapter::MetricsAdapter;
use crate::features::metrics::use_cases::GetMetricsUseCase;
use crate::infrastructure::observability::metrics::Metrics;

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
