use super::{ImageAdjustments, ImageBlendMode, ImageDrawMode};

/// An optional encoded preview of the same image, with its own cache identity.
#[derive(Debug, Clone, Copy)]
pub struct ImagePaintThumbnail<'a> {
    pub id: u64,
    pub encoded: &'a [u8],
}

/// Image paint parameters shared by viewport and export backends.
#[derive(Debug, Clone, Copy)]
pub struct ImagePaintOptions<'a> {
    pub mode: ImageDrawMode,
    pub adjustments: ImageAdjustments,
    pub opacity: f32,
    pub corner_radius: f32,
    pub transform: Option<[f32; 6]>,
    pub blend_mode: ImageBlendMode,
    pub original_size: Option<[f32; 2]>,
    pub tile_scale: f32,
    pub nearest_sampling: bool,
    pub thumbnail: Option<ImagePaintThumbnail<'a>>,
}

impl Default for ImagePaintOptions<'_> {
    fn default() -> Self {
        Self {
            mode: ImageDrawMode::Fill,
            adjustments: ImageAdjustments::default(),
            opacity: 1.0,
            corner_radius: 0.0,
            transform: None,
            blend_mode: ImageBlendMode::Normal,
            original_size: None,
            tile_scale: 1.0,
            nearest_sampling: false,
            thumbnail: None,
        }
    }
}
