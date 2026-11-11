use std::{
    sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

use ahash::{AHashMap, RandomState};
use bytes::Bytes;
use tokio::time::{self, Duration};
use tokio_util::sync::CancellationToken;

use crate::{entry::Entry, errors::StorageError};

const SHARDS: usize = 256;
const HASHMAP_NODE_OVERHEAD: usize = 32;
const CLEANING_INTERVAL_MS: u64 = 100;

type Shard = RwLock<AHashMap<Bytes, Entry>>;
type Result<T> = std::result::Result<T, StorageError>;

pub struct Storage {
    db: Vec<Shard>,
    max_memory: usize,
    used_memory: AtomicUsize,
    hash_builder: RandomState,
}

impl Storage {
    pub fn new(max_memory: usize) -> Self {
        let hash_builder = RandomState::new();
        let mut db = Vec::with_capacity(SHARDS);

        for _ in 0..SHARDS {
            db.push(RwLock::new(AHashMap::with_hasher(hash_builder.clone())));
        }

        Self {
            db,
            max_memory,
            used_memory: AtomicUsize::new(0),
            hash_builder,
        }
    }

    pub(crate) fn set(&self, key: Bytes, value: Bytes, expires_at: Option<Instant>) -> Result<()> {
        let entry = Entry::new(value, expires_at);

        let key_overhead = key.len() + std::mem::size_of::<Bytes>();
        let value_overhead = entry.memory_size();
        let incoming_size = key_overhead + value_overhead + HASHMAP_NODE_OVERHEAD;

        let used_memory = self.used_memory.load(Ordering::Relaxed);
        if used_memory + incoming_size > self.max_memory {
            return Err(StorageError::OutOfMemory(self.max_memory));
        }

        let shard_index = self.get_shard_idx(&key);
        let mut map = self.db[shard_index]
            .write()
            .map_err(|_| StorageError::LockPoisoned)?;

        if let Some(old_value) = map.insert(key, entry) {
            let old_value_overhead = old_value.memory_size();
            let previous_size = key_overhead + old_value_overhead + HASHMAP_NODE_OVERHEAD;

            self.used_memory.fetch_sub(previous_size, Ordering::Relaxed);
        }

        self.used_memory.fetch_add(incoming_size, Ordering::Relaxed);

        Ok(())
    }

    pub(crate) fn get(&self, key: &Bytes) -> Result<Option<Bytes>> {
        let shard_index = self.get_shard_idx(key);
        let map = self.db[shard_index]
            .read()
            .map_err(|_| StorageError::LockPoisoned)?;

        let Some(value) = map.get(key) else {
            return Ok(None);
        };

        if value.is_expired() {
            drop(map);
            let _ = self.del(key)?;
            Ok(None)
        } else {
            Ok(Some(value.data.clone()))
        }
    }

    pub(crate) fn del(&self, key: &Bytes) -> Result<bool> {
        let shard_index = self.get_shard_idx(key);
        let mut map = self.db[shard_index]
            .write()
            .map_err(|_| StorageError::LockPoisoned)?;

        if let Some(value) = map.remove(key) {
            let key_overhead = key.len() + std::mem::size_of::<Bytes>();
            let value_overhead = value.memory_size();
            let previous_size = key_overhead + value_overhead + HASHMAP_NODE_OVERHEAD;

            self.used_memory.fetch_sub(previous_size, Ordering::Relaxed);

            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(crate) fn exists(&self, key: &Bytes) -> Result<bool> {
        let shard_index = self.get_shard_idx(key);
        let map = self.db[shard_index]
            .read()
            .map_err(|_| StorageError::LockPoisoned)?;

        let Some(value) = map.get(key) else {
            return Ok(false);
        };

        if value.is_expired() {
            drop(map);
            let _ = self.del(key)?;
            Ok(false)
        } else {
            Ok(true)
        }
    }

    fn get_shard_idx(&self, key: &Bytes) -> usize {
        let hash = self.hash_builder.hash_one(key);
        (hash as usize) & (SHARDS - 1)
    }

    fn purge_expired_keys(&self) {}
}

pub fn spawn_storage_cleaner(storage: Arc<Storage>, token: CancellationToken) {
    let mut interval = time::interval(Duration::from_millis(CLEANING_INTERVAL_MS));

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    storage.purge_expired_keys();
                }
                _ = token.cancelled() => {
                    tracing::info!("Shutdown signal received; stopping Storage cleaner thread");
                    break;
                }
            }
        }
    });
}
