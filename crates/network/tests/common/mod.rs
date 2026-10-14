use std::{net::SocketAddr, time::Duration};

use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
    time::sleep,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

/// Finds a free ephemeral port, spawns `network::run`, and returns
/// the bound address, tracker, cancellation token, and server task handle.
#[allow(dead_code)]
pub async fn spawn_test_server(
    max_conns: usize,
) -> (SocketAddr, TaskTracker, CancellationToken, JoinHandle<()>) {
    let temp_listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind ephemeral port");
    let addr = temp_listener
        .local_addr()
        .expect("failed to get local address");
    drop(temp_listener);

    let tracker = TaskTracker::new();
    let token = CancellationToken::new();

    let server_tracker = tracker.clone();
    let server_token = token.clone();
    let addr_str = addr.to_string();

    let handle = tokio::spawn(async move {
        network::run(&addr_str, max_conns, &server_tracker, &server_token)
            .await
            .expect("server run failed");
    });

    (addr, tracker, token, handle)
}

/// Connects to the test server, retrying briefly until the listener is bound.
#[allow(dead_code)]
pub async fn connect_client(addr: SocketAddr) -> TcpStream {
    for _ in 0..50 {
        if let Ok(stream) = TcpStream::connect(addr).await {
            return stream;
        }
        sleep(Duration::from_millis(5)).await;
    }
    panic!("failed to connect to test server at {}", addr);
}
