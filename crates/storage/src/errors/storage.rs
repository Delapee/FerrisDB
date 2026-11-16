use std::fmt::Write;

use bytes::{Bytes, BytesMut};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub(crate) enum StorageError {
    #[error("Internal storage error: Shard lock is poisoned")]
    LockPoisoned,

    #[error("OOM command not allowed when used memory > {0}")]
    OutOfMemory(usize),
}

impl From<StorageError> for Bytes {
    fn from(value: StorageError) -> Self {
        if value == StorageError::LockPoisoned {
            return Bytes::from_static(b"Internal server error");
        }

        let mut buf = BytesMut::with_capacity(32);
        let _ = write!(&mut buf, "{}", value);
        buf.freeze()
    }
}
