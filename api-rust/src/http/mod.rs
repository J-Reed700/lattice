mod error;
pub mod routes;
pub mod state;

use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use self::state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .nest("/v1/sync", routes::sync::routes())
        .route("/healthz", axum::routing::get(routes::health::healthz))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
