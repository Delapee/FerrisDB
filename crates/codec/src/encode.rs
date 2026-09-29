use bytes::{BufMut, BytesMut};
use std::io::Write;

use crate::{
    Frame,
    constants::{CRLF, CRLF_LEN},
};

const MAX_INT_STRING_LEN: usize = 20;
const NULL_FRAME_LEN: usize = 1 + 2 + CRLF_LEN;

pub fn encode(frame: &Frame, buf: &mut BytesMut) {
    match frame {
        Frame::SimpleString(bytes) => {
            let size = 1 + bytes.len() + CRLF_LEN;
            buf.reserve(size);

            buf.put_u8(b'+');
            buf.put(&bytes[..]);
            buf.put_slice(CRLF);
        }
        Frame::Error(bytes) => {
            let size = 1 + bytes.len() + CRLF_LEN;
            buf.reserve(size);

            buf.put_u8(b'-');
            buf.put(&bytes[..]);
            buf.put_slice(CRLF);
        }
        Frame::Integer(int) => {
            let size = 1 + MAX_INT_STRING_LEN + CRLF_LEN;
            buf.reserve(size);

            buf.put_u8(b':');
            let _ = write!(buf.writer(), "{int}");
            buf.put_slice(CRLF);
        }
        Frame::BulkString(bytes) => {
            let size = 1 + MAX_INT_STRING_LEN + CRLF_LEN + bytes.len() + CRLF_LEN;
            buf.reserve(size);

            buf.put_u8(b'$');
            let _ = write!(buf.writer(), "{}", bytes.len());
            buf.put_slice(CRLF);
            buf.put(&bytes[..]);
            buf.put_slice(CRLF);
        }
        Frame::NullBulkString => {
            buf.reserve(NULL_FRAME_LEN);
            buf.put_slice(b"$-1\r\n");
        }
        Frame::Array(frames) => {
            let header_size = 1 + MAX_INT_STRING_LEN + CRLF_LEN;
            buf.reserve(header_size);

            buf.put_u8(b'*');
            let _ = write!(buf.writer(), "{}", frames.len());
            buf.put_slice(CRLF);

            for frame in frames {
                encode(frame, buf);
            }
        }
        Frame::NullArray => {
            buf.reserve(NULL_FRAME_LEN);
            buf.put_slice(b"*-1\r\n");
        }
    };
}
