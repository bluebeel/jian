//! Compiled-SkSL `RuntimeEffect` cache.
//!
//! [`DrawOp::ShaderRect`](jian_core::render::DrawOp) carries RAW SkSL
//! source. The editor repaints every frame, so compiling the program on
//! each draw would be a per-frame `make_for_shader` call — the same
//! mistake the typeface cache in `op-host-native` exists to avoid. This
//! cache reuses a bounded working set of refcounted `RuntimeEffect` handles.
//! Unique shader edits evict the least recently used entry instead of
//! retaining every program for the lifetime of the editor or render worker.
//!
//! Compile *failures* are cached too (as `None`) so an invalid shader
//! doesn't burn the SkSL compiler on every redraw. A `None` entry tells
//! the caller to paint the solid fallback.
//!
//! `RuntimeEffect` is an `RCHandle` (atomic-refcounted) — cloning it to
//! hand into a fresh `RuntimeShaderBuilder` per frame is a cheap
//! refcount bump, NOT a recompile.

use skia_safe::RuntimeEffect;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Hash an SkSL source string to its cache key. Public so render paths
/// in other crates (the native host's own cache) can key on the same
/// value if they share source.
pub fn sksl_hash(sksl: &str) -> u64 {
    let mut h = DefaultHasher::new();
    sksl.hash(&mut h);
    h.finish()
}

const MAX_ENTRIES: usize = 128;
const MAX_SOURCE_BYTES: usize = 2 * 1024 * 1024;

struct Entry {
    source: String,
    effect: Option<RuntimeEffect>,
    last_used: u64,
}

pub struct ShaderCache {
    /// `None` effect = a source that failed to compile; invalid programs share
    /// the same eviction budget so repeated edits cannot grow this indefinitely.
    effects: HashMap<u64, Entry>,
    source_bytes: usize,
    tick: u64,
    /// Count of `make_for_shader` invocations, including recompiled evictions — the
    /// compile-cache proof reads this to confirm compile-once.
    compile_count: u64,
    max_entries: usize,
    max_source_bytes: usize,
}

impl Default for ShaderCache {
    fn default() -> Self {
        Self::with_limits(MAX_ENTRIES, MAX_SOURCE_BYTES)
    }
}

impl ShaderCache {
    pub fn new() -> Self {
        Self::default()
    }

    fn with_limits(max_entries: usize, max_source_bytes: usize) -> Self {
        Self {
            effects: HashMap::new(),
            source_bytes: 0,
            tick: 0,
            compile_count: 0,
            max_entries,
            max_source_bytes,
        }
    }

    /// Return the compiled effect for `sksl`, compiling + caching it on
    /// first sight. Returns `None` (cached) when the source fails to
    /// compile — the caller then paints its solid fallback. Never
    /// panics: `make_for_shader` returns a `Result` and the `Err` arm is
    /// stored as a cached `None`.
    pub fn get_or_compile(&mut self, sksl: &str) -> Option<RuntimeEffect> {
        let key = sksl_hash(sksl);
        self.tick = self.tick.wrapping_add(1);
        if let Some(hit) = self.effects.get_mut(&key) {
            // Hash collisions may replace a resident source, but must never
            // substitute another shader's pixels or cached compile error.
            if hit.source == sksl {
                hit.last_used = self.tick;
                return hit.effect.clone();
            }
        }
        let compiled = RuntimeEffect::make_for_shader(sksl, None).ok();
        self.compile_count = self.compile_count.saturating_add(1);
        if self.max_entries == 0 || sksl.len() > self.max_source_bytes {
            return compiled;
        }
        if let Some(old) = self.effects.remove(&key) {
            self.source_bytes = self.source_bytes.saturating_sub(old.source.len());
        }
        while self.effects.len() >= self.max_entries
            || self.source_bytes.saturating_add(sksl.len()) > self.max_source_bytes
        {
            let Some((&oldest, _)) = self.effects.iter().min_by_key(|(_, e)| e.last_used) else {
                break;
            };
            if let Some(old) = self.effects.remove(&oldest) {
                self.source_bytes = self.source_bytes.saturating_sub(old.source.len());
            }
        }
        self.source_bytes += sksl.len();
        self.effects.insert(
            key,
            Entry {
                source: sksl.to_owned(),
                effect: compiled.clone(),
                last_used: self.tick,
            },
        );
        compiled
    }

    /// Number of compile attempts performed so far. Used by the
    /// render-proof / tests to assert compile-once-per-unique-source.
    pub fn compile_count(&self) -> u64 {
        self.compile_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: &str = "half4 main(float2 p) { return half4(1,0,0,1); }";
    const BLUE: &str = "half4 main(float2 p) { return half4(0,0,1,1); }";
    const GREEN: &str = "half4 main(float2 p) { return half4(0,1,0,1); }";

    #[test]
    fn hot_shader_survives_eviction_and_errors_are_cached() {
        let mut cache = ShaderCache::with_limits(2, 1024);
        assert!(cache.get_or_compile(RED).is_some());
        assert!(cache.get_or_compile(BLUE).is_some());
        assert!(cache.get_or_compile(RED).is_some());
        assert!(cache.get_or_compile(GREEN).is_some());
        assert!(cache.effects.contains_key(&sksl_hash(RED)));
        assert!(!cache.effects.contains_key(&sksl_hash(BLUE)));
        assert_eq!(cache.compile_count(), 3);
        assert!(cache.get_or_compile("invalid shader").is_none());
        let attempts = cache.compile_count();
        assert!(cache.get_or_compile("invalid shader").is_none());
        assert_eq!(cache.compile_count(), attempts);
    }

    #[test]
    fn unique_shader_edits_remain_within_both_limits() {
        let mut cache = ShaderCache::with_limits(3, 100);
        for i in 0..100 {
            let source = format!("// edit {i}\n{RED}");
            assert!(cache.get_or_compile(&source).is_some());
            assert!(cache.effects.len() <= 3);
            assert!(cache.source_bytes <= 100);
            assert_eq!(
                cache.source_bytes,
                cache.effects.values().map(|e| e.source.len()).sum()
            );
        }
        assert_eq!(cache.compile_count(), 100);
        assert!(cache.effects.len() < 3, "the byte limit must also evict");
    }

    #[test]
    fn oversize_shader_still_compiles_without_retaining_source() {
        let mut cache = ShaderCache::with_limits(3, 1);
        assert!(cache.get_or_compile(RED).is_some());
        assert!(cache.effects.is_empty());
        assert_eq!(cache.source_bytes, 0);
    }
}
