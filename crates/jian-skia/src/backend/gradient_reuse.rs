//! Bounded exact-input reuse of immutable Jian gradient shaders.
//! Paint alpha, CTM, blend, geometry and clips remain at the caller.
use jian_core::render::GradientStop;
use skia_safe::{gradient, Point, Shader, TileMode};
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    hash::{Hash, Hasher},
    mem::size_of,
};

const MAX_STOPS: usize = 256;
const MAX_ENTRIES: usize = 256;
const MAX_BYTES: usize = 4 * 1024 * 1024;
struct Entry {
    radial: bool,
    geometry: [u32; 4],
    stops: Vec<[u32; 2]>,
    shader: Shader,
    bytes: usize,
}
#[derive(Default)]
struct Cache {
    entries: HashMap<u64, Entry>,
    order: VecDeque<u64>,
    bytes: usize,
}
fn stop_bits(stop: &GradientStop) -> [u32; 2] {
    let c = stop.color;
    [
        stop.offset.to_bits(),
        (u32::from(c.a()) << 24)
            | (u32::from(c.r()) << 16)
            | (u32::from(c.g()) << 8)
            | u32::from(c.b()),
    ]
}
impl Cache {
    fn charged(&self) -> usize {
        self.bytes
            .saturating_add(size_of::<Self>())
            .saturating_add(
                self.entries
                    .capacity()
                    .saturating_mul(size_of::<(u64, Entry)>() + 16),
            )
            .saturating_add(self.order.capacity().saturating_mul(size_of::<u64>()))
    }
    fn insert(
        &mut self,
        hash: u64,
        radial: bool,
        geometry: [u32; 4],
        stops: &[GradientStop],
        shader: Shader,
    ) {
        let stops: Vec<_> = stops.iter().map(stop_bits).collect();
        // Includes key storage and conservative native-stop/entry overhead;
        // opaque native allocator RSS is not an exact accounting claim.
        let bytes = stops.capacity() * size_of::<[u32; 2]>() + stops.len() * 64 + 512;
        if bytes > MAX_BYTES {
            return;
        }
        if let Some(old) = self.entries.remove(&hash) {
            self.bytes -= old.bytes;
        } else {
            self.order.push_back(hash);
        }
        self.bytes += bytes;
        self.entries.insert(
            hash,
            Entry {
                radial,
                geometry,
                stops,
                shader,
                bytes,
            },
        );
        while self.entries.len() > MAX_ENTRIES || self.charged() > MAX_BYTES {
            let Some(old) = self.order.pop_front() else {
                break;
            };
            if let Some(entry) = self.entries.remove(&old) {
                self.bytes -= entry.bytes;
            }
        }
    }
}
thread_local! { static CACHE: RefCell<Cache> = RefCell::new(Cache::default()); }

pub(super) fn shader(radial: bool, geometry: [f32; 4], stops: &[GradientStop]) -> Option<Shader> {
    if stops.is_empty() {
        return None;
    }
    let bits = geometry.map(f32::to_bits);
    let hash = (stops.len() <= MAX_STOPS).then(|| {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        radial.hash(&mut h);
        bits.hash(&mut h);
        stops.len().hash(&mut h);
        for stop in stops {
            stop_bits(stop).hash(&mut h);
        }
        h.finish()
    });
    if let Some(shader) = hash.and_then(|hash| {
        CACHE.with(|cache| {
            cache
                .borrow()
                .entries
                .get(&hash)
                .filter(|entry| {
                    entry.radial == radial
                        && entry.geometry == bits
                        && entry.stops.len() == stops.len()
                        && entry
                            .stops
                            .iter()
                            .zip(stops)
                            .all(|(a, b)| *a == stop_bits(b))
                })
                .map(|entry| entry.shader.clone())
        })
    }) {
        return Some(shader);
    }
    // Build outside the cache borrow; refused/transient failures are retryable.
    let colors: Vec<_> = stops
        .iter()
        .map(|s| crate::color::to_sk_color(s.color))
        .collect();
    let offsets: Vec<_> = stops.iter().map(|s| s.offset.clamp(0.0, 1.0)).collect();
    let colors = gradient::Colors::new(&colors, Some(offsets.as_slice()), TileMode::Clamp, None);
    let gradient = gradient::Gradient::new(colors, gradient::Interpolation::default());
    let [a, b, c, d] = geometry;
    let shader = if radial {
        gradient::shaders::radial_gradient((Point::new(a, b), c), &gradient, None)
    } else {
        gradient::shaders::linear_gradient((Point::new(a, b), Point::new(c, d)), &gradient, None)
    }?;
    if let Some(hash) = hash {
        CACHE.with(|cache| {
            cache
                .borrow_mut()
                .insert(hash, radial, bits, stops, shader.clone())
        });
    }
    Some(shader)
}
