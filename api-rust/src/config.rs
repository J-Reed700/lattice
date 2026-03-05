use std::env;

use anyhow::Context;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub max_db_connections: u32,
    pub run_migrations: bool,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
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

        Ok(Self {
            host,
            port,
            database_url,
            max_db_connections,
            run_migrations,
        })
    }
}
