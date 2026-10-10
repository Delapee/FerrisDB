use std::sync::Arc;

use tokio::{net::TcpListener, sync::Semaphore};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::{NetworkError, connection::Connection};

pub async fn run(
    addr: &str,
    max_conns: usize,
    tracker: &TaskTracker,
    token: &CancellationToken,
) -> Result<(), NetworkError> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(address = %addr, max_connections = max_conns, "TCP engine listening for connections");

    let semaphore = Arc::new(Semaphore::new(max_conns));

    loop {
        let permit = tokio::select! {
            _ = token.cancelled() => {
                tracing::info!("Shutdown signal received; stopping TCP accept loop");
                break;
            }
            res = semaphore.clone().acquire_owned() => match res {
                Ok(p) => p,
                Err(_) => {
                    tracing::info!("Listener semaphore closed, shutting down accept loop");
                    break;
                }
            },
        };

        tokio::select! {
            res = listener.accept() => {
                let (socket, remote_addr) = res?;
                tracing::debug!(client_addr = %remote_addr, "Accepted incoming connection");

                let cloned_token = token.clone();
                tracker.spawn(async move {
                    let _permit = permit;
                    let mut conn = Connection::new(socket, cloned_token);

                    if let Err(err) = conn.run().await {
                        tracing::debug!(client_addr = %remote_addr, error = %err, "Client connection closed with error");
                    }
                });
            }
            _ = token.cancelled() => {
                tracing::info!("Shutdown signal received; stopping TCP accept loop");
                break;
            }
        }
    }

    Ok(())
}
