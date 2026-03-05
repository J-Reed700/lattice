use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use api_rust::config::AppConfig;
use api_rust::http::router;
use api_rust::http::state::AppState;
use api_rust::persistence::db::create_pool;
use api_rust::persistence::sync_repository::PgSyncRepository;
use api_rust::sync::service::SyncServiceImpl;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();

    let config = AppConfig::from_env()?;
    let pool = create_pool(&config.database_url, config.max_db_connections)
        .await
        .context("failed to initialize postgres pool")?;

    if config.run_migrations {
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .context("failed to run database migrations")?;
    }

    let sync_repo = Arc::new(PgSyncRepository::new(pool.clone()));
    let sync_service = Arc::new(SyncServiceImpl::new(sync_repo));
    let state = AppState::new(pool, sync_service);
    let app = router(state);

    let socket: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .context("failed to parse server bind address")?;
    let listener = tokio::net::TcpListener::bind(socket)
        .await
        .context("failed to bind server socket")?;

    info!("api-rust sync service listening on {}", socket);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server terminated with error")?;

    Ok(())
}

fn init_tracing() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "api_rust=info,tower_http=info".into());

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .compact()
        .init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "failed to install ctrl+c handler");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "failed to install SIGTERM handler");
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
