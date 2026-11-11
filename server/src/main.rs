use std::sync::Arc;

use config::ServerConfig;
use network::NetworkError;
use storage::Storage;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

#[tokio::main]
async fn main() -> Result<(), NetworkError> {
    telemetry::init();
    tracing::info!("Initializing FerrisDB telemetry and logging");

    let config = ServerConfig::parse_config();
    let addr = format!("{}:{}", config.host, config.port);
    tracing::debug!(?config, "Configuration loaded successfully");

    tracing::info!(
        address = %addr,
        max_connections = config.max_connections,
        max_memory = config.max_memory,
        "Starting FerrisDB server"
    );

    let storage = Arc::new(Storage::new(config.max_memory));
    let tracker = TaskTracker::new();
    let token = CancellationToken::new();

    tracing::debug!("Spawning shutdown signal listener");
    network::spawn_shutdown_listener(token.clone());

    tracing::debug!("Spawning storage cleaner");
    storage::spawn_storage_cleaner(storage.clone(), token.clone());

    tracing::info!("Server initialization complete; starting network listener");

    let run_result = network::run(&addr, config.max_connections, storage, &tracker, &token).await;
    if let Err(ref err) = run_result {
        tracing::error!(error = %err, "Server encountered a network error");
        token.cancel();
    } else {
        tracing::info!("Network engine stopped");
    }

    tracing::info!("Shutting down FerrisDB: waiting for background tasks to complete");
    tracker.close();
    tracker.wait().await;

    tracing::info!("FerrisDB shutdown complete");

    run_result
}
