//! HTTP rendering belongs to the transport adapter, not the sync core.
use crate::error::AppError;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Validation(_) => StatusCode::BAD_REQUEST,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Db(_) | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let body = Json(ErrorBody {
            error: self.to_string(),
        });
        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn errors_keep_status_contracts_and_hide_database_details() {
        for (error, status) in [
            (
                AppError::Validation("bad input".into()),
                StatusCode::BAD_REQUEST,
            ),
            (AppError::NotFound("missing".into()), StatusCode::NOT_FOUND),
            (AppError::Conflict("stale".into()), StatusCode::CONFLICT),
            (
                AppError::Internal("failed".into()),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ] {
            assert_eq!(error.into_response().status(), status);
        }
        let response = AppError::Db(Box::new(std::io::Error::other("private database details")))
            .into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"], "database error");
    }
}
