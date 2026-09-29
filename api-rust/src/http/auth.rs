use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{HeaderMap, header::AUTHORIZATION},
    middleware::Next,
    response::Response,
};
use subtle::ConstantTimeEq;

use crate::error::{AppError, AppResult};

/// One server-configured opaque bearer credential mapped to one tenant.
/// This is a fail-closed scaffold for a single configured principal; clients
/// cannot select or override its tenant identity.
#[derive(Clone)]
pub struct AuthConfig {
    token: Arc<str>,
    user_id: i64,
}

impl AuthConfig {
    pub fn new(token: impl Into<String>, user_id: i64) -> anyhow::Result<Self> {
        let token = token.into();
        anyhow::ensure!(
            token.len() >= 32,
            "API_BEARER_TOKEN must contain at least 32 bytes"
        );
        anyhow::ensure!(
            !token.bytes().any(|byte| byte.is_ascii_whitespace()),
            "API_BEARER_TOKEN must not contain whitespace"
        );
        anyhow::ensure!(user_id > 0, "API_USER_ID must be a positive integer");
        Ok(Self {
            token: Arc::from(token),
            user_id,
        })
    }

    pub fn principal_from_headers(&self, headers: &HeaderMap) -> AppResult<AuthenticatedPrincipal> {
        let mut values = headers.get_all(AUTHORIZATION).iter();
        let value = values.next().ok_or(AppError::Unauthorized)?;
        if values.next().is_some() {
            return Err(AppError::Unauthorized);
        }
        let value = value.to_str().map_err(|_| AppError::Unauthorized)?;
        let (scheme, supplied) = value.split_once(' ').ok_or(AppError::Unauthorized)?;
        if !scheme.eq_ignore_ascii_case("bearer")
            || supplied.is_empty()
            || supplied.bytes().any(|byte| byte.is_ascii_whitespace())
        {
            return Err(AppError::Unauthorized);
        }
        let supplied = supplied.as_bytes();
        let expected = self.token.as_bytes();
        if supplied.len() != expected.len() || !bool::from(supplied.ct_eq(expected)) {
            return Err(AppError::Unauthorized);
        }
        Ok(AuthenticatedPrincipal {
            user_id: self.user_id,
        })
    }
}

pub async fn require_bearer(
    State(auth): State<AuthConfig>,
    mut request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let principal = auth.principal_from_headers(request.headers())?;
    request.extensions_mut().insert(principal);
    Ok(next.run(request).await)
}

#[derive(Clone, Copy, Debug)]
pub struct AuthenticatedPrincipal {
    pub user_id: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "fixture-bearer-token-that-is-long-enough-0123456789";

    fn auth() -> AuthConfig {
        AuthConfig::new(TOKEN, 73).unwrap()
    }

    #[test]
    fn missing_or_invalid_bearer_credentials_are_unauthorized() {
        for header in [
            None,
            Some("Basic abc"),
            Some("Bearer wrong-token"),
            Some("Bearer "),
        ] {
            let mut headers = HeaderMap::new();
            if let Some(value) = header {
                headers.insert(AUTHORIZATION, value.parse().unwrap());
            }
            assert!(matches!(
                auth().principal_from_headers(&headers),
                Err(AppError::Unauthorized)
            ));
        }
        let mut duplicate = HeaderMap::new();
        duplicate.append(AUTHORIZATION, format!("Bearer {TOKEN}").parse().unwrap());
        duplicate.append(AUTHORIZATION, format!("Bearer {TOKEN}").parse().unwrap());
        assert!(matches!(
            auth().principal_from_headers(&duplicate),
            Err(AppError::Unauthorized)
        ));
    }

    #[test]
    fn bearer_token_maps_to_configured_tenant_and_ignores_forged_user_header() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, format!("Bearer {TOKEN}").parse().unwrap());
        headers.insert("x-user-id", "999999".parse().unwrap());
        let principal = auth().principal_from_headers(&headers).unwrap();
        assert_eq!(principal.user_id, 73);
    }

    #[test]
    fn rejects_weak_server_credentials_and_invalid_tenant_configuration() {
        assert!(AuthConfig::new("short", 1).is_err());
        assert!(AuthConfig::new(TOKEN, 0).is_err());
    }
}
