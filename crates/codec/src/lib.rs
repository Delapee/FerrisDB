//! REdis Serialization Protocol (RESP2) frame encoder and streaming decoder.
//!
//! `codec` implements non-destructive, zero-copy wire framing between asynchronous
//! network byte streams ([`bytes::BytesMut`]) and in-memory [`Frame`] representations.
//!
//! # Architecture & Guarantees
//!
//! ### 1. Two-Phase Non-Destructive Decoding
//! Partial TCP packet delivery is handled without state machine overhead or buffer churn:
//! - **Phase 1 (`check`):** Speculatively scans a read-only cursor ([`std::io::Cursor`])
//!   to ensure a complete, valid frame is buffered. If incomplete, [`decode`] returns
//!   `Ok(None)` and leaves the caller's [`bytes::BytesMut`] unmodified.
//! - **Phase 2 (`parse`):** Executes only after Phase 1 succeeds. Extracts the frame in
//!   a single pass under validated invariants.
//!
//! Any [`ParseError`] returned by [`decode`] signifies an unrecoverable protocol corruption
//! or DoS boundary breach. The underlying connection must be terminated.
//!
//! ### 2. Zero-Copy Payload Slicing
//! String and binary payloads ([`Frame::BulkString`], [`Frame::SimpleString`], [`Frame::Error`])
//! are sliced directly from the read buffer into shared [`bytes::Bytes`] handles using
//! [`bytes::BytesMut::split_to`]. Payloads avoid heap copies during decode. [`Frame::Array`]
//! allocates only the outer [`Vec<Frame>`] container.
//!
//! ### 3. SIMD-Accelerated Scanning
//! CRLF (`\r\n`) line terminators are scanned using [`memchr::memmem::find`],
//! leveraging CPU SIMD vector extensions (AVX2, SSE4.2, NEON).
//!
//! ### 4. Denial-of-Service (DoS) Limits
//! Framing boundaries are strictly bounded during Phase 1:
//! - [`constants::LINE_LIMIT`] (64 KiB): Halts unbounded scans for line terminators.
//! - [`constants::MAX_BULK_SIZE`] (512 MiB): Rejects oversized payload declarations.
//! - [`constants::MAX_ARRAY_ELEMENTS`] (1024): Prevents memory pre-allocation attacks.
//! - [`constants::MAX_DEPTH`] (128): Prevents call-stack exhaustion on nested arrays.
//!
//! ### 5. Amortized Buffer Pre-Reservation
//! [`encode()`] writes frames directly into a destination [`bytes::BytesMut`] using
//! [`bytes::BytesMut::reserve`] with exact or tight upper-bound bounds, avoiding intermediate
//! reallocations.
//!
//! # Examples
//!
//! ```rust
//! use bytes::{Bytes, BytesMut};
//! use codec::{Frame, decode, encode};
//!
//! let mut stream = BytesMut::new();
//!
//! // 1. Encode an outgoing command: ["SET", "key", "val"]
//! let command = Frame::Array(vec![
//!     Frame::BulkString(Bytes::from_static(b"SET")),
//!     Frame::BulkString(Bytes::from_static(b"key")),
//!     Frame::BulkString(Bytes::from_static(b"val")),
//! ]);
//! encode(&command, &mut stream);
//!
//! // 2. Handle partial frame receipt: returns Ok(None) and preserves buffer
//! let mut incoming = BytesMut::new();
//! incoming.extend_from_slice(b"+P");
//! assert_eq!(decode(&mut incoming), Ok(None));
//! assert_eq!(incoming.len(), 2);
//!
//! // 3. Complete frame and pipeline next frame (:42)
//! incoming.extend_from_slice(b"ONG\r\n:42\r\n");
//!
//! let f1 = decode(&mut incoming).unwrap();
//! assert_eq!(f1, Some(Frame::SimpleString(Bytes::from_static(b"PONG"))));
//!
//! let f2 = decode(&mut incoming).unwrap();
//! assert_eq!(f2, Some(Frame::Integer(42)));
//!
//! assert!(incoming.is_empty());
//! ```

pub mod constants;
pub mod decoder;
pub mod encode;
pub mod error;
pub mod frame;

pub use decoder::decode;
pub use encode::encode;
pub use error::ParseError;
pub use frame::Frame;
