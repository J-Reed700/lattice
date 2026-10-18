use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricDto {
    pub name: String,
    pub value: f64,
    pub unit: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsSnapshotDto {
    pub documents_indexed: u64,
    pub searches_performed: u64,
    pub qa_queries: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub avg_search_time_ms: f64,
    pub avg_qa_time_ms: f64,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordMetricResultDto {
    pub success: bool,
    pub message: Option<String>,
}
