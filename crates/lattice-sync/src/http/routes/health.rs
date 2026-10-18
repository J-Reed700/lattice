use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::error::AppResult;
use crate::http::state::AppState;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    status: &'static str,
    db: &'static str,
}

pub async fn healthz(State(state): State<AppState>) -> AppResult<Json<HealthResponse>> {
    let _: i64 = sqlx::query_scalar("SELECT 1::bigint")
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(HealthResponse {
        status: "ok",
        db: "up",
    }))
}
