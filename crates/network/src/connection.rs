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
                        break;
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
