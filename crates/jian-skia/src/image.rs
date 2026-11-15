//! Exact bounded image reuse. Immutable Arc source owners supply the hot-byte
//! lookup proof; digests only accelerate complete content equality on misses.
//! Oversized valid images draw through a one-call slot instead of disappearing.
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use jian_core::render::ImageSource;
use skia_safe::Image as SkImage;
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::Arc,
};

pub(crate) const DEFAULT_BYTE_CAP: usize = 128 * 1024 * 1024;
const MAX_ENTRIES: usize = 1024;
const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;
#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct Key {
    kind: u8,
    hash: u64,
    len: usize,
}
struct CacheEntry {
    source: ImageSource,
    image: Option<SkImage>,
    bytes: usize,
    source_bytes: usize,
    last_used: u64,
}
pub(crate) struct ImageCache {
    decoded: HashMap<Key, CacheEntry>,
    // Only the exact retained strong owner's address is indexed. Removing a
    // source removes this entry before its allocation can be recycled.
    pointers: HashMap<usize, Key>,
    byte_total: usize,
    source_total: usize,
    byte_cap: usize,
    tick: u64,
    transient: Option<SkImage>,
}
impl Default for ImageCache {
    fn default() -> Self {
        Self::with_byte_cap(DEFAULT_BYTE_CAP)
    }
}
impl ImageCache {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_byte_cap(cap: usize) -> Self {
        Self {
            decoded: HashMap::new(),
            pointers: HashMap::new(),
            byte_total: 0,
            source_total: 0,
            byte_cap: cap,
            tick: 0,
            transient: None,
        }
    }
    pub fn get_or_decode(&mut self, source: &ImageSource) -> Option<&SkImage> {
        self.transient = None;
        let key = self.key(source);
        let tick = self.next_tick();
        if self
            .decoded
            .get(&key)
            .is_some_and(|entry| same_source(&entry.source, source))
        {
            let entry = self.decoded.get_mut(&key)?;
            entry.last_used = tick;
            // Arc identity avoids byte work on the common repeated owner. A
            // content-equal new owner replaces just this tiny ownership proof.
            if let (ImageSource::Bytes(old), ImageSource::Bytes(new)) = (&mut entry.source, source)
            {
                if !Arc::ptr_eq(old, new) {
                    self.pointers.remove(&(Arc::as_ptr(old) as usize));
                    self.source_total = self.source_total.saturating_sub(entry.source_bytes);
                    *old = Arc::clone(new);
                    entry.source_bytes = source_storage(source);
                    self.source_total = self.source_total.saturating_add(entry.source_bytes);
                    self.pointers.insert(Arc::as_ptr(new) as usize, key);
                }
            }
            // Larger capacity on an equivalent input does not need retention;
            // preserve the cache bounds while returning its immutable pixels.
            if self.source_total > MAX_SOURCE_BYTES {
                self.transient = self.remove(key).and_then(|entry| entry.image);
                return self.transient.as_ref();
            }
            return self.decoded.get(&key)?.image.as_ref();
        }
        // Digest collisions never authorize a different source or failure.
        self.remove(key);
        let image = decode(source);
        let bytes = estimate_bytes(&image);
        let source_bytes = source_storage(source);
        if bytes > self.byte_cap || source_bytes > MAX_SOURCE_BYTES {
            self.transient = image;
            return self.transient.as_ref();
        }
        self.byte_total = self.byte_total.saturating_add(bytes);
        self.source_total = self.source_total.saturating_add(source_bytes);
        self.decoded.insert(
            key,
            CacheEntry {
                source: source.clone(),
                image,
                bytes,
                source_bytes,
                last_used: tick,
            },
        );
        if let ImageSource::Bytes(owner) = source {
            self.pointers.insert(Arc::as_ptr(owner) as usize, key);
        }
        self.evict_if_over_budget(key);
        self.decoded.get(&key)?.image.as_ref()
    }
    fn key(&self, source: &ImageSource) -> Key {
        if let ImageSource::Bytes(owner) = source {
            if let Some(key) = self.pointers.get(&(Arc::as_ptr(owner) as usize)) {
                if self.decoded.get(key).is_some_and(|entry|matches!(&entry.source,ImageSource::Bytes(retained) if Arc::ptr_eq(retained,owner))) {
                    return *key;
                }
            }
        }
        let (kind, bytes) = match source {
            ImageSource::DataUrl(s) => (0, s.as_bytes()),
            ImageSource::Bytes(s) => (1, s.as_slice()),
            ImageSource::Url(s) => (2, s.as_bytes()),
        };
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hash);
        Key {
            kind,
            hash: hash.finish(),
            len: bytes.len(),
        }
    }
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.decoded.len()
    }
    #[cfg(test)]
    pub fn byte_total(&self) -> usize {
        self.byte_total
    }
    fn next_tick(&mut self) -> u64 {
        if self.tick == u64::MAX {
            // Preserve relative recency rather than letting wraparound turn a
            // new use into the oldest retained image.
            let mut order: Vec<_> = self
                .decoded
                .iter()
                .map(|(key, e)| (*key, e.last_used))
                .collect();
            order.sort_by_key(|(_, tick)| *tick);
            for (rank, (key, _)) in order.into_iter().enumerate() {
                self.decoded.get_mut(&key).unwrap().last_used = rank as u64;
            }
            self.tick = self.decoded.len() as u64;
        }
        self.tick += 1;
        self.tick
    }
    fn remove(&mut self, key: Key) -> Option<CacheEntry> {
        let entry = self.decoded.remove(&key)?;
        self.byte_total = self.byte_total.saturating_sub(entry.bytes);
        self.source_total = self.source_total.saturating_sub(entry.source_bytes);
        if let ImageSource::Bytes(owner) = &entry.source {
            self.pointers.remove(&(Arc::as_ptr(owner) as usize));
        }
        Some(entry)
    }
    fn evict_if_over_budget(&mut self, protect: Key) {
        while self.byte_total > self.byte_cap
            || self.source_total > MAX_SOURCE_BYTES
            || self.decoded.len() > MAX_ENTRIES
        {
            let victim = self
                .decoded
                .iter()
                .filter(|(key, _)| **key != protect)
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| *key);
            let Some(victim) = victim else {
                break;
            };
            self.remove(victim);
        }
    }
}
fn same_source(a: &ImageSource, b: &ImageSource) -> bool {
    match (a, b) {
        (ImageSource::Bytes(a), ImageSource::Bytes(b)) => {
            Arc::ptr_eq(a, b) || a.as_slice() == b.as_slice()
        }
        (ImageSource::DataUrl(a), ImageSource::DataUrl(b))
        | (ImageSource::Url(a), ImageSource::Url(b)) => a == b,
        _ => false,
    }
}
fn source_storage(source: &ImageSource) -> usize {
    let overhead = std::mem::size_of::<CacheEntry>() + 128;
    overhead.saturating_add(match source {
        // Charge the retained original plus the native decoder's encoded copy.
        ImageSource::Bytes(b) => b.capacity().saturating_add(b.len()).saturating_add(32),
        ImageSource::DataUrl(s) => s.capacity().saturating_add(s.len()),
        ImageSource::Url(s) => s.capacity(),
    })
}
fn estimate_bytes(image: &Option<SkImage>) -> usize {
    image
        .as_ref()
        .map_or(0, |image| image.image_info().compute_min_byte_size())
}

fn decode(source: &ImageSource) -> Option<SkImage> {
    match source {
        ImageSource::DataUrl(s) => {
            let bytes = decode_data_url(s)?;
            decode_bytes(&bytes)
        }
        ImageSource::Bytes(b) => decode_bytes(b.as_slice()),
        ImageSource::Url(_) => None,
    }
}

/// Parse `data:image/...;base64,<payload>`. We accept any media type so
/// PNG / JPG / GIF / WebP all flow through the same path. Non-base64
/// data URLs (rare) are not supported — they're a mis-fit for embedded
/// binary assets.
fn decode_data_url(s: &str) -> Option<Vec<u8>> {
    let rest = s.strip_prefix("data:")?;
    let (header, payload) = rest.split_once(',')?;
    if !header.split(';').any(|p| p == "base64") {
        return None;
    }
    STANDARD.decode(payload.trim()).ok()
}

fn decode_bytes(bytes: &[u8]) -> Option<SkImage> {
    let data = skia_safe::Data::new_copy(bytes);
    SkImage::from_encoded(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Tiny 1x1 transparent PNG — the smallest valid PNG, used as a
    /// happy-path decode test that doesn't need a real fixture file.
    const TINY_PNG_B64: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=";

    #[test]
    fn data_url_decodes_to_skia_image() {
        let url = format!("data:image/png;base64,{}", TINY_PNG_B64);
        let mut cache = ImageCache::new();
        let img = cache.get_or_decode(&ImageSource::DataUrl(url));
        assert!(img.is_some(), "expected the 1x1 PNG to decode");
        let img = img.unwrap();
        assert_eq!(img.width(), 1);
        assert_eq!(img.height(), 1);
    }

    #[test]
    fn cache_returns_same_image_on_second_lookup() {
        let url = format!("data:image/png;base64,{}", TINY_PNG_B64);
        let mut cache = ImageCache::new();
        let first = cache.get_or_decode(&ImageSource::DataUrl(url.clone()));
        assert!(first.is_some());
        let dims = (first.unwrap().width(), first.unwrap().height());
        let second = cache.get_or_decode(&ImageSource::DataUrl(url));
        assert!(second.is_some());
        assert_eq!((second.unwrap().width(), second.unwrap().height()), dims);
    }

    #[test]
    fn url_source_returns_none() {
        let mut cache = ImageCache::new();
        assert!(cache
            .get_or_decode(&ImageSource::Url("https://example.com/x.png".into()))
            .is_none());
    }

    #[test]
    fn malformed_data_url_returns_none() {
        let mut cache = ImageCache::new();
        assert!(cache
            .get_or_decode(&ImageSource::DataUrl("not a data url".into()))
            .is_none());
        assert!(cache
            .get_or_decode(&ImageSource::DataUrl("data:image/png,not-base64".into()))
            .is_none());
    }

    #[test]
    fn bytes_source_decodes() {
        let bytes = STANDARD.decode(TINY_PNG_B64).unwrap();
        let mut cache = ImageCache::new();
        let img = cache.get_or_decode(&ImageSource::Bytes(Arc::new(bytes)));
        assert!(img.is_some());
    }

    #[test]
    fn lru_evicts_oldest_on_overflow() {
        // Two distinct decode-able sources whose cache_keys are stable
        // string literals (DataUrl) so eviction is testable without
        // worrying about Arc-pointer reuse. Each decodes to a 1×1
        // PNG = 4 RGBA bytes; cap = 4 → second insert tips over and
        // evicts the older entry.
        //
        // We synthesise the second payload by using a distinct
        // (still-valid) base64 PNG. `TINY_PNG_B64` is a 1×1 transparent
        // PNG; `TINY_PNG_B64_RED` is a 1×1 red PNG. Both decode fine.
        let mut cache = ImageCache::with_byte_cap(4);
        let src_a = ImageSource::DataUrl(format!("data:image/png;base64,{}", TINY_PNG_B64));
        let src_b = ImageSource::DataUrl(format!("data:image/png;base64,{}", TINY_PNG_B64_RED));
        assert!(cache.get_or_decode(&src_a).is_some());
        assert_eq!(cache.len(), 1);
        // Second insert tips the cap → older entry evicts.
        assert!(cache.get_or_decode(&src_b).is_some());
        assert_eq!(cache.len(), 1, "LRU should have evicted the oldest");
        // Re-insert the first — second now becomes the older entry
        // and gets evicted in turn.
        assert!(cache.get_or_decode(&src_a).is_some());
        assert_eq!(cache.len(), 1);
    }

    /// 1×1 red PNG, distinct from `TINY_PNG_B64`.
    const TINY_PNG_B64_RED: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

    #[test]
    fn under_budget_keeps_all_entries() {
        let mut cache = ImageCache::with_byte_cap(1024 * 1024);
        // Three distinct cache keys via three different Url strings;
        // they all decode to None but each takes its own cache slot.
        // Url decode-failure entries count as 0 bytes, so all three
        // sit comfortably under any cap.
        for n in 0..3 {
            cache.get_or_decode(&ImageSource::Url(format!("u-{}", n)));
        }
        assert_eq!(cache.len(), 3);
        assert!(cache.byte_total() <= 1024 * 1024);
    }
}
