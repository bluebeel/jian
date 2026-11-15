//! Lexical preparation of one reusable glyph/decor mask, never a document cache.
use crate::geometry::Rect;
#[derive(Clone, Copy, Debug)]
#[repr(u32)]
pub enum TextMaskAction {
    Begin = 0,
    Finish = 1,
    Replay = 2,
    Discard = 3,
}
#[derive(Clone, Copy, Debug)]
pub enum TextMaskResult {
    /// Unsupported/refused with no prepared picture; keep the ordinary draw.
    Unavailable,
    Recording,
    /// None keeps full field support; Some(empty) proves no painted coverage.
    Ready(Option<Rect>),
    Replayed,
}
