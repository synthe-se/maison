use std::net::SocketAddr;

use maison_backend::app_parts_from_env;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    init_tracing();

    let (app, state) = app_parts_from_env().expect("failed to build app");
    state
        .validate_runtime_security()
        .expect("insecure runtime configuration");
    let addr: SocketAddr = state
        .config
        .listen_address()
        .parse()
        .expect("invalid listen address");
    let listener = TcpListener::bind(addr).await.expect("bind failed");

    tracing::info!(address = %listener.local_addr().expect("local addr"), "listening");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .expect("server error");

    state.shutdown().await;
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler");
        signal.recv().await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received");
}

fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "maison_backend=info,tower_http=warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}
