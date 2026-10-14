mod common;

use std::time::Duration;

use bytes::{BufMut, Bytes, BytesMut};
use codec::Frame;
use common::{connect_client, spawn_test_server};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    time::timeout,
};

#[tokio::test]
async fn test_massive_sequential_burst_pipelining() {
    let (addr, tracker, token, server_handle) = spawn_test_server(10).await;
    let mut client = connect_client(addr).await;

    // 1. Build a continuous burst of 1,000 PING commands without waiting for intermediate replies
    let burst_count = 1_000;
    let mut request_buffer = BytesMut::with_capacity(burst_count * 7);
    for _ in 0..burst_count {
        request_buffer.put_slice(b"+PING\r\n");
    }

    // 2. Send the entire burst in a single write operation
    client
        .write_all(&request_buffer)
        .await
        .expect("failed to write burst payload");
    client.flush().await.expect("failed to flush client stream");

    // 3. Read all 1,000 responses ("+OK\r\n" = 5 bytes each)
    let single_response = b"+OK\r\n";
    let expected_total_bytes = single_response.len() * burst_count;
    let mut response_buffer = vec![0u8; expected_total_bytes];

    timeout(
        Duration::from_secs(5),
        client.read_exact(&mut response_buffer),
    )
    .await
    .expect("timed out waiting for 1,000 pipelined responses")
    .expect("failed to read pipelined response bytes");

    // 4. Verify exact ordering and zero byte desynchronization
    for (idx, chunk) in response_buffer
        .chunks_exact(single_response.len())
        .enumerate()
    {
        assert_eq!(
            chunk, single_response,
            "response mismatch at command index {}",
            idx
        );
    }

    // 5. Clean teardown
    drop(client);
    token.cancel();
    tracker.close();
    tracker.wait().await;
    server_handle.await.expect("server task panicked");
}

#[tokio::test]
async fn test_heterogeneous_frames_pipeline() {
    let (addr, tracker, token, server_handle) = spawn_test_server(10).await;
    let mut client = connect_client(addr).await;

    // 1. Construct a pipeline mixing small frames and large payloads (64 KB)
    //    to force dynamic growth of `read_buffer` and `write_buffer`.
    let large_payload_size = 64 * 1024;
    let large_data = vec![b'X'; large_payload_size];

    let mut pipeline_buffer = BytesMut::new();

    // Frame 1: Short SimpleString
    codec::encode(
        &Frame::SimpleString(Bytes::from_static(b"PING")),
        &mut pipeline_buffer,
    );

    // Frame 2: 64 KB BulkString (exceeds initial 4 KB buffer capacity)
    codec::encode(
        &Frame::BulkString(Bytes::from(large_data)),
        &mut pipeline_buffer,
    );

    // Frame 3: Integer
    codec::encode(&Frame::Integer(987654321), &mut pipeline_buffer);

    // Frame 4: Array of BulkStrings
    codec::encode(
        &Frame::Array(vec![
            Frame::BulkString(Bytes::from_static(b"ECHO")),
            Frame::BulkString(Bytes::from_static(b"heterogeneous_payload")),
        ]),
        &mut pipeline_buffer,
    );

    // Frame 5: Trailing short SimpleString
    codec::encode(
        &Frame::SimpleString(Bytes::from_static(b"PING")),
        &mut pipeline_buffer,
    );

    // 2. Send heterogeneous batch in one shot
    client
        .write_all(&pipeline_buffer)
        .await
        .expect("failed to write heterogeneous pipeline");
    client.flush().await.expect("failed to flush client stream");

    // 3. Expect 5 "+OK\r\n" responses (25 bytes total)
    let expected_responses = 5;
    let single_response = b"+OK\r\n";
    let mut response_buffer = vec![0u8; single_response.len() * expected_responses];

    timeout(
        Duration::from_secs(5),
        client.read_exact(&mut response_buffer),
    )
    .await
    .expect("timed out waiting for heterogeneous pipeline responses")
    .expect("failed to read response bytes");

    for (idx, chunk) in response_buffer
        .chunks_exact(single_response.len())
        .enumerate()
    {
        assert_eq!(
            chunk, single_response,
            "corrupted frame response at index {}",
            idx
        );
    }

    // 4. Clean teardown
    drop(client);
    token.cancel();
    tracker.close();
    tracker.wait().await;
    server_handle.await.expect("server task panicked");
}
