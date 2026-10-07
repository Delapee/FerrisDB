//! Protocol constraints, limits, and wire-format delimiters for the RESP codec.
//!
//! This module defines the architectural boundaries and safety thresholds enforced by
//! FerrisDB during RESP (REdis Serialization Protocol) frame decoding and encoding.
//! These constants serve as defense-in-depth safeguards against Denial of Service (DoS),
//! stack exhaustion, and memory exhaustion attacks from untrusted network peers.

/// Maximum number of bytes scanned when searching for a CRLF line terminator (64 KiB).
///
/// Under the RESP protocol, inline commands, simple strings, integers, errors, and bulk string
/// length headers are terminated by a [`CRLF`] (`\r\n`) sequence. Without an explicit threshold,
/// an adversarial client could stream an unbounded sequence of non-CRLF bytes, causing unbounded
/// buffer accumulation and linear search overhead.
///
/// # Invariants & Guarantees
/// If a line scan exceeds `LINE_LIMIT` bytes without encountering `\r\n`, the decoder terminates
/// the parse attempt immediately with [`ParseError::LineLimitExceeded`](crate::ParseError::LineLimitExceeded).
pub const LINE_LIMIT: usize = 64 * 1024;

/// Maximum payload size permitted for a single bulk string frame (512 MiB).
///
/// This threshold adheres to the canonical Redis protocol specification limit for bulk strings.
/// Any frame declaring a length exceeding this boundary is rejected during the preliminary
/// validation phase before any payload allocation or buffering takes place.
///
/// # Invariants & Guarantees
/// If a bulk string header specifies a byte count greater than `MAX_BULK_SIZE`, decoding fails
/// immediately with [`ParseError::FrameTooLarge`](crate::ParseError::FrameTooLarge).
pub const MAX_BULK_SIZE: usize = 512 * 1024 * 1024;

/// Maximum recursion depth allowed for nested RESP arrays (128 levels).
///
/// RESP arrays can recursively contain other arrays. To prevent call stack exhaustion
/// during recursive traversal, the nesting level is strictly tracked and bounded during
/// the Phase 1 (`check`) validation pass. Phase 2 inherits this safety bound by construction.
///
/// # Invariants & Guarantees
/// If array nesting depth exceeds `MAX_DEPTH`, parsing fails immediately with
/// [`ParseError::NestedDepthExceeded`](crate::ParseError::NestedDepthExceeded).
pub const MAX_DEPTH: usize = 128;

/// Maximum number of elements permitted in a single RESP array frame (1024 elements).
///
/// This limit prevents memory exhaustion attacks where a malicious client declares an
/// astronomical array length (e.g. `*2147483647\r\n`), causing the server to pre-allocate
/// excessive vector capacity before elements are received.
///
/// # Invariants & Guarantees
/// If an array header declares more than `MAX_ARRAY_ELEMENTS`, decoding fails
/// immediately with [`ParseError::MaxArrayElementsExceeded`](crate::ParseError::MaxArrayElementsExceeded).
pub const MAX_ARRAY_ELEMENTS: usize = 1024;

/// Canonical two-byte line delimiter (`\r\n`) mandated by the RESP protocol.
///
/// Terminating delimiter for all RESP headers, simple strings, integers, errors, and bulk payloads.
pub const CRLF: &[u8] = b"\r\n";

/// Length in bytes of the [`CRLF`] delimiter sequence (`2`).
///
/// Pre-computed constant used in cursor arithmetic, buffer reservation, and slicing operations.
pub const CRLF_LEN: usize = CRLF.len();
