use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("I/O error encountered: {0}")]
    Io(#[from] std::io::Error),

    #[error("Protocol error: {0}")]
    Protocol(#[from] codec::ParseError),

    #[error("Connection limit reached: server at maximum capacity")]
    ConnectionLimitExceeded,

    #[error("Connection closed unexpectedly by client")]
    UnexpectedEof,
}
