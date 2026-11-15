//! Exact command-bit reuse of immutable local native paths, never Doc revisions.
use jian_core::render::PathCommand;
use skia_safe::Path;
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    hash::{Hash, Hasher},
    mem::size_of,
};
const MAX_ENTRIES: usize = 256;
const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_COMMANDS: usize = 2048;
const MAX_ENTRY_BYTES: usize = 64 * 1024;
struct Entry {
    commands: Box<[PathCommand]>,
    path: Path,
    bytes: usize,
}
#[derive(Default)]
struct Cache {
    entries: HashMap<u64, Entry>,
    order: VecDeque<u64>,
    bytes: usize,
}
fn bits(command: &PathCommand) -> [u32; 7] {
    let mut bits = [0; 7];
    let (tag, points) = match *command {
        PathCommand::MoveTo(a) => (0, [Some(a), None, None]),
        PathCommand::LineTo(a) => (1, [Some(a), None, None]),
        PathCommand::QuadTo(a, b) => (2, [Some(a), Some(b), None]),
        PathCommand::CubicTo(a, b, c) => (3, [Some(a), Some(b), Some(c)]),
        PathCommand::Close => (4, [None, None, None]),
    };
    bits[0] = tag;
    for (i, point) in points.into_iter().enumerate() {
        if let Some(point) = point {
            bits[i * 2 + 1] = point.x.to_bits();
            bits[i * 2 + 2] = point.y.to_bits();
        }
    }
    bits
}
impl Cache {
    fn charged(&self) -> usize {
        self.bytes
            .saturating_add(size_of::<Self>())
            .saturating_add(self.entries.capacity() * (size_of::<(u64, Entry)>() + 16))
            .saturating_add(self.order.capacity() * size_of::<u64>())
    }
    fn insert(&mut self, hash: u64, commands: &[PathCommand], path: Path) {
        let bytes = commands
            .len()
            .saturating_mul(size_of::<PathCommand>())
            .saturating_add(path.approximate_bytes_used())
            .saturating_add(512);
        if bytes > MAX_ENTRY_BYTES {
            return;
        }
        let commands = commands.into();
        if let Some(old) = self.entries.remove(&hash) {
            self.bytes -= old.bytes;
        } else {
            self.order.push_back(hash);
        }
        self.bytes += bytes;
        self.entries.insert(
            hash,
            Entry {
                commands,
                path,
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
thread_local! {static CACHE:RefCell<Cache>=RefCell::new(Cache::default());}
pub(super) fn get_or_build(commands: &[PathCommand], build: impl FnOnce() -> Path) -> Path {
    // Tiny contours remain their direct builder path; no drawing is omitted.
    if commands.len() < 16 || commands.len() > MAX_COMMANDS {
        return build();
    }
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    commands.len().hash(&mut hash);
    for command in commands {
        bits(command).hash(&mut hash);
    }
    let hash = hash.finish();
    if let Some(path) = CACHE.with(|cache| {
        cache
            .borrow()
            .entries
            .get(&hash)
            .filter(|entry| {
                entry.commands.len() == commands.len()
                    && entry
                        .commands
                        .iter()
                        .zip(commands)
                        .all(|(a, b)| bits(a) == bits(b))
            })
            .map(|entry| entry.path.clone())
    }) {
        return path;
    }
    let path = build();
    CACHE.with(|cache| cache.borrow_mut().insert(hash, commands, path.clone()));
    path
}
