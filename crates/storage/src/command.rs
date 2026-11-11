use std::time::{Duration, Instant};

use bytes::Bytes;
use codec::Frame;

use crate::{errors::CommandError, storage::Storage};

pub enum Command {
    Ping(Option<Bytes>),
    Echo(Bytes),
    Set {
        key: Bytes,
        value: Bytes,
        expired_at: Option<Instant>,
    },
    Get {
        key: Bytes,
    },
    Del {
        keys: Vec<Bytes>,
    },
    Exists {
        keys: Vec<Bytes>,
    },
}

impl Command {
    pub fn execute(self, storage: &Storage) -> Frame {
        match self {
            Command::Ping(had_bytes) => match had_bytes {
                Some(bytes) => Frame::BulkString(bytes),
                None => Frame::SimpleString(Bytes::from("PONG")),
            },
            Command::Echo(bytes) => Frame::BulkString(bytes),
            Command::Set {
                key,
                value,
                expired_at,
            } => match storage.set(key, value, expired_at) {
                Ok(_) => Frame::SimpleString(Bytes::from("OK")),
                Err(err) => Frame::Error(Bytes::from(err)),
            },
            Command::Get { key } => match storage.get(&key) {
                Ok(None) => Frame::NullBulkString,
                Ok(Some(value)) => Frame::BulkString(value),
                Err(err) => Frame::Error(Bytes::from(err)),
            },
            Command::Del { keys } => {
                let mut deleted = 0;
                for key in keys {
                    match storage.del(&key) {
                        Ok(false) => continue,
                        Ok(true) => deleted += 1,
                        Err(err) => return Frame::Error(Bytes::from(err)),
                    };
                }
                Frame::Integer(deleted)
            }
            Command::Exists { keys } => {
                let mut exists = 0;
                for key in keys {
                    match storage.exists(&key) {
                        Ok(false) => continue,
                        Ok(true) => exists += 1,
                        Err(err) => return Frame::Error(Bytes::from(err)),
                    };
                }
                Frame::Integer(exists)
            }
        }
    }
}

impl TryFrom<Frame> for Command {
    type Error = CommandError;

    fn try_from(value: Frame) -> Result<Self, Self::Error> {
        let elements = match value {
            Frame::Array(array) => array,
            Frame::SimpleString(_) | Frame::BulkString(_) => vec![value],
            _ => {
                return Err(CommandError::InvalidFormat);
            }
        };

        let [cmd_frame, args @ ..] = &elements[..] else {
            return Err(CommandError::InvalidFormat);
        };
        let cmd = take_bytes(cmd_frame).ok_or(CommandError::InvalidFormat)?;

        if cmd.eq_ignore_ascii_case(b"PING") {
            return match args {
                [] => Ok(Command::Ping(None)),
                [msg_frame] => {
                    let msg = take_bytes(msg_frame).ok_or(CommandError::InvalidFormat)?;
                    Ok(Command::Ping(Some(msg)))
                }
                _ => Err(CommandError::WrongArity("PING", "0 or 1")),
            };
        }

        if cmd.eq_ignore_ascii_case(b"ECHO") {
            return match args {
                [msg_frame] => {
                    let msg = take_bytes(msg_frame).ok_or(CommandError::InvalidFormat)?;
                    Ok(Command::Echo(msg))
                }
                _ => Err(CommandError::WrongArity("ECHO", "1")),
            };
        }

        if cmd.eq_ignore_ascii_case(b"SET") {
            return match args {
                [key_frame, value_frame] => {
                    let key = take_bytes(key_frame).ok_or(CommandError::InvalidFormat)?;
                    let value = take_bytes(value_frame).ok_or(CommandError::InvalidFormat)?;

                    Ok(Command::Set {
                        key,
                        value,
                        expired_at: None,
                    })
                }
                [key_frame, value_frame, time_unit, ttl] => {
                    let key = take_bytes(key_frame).ok_or(CommandError::InvalidFormat)?;
                    let value = take_bytes(value_frame).ok_or(CommandError::InvalidFormat)?;
                    let time_unit = take_bytes(time_unit).ok_or(CommandError::InvalidFormat)?;
                    let ttl = take_bytes(ttl).ok_or(CommandError::InvalidFormat)?;

                    let ttl = str::from_utf8(&ttl)
                        .map_err(|_| CommandError::InvalidFormat)?
                        .parse::<u64>()
                        .map_err(|_| CommandError::InvalidFormat)?;

                    let expired_at = if time_unit.eq_ignore_ascii_case(b"EX") {
                        Instant::now() + Duration::from_secs(ttl)
                    } else if time_unit.eq_ignore_ascii_case(b"PX") {
                        Instant::now() + Duration::from_millis(ttl)
                    } else {
                        return Err(CommandError::InvalidFormat);
                    };

                    Ok(Command::Set {
                        key,
                        value,
                        expired_at: Some(expired_at),
                    })
                }
                _ => Err(CommandError::WrongArity("SET", "2 or 4 if TTL is provided")),
            };
        }

        if cmd.eq_ignore_ascii_case(b"GET") {
            return match args {
                [key_frame] => {
                    let key = take_bytes(key_frame).ok_or(CommandError::InvalidFormat)?;
                    Ok(Command::Get { key })
                }
                _ => Err(CommandError::WrongArity("GET", "1")),
            };
        }

        if cmd.eq_ignore_ascii_case(b"DEL") {
            if args.is_empty() {
                return Err(CommandError::WrongArity("DEL", "at least 1"));
            }

            let keys = take_all_bytes(args)?;
            return Ok(Command::Del { keys });
        }

        if cmd.eq_ignore_ascii_case(b"EXISTS") {
            if args.is_empty() {
                return Err(CommandError::WrongArity("EXISTS", "at least 1"));
            }

            let keys = take_all_bytes(args)?;
            return Ok(Command::Exists { keys });
        }

        Err(CommandError::UnknownCommand(
            String::from_utf8_lossy(&cmd).to_string(),
        ))
    }
}

fn take_bytes(frame: &Frame) -> Option<Bytes> {
    match frame {
        Frame::SimpleString(bytes) => Some(bytes.clone()),
        Frame::BulkString(bytes) => Some(bytes.clone()),
        _ => None,
    }
}

fn take_all_bytes(elements: &[Frame]) -> Result<Vec<Bytes>, CommandError> {
    elements
        .iter()
        .map(|frame| take_bytes(frame).ok_or(CommandError::InvalidFormat))
        .collect()
}
