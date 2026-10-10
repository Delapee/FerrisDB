use clap::Parser;

/// FerrisDB Configuration Parser
/// This struct defines how the server can be configured via CLI arguments or environment variables.
#[derive(Parser, Debug)]
#[command(name = "ferrisdb", version, about = "A high-performance Redis clone in Rust", long_about = None)]
pub struct ServerConfig {
    /// IP address or hostname to bind the TCP listener
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Port number to bind the TCP listener
    #[arg(short, long, default_value_t = 6379)]
    pub port: u16,

    /// Maximum memory limit in bytes before eviction policies kick in (Default: 1GB)
    #[arg(long, default_value_t = 1073741824)]
    pub max_memory: usize,

    /// Maximum number of concurrent client connections allowed
    #[arg(long, default_value_t = 1024)]
    pub max_connections: usize,
}

impl ServerConfig {
    /// Parses the command line arguments and returns the configuration.
    pub fn parse_config() -> Self {
        Self::parse()
    }
}
