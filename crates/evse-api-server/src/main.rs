mod server;

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use evse_api_core::manager::SessionManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let manager = Arc::new(SessionManager::new());
    let app = server::build_router(manager);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("EVSE API server listening on ws://{}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}
