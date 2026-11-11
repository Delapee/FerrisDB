use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum CommandError {
    #[error("Protocol error: expected a valid RESP Array or String format for the command")]
    InvalidFormat,

    #[error("Unknown command: '{0}'")]
    UnknownCommand(String),

    #[error("Wrong number of arguments for '{0}' command, expected {1}")]
    WrongArity(&'static str, &'static str),
}
