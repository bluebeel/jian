//! Parse and normalize a requested family stack once, then share its exact
//! normalized character-cache key across every glyph. Typeface selection and
//! the lifetime of existing character/weight cache entries stay unchanged.

use super::{font_family_candidates, FontResolver, ResolvedTypeface};
use skia_safe::Typeface;
use std::{collections::HashMap, sync::Arc};

const MAX_REQUESTS: usize = 256;
const MAX_REQUEST_BYTES: usize = 1024 * 1024;

pub(super) struct Families {
    pub(super) raw: Vec<String>,
    pub(super) folded: Arc<[String]>,
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub(super) struct FontKey {
    // Arc hashes/compares the complete string slice, never pointer identity.
    pub(super) families: Arc<[String]>,
    pub(super) codepoint: i32,
    pub(super) weight: u16,
    pub(super) italic: bool,
}

#[derive(Default)]
pub(super) struct FontResolverCache {
    pub(super) chars: HashMap<FontKey, Option<ResolvedTypeface>>,
    pub(super) weights: HashMap<(u32, u16), Typeface>,
    requests: HashMap<String, Arc<Families>>,
    shaping: HashMap<String, Arc<[String]>>,
    request_bytes: usize,
}

impl FontResolverCache {
    fn reserve_request(&mut self, bytes: usize) {
        if self.requests.len() + self.shaping.len() >= MAX_REQUESTS
            || self.request_bytes.saturating_add(bytes) > MAX_REQUEST_BYTES
        {
            self.requests.clear();
            self.shaping.clear();
            self.request_bytes = 0;
        }
        self.request_bytes += bytes;
    }
}

impl FontResolver {
    pub(super) fn prepared_families(&self, family: Option<&str>) -> Arc<Families> {
        let key = family.unwrap_or_default();
        if let Some(value) = self.cache.borrow().requests.get(key) {
            return Arc::clone(value);
        }
        let raw = font_family_candidates(family);
        let folded = raw
            .iter()
            .map(|name| name.to_ascii_lowercase())
            .collect::<Vec<_>>();
        let bytes = key
            .len()
            .saturating_add(std::mem::size_of::<(String, Arc<Families>)>())
            .saturating_add(std::mem::size_of::<Families>() + 4 * std::mem::size_of::<usize>())
            .saturating_add(raw.capacity().saturating_mul(std::mem::size_of::<String>()))
            .saturating_add(folded.len().saturating_mul(std::mem::size_of::<String>()))
            .saturating_add(
                raw.iter()
                    .chain(&folded)
                    .map(String::capacity)
                    .sum::<usize>(),
            );
        let value = Arc::new(Families {
            raw,
            folded: folded.into(),
        });
        if bytes <= MAX_REQUEST_BYTES {
            let mut cache = self.cache.borrow_mut();
            cache.reserve_request(bytes);
            cache.requests.insert(key.to_owned(), Arc::clone(&value));
        }
        value
    }

    /// The same ordered canonical names as font_families_for_shaping, shared
    /// until the resolver observes a new registered-font generation.
    pub fn font_families_for_shaping_shared(&self, family: Option<&str>) -> Arc<[String]> {
        self.refresh_if_stale();
        let key = family.unwrap_or_default();
        if let Some(value) = self.cache.borrow().shaping.get(key) {
            return Arc::clone(value);
        }
        let names = self.font_families_for_shaping(family);
        let bytes = key
            .len()
            .saturating_add(
                std::mem::size_of::<(String, Arc<[String]>)>() + 2 * std::mem::size_of::<usize>(),
            )
            .saturating_add(names.len().saturating_mul(std::mem::size_of::<String>()))
            .saturating_add(names.iter().map(String::capacity).sum::<usize>());
        let value: Arc<[String]> = names.into();
        if bytes <= MAX_REQUEST_BYTES {
            let mut cache = self.cache.borrow_mut();
            cache.reserve_request(bytes);
            cache.shaping.insert(key.to_owned(), Arc::clone(&value));
        }
        value
    }
}
