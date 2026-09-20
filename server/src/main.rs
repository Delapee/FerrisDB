use config::ServerConfig;

#[tokio::main]
async fn main() {
    telemetry::init();

    let config = ServerConfig::parse_config();

    tracing::info!(
        "Starting FerrisDB (Redis Clone) on {}:{}",
        config.host,
        config.port
    );

    tracing::debug!("Loaded configuration: {:?}", config);

    // TODO: Bind the tokio::net::TcpListener

    tracing::info!("Server initialization complete. Shutting down (Temporary).");
}
