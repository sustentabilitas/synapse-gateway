use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use synapse_proxy::build_router;
use synapse_proxy::config::Config;
use synapse_proxy::proxy::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("install rustls CryptoProvider");
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = Config::load()?;
    let shutting_down = Arc::new(AtomicBool::new(false));
    let state = AppState {
        routes: Arc::new(config.routes),
        client: reqwest::Client::new(),
        shutting_down: shutting_down.clone(),
    };

    let listener = tokio::net::TcpListener::bind(&config.addr).await?;
    tracing::info!(addr = %config.addr, "synapse-proxy listening");
    axum::serve(listener, build_router(state))
        .with_graceful_shutdown(shutdown_signal(shutting_down))
        .await?;
    Ok(())
}

async fn shutdown_signal(shutting_down: Arc<AtomicBool>) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install ctrl-c handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
    tracing::info!("synapse-proxy received shutdown signal; draining");
    shutting_down.store(true, Ordering::SeqCst);
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
}
