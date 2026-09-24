use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    /// Buffer does not contain enough bytes yet to complete the frame.
    /// Not fatal: the network loop should read more bytes from the socket.
    #[error("not enough bytes in buffer to parse complete frame")]
    Incomplete,

    /// The prefix byte is not one of: '+', '-', ':', '$', or '*'.
    #[error("invalid frame prefix: '{0}' ({0:#04x})")]
    InvalidPrefix(u8),

    /// Parsing integer lengths or values failed (malformed ASCII digits or integer overflow).
    #[error("malformed integer declaration")]
    MalformedInteger,

    /// Missing or corrupt \r\n line ending.
    #[error("expected CRLF (\\r\\n) delimiter")]
    ExpectedCrLf,

    /// Declared bulk string or frame length exceeds the configured maximum threshold.
    #[error("frame size {declared} bytes exceeds maximum allowed limit of {max} bytes")]
    FrameTooLarge { declared: usize, max: usize },

    #[error(
        "Array elements count exceeds limits: declared {declared} elements, max {max} elements"
    )]
    MaxArrayElementsExceeded { declared: usize, max: usize },

    /// Line search exceeded maximum threshold without finding \r\n (anti-DoS safeguard).
    #[error("line scan reached {scanned} bytes without finding CRLF (limit: {limit})")]
    LineLimitExceeded { scanned: usize, limit: usize },

    /// Nested array recursion exceeded the safe execution depth.
    #[error("nested array depth {depth} exceeds recursion limit of {max_depth}")]
    NestedDepthExceeded { depth: usize, max_depth: usize },
}
