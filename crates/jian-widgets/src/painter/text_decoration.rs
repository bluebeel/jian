//! A styled line slice's decoration geometry, separate from glyph paint.
use crate::geometry::{Color, Point2D};

#[derive(Debug, Clone, Copy)]
pub struct TextDecorationRequest<'a> {
    pub text: &'a str,
    pub font_family: &'a str,
    pub font_size: f32,
    pub font_weight: u16,
    pub italic: bool,
    pub kerning: Option<bool>,
    pub letter_spacing: f32,
    pub line_height: f32,
    pub justify_extra: f32,
    pub font_features: &'a std::collections::BTreeMap<String, u32>,
    pub origin: Point2D,
    pub width: f32,
    pub color: Color,
    pub underline: bool,
    pub strikethrough: bool,
    pub skip_ink: bool,
}
