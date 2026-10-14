use bytes::{Bytes, BytesMut};
use codec::Frame;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_util::sync::CancellationToken;

use crate::error::NetworkError;

const BUFFER_SIZE: usize = 4 * 1024; // 4KB

pub(crate) struct Connection {
    stream: TcpStream,
    token: CancellationToken,
    read_buffer: BytesMut,
    write_buffer: BytesMut,
}

impl Connection {
    pub(crate) fn new(socket: TcpStream, token: CancellationToken) -> Self {
        Self {
            stream: socket,
            token,
            read_buffer: BytesMut::with_capacity(BUFFER_SIZE),
            write_buffer: BytesMut::with_capacity(BUFFER_SIZE),
        }
    }

    pub(crate) async fn run(&mut self) -> Result<(), NetworkError> {
        loop {
            tokio::select! {
                res = self.stream.read_buf(&mut self.read_buffer) => {
                    let read_bytes = res?;
                    if read_bytes == 0 {
                        if self.read_buffer.is_empty() {
                            break;
                        } else {
                            return Err(NetworkError::UnexpectedEof);
                        }
                    }

                    self.process_pipeline().await?;
                }
                _ = self.token.cancelled() => {
                    tracing::debug!("Client session terminating due to server shutdown");
                    let _ = self.process_pipeline().await;
                    break;
                }
            }
        }

        Ok(())
    }

    async fn process_pipeline(&mut self) -> Result<(), NetworkError> {
        let decode_result = loop {
            match codec::decode(&mut self.read_buffer) {
                Ok(Some(_frame)) => {
                    let frame = Frame::SimpleString(Bytes::from("OK"));
                    codec::encode(&frame, &mut self.write_buffer);
                }
                Ok(None) => break Ok(()),
                Err(err) => break Err(err.into()),
            }
        };

        if !self.write_buffer.is_empty() {
            self.stream.write_all_buf(&mut self.write_buffer).await?;
        }

        decode_result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    use bytes::BufMut;
    use tokio::{
        net::TcpListener,
        time::{sleep, timeout},
    };

    const OK_BYTES_RESPONSE: &[u8; 5] = b"+OK\r\n";

    async fn create_connected_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind ephemeral listener");
        let addr = listener.local_addr().expect("failed to get local address");

        let (client_res, server_res) = tokio::join!(TcpStream::connect(addr), listener.accept());

        let client = client_res.expect("client failed to connect");
        let (server, _) = server_res.expect("server failed to accept connection");

        (client, server)
    }

    #[tokio::test]
    async fn test_fragmented_tcp_chunks() {
        let (mut client, server) = create_connected_pair().await;

        let mut connection = Connection::new(server, CancellationToken::new());
        let conn_handle = tokio::spawn(async move { connection.run().await });

        let payload = b"+PING\r\n";
        for &byte in payload {
            client
                .write_all(&[byte])
                .await
                .expect("failed to write byte");
            client
                .flush()
                .await
                .expect("failed to flush byte to socket");

            sleep(Duration::from_millis(5)).await;
        }

        let mut response_buf = [0u8; OK_BYTES_RESPONSE.len()];
        timeout(Duration::from_secs(2), client.read_exact(&mut response_buf))
            .await
            .expect("timed out waiting for response")
            .expect("failed to read response");

        assert_eq!(&response_buf, OK_BYTES_RESPONSE);

        drop(client);

        let run_res = conn_handle.await.expect("connection task panicked");
        assert!(run_res.is_ok());
    }

    #[tokio::test]
    async fn test_pipelining_batch_processing() {
        let (mut client, server) = create_connected_pair().await;

        let mut connection = Connection::new(server, CancellationToken::new());
        let conn_handle = tokio::spawn(async move { connection.run().await });

        let batch_count = 10;
        let mut batch = BytesMut::new();
        for _ in 0..batch_count {
            batch.put_slice(b"+PING\r\n");
        }

        client
            .write_all(&batch)
            .await
            .expect("failed to write pipelined batch");
        client.flush().await.expect("failed to flush batch");

        let mut response_buf = vec![0u8; OK_BYTES_RESPONSE.len() * batch_count];
        client
            .read_exact(&mut response_buf)
            .await
            .expect("failed to read aggregated responses");

        for chunk in response_buf.as_chunks::<{ OK_BYTES_RESPONSE.len() }>().0 {
            assert_eq!(chunk, OK_BYTES_RESPONSE);
        }

        drop(client);

        let run_res = conn_handle.await.expect("task panicked");
        assert!(run_res.is_ok());
    }

    #[tokio::test]
    async fn test_client_clean_eof_shutdown() {
        let (client, server) = create_connected_pair().await;

        let mut connection = Connection::new(server, CancellationToken::new());
        let conn_handle = tokio::spawn(async move { connection.run().await });

        drop(client);

        let run_res = conn_handle.await.expect("task panicked");
        assert!(run_res.is_ok());
    }

    #[tokio::test]
    async fn test_error_priority_and_partial_flush() {
        let (mut client, server) = create_connected_pair().await;

        let mut connection = Connection::new(server, CancellationToken::new());
        let conn_handle = tokio::spawn(async move { connection.run().await });

        let mut mixed_payload = BytesMut::new();
        mixed_payload.put_slice(b"+PING\r\n");
        mixed_payload.put_slice(b"#INVALID\r\n");

        client
            .write_all(&mixed_payload)
            .await
            .expect("failed to write payload");
        client.flush().await.expect("failed to flush");

        let mut valid_response = [0u8; OK_BYTES_RESPONSE.len()];
        client
            .read_exact(&mut valid_response)
            .await
            .expect("failed to read valid response before connection dropped");
        assert_eq!(&valid_response, OK_BYTES_RESPONSE);

        let run_res = conn_handle.await.expect("task panicked");
        assert!(run_res.is_err());
    }

    #[tokio::test]
    async fn test_cancellation_drains_residual_commands() {
        let (mut client, server) = create_connected_pair().await;
        let token = CancellationToken::new();

        let mut connection = Connection::new(server, token.clone());
        let conn_handle = tokio::spawn(async move { connection.run().await });

        client
            .write_all(b"+PING\r\n")
            .await
            .expect("failed to send command");
        client.flush().await.expect("failed to flush");

        sleep(Duration::from_millis(20)).await;
        token.cancel();

        let mut response_buf = [0u8; OK_BYTES_RESPONSE.len()];
        client
            .read_exact(&mut response_buf)
            .await
            .expect("failed to read response during shutdown drain");
        assert_eq!(&response_buf, OK_BYTES_RESPONSE);

        let run_res = conn_handle.await.expect("task panicked");
        assert!(run_res.is_ok());
    }
}
