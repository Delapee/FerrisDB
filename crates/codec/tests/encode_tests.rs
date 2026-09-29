use bytes::{Bytes, BytesMut};
use codec::{Frame, encode};

#[test]
fn test_encode_simple_string() {
    let mut buf = BytesMut::new();
    encode(&Frame::SimpleString(Bytes::from("Ok")), &mut buf);

    assert_eq!(buf, "+Ok\r\n");
}

#[test]
fn test_encode_error() {
    let mut buf = BytesMut::new();
    encode(&Frame::Error(Bytes::from("Error message")), &mut buf);

    assert_eq!(buf, "-Error message\r\n");
}

#[test]
fn test_encode_integer_positive() {
    let mut buf = BytesMut::new();
    encode(&Frame::Integer(i64::MAX), &mut buf);

    assert_eq!(buf, format!(":{}\r\n", i64::MAX));
}

#[test]
fn test_encode_integer_negative() {
    let mut buf = BytesMut::new();
    encode(&Frame::Integer(i64::MIN), &mut buf);

    assert_eq!(buf, format!(":{}\r\n", i64::MIN));
}

#[test]
fn test_encode_bulk_string() {
    let mut buf = BytesMut::new();
    encode(&Frame::BulkString(Bytes::from("hello")), &mut buf);

    assert_eq!(buf, "$5\r\nhello\r\n");
}

#[test]
fn test_encode_bulk_string_empty() {
    let mut buf = BytesMut::new();
    encode(&Frame::BulkString(Bytes::new()), &mut buf);

    assert_eq!(buf, "$0\r\n\r\n");
}

#[test]
fn test_encode_bulk_string_null() {
    let mut buf = BytesMut::new();
    encode(&Frame::NullBulkString, &mut buf);

    assert_eq!(buf, "$-1\r\n");
}

#[test]
fn test_encode_array_simple() {
    let mut buf = BytesMut::new();
    let frame = Frame::Array(vec![
        Frame::Integer(1),
        Frame::BulkString(Bytes::from("world")),
        Frame::SimpleString(Bytes::from("OK")),
    ]);

    encode(&frame, &mut buf);

    assert_eq!(buf, "*3\r\n:1\r\n$5\r\nworld\r\n+OK\r\n");
}

#[test]
fn test_encode_array_nested() {
    let mut buf = BytesMut::new();
    let frame = Frame::Array(vec![
        Frame::Array(vec![
            Frame::Integer(1),
            Frame::Integer(2),
            Frame::Integer(3),
        ]),
        Frame::Array(vec![
            Frame::SimpleString(Bytes::from_static(b"Hello")),
            Frame::Error(Bytes::from_static(b"World")),
        ]),
    ]);

    encode(&frame, &mut buf);

    assert_eq!(
        buf,
        "*2\r\n*3\r\n:1\r\n:2\r\n:3\r\n*2\r\n+Hello\r\n-World\r\n"
    );
}

#[test]
fn test_encode_array_empty() {
    let mut buf = BytesMut::new();
    encode(&Frame::Array(vec![]), &mut buf);

    assert_eq!(buf, "*0\r\n");
}

#[test]
fn test_encode_array_null() {
    let mut buf = BytesMut::new();
    encode(&Frame::NullArray, &mut buf);

    assert_eq!(buf, "*-1\r\n");
}
