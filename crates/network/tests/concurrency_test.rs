mod common;

use std::time::Duration;

use common::{connect_client, spawn_test_server};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    task::JoinSet,
    time::timeout,
};

#[tokio::test]
async fn test_semaphore_saturation_and_backpressure() {
    // 1. Start server with strict max_conns = 5
    let max_conns = 5;
    let (addr, tracker, token, server_handle) = spawn_test_server(max_conns).await;

    let mut idle_clients = Vec::with_capacity(max_conns);

    // 2. Connect 5 clients and verify they are all actively served
    for _ in 0..max_conns {
        let mut client = connect_client(addr).await;
        client
            .write_all(b"+PING\r\n")
            .await
            .expect("failed to write ping");

        let mut buf = [0u8; 5];
        timeout(Duration::from_secs(1), client.read_exact(&mut buf))
            .await
            .expect("active client timed out")
            .expect("failed to read response");
        assert_eq!(&buf, b"+OK\r\n");

        // Keep client open and idle, holding its semaphore permit
        idle_clients.push(client);
    }

    // 3. Connect a 6th client; the OS TCP backlog accepts the handshake,
    //    but the server loop is blocked waiting for a semaphore permit.
    let mut sixth_client = connect_client(addr).await;
    sixth_client
        .write_all(b"+PING\r\n")
        .await
        .expect("failed to write on 6th client");

    let mut sixth_buf = [0u8; 5];
    let blocked_res = timeout(
        Duration::from_millis(150),
        sixth_client.read_exact(&mut sixth_buf),
    )
    .await;

    assert!(
        blocked_res.is_err(),
        "6th client should not be served while all 5 permits are held"
    );

    // 4. Disconnect 1 of the initial 5 clients to release a semaphore permit
    let dropped_client = idle_clients.pop().expect("no idle client to pop");
    drop(dropped_client);

    // 5. Verify the 6th client is now immediately accepted and served
    timeout(
        Duration::from_secs(2),
        sixth_client.read_exact(&mut sixth_buf),
    )
    .await
    .expect("6th client was not unblocked after permit release")
    .expect("failed to read response on 6th client");

    assert_eq!(&sixth_buf, b"+OK\r\n");

    // 6. Clean teardown
    drop(sixth_client);
    drop(idle_clients);
    token.cancel();
    tracker.close();
    tracker.wait().await;
    server_handle.await.expect("server task panicked");
}

#[tokio::test]
async fn test_mass_concurrent_connect_disconnect_no_permit_leaks() {
    let max_conns = 5;
    let (addr, tracker, token, server_handle) = spawn_test_server(max_conns).await;

    // Wait until server is ready by making 1 quick connection
    drop(connect_client(addr).await);

    // 1. Spawn 50 concurrent tasks opening and closing sockets with small bursts
    let mut join_set = JoinSet::new();
    for _ in 0..50 {
        join_set.spawn(async move {
            let mut client = TcpStream::connect(addr)
                .await
                .expect("concurrent client failed to connect");

            for _ in 0..3 {
                client
                    .write_all(b"+PING\r\n")
                    .await
                    .expect("failed to write burst");
                let mut buf = [0u8; 5];
                timeout(Duration::from_secs(5), client.read_exact(&mut buf))
                    .await
                    .expect("timed out waiting for response under load")
                    .expect("failed to read response");
                assert_eq!(&buf, b"+OK\r\n");
            }
            // Socket drops here on scope exit, releasing permit
        });
    }

    while let Some(res) = join_set.join_next().await {
        res.expect("concurrent client task panicked");
    }

    // 2. Verify zero permit leaks: we must still be able to open `max_conns` (5) simultaneous connections
    let mut check_clients = Vec::with_capacity(max_conns);
    for _ in 0..max_conns {
        let mut client = connect_client(addr).await;
        client
            .write_all(b"+PING\r\n")
            .await
            .expect("failed to write verification ping");

        let mut buf = [0u8; 5];
        timeout(Duration::from_secs(2), client.read_exact(&mut buf))
            .await
            .expect("permit leak detected: could not serve full capacity after mass churn")
            .expect("failed to read verification response");
        assert_eq!(&buf, b"+OK\r\n");

        check_clients.push(client);
    }

    drop(check_clients);
    token.cancel();
    tracker.close();
    tracker.wait().await;
    server_handle.await.expect("server task panicked");
}

#[tokio::test]
async fn test_fault_isolation_between_connections() {
    let (addr, tracker, token, server_handle) = spawn_test_server(10).await;

    // 1. Connect 3 clients
    let mut client_ok_1 = connect_client(addr).await;
    let mut client_bad = connect_client(addr).await;
    let mut client_ok_2 = connect_client(addr).await;

    // 2. Client 2 sends malformed RESP bytes (invalid prefix '#')
    client_bad
        .write_all(b"#MALFORMED_PROTOCOL_FRAME\r\n")
        .await
        .expect("failed to write malformed bytes");
    client_bad
        .flush()
        .await
        .expect("failed to flush bad client");

    // Verify `client_bad` is disconnected by the server (EOF -> 0 bytes read or connection reset)
    let mut dummy = [0u8; 16];
    let bad_read = timeout(Duration::from_secs(2), client_bad.read(&mut dummy))
        .await
        .expect("timed out waiting for server to drop malformed client");

    match bad_read {
        Ok(0) | Err(_) => {} // Expected: clean EOF or reset on protocol violation
        Ok(n) => panic!(
            "expected malformed client to be closed, but read {} bytes",
            n
        ),
    }

    // 3. Validate that `client_ok_1` and `client_ok_2` continue operating without interruption
    for client in [&mut client_ok_1, &mut client_ok_2] {
        client
            .write_all(b"+PING\r\n")
            .await
            .expect("healthy client failed to write");
        let mut buf = [0u8; 5];
        timeout(Duration::from_secs(2), client.read_exact(&mut buf))
            .await
            .expect("healthy client timed out")
            .expect("healthy client failed to read");
        assert_eq!(&buf, b"+OK\r\n");
    }

    drop(client_ok_1);
    drop(client_ok_2);
    token.cancel();
    tracker.close();
    tracker.wait().await;
    server_handle.await.expect("server task panicked");
}
