use std::{io, str::FromStr};

use bytes::{Buf, Bytes, BytesMut};

use crate::{
    ParseError,
    constants::{CRLF, CRLF_LEN, LINE_LIMIT, MAX_ARRAY_ELEMENTS, MAX_BULK_SIZE, MAX_DEPTH},
    frame::Frame,
};

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

            if let Some(crlf_offset) = remaining.windows(CRLF_LEN).position(|win| win == CRLF) {
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

            if let Some(crlf_offset) = remaining.windows(CRLF_LEN).position(|win| win == CRLF) {
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

            if let Some(crlf_offset) = remaining.windows(CRLF_LEN).position(|win| win == CRLF) {
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

fn parse(src: &mut BytesMut) -> Result<Frame, ParseError> {
    match src[0] {
        b'+' => Ok(Frame::SimpleString(get_payload(src))),
        b'-' => Ok(Frame::Error(get_payload(src))),
        b':' => {
            let Some(crlf_offset) = src.windows(CRLF_LEN).position(|win| win == CRLF) else {
                unreachable!("check already validated CRLF");
            };

            let value = parse_to_number::<i64>(&src[1..crlf_offset])?;

            src.advance(crlf_offset + CRLF_LEN);

            Ok(Frame::Integer(value))
        }
        b'$' => {
            let Some(crlf_offset) = src.windows(CRLF_LEN).position(|win| win == CRLF) else {
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
            let Some(crlf_offset) = src.windows(CRLF_LEN).position(|win| win == CRLF) else {
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

fn check_simple_data_types(cursor: &mut io::Cursor<&[u8]>) -> Result<(), ParseError> {
    let initial_pos = cursor.position() as usize;
    let remaining = &cursor.get_ref()[initial_pos..];

    if let Some(crlf_offset) = remaining.windows(CRLF_LEN).position(|win| win == CRLF) {
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

fn get_payload(src: &mut BytesMut) -> Bytes {
    let Some(crlf_offset) = src.windows(CRLF_LEN).position(|win| win == CRLF) else {
        unreachable!("check already validated CRLF");
    };

    let total_bytes = crlf_offset + CRLF_LEN;
    let chain = src.split_to(total_bytes);

    chain.freeze().slice(1..crlf_offset)
}

fn parse_to_number<T: FromStr>(number: &[u8]) -> Result<T, ParseError> {
    str::from_utf8(number)
        .map_err(|_| ParseError::MalformedInteger)?
        .parse::<T>()
        .map_err(|_| ParseError::MalformedInteger)
}
