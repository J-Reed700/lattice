use axum::http::HeaderValue;
use std::env;

use crate::http::auth::AuthConfig;
use anyhow::Context;

#[derive(Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub max_db_connections: u32,
    pub run_migrations: bool,
    pub auth: AuthConfig,
    pub cors_allowed_origins: Vec<HeaderValue>,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = env::var("PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse::<u16>()
            .context("PORT must be a valid u16")?;
        let database_url = env::var("DATABASE_URL")
            .context("DATABASE_URL is required (example: postgres://user:pass@localhost/recall)")?;
        let max_db_connections = env::var("MAX_DB_CONNECTIONS")
            .unwrap_or_else(|_| "20".to_string())
            .parse::<u32>()
            .context("MAX_DB_CONNECTIONS must be a valid u32")?;
        let run_migrations = env::var("RUN_MIGRATIONS")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(true);
        let bearer_token = env::var("API_BEARER_TOKEN")
            .context("API_BEARER_TOKEN is required; generate one with `openssl rand -hex 32`")?;
        let user_id = env::var("API_USER_ID")
            .context("API_USER_ID is required and must identify the configured tenant")?
            .parse::<i64>()
            .context("API_USER_ID must be a positive integer")?;
        let auth = AuthConfig::new(bearer_token, user_id)?;
        let origins = env::var("CORS_ALLOWED_ORIGINS").unwrap_or_else(|_| {
            "http://localhost:5173,http://127.0.0.1:5173,tauri://localhost,http://tauri.localhost".to_string()
        });
        let cors_allowed_origins = parse_cors_origins(&origins)?;

        Ok(Self {
            host,
            port,
            database_url,
            max_db_connections,
            run_migrations,
            auth,
            cors_allowed_origins,
        })
    }
}

fn parse_cors_origins(origins: &str) -> anyhow::Result<Vec<HeaderValue>> {
    let parsed = origins
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .map(|origin| {
            anyhow::ensure!(
                origin != "*",
                "CORS_ALLOWED_ORIGINS must list explicit origins, not `*`"
            );
            HeaderValue::from_str(origin).context("CORS_ALLOWED_ORIGINS contains an invalid origin")
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    anyhow::ensure!(
        !parsed.is_empty(),
        "CORS_ALLOWED_ORIGINS must contain at least one explicit origin"
    );
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::parse_cors_origins;

    #[test]
    fn cors_requires_explicit_non_wildcard_origins() {
        assert!(parse_cors_origins("*").is_err());
        assert!(parse_cors_origins(" , ").is_err());
        let origins = parse_cors_origins("https://app.example, http://localhost:5173").unwrap();
        assert_eq!(origins.len(), 2);
        assert_eq!(origins[0], "https://app.example");
    }
}
