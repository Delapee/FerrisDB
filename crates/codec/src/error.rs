//! Error types and diagnostic classifications for RESP parsing and frame decoding.
//!
//! This module defines [`ParseError`], the canonical error enumeration returned by
//! [`decode`](crate::decode) and its supporting validation routines.
//!
//! # Error Classification & Connection Lifecycle
//!
//! In low-latency networked protocols, error handling must strictly bifurcate into two
//! distinct operational categories:
//!
//! 1. **Internal Incomplete Input Signaling ([`ParseError::Incomplete`]):**
//!    Used internally during Phase 1 (`check`) to indicate that the incoming buffer contains
//!    a valid partial frame requiring more bytes. [`decode`](crate::decode) catches this variant
//!    and returns `Ok(None)` while leaving the caller's buffer completely unmodified.
//!    Callers will never receive `Err(ParseError::Incomplete)`.
//!
//! 2. **Fatal Protocol Violations & Security Bounds (All other variants):**
//!    Indicates corrupt framing data, invalid prefix specifiers, integer overflow, or an
//!    explicit breach of configured security boundaries (e.g., [`ParseError::FrameTooLarge`],
//!    [`ParseError::LineLimitExceeded`]). These errors signal either an out-of-spec client or
//!    an active Denial-of-Service attempt. Any [`Err`] returned by [`decode`](crate::decode) is
//!    fatal and requires terminating the connection immediately.

use thiserror::Error;

/// Enumeration of errors that can occur while decoding a RESP frame from a byte buffer.
///
/// Implements standard [`std::error::Error`] via [`thiserror::Error`].
///
/// # Examples
///
/// ```rust
/// use codec::{Frame, ParseError, decode};
/// use bytes::BytesMut;
///
/// // An empty buffer yields Ok(None) (transient incomplete state)
/// let mut buf = BytesMut::new();
/// assert_eq!(decode(&mut buf), Ok(None));
///
/// // A corrupted prefix byte returns an immediate fatal error
/// let mut corrupt = BytesMut::from("?INVALID\r\n");
/// assert_eq!(decode(&mut corrupt), Err(ParseError::InvalidPrefix(b'?')));
/// ```
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    /// Internal signal indicating that the buffer contains a partial frame requiring more bytes.
    ///
    /// This variant is intercepted internally by [`decode`](crate::decode) and converted into
    /// `Ok(None)` while preserving buffer contents. Callers of `decode` will never observe this
    /// variant as an [`Err`].
    #[error("not enough bytes in buffer to parse complete frame")]
    Incomplete,

    /// Encountered a leading byte that does not map to a recognized RESP frame type.
    ///
    /// Valid RESP type markers are:
    /// - `+` for simple strings
    /// - `-` for errors
    /// - `:` for integers
    /// - `$` for bulk strings
    /// - `*` for arrays
    ///
    /// Contains the unrecognized byte value that triggered the error.
    #[error("invalid frame prefix: '{0}' ({0:#04x})")]
    InvalidPrefix(u8),

    /// A numeric payload or length header could not be parsed into a valid integer.
    ///
    /// This error is raised when integer fields contain non-ASCII digit characters, are
    /// empty, represent numbers smaller than `-1` where prohibited (e.g. negative lengths),
    /// or cause integer overflow when parsed into [`i64`] or [`isize`].
    #[error("malformed integer declaration")]
    MalformedInteger,

    /// Expected a CRLF (`\r\n`) delimiter sequence at the expected boundary, but found other bytes.
    ///
    /// Most commonly triggered when a bulk string payload is not properly terminated by `\r\n`,
    /// indicating frame corruption or byte desynchronization.
    #[error("expected CRLF (\\r\\n) delimiter")]
    ExpectedCrLf,

    /// Declared bulk string payload size exceeds the maximum permitted memory limit.
    ///
    /// Enforced against [`MAX_BULK_SIZE`](crate::constants::MAX_BULK_SIZE) to prevent
    /// unbounded memory allocations from malicious or erroneous client requests.
    #[error("frame size {declared} bytes exceeds maximum allowed limit of {max} bytes")]
    FrameTooLarge {
        /// The payload size in bytes claimed by the bulk string length header.
        declared: usize,
        /// The maximum allowed payload size configured by the server.
        max: usize,
    },

    /// An array header declared more elements than the configured safety limit.
    ///
    /// Enforced against [`MAX_ARRAY_ELEMENTS`](crate::constants::MAX_ARRAY_ELEMENTS)
    /// to mitigate memory exhaustion from massive array capacity pre-allocations.
    #[error(
        "Array elements count exceeds limits: declared {declared} elements, max {max} elements"
    )]
    MaxArrayElementsExceeded {
        /// The element count claimed by the array length header.
        declared: usize,
        /// The maximum number of elements permitted in a single array frame.
        max: usize,
    },

    /// Scanning for a CRLF line delimiter reached the safety threshold without finding one.
    ///
    /// Protects against slowloris-style attacks or streaming malformed data without newlines,
    /// bound by [`LINE_LIMIT`](crate::constants::LINE_LIMIT).
    #[error("line scan reached {scanned} bytes without finding CRLF (limit: {limit})")]
    LineLimitExceeded {
        /// Number of contiguous bytes scanned without encountering CRLF.
        scanned: usize,
        /// The configured scan limit threshold.
        limit: usize,
    },

    /// Nested array recursion exceeded the maximum allowable call depth.
    ///
    /// Protects against call-stack overflow vulnerabilities when traversing recursively
    /// nested array structures, bound by [`MAX_DEPTH`](crate::constants::MAX_DEPTH).
    #[error("nested array depth {depth} exceeds recursion limit of {max_depth}")]
    NestedDepthExceeded {
        /// The nesting depth reached during array traversal.
        depth: usize,
        /// The maximum permitted nesting depth.
        max_depth: usize,
    },
}
