//! In-memory data structures representing RESP protocol frames.
//!
//! This module provides [`Frame`], the fundamental enum representing all valid frame
//! types in the REdis Serialization Protocol (RESP2).
//!
//! # Zero-Copy Architecture & Memory Guarantees
//!
//! High-throughput database systems demand minimal memory allocations and cache-friendly
//! representations. To achieve low latency on the hot path:
//!
//! - **Reference-Counted Buffer Slices:** String and binary payloads are encapsulated in
//!   [`bytes::Bytes`]. When frames are decoded from a [`BytesMut`](bytes::BytesMut) network
//!   buffer, data is extracted via [`split_to`](bytes::BytesMut::split_to) and [`freeze`](bytes::BytesMut::freeze).
//!   This produces a reference-counted window pointing directly to the underlying OS socket
//!   read buffer without copying bytes.
//! - **Constant-Time Cloning:** Cloning a [`Frame::BulkString`], [`Frame::SimpleString`], or
//!   [`Frame::Error`] is an O(1) atomic pointer increment, making frame redistribution across
//!   worker threads and actor channels essentially zero-cost.
//! - **Heap Allocations:** Only [`Frame::Array`] allocates a new container ([`Vec`]) on the
//!   heap. Binary and string payloads share the backing socket read buffer via reference-counted
//!   [`Bytes`] handles without copying payload bytes.

use bytes::Bytes;

/// An in-memory representation of a single RESP (REdis Serialization Protocol) frame.
///
/// Each variant directly mirrors a protocol frame type, preserving the semantic distinction
/// between binary data, simple status strings, errors, integers, and composite collections.
///
/// # Examples
///
/// Constructing and inspecting various frames:
///
/// ```rust
/// use codec::Frame;
/// use bytes::Bytes;
///
/// // Construct a simple status string: "+PONG\r\n"
/// let ping_resp = Frame::SimpleString(Bytes::from_static(b"PONG"));
///
/// // Construct a binary-safe bulk string: "$5\r\nhello\r\n"
/// let bulk = Frame::BulkString(Bytes::from("hello"));
///
/// // Construct a null bulk string: "$-1\r\n"
/// let null_bulk = Frame::NullBulkString;
///
/// // Construct a command array: "*2\r\n$3\r\nGET\r\n$4\r\nuser\r\n"
/// let command = Frame::Array(vec![
///     Frame::BulkString(Bytes::from("GET")),
///     Frame::BulkString(Bytes::from("user")),
/// ]);
///
/// match &command {
///     Frame::Array(elements) => assert_eq!(elements.len(), 2),
///     _ => unreachable!(),
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    /// A simple string frame prefixed by `+`.
    ///
    /// Simple strings are non-binary-safe, single-line status responses (e.g. `+OK\r\n` or `+PONG\r\n`).
    /// They cannot contain CR (`\r`) or LF (`\n`) characters within their payload.
    ///
    /// Backed by a zero-copy [`Bytes`] handle into the read buffer.
    SimpleString(Bytes),

    /// An error frame prefixed by `-`.
    ///
    /// Errors are functionally identical to simple strings but convey an error message
    /// from the server (e.g. `-ERR unknown command\r\n` or `-WRONGTYPE ...\r\n`).
    ///
    /// Backed by a zero-copy [`Bytes`] handle into the read buffer.
    Error(Bytes),

    /// A 64-bit signed integer frame prefixed by `:`.
    ///
    /// Represents integers formatted as ASCII digits terminated by CRLF (e.g. `:1000\r\n` or `:-42\r\n`).
    /// Fits entirely in machine registers without heap allocation.
    Integer(i64),

    /// A binary-safe bulk string frame prefixed by `$`.
    ///
    /// Represents arbitrary binary payloads of known length up to
    /// [`MAX_BULK_SIZE`](crate::constants::MAX_BULK_SIZE) (e.g. `$5\r\nhello\r\n`).
    /// Bulk strings are completely binary-safe and can contain embedded null bytes,
    /// carriage returns, or arbitrary serialized binary blobs.
    ///
    /// Payload extraction is strictly zero-copy via [`Bytes`].
    BulkString(Bytes),

    /// A null bulk string frame represented on the wire as `$-1\r\n`.
    ///
    /// Communicates the absence of a value (equivalent to SQL `NULL` or Redis `nil`),
    /// typically returned when querying non-existent keys.
    NullBulkString,

    /// An ordered, heterogeneous array of frames prefixed by `*`.
    ///
    /// Represents composite commands and nested responses (e.g. `*2\r\n$3\r\nfoo\r\n$3\r\nbar\r\n`).
    /// Elements can be of any [`Frame`] variant, including nested arrays up to
    /// [`MAX_DEPTH`](crate::constants::MAX_DEPTH) levels.
    Array(Vec<Frame>),

    /// A null array frame represented on the wire as `*-1\r\n`.
    ///
    /// Communicates the absence of an array collection (e.g. a `BLPOP` timeout), distinct
    /// from an empty array `*0\r\n` (which is represented as `Frame::Array(Vec::new())`).
    NullArray,
}
