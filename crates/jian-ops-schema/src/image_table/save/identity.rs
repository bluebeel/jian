//! Exact digests reused only while the immutable Arc allocation still matches.
//! Weak allocations are charged at full payload size: even without a strong
//! owner their backing may remain allocated until the Weak is evicted.
use super::{fnv1a64, AllocationKey};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, OnceLock, Weak},
};
const MAX_ENTRIES: usize = 8192;
const MAX_ALLOCATION_BYTES: usize = 128 * 1024 * 1024;
#[derive(Clone, Copy)]
pub(super) struct Digest {
    pub full: u64,
    pub paint: u64,
}
struct Entry {
    payload: Weak<str>,
    digest: Digest,
    serial: u64,
    charge: usize,
}
#[derive(Default)]
struct Cache {
    entries: HashMap<AllocationKey, Entry>,
    order: VecDeque<(AllocationKey, u64)>,
    bytes: usize,
    serial: u64,
}
impl Cache {
    fn get(&self, key: &AllocationKey, payload: &Arc<str>) -> Option<Digest> {
        let entry = self.entries.get(key)?;
        let retained = entry.payload.upgrade()?;
        Arc::ptr_eq(&retained, payload).then_some(entry.digest)
    }
    fn insert(&mut self, key: AllocationKey, payload: &Arc<str>, digest: Digest, charge: usize) {
        if self.serial == u64::MAX {
            self.entries.clear();
            self.order.clear();
            self.bytes = 0;
            self.serial = 0;
        }
        if let Some(old) = self.entries.remove(&key) {
            self.bytes -= old.charge;
        }
        while self.order.len() >= MAX_ENTRIES
            || charge > MAX_ALLOCATION_BYTES.saturating_sub(self.bytes)
        {
            let Some((key, serial)) = self.order.pop_front() else {
                break;
            };
            if self
                .entries
                .get(&key)
                .is_some_and(|entry| entry.serial == serial)
            {
                self.bytes -= self.entries.remove(&key).unwrap().charge;
            }
        }
        self.serial += 1;
        self.entries.insert(
            key,
            Entry {
                payload: Arc::downgrade(payload),
                digest,
                serial: self.serial,
                charge,
            },
        );
        self.order.push_back((key, self.serial));
        self.bytes += charge;
    }
}
pub(super) fn digest(payload: &Arc<str>, key: AllocationKey) -> Digest {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(Cache::default()));
    if let Some(digest) = cache
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&key, payload)
    {
        return digest;
    }
    // No payload-sized hashing runs while the shared metadata lock is held.
    let digest = Digest {
        full: fnv1a64(payload.as_bytes()),
        paint: crate::node::image_src::paint_image_id(payload),
    };
    let charge = payload.len().saturating_add(256);
    if charge <= MAX_ALLOCATION_BYTES {
        let mut cache = cache.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(existing) = cache.get(&key, payload) {
            return existing;
        }
        cache.insert(key, payload, digest, charge);
    }
    digest
}
