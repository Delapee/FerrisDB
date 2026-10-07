//! Streaming RESP frame decoding and zero-copy byte parsing.
//!
//! This module implements high-performance, non-destructive frame extraction from
//! asynchronous network read buffers ([`BytesMut`]).
//!
//! # Architecture: Two-Phase Parsing Pipeline
//!
//! Streaming protocols over TCP face byte fragmentation: network packets may deliver
//! partial frames, multiple pipelined frames, or arbitrary byte splits. To handle this
//! efficiently without copying data or maintaining complex partial-state state machines,
//! [`decode`] implements a strict **two-phase parsing pipeline**:
//!
//! 1. **Phase 1: Speculative Non-Destructive Validation (`check`)**
//!    A lightweight, read-only cursor ([`io::Cursor`]) scans the buffer to verify
//!    whether a complete, syntactically valid frame is present.
//!    - **SIMD Delimiter Scanning:** CRLF line terminators are discovered at memory bus
//!      bandwidth using [`memchr::memmem::find`].
//!    - **DoS Boundary Enforcement:** Scans enforce [`LINE_LIMIT`], [`MAX_BULK_SIZE`],
//!      [`MAX_ARRAY_ELEMENTS`], and [`MAX_DEPTH`] during this pass.
//!    - **Atomic Rollback:** If the buffer ends before the frame is complete, the check
//!      pass returns [`ParseError::Incomplete`]. Because Phase 1 is purely speculative and
//!      modifies only the local cursor, the caller's [`BytesMut`] remains 100% pristine.
//!      [`decode`] intercepts this error and returns `Ok(None)`, signaling the I/O loop
//!      to read more bytes.
//!
//! 2. **Phase 2: Zero-Copy Frame Extraction (`parse`)**
//!    Only invoked once Phase 1 confirms the buffer holds an entire valid frame.
//!    - **Zero-Copy Slicing:** Consumes the exact byte lengths verified in Phase 1 via
//!      [`BytesMut::split_to`] and [`BytesMut::advance`].
//!    - Payloads are converted into reference-counted [`Bytes`] handles using
//!      [`BytesMut::freeze`], yielding zero heap copies of the underlying network bytes.
//!    - Because Phase 1 already validated syntax and boundaries, Phase 2 operates under
//!      guaranteed invariants and cannot fail due to unexpected EOF or malformed delimiters.

use std::{io, str::FromStr};

use bytes::{Buf, Bytes, BytesMut};
use memchr::memmem;

use crate::{
    ParseError,
    constants::{CRLF, CRLF_LEN, LINE_LIMIT, MAX_ARRAY_ELEMENTS, MAX_BULK_SIZE, MAX_DEPTH},
    frame::Frame,
};

/// Decodes the next complete [`Frame`] from the provided byte buffer, if available.
///
/// If a complete frame is parsed, the corresponding bytes are consumed from `src` and
/// `Ok(Some(Frame))` is returned. If `src` contains an incomplete frame or is empty,
/// `src` remains unmodified and `Ok(None)` is returned, signaling that the caller must
/// await more data from the network before retrying.
///
/// # Pipelining Support
///
/// Multiple frames packed consecutively into `src` (pipelining) are supported natively.
/// Each invocation of `decode` consumes exactly one frame. If remaining bytes exist in `src`,
/// subsequent calls to `decode` will parse succeeding frames until `src` is exhausted.
///
/// # Examples
///
/// Basic decoding and incomplete buffer handling:
///
/// ```rust
/// use codec::{Frame, decode};
/// use bytes::{Bytes, BytesMut};
///
/// let mut buf = BytesMut::new();
///
/// // Partial frame received: returns Ok(None) without consuming buffer
/// buf.extend_from_slice(b"+OK");
/// assert_eq!(decode(&mut buf), Ok(None));
/// assert_eq!(buf.len(), 3);
///
/// // Remainder of frame arrives: full frame decoded and consumed
/// buf.extend_from_slice(b"\r\n");
/// let frame = decode(&mut buf).unwrap();
/// assert_eq!(frame, Some(Frame::SimpleString(Bytes::from_static(b"OK"))));
/// assert!(buf.is_empty());
/// ```
///
/// Pipelined requests in a single buffer:
///
/// ```rust
/// use codec::{Frame, decode};
/// use bytes::{Bytes, BytesMut};
///
/// let mut buf = BytesMut::from("+PING\r\n:100\r\n");
///
/// let f1 = decode(&mut buf).unwrap();
/// assert_eq!(f1, Some(Frame::SimpleString(Bytes::from_static(b"PING"))));
///
/// let f2 = decode(&mut buf).unwrap();
/// assert_eq!(f2, Some(Frame::Integer(100)));
///
/// assert!(buf.is_empty());
/// ```
///
/// # Errors
///
/// Returns `Err` if `src` contains malformed protocol data or violates security bounds:
/// - [`ParseError::InvalidPrefix`]: Encountered an unrecognized frame type prefix byte.
/// - [`ParseError::MalformedInteger`]: Numeric or length declaration cannot be parsed.
/// - [`ParseError::ExpectedCrLf`]: Trailing `\r\n` delimiter is missing or misplaced.
/// - [`ParseError::FrameTooLarge`]: Bulk string payload declaration exceeds [`MAX_BULK_SIZE`].
/// - [`ParseError::MaxArrayElementsExceeded`]: Array element count exceeds [`MAX_ARRAY_ELEMENTS`].
/// - [`ParseError::LineLimitExceeded`]: CRLF delimiter scan exceeded [`LINE_LIMIT`].
/// - [`ParseError::NestedDepthExceeded`]: Recursive array nesting depth exceeded [`MAX_DEPTH`].
///
/// All [`Err`] returns indicate fatal protocol corruption or DoS threshold violations;
/// the underlying stream is desynchronized and the connection must be closed.
///
/// # Performance
///
/// - **Zero-Copy Slicing:** Bulk and simple strings are sliced directly into [`Bytes`]
///   via [`BytesMut::split_to`] and [`BytesMut::freeze`], avoiding heap copying of payload data.
/// - **SIMD Acceleration:** CRLF delimiter scans utilize [`memchr::memmem::find`].
/// - **Complexity:** O(L) time complexity where L is the byte length of the frame.
pub fn decode(src: &mut BytesMut) -> Result<Option<Frame>, ParseError> {
    if src.is_empty() {
        return Ok(None);
    }

    let mut cursor = io::Cursor::new(&src[..]);
    let mut depth = 0;

    match check(&mut cursor, &mut depth) {
        Ok(()) => {
            let frame = parse(src)?;
            Ok(Some(frame))
        }
        Err(ParseError::Incomplete) => Ok(None),
        Err(err) => Err(err),
    }
}

/// Phase 1: Speculatively validates the syntactic integrity and length bounds of a frame.
///
/// Inspects bytes via a cursor without mutating the source buffer.
/// Tracks recursion depth to prevent stack exhaustion on deeply nested arrays.
fn check(cursor: &mut io::Cursor<&[u8]>, depth: &mut usize) -> Result<(), ParseError> {
    if !cursor.has_remaining() {
        return Err(ParseError::Incomplete);
    }

    let prefix = cursor.get_u8();
    match prefix {
        b'+' | b'-' => check_simple_data_types(cursor),
        b':' => {
            let initial_pos = cursor.position() as usize;
            let remaining = &cursor.get_ref()[initial_pos..];

            if let Some(crlf_offset) = memmem::find(remaining, CRLF) {
                if crlf_offset > LINE_LIMIT {
                    return Err(ParseError::LineLimitExceeded {
                        scanned: crlf_offset,
                        limit: LINE_LIMIT,
                    });
                }

                let _ = parse_to_number::<i64>(&remaining[..crlf_offset])?;

                let new_pos = initial_pos + crlf_offset + CRLF_LEN;
                cursor.set_position(new_pos as u64);

                Ok(())
            } else {
                if remaining.len() > LINE_LIMIT {
                    return Err(ParseError::LineLimitExceeded {
                        scanned: remaining.len(),
                        limit: LINE_LIMIT,
                    });
                }

                Err(ParseError::Incomplete)
            }
        }
        b'$' => {
            let initial_pos = cursor.position() as usize;
            let remaining = &cursor.get_ref()[initial_pos..];

            if let Some(crlf_offset) = memmem::find(remaining, CRLF) {
                if crlf_offset > LINE_LIMIT {
                    return Err(ParseError::LineLimitExceeded {
                        scanned: crlf_offset,
                        limit: LINE_LIMIT,
                    });
                }

                let length = parse_to_number::<isize>(&remaining[..crlf_offset])?;

                if length < -1 {
                    return Err(ParseError::MalformedInteger);
                }

                if length == -1 {
                    let new_pos = initial_pos + crlf_offset + CRLF_LEN;
                    cursor.set_position(new_pos as u64);
                    return Ok(());
                }

                if length as usize > MAX_BULK_SIZE {
                    return Err(ParseError::FrameTooLarge {
                        declared: length as usize,
                        max: MAX_BULK_SIZE,
                    });
                }

                let end = crlf_offset + CRLF_LEN + length as usize;
                let total_bytes = end + CRLF_LEN;

                if remaining.len() < total_bytes {
                    return Err(ParseError::Incomplete);
                }

                if &remaining[end..total_bytes] != CRLF {
                    return Err(ParseError::ExpectedCrLf);
                }

                let new_pos = initial_pos + total_bytes;
                cursor.set_position(new_pos as u64);

                Ok(())
            } else {
                if remaining.len() > LINE_LIMIT {
                    return Err(ParseError::LineLimitExceeded {
                        scanned: remaining.len(),
                        limit: LINE_LIMIT,
                    });
                }

                Err(ParseError::Incomplete)
            }
        }
        b'*' => {
            *depth += 1;

            if *depth > MAX_DEPTH {
                return Err(ParseError::NestedDepthExceeded {
                    depth: *depth,
                    max_depth: MAX_DEPTH,
                });
            }

            let initial_pos = cursor.position() as usize;
            let remaining = &cursor.get_ref()[initial_pos..];

            if let Some(crlf_offset) = memmem::find(remaining, CRLF) {
                if crlf_offset > LINE_LIMIT {
                    return Err(ParseError::LineLimitExceeded {
                        scanned: crlf_offset,
                        limit: LINE_LIMIT,
                    });
                }

                let length = parse_to_number::<isize>(&remaining[..crlf_offset])?;

                if length < -1 {
                    return Err(ParseError::MalformedInteger);
                }

                if length == -1 || length == 0 {
                    *depth -= 1;

                    let new_pos = initial_pos + crlf_offset + CRLF_LEN;
                    cursor.set_position(new_pos as u64);

                    return Ok(());
                }

                if length as usize > MAX_ARRAY_ELEMENTS {
                    return Err(ParseError::MaxArrayElementsExceeded {
                        declared: length as usize,
                        max: MAX_ARRAY_ELEMENTS,
                    });
                }

                let new_pos = initial_pos + crlf_offset + CRLF_LEN;
                cursor.set_position(new_pos as u64);

                for _ in 0..length {
                    check(cursor, depth)?;
                }

                *depth -= 1;

                Ok(())
            } else {
                if remaining.len() > LINE_LIMIT {
                    return Err(ParseError::LineLimitExceeded {
                        scanned: remaining.len(),
                        limit: LINE_LIMIT,
                    });
                }

                Err(ParseError::Incomplete)
            }
        }
        _ => Err(ParseError::InvalidPrefix(prefix)),
    }
}

/// Phase 2: Consumes and constructs a [`Frame`] from the validated buffer.
///
/// Precondition: `check` has already returned `Ok(())` for this frame.
fn parse(src: &mut BytesMut) -> Result<Frame, ParseError> {
    match src[0] {
        b'+' => Ok(Frame::SimpleString(get_payload(src))),
        b'-' => Ok(Frame::Error(get_payload(src))),
        b':' => {
            let Some(crlf_offset) = memmem::find(src, CRLF) else {
                unreachable!("check already validated CRLF");
            };

            let value = parse_to_number::<i64>(&src[1..crlf_offset])?;

            src.advance(crlf_offset + CRLF_LEN);

            Ok(Frame::Integer(value))
        }
        b'$' => {
            let Some(crlf_offset) = memmem::find(src, CRLF) else {
                unreachable!("check already validated CRLF");
            };

            let length = parse_to_number::<isize>(&src[1..crlf_offset])?;

            if length == -1 {
                src.advance(crlf_offset + CRLF_LEN);
                return Ok(Frame::NullBulkString);
            }

            src.advance(crlf_offset + CRLF_LEN);
            let payload = src.split_to(length as usize).freeze();
            src.advance(CRLF_LEN);

            Ok(Frame::BulkString(payload))
        }
        b'*' => {
            let Some(crlf_offset) = memmem::find(src, CRLF) else {
                unreachable!("check already validated CRLF");
            };

            let length = parse_to_number::<isize>(&src[1..crlf_offset])?;

            if length == -1 {
                src.advance(crlf_offset + CRLF_LEN);
                return Ok(Frame::NullArray);
            }

            if length == 0 {
                src.advance(crlf_offset + CRLF_LEN);
                return Ok(Frame::Array(Vec::with_capacity(0)));
            }

            src.advance(crlf_offset + CRLF_LEN);
            let mut frames = Vec::with_capacity(length as usize);

            for _ in 0..length {
                let frame = parse(src)?;
                frames.push(frame);
            }

            Ok(Frame::Array(frames))
        }
        _ => unreachable!(),
    }
}

/// Checks the framing of simple strings (`+`) and errors (`-`).
fn check_simple_data_types(cursor: &mut io::Cursor<&[u8]>) -> Result<(), ParseError> {
    let initial_pos = cursor.position() as usize;
    let remaining = &cursor.get_ref()[initial_pos..];

    if let Some(crlf_offset) = memmem::find(remaining, CRLF) {
        if crlf_offset > LINE_LIMIT {
            return Err(ParseError::LineLimitExceeded {
                scanned: crlf_offset,
                limit: LINE_LIMIT,
            });
        }

        let new_pos = initial_pos + crlf_offset + CRLF_LEN;
        cursor.set_position(new_pos as u64);

        Ok(())
    } else {
        if remaining.len() > LINE_LIMIT {
            return Err(ParseError::LineLimitExceeded {
                scanned: remaining.len(),
                limit: LINE_LIMIT,
            });
        }

        Err(ParseError::Incomplete)
    }
}

/// Extracts a simple string or error payload without heap copying.
///
/// Slices and splits the payload from `src` using [`BytesMut::split_to`].
fn get_payload(src: &mut BytesMut) -> Bytes {
    let Some(crlf_offset) = memmem::find(src, CRLF) else {
        unreachable!("check already validated CRLF");
    };

    let total_bytes = crlf_offset + CRLF_LEN;
    let chain = src.split_to(total_bytes);

    chain.freeze().slice(1..crlf_offset)
}

/// Parses an ASCII decimal byte slice into a target integer type.
fn parse_to_number<T: FromStr>(number: &[u8]) -> Result<T, ParseError> {
    str::from_utf8(number)
        .map_err(|_| ParseError::MalformedInteger)?
        .parse::<T>()
        .map_err(|_| ParseError::MalformedInteger)
}
