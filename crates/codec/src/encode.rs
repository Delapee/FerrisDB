//! RESP frame serialization and buffer encoding.
//!
//! This module provides functions for serializing [`Frame`] instances into raw bytes
//! formatted according to the RESP wire protocol specification.
//!
//! # Allocation & Capacity Reservation Strategy
//!
//! Serialization writes directly into a caller-supplied [`BytesMut`] buffer.
//! Before appending each segment (prefix, length header, payload, and CRLF delimiters),
//! [`encode`] explicitly invokes [`BytesMut::reserve`] with an exact or upper-bound
//! byte estimate. This eliminates intermediate reallocations on the hot path and minimizes
//! heap fragmentation.

use std::io::Write;

use bytes::{BufMut, BytesMut};

use crate::{
    Frame,
    constants::{CRLF, CRLF_LEN},
};

/// Maximum ASCII character length required to represent any 64-bit integer, including sign.
///
/// In decimal representation, [`i64::MIN`] (`-9223372036854775808`) occupies exactly 20 bytes.
const MAX_INT_STRING_LEN: usize = 20;

/// Byte length of a null bulk string (`$-1\r\n`) or null array (`*-1\r\n`) wire representation.
///
/// Composed of: 1-byte prefix + 2-byte indicator (`-1`) + 2-byte CRLF (`\r\n`) = 5 bytes.
const NULL_FRAME_LEN: usize = 1 + 2 + CRLF_LEN;

/// Encodes a [`Frame`] into its RESP wire-format byte representation.
///
/// The serialized bytes are appended directly to the provided [`BytesMut`] buffer.
/// If the buffer does not have enough remaining capacity, it will automatically grow,
/// pre-reserving required capacity for each component.
///
/// # Wire Format Mappings
///
/// - [`Frame::SimpleString`][]: `+<payload>\r\n`
/// - [`Frame::Error`][]: `-<message>\r\n`
/// - [`Frame::Integer`][]: `:<value>\r\n`
/// - [`Frame::BulkString`][]: `$<len>\r\n<payload>\r\n`
/// - [`Frame::NullBulkString`][]: `$-1\r\n`
/// - [`Frame::Array`][]: `*<count>\r\n<element 1>...<element N>`
/// - [`Frame::NullArray`][]: `*-1\r\n`
///
/// # Examples
///
/// ```rust
/// use codec::{Frame, encode};
/// use bytes::{Bytes, BytesMut};
///
/// let mut buf = BytesMut::new();
///
/// // Encode a Simple String
/// encode(&Frame::SimpleString(Bytes::from_static(b"OK")), &mut buf);
/// assert_eq!(&buf[..], b"+OK\r\n");
/// buf.clear();
///
/// // Encode an Array command: ["SET", "key", "val"]
/// let cmd = Frame::Array(vec![
///     Frame::BulkString(Bytes::from("SET")),
///     Frame::BulkString(Bytes::from("key")),
///     Frame::BulkString(Bytes::from("val")),
/// ]);
/// encode(&cmd, &mut buf);
/// assert_eq!(&buf[..], b"*3\r\n$3\r\nSET\r\n$3\r\nkey\r\n$3\r\nval\r\n");
/// ```
///
/// # Performance
///
/// - **Time Complexity:** O(N), where N is the total serialized byte length plus the number
///   of array elements.
/// - **Buffer Pre-Reservation:** Writes directly into `buf` without intermediate allocations.
///   Invokes [`BytesMut::reserve`] before each segment to leverage amortized buffer growth.
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
