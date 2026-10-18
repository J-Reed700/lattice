use axum::extract::{Extension, Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use uuid::Uuid;

use crate::error::AppResult;
use crate::http::auth::AuthenticatedPrincipal;
use crate::http::state::AppState;
use crate::sync::types::{
    AckRequest, AckResponse, PullChangesRequest, PullChangesResponse, PushChangesRequest,
    PushChangesResponse, RegisterDeviceRequest, ResolveConflictRequest, SyncStatusResponse,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/devices/register", post(register_device))
        .route("/push", post(push_changes))
        .route("/pull", post(pull_changes))
        .route("/ack", post(ack_checkpoint))
        .route("/conflicts/resolve", post(resolve_conflict))
        .route("/status/{device_id}", get(get_status))
}

async fn register_device(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Json(request): Json<RegisterDeviceRequest>,
) -> AppResult<Json<crate::sync::types::DeviceResponse>> {
    let user_id = principal.user_id;
    let response = state.sync_service.register_device(user_id, request).await?;
    Ok(Json(response))
}

async fn push_changes(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Json(request): Json<PushChangesRequest>,
) -> AppResult<Json<PushChangesResponse>> {
    let user_id = principal.user_id;
    let response = state.sync_service.push_changes(user_id, request).await?;
    Ok(Json(response))
}

async fn pull_changes(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Json(request): Json<PullChangesRequest>,
) -> AppResult<Json<PullChangesResponse>> {
    let user_id = principal.user_id;
    let response = state.sync_service.pull_changes(user_id, request).await?;
    Ok(Json(response))
}

async fn ack_checkpoint(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Json(request): Json<AckRequest>,
) -> AppResult<Json<AckResponse>> {
    let user_id = principal.user_id;
    let response = state.sync_service.ack_checkpoint(user_id, request).await?;
    Ok(Json(response))
}

async fn resolve_conflict(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Json(request): Json<ResolveConflictRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = principal.user_id;
    state
        .sync_service
        .resolve_conflict(user_id, request)
        .await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn get_status(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(device_id): Path<Uuid>,
) -> AppResult<Json<SyncStatusResponse>> {
    let user_id = principal.user_id;
    let response = state.sync_service.get_status(user_id, device_id).await?;
    Ok(Json(response))
}
