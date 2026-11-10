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
    state.database.ping().await?;

    Ok(Json(HealthResponse {
        status: "ok",
        db: "up",
    }))
}
