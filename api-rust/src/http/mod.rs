pub mod auth;
mod error;
pub mod routes;
pub mod state;

use axum::http::{
    Method,
    header::{AUTHORIZATION, CONTENT_TYPE},
};
use axum::{Router, middleware};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use self::state::AppState;

pub fn router(state: AppState) -> Router {
    let sync_routes = routes::sync::routes().route_layer(middleware::from_fn_with_state(
        state.auth.clone(),
        auth::require_bearer,
    ));
    Router::new()
        .nest("/v1/sync", sync_routes)
        .route("/healthz", axum::routing::get(routes::health::healthz))
        .layer(
            CorsLayer::new()
                .allow_origin(state.cors_allowed_origins.clone())
                .allow_methods([Method::GET, Method::POST])
                .allow_headers([AUTHORIZATION, CONTENT_TYPE]),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
