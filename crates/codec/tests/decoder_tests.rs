use bytes::{Bytes, BytesMut};
use codec::{Frame, ParseError, decode};

use codec::constants::{LINE_LIMIT, MAX_ARRAY_ELEMENTS, MAX_BULK_SIZE, MAX_DEPTH};

// Happy Path
#[test]
fn test_decode_empty_buffer_returns_none() {
    let mut buf = BytesMut::new();

    let result = decode(&mut buf);
    let expected = Ok(None);

    assert_eq!(result, expected);
}

#[test]
fn test_decode_simple_string_success() {
    let mut buf = BytesMut::from("+OK\r\n");

    let result = decode(&mut buf).unwrap();
    let expected = Some(Frame::SimpleString(Bytes::from_static(b"OK")));

    assert_eq!(result, expected);
    assert!(buf.is_empty());
}

#[test]
fn test_decode_error_success() {
    let mut buf = BytesMut::from("-ERR unknown command\r\n");

    let result = decode(&mut buf).unwrap();
    let expected = Some(Frame::Error(Bytes::from_static(b"ERR unknown command")));

    assert_eq!(result, expected);
    assert!(buf.is_empty());
}

#[test]
fn test_decode_integer_positive_success() {
    let mut buf = BytesMut::from(":1000\r\n");

    let result = decode(&mut buf).unwrap();
    let expected = Some(Frame::Integer(1000));

    assert_eq!(result, expected);
    assert!(buf.is_empty());
}

#[test]
fn test_decode_integer_negative_success() {
    let mut buf = BytesMut::from(":-42\r\n");

    let result = decode(&mut buf).unwrap();
    let expected = Some(Frame::Integer(-42));

    assert_eq!(result, expected);
    assert!(buf.is_empty());
}

#[test]
fn test_decode_null_bulk_string_success() {
    let mut buf = BytesMut::from("$-1\r\n");

    let result = decode(&mut buf).unwrap();
    let expected = Some(Frame::NullBulkString);

    assert_eq!(result, expected);
    assert!(buf.is_empty());
}

#[test]
fn test_decode_bulk_string_empty_success() {
    let mut buf = BytesMut::from("$0\r\n\r\n");

    let result = decode(&mut buf).unwrap();
    let expected = Some(Frame::BulkString(Bytes::from_static(b"")));

    assert_eq!(result, expected);
    assert!(buf.is_empty());
}

#[test]
fn test_decode_bulk_string_success() {
    let mut buf = BytesMut::from("$5\r\nhello\r\n");

    let result = decode(&mut buf).unwrap();
    let expected = Some(Frame::BulkString(Bytes::from_static(b"hello")));

    assert_eq!(result, expected);
    assert!(buf.is_empty());
}

#[test]
fn test_decode_bulk_string_binary_data_success() {
    let mut buf = BytesMut::from(&b"$4\r\n\x00\xff\x00\xaa\r\n"[..]);

    let result = decode(&mut buf).unwrap();
    let expected = Some(Frame::BulkString(Bytes::from_static(b"\x00\xff\x00\xaa")));

    assert_eq!(result, expected);
    assert!(buf.is_empty());
}

#[test]
fn test_decode_null_array_success() {
    let mut buf = BytesMut::from("*-1\r\n");

    let result = decode(&mut buf).unwrap();
    let expected = Some(Frame::NullArray);

    assert_eq!(result, expected);
    assert!(buf.is_empty());
}

#[test]
fn test_decode_array_empty_success() {
    let mut buf = BytesMut::from("*0\r\n");

    let result = decode(&mut buf).unwrap();
    let expected = Some(Frame::Array(vec![]));

    assert_eq!(result, expected);
    assert!(buf.is_empty());
}

#[test]
fn test_decode_array_simple_success() {
    let mut buf = BytesMut::from("*2\r\n$4\r\nECHO\r\n:100\r\n");

    let result = decode(&mut buf).unwrap();
    let expected = Some(Frame::Array(vec![
        Frame::BulkString(Bytes::from_static(b"ECHO")),
        Frame::Integer(100),
    ]));

    assert_eq!(result, expected);
    assert!(buf.is_empty());
}

#[test]
fn test_decode_array_nested_success() {
    let mut buf = BytesMut::from("*2\r\n$4\r\nPING\r\n*2\r\n:100\r\n*1\r\n+OK\r\n");

    let result = decode(&mut buf).unwrap();
    let expected = Some(Frame::Array(vec![
        Frame::BulkString(Bytes::from_static(b"PING")),
        Frame::Array(vec![
            Frame::Integer(100),
            Frame::Array(vec![Frame::SimpleString(Bytes::from_static(b"OK"))]),
        ]),
    ]));

    assert_eq!(result, expected);
    assert!(buf.is_empty());
}

// Pipelining
#[test]
fn test_decode_pipelined_multiple_frames_success() {
    let mut buf = BytesMut::from("+PING\r\n+PING\r\n");

    let result_first_call = decode(&mut buf).unwrap();
    let result_second_call = decode(&mut buf).unwrap();
    let expected = Some(Frame::SimpleString(Bytes::from_static(b"PING")));

    assert_eq!(result_first_call, expected);
    assert_eq!(result_second_call, expected);
    assert!(buf.is_empty());
}

// Incomplete
#[test]
fn test_decode_incomplete_prefix_only() {
    let mut buf = BytesMut::from("+");

    let result = decode(&mut buf);
    let expected = Ok(None);

    assert_eq!(result, expected);
    assert_eq!(&buf[..], b"+");
}

#[test]
fn test_decode_incomplete_bulk_string_body() {
    let mut buf = BytesMut::from("$5\r\nhel");

    let result = decode(&mut buf);
    let expected = Ok(None);

    assert_eq!(result, expected);
    assert_eq!(&buf[..], b"$5\r\nhel");
}

#[test]
fn test_decode_incomplete_bulk_string_missing_trailing_crlf() {
    let mut buf = BytesMut::from("$5\r\nhello");

    let result = decode(&mut buf);
    let expected = Ok(None);

    assert_eq!(result, expected);
    assert_eq!(&buf[..], b"$5\r\nhello");
}

#[test]
fn test_decode_incomplete_array_elements() {
    let mut buf = BytesMut::from("*2\r\n$3\r\nGET\r\n");

    let result = decode(&mut buf);
    let expected = Ok(None);

    assert_eq!(result, expected);
    assert_eq!(&buf[..], b"*2\r\n$3\r\nGET\r\n");
}

#[test]
fn test_decode_incomplete_simple_type_missing_crlf() {
    let mut buf = BytesMut::from("+OK");

    let result = decode(&mut buf);
    let expected = Ok(None);

    assert_eq!(result, expected);
    assert_eq!(&buf[..], b"+OK");
}

#[test]
fn test_decode_incomplete_bulk_string_length_header() {
    let mut buf = BytesMut::from("$12");

    let result = decode(&mut buf);
    let expected = Ok(None);

    assert_eq!(result, expected);
    assert_eq!(&buf[..], b"$12");
}

#[test]
fn test_decode_incomplete_nested_array_inner_element() {
    let mut buf = BytesMut::from("*1\r\n*1\r\n$4\r\nPI");

    let result = decode(&mut buf);
    let expected = Ok(None);

    assert_eq!(result, expected);
    assert_eq!(&buf[..], b"*1\r\n*1\r\n$4\r\nPI");
}

// Error
#[test]
fn test_decode_invalid_prefix_fails() {
    let mut buf = BytesMut::from("?OK\r\n");

    let result = decode(&mut buf);
    let expected = Err(ParseError::InvalidPrefix(b'?'));

    assert_eq!(result, expected);
    assert_eq!(&buf[..], b"?OK\r\n");
}

#[test]
fn test_decode_malformed_integer_not_digits_fails() {
    let mut buf = BytesMut::from(":abc\r\n");

    let result = decode(&mut buf);
    let expected = Err(ParseError::MalformedInteger);

    assert_eq!(result, expected);
    assert_eq!(&buf[..], b":abc\r\n");
}

#[test]
fn test_decode_integer_overflow_fails() {
    let mut buf = BytesMut::from(":999999999999999999999999999999999999999\r\n");

    let result = decode(&mut buf);
    let expected = Err(ParseError::MalformedInteger);

    assert_eq!(result, expected);
    assert_eq!(&buf[..], b":999999999999999999999999999999999999999\r\n");
}

#[test]
fn test_decode_empty_numeric_field_fails() {
    let mut buf = BytesMut::from(":\r\n");

    let result = decode(&mut buf);
    let expected = Err(ParseError::MalformedInteger);

    assert_eq!(result, expected);
    assert_eq!(&buf[..], b":\r\n");
}

#[test]
fn test_decode_negative_length_less_than_minus_one_fails() {
    let mut buf = BytesMut::from("$-2\r\n");

    let result = decode(&mut buf);
    let expected = Err(ParseError::MalformedInteger);

    assert_eq!(result, expected);
    assert_eq!(&buf[..], b"$-2\r\n");
}

#[test]
fn test_decode_bulk_string_missing_crlf_delimiter_fails() {
    let mut buf = BytesMut::from("$5\r\nhelloXX");

    let result = decode(&mut buf);
    let expected = Err(ParseError::ExpectedCrLf);

    assert_eq!(result, expected);
    assert_eq!(&buf[..], b"$5\r\nhelloXX");
}

#[test]
fn test_decode_line_limit_exceeded_fails() {
    let oversized = LINE_LIMIT + 1;
    let mut raw = Vec::with_capacity(1 + oversized);
    raw.push(b'+');
    raw.resize(1 + oversized, b'A');

    let mut buf = BytesMut::from(&raw[..]);

    let result = decode(&mut buf);
    let expected = Err(ParseError::LineLimitExceeded {
        scanned: oversized,
        limit: LINE_LIMIT,
    });

    assert_eq!(result, expected);
    assert_eq!(&buf[..], &raw[..]);
}

#[test]
fn test_decode_frame_too_large_exceeds_max_bulk_size_fails() {
    let declared = MAX_BULK_SIZE + 1;
    let payload = format!("${declared}\r\n");
    let mut buf = BytesMut::from(payload.as_bytes());

    let result = decode(&mut buf);
    let expected = Err(ParseError::FrameTooLarge {
        declared,
        max: MAX_BULK_SIZE,
    });

    assert_eq!(result, expected);
    assert_eq!(&buf[..], payload.as_bytes());
}

#[test]
fn test_decode_array_elements_exceeds_max_elements_fails() {
    let declared = MAX_ARRAY_ELEMENTS + 1;
    let payload = format!("*{declared}\r\n");
    let mut buf = BytesMut::from(payload.as_bytes());

    let result = decode(&mut buf);
    let expected = Err(ParseError::MaxArrayElementsExceeded {
        declared,
        max: MAX_ARRAY_ELEMENTS,
    });

    assert_eq!(result, expected);
    assert_eq!(&buf[..], payload.as_bytes());
}

#[test]
fn test_decode_nested_depth_exceeded_fails() {
    let depth = MAX_DEPTH + 1;
    let mut raw = Vec::new();
    for _ in 0..depth {
        raw.extend_from_slice(b"*1\r\n");
    }
    raw.extend_from_slice(b"+OK\r\n");

    let mut buf = BytesMut::from(&raw[..]);

    let result = decode(&mut buf);
    let expected = Err(ParseError::NestedDepthExceeded {
        depth,
        max_depth: MAX_DEPTH,
    });

    assert_eq!(result, expected);
    assert_eq!(&buf[..], &raw[..]);
}

#[test]
fn test_decode_array_with_corrupt_child_fails() {
    let mut buf = BytesMut::from("*2\r\n+OK\r\n:not_a_number\r\n");

    let result = decode(&mut buf);
    let expected = Err(ParseError::MalformedInteger);

    assert_eq!(result, expected);
    assert_eq!(&buf[..], b"*2\r\n+OK\r\n:not_a_number\r\n");
}
