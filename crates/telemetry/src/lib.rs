use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Initializes the global telemetry and structured logging system.
/// Must be called only once at the beginning of the entry point.
pub fn init() {
    // Attempts to read the RUST_LOG environment variable. If absent, defaults to 'info'.
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // Configures the formatting layer for standard output.
    // 'with_thread_ids(true)' is vital for tracking concurrent tasks across worker threads in Tokio.
    let formatting_layer = fmt::layer()
        .pretty()
        .with_target(true)
        .with_thread_ids(true);

    // Globally registers the subscriber registry.
    tracing_subscriber::registry()
        .with(env_filter)
        .with(formatting_layer)
        .init();
}
