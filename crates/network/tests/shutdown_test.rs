mod common;

use std::time::Duration;

use bytes::{BufMut, BytesMut};
use common::{connect_client, spawn_test_server};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::{sleep, timeout},
};

#[tokio::test]
async fn test_immediate_admission_closure_on_cancel() {
    let (addr, tracker, token, server_handle) = spawn_test_server(10).await;

    // Ensure the listener is actively up before cancelling
    let initial_client = connect_client(addr).await;
    drop(initial_client);

    // 1. Trigger cancellation and wait for the accept loop to exit
    token.cancel();
    timeout(Duration::from_secs(2), server_handle)
        .await
        .expect("server accept loop timed out on cancellation")
        .expect("server task panicked");

    // 2. Verify any subsequent connection attempt is immediately refused
    let connect_res = TcpStream::connect(addr).await;
    assert!(
        connect_res.is_err(),
        "new TCP connection should be refused after accept loop shutdown"
    );

    tracker.close();
    tracker.wait().await;
}

#[tokio::test]
async fn test_active_client_draining_during_shutdown() {
    let (addr, tracker, token, server_handle) = spawn_test_server(10).await;
    let mut client = connect_client(addr).await;

    // 1. Build a pipeline of 200 commands
    let batch_count = 200;
    let mut pipeline = BytesMut::with_capacity(batch_count * 7);
    for _ in 0..batch_count {
        pipeline.put_slice(b"+PING\r\n");
    }

    // 2. Send the pipeline and immediately trigger server cancellation
    client
        .write_all(&pipeline)
        .await
        .expect("failed to write pipeline before shutdown");
    client.flush().await.expect("failed to flush client");

    // Brief yield so the socket receives the bytes before cancellation fires
    sleep(Duration::from_millis(10)).await;
    token.cancel();

    // 3. Verify the client receives all 200 responses intact before the socket closes
    let single_response = b"+OK\r\n";
    let mut response_buf = vec![0u8; single_response.len() * batch_count];

    timeout(Duration::from_secs(3), client.read_exact(&mut response_buf))
        .await
        .expect("timed out draining responses from shutting down server")
        .expect("failed to read drained responses");

    for chunk in response_buf.chunks_exact(single_response.len()) {
        assert_eq!(chunk, single_response);
    }

    // 4. Verify tracker completes cleanly
    tracker.close();
    timeout(Duration::from_secs(2), tracker.wait())
        .await
        .expect("tracker.wait() deadlocked after draining active client");
    server_handle.await.expect("server task panicked");
}

#[tokio::test]
async fn test_idle_clients_resolution_on_shutdown() {
    let (addr, tracker, token, server_handle) = spawn_test_server(10).await;

    // 1. Connect 5 clients that remain completely idle (sending zero traffic)
    let mut idle_clients = Vec::new();
    for _ in 0..5 {
        let client = connect_client(addr).await;
        idle_clients.push(client);
    }

    // Give the server a moment to spawn all 5 `Connection::run` tasks
    sleep(Duration::from_millis(20)).await;

    // 2. Fire cancellation token while all 5 clients are blocked on `read_buf`
    token.cancel();
    tracker.close();

    // 3. Verify `tracker.wait()` resolves in milliseconds without deadlocking
    timeout(Duration::from_millis(500), tracker.wait())
        .await
        .expect("deadlock detected: idle clients blocked graceful shutdown");

    timeout(Duration::from_millis(500), server_handle)
        .await
        .expect("server accept loop did not exit in time")
        .expect("server task panicked");

    // 4. Confirm the server closed the remote ends of the idle sockets (EOF)
    for mut client in idle_clients {
        let mut buf = [0u8; 1];
        let read_res = timeout(Duration::from_millis(200), client.read(&mut buf))
            .await
            .expect("idle client socket was not closed by server")
            .expect("I/O error checking EOF on idle client");
        assert_eq!(read_res, 0, "expected clean EOF (0 bytes) on idle client");
    }
}
