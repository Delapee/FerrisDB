use bytes::BytesMut;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use crate::error::NetworkError;

const BUFFER_SIZE: usize = 4 * 1024; // 4KB

pub(crate) struct Connection {
    stream: TcpStream,
    read_buffer: BytesMut,
    write_buffer: BytesMut,
}

impl Connection {
    pub(crate) fn new(socket: TcpStream) -> Self {
        Self {
            stream: socket,
            read_buffer: BytesMut::with_capacity(BUFFER_SIZE),
            write_buffer: BytesMut::with_capacity(BUFFER_SIZE),
        }
    }

    pub(crate) async fn run(&mut self) -> Result<(), NetworkError> {
        loop {
            let read_bytes = self.stream.read_buf(&mut self.read_buffer).await?;
            if read_bytes == 0 {
                break;
            }

            while let Some(frame) = codec::decode(&mut self.read_buffer)? {
                codec::encode(&frame, &mut self.write_buffer);
            }

            if !self.write_buffer.is_empty() {
                self.stream.write_all_buf(&mut self.write_buffer).await?;
                self.write_buffer.clear();
            }
        }

        Ok(())
    }
}
