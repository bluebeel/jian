/// Raster image placement mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageDrawMode {
    #[default]
    Fill,
    Fit,
    Crop,
    Tile,
    Stretch,
}

/// Compositing mode applied to an isolated paint layer or raster image.
///
/// `Normal` preserves the historical source-over behaviour. The remaining
/// variants mirror the blend modes supported by the canonical `.op` schema
/// and map directly onto Skia / CanvasKit paint modes in host backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageBlendMode {
    #[default]
    Normal,
    Darken,
    Multiply,
    Screen,
    Overlay,
    Lighten,
    Difference,
    Hue,
    Saturation,
    Color,
    Luminosity,
    SoftLight,
    ColorDodge,
    ColorBurn,
    HardLight,
    Exclusion,
}

/// Per-image adjustment values in the UI slider range `[-100, 100]`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ImageAdjustments {
    pub exposure: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub temperature: f32,
    pub tint: f32,
    pub highlights: f32,
    pub shadows: f32,
}

impl ImageAdjustments {
    pub fn is_neutral(self) -> bool {
        self.exposure == 0.0
            && self.contrast == 0.0
            && self.saturation == 0.0
            && self.temperature == 0.0
            && self.tint == 0.0
            && self.highlights == 0.0
            && self.shadows == 0.0
    }
}

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
    pub rotation_degrees: Option<f32>,
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
            rotation_degrees: None,
            blend_mode: ImageBlendMode::Normal,
            original_size: None,
            tile_scale: 1.0,
            nearest_sampling: false,
            thumbnail: None,
        }
    }
}
