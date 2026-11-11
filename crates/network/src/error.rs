use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("I/O error encountered: {0}")]
    Io(#[from] std::io::Error),

    #[error("Protocol error: {0}")]
    Protocol(#[from] codec::ParseError),

    #[error("Command error: {0}")]
    Command(#[from] storage::CommandError),

    #[error("Connection closed unexpectedly by client")]
    UnexpectedEof,
}
