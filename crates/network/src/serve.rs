use std::sync::Arc;

use tokio::{net::TcpListener, sync::Semaphore};

use crate::{NetworkError, connection::Connection};

pub async fn run(addr: &str, max_conns: usize) -> Result<(), NetworkError> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(address = %addr, max_connections = max_conns, "TCP engine listening for connections");

    let semaphore = Arc::new(Semaphore::new(max_conns));

    loop {
        let permit = match semaphore.clone().acquire_owned().await {
            Ok(p) => p,
            Err(_) => {
                tracing::info!("Listener semaphore closed, shutting down accept loop");
                break;
            }
        };

        let (socket, remote_addr) = listener.accept().await?;
        tracing::debug!(client_addr = %remote_addr, "Accepted incoming connection");

        tokio::spawn(async move {
            let _permit = permit;
            let mut conn = Connection::new(socket);

            if let Err(err) = conn.run().await {
                tracing::debug!(client_addr = %remote_addr, error = %err, "Client connection closed with error");
            }
        });
    }

    Ok(())
}
