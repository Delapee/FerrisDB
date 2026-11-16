use std::{
    sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

use ahash::{AHashMap, RandomState};
use bytes::Bytes;
use indexmap::IndexMap;
use tokio::time::{self, Duration, MissedTickBehavior};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::{entry::Entry, errors::StorageError};

const SHARDS: usize = 256;
const HASHMAP_NODE_OVERHEAD: usize = 32;
const CLEANING_INTERVAL_MS: u64 = 100;
const CLEANING_TIME_BUDGET_MS: u64 = 10;
const CLEANING_CYCLE_LOOKUPS_PER_LOOP: usize = 16;
const CLEANING_ENTRY_LOOKUP_OFFSET: usize = 20;
const CLEANING_ENTRY_LOOKUP_REPEAT: usize = CLEANING_ENTRY_LOOKUP_OFFSET >> 2;

type Result<T> = std::result::Result<T, StorageError>;

pub struct Storage {
    db: Vec<RwLock<Shard>>,
    max_memory: usize,
    used_memory: AtomicUsize,
    hash_builder: RandomState,
}

struct Shard {
    entries: AHashMap<Bytes, Entry>,
    expires: IndexMap<Bytes, Instant, ahash::RandomState>,
}

impl Storage {
    pub fn new(max_memory: usize) -> Self {
        let hash_builder = RandomState::new();
        let mut db = Vec::with_capacity(SHARDS);

        for _ in 0..SHARDS {
            db.push(RwLock::new(Shard {
                entries: AHashMap::with_hasher(hash_builder.clone()),
                expires: IndexMap::with_hasher(hash_builder.clone()),
            }));
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

        if let Some(instant) = expires_at {
            map.expires.insert(key.clone(), instant);
        } else {
            map.expires.swap_remove(&key);
        }

        if let Some(old_value) = map.entries.insert(key, entry) {
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

        let Some(value) = map.entries.get(key) else {
            return Ok(None);
        };

        if value.is_expired() {
            drop(map);
            self.del(key)?;
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

        if let Some(value) = map.entries.remove(key) {
            self.delete_used_memory(key, &value);

            if value.expires_at.is_some() {
                map.expires.swap_remove(key);
            }

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

        let Some(value) = map.entries.get(key) else {
            return Ok(false);
        };

        if value.is_expired() {
            drop(map);
            self.del(key)?;
            Ok(false)
        } else {
            Ok(true)
        }
    }

    fn get_shard_idx(&self, key: &Bytes) -> usize {
        let hash = self.hash_builder.hash_one(key);
        (hash as usize) & (SHARDS - 1)
    }

    fn delete_used_memory(&self, key: &Bytes, value: &Entry) {
        let key_overhead = key.len() + std::mem::size_of::<Bytes>();
        let value_overhead = value.memory_size();
        let previous_size = key_overhead + value_overhead + HASHMAP_NODE_OVERHEAD;

        self.used_memory.fetch_sub(previous_size, Ordering::Relaxed);
    }

    fn purge_expired_keys(&self, cursor: usize) -> (usize, usize) {
        let start_time = Instant::now();

        let mut shards_checked: usize = 0;
        let mut deleted: usize = 0;
        let mut rng: u64 = self.hash_builder.hash_one(start_time);

        'outer: while shards_checked < CLEANING_CYCLE_LOOKUPS_PER_LOOP {
            let shard_idx = (cursor + shards_checked) & (SHARDS - 1);

            let Ok(map) = self.db[shard_idx].read() else {
                shards_checked += 1;
                continue;
            };

            if map.expires.is_empty() {
                drop(map);
                shards_checked += 1;
                continue;
            }

            drop(map);

            loop {
                if start_time.elapsed() >= Duration::from_millis(CLEANING_TIME_BUDGET_MS) {
                    break 'outer;
                }

                let Ok(mut map) = self.db[shard_idx].write() else {
                    break;
                };

                let now = Instant::now();
                let mut expired_entry = 0;
                let is_full_scan = map.expires.len() <= CLEANING_ENTRY_LOOKUP_OFFSET;

                let iter_end = match is_full_scan {
                    true => map.expires.len(),
                    false => CLEANING_ENTRY_LOOKUP_OFFSET,
                };

                for idx in (0..iter_end).rev() {
                    let idx = match is_full_scan {
                        true => idx,
                        false => {
                            rng = xorshift64(rng);
                            (rng as usize) % map.expires.len()
                        }
                    };

                    let Some((_, instant)) = map.expires.get_index(idx) else {
                        continue;
                    };

                    if *instant > now {
                        continue;
                    }

                    let Some((key, _)) = map.expires.swap_remove_index(idx) else {
                        continue;
                    };

                    let Some(value) = map.entries.remove(&key) else {
                        continue;
                    };

                    self.delete_used_memory(&key, &value);
                    expired_entry += 1;
                }

                drop(map);
                deleted += expired_entry;

                if is_full_scan || expired_entry < CLEANING_ENTRY_LOOKUP_REPEAT {
                    break;
                }
            }

            shards_checked += 1;
        }

        fn xorshift64(state: u64) -> u64 {
            let mut x = match state {
                0 => state | 1,
                _ => state,
            };

            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;

            x
        }

        ((cursor + shards_checked) & (SHARDS - 1), deleted)
    }
}

pub fn spawn_storage_cleaner(
    storage: Arc<Storage>,
    tracker: TaskTracker,
    token: CancellationToken,
) {
    let mut interval = time::interval(Duration::from_millis(CLEANING_INTERVAL_MS));
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    tracker.spawn(async move {
        let mut shard_cursor: usize = 0;
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let (new_cursor, deleted) = storage.purge_expired_keys(shard_cursor);
                    shard_cursor = new_cursor;
                    if deleted > 0 {
                        tracing::trace!("Purged {deleted} expired entries from storage");
                    }
                }
                _ = token.cancelled() => {
                    tracing::info!("Shutdown signal received; stopping Storage cleaner thread");
                    break;
                }
            }
        }
    });
}
