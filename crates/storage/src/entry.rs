use std::time::Instant;

use bytes::Bytes;

pub(crate) struct Entry {
    pub(crate) data: Bytes,
    pub(crate) expires_at: Option<Instant>,
}

impl Entry {
    pub(crate) fn new(data: Bytes, expires_at: Option<Instant>) -> Self {
        Self { data, expires_at }
    }

    pub(crate) fn memory_size(&self) -> usize {
        self.data.len() + std::mem::size_of::<Self>()
    }

    pub(crate) fn is_expired(&self) -> bool {
        if let Some(ttl) = self.expires_at {
            Instant::now() >= ttl
        } else {
            false
        }
    }
}
