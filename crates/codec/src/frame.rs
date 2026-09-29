use bytes::Bytes;

#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    SimpleString(Bytes),
    Error(Bytes),
    Integer(i64),
    /// Zero-copy binary chunk over the underlying buffer
    BulkString(Bytes),
    /// Null bulk string representation: $-1\r\n
    NullBulkString,
    /// Heterogeneous collection of frames: *<len>\r\n...
    Array(Vec<Frame>),
    /// Null array representation: *-1\r\n
    NullArray,
}
