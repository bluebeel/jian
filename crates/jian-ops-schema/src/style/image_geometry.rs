//! Explicit normalized source/tile image fields. Legacy mode math is separate.
use serde::{Deserialize, Serialize};
fn identity() -> [f64; 6] {
    [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
}
fn yes() -> bool {
    true
}
fn repeat() -> ImageRepeat {
    ImageRepeat::Repeat
}
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[cfg_attr(feature = "export-ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "export-ts", ts(export, export_to = "ops.ts"))]
#[serde(rename_all = "snake_case")]
pub enum ImageExtend {
    #[default]
    Decal,
    Pad,
    Repeat,
    Reflect,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "export-ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "export-ts", ts(export, export_to = "ops.ts"))]
#[serde(rename_all = "camelCase")]
pub struct ImageFieldGeometry {
    #[serde(default)]
    pub units: ImageFieldUnits,
    /// SVG-column-order sourceUV→paintUV, or sourceUV→tileUV when patterned.
    #[serde(default = "identity")]
    pub transform: [f64; 6],
    #[serde(default)]
    pub extend_x: ImageExtend,
    #[serde(default)]
    pub extend_y: ImageExtend,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<Box<ImagePatternGeometry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<Box<ImageViewportGeometry>>,
}
impl Default for ImageFieldGeometry {
    fn default() -> Self {
        Self {
            units: ImageFieldUnits::Normalized,
            transform: identity(),
            extend_x: Default::default(),
            extend_y: Default::default(),
            pattern: None,
            viewport: None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "export-ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "export-ts", ts(export, export_to = "ops.ts"))]
#[serde(rename_all = "camelCase")]
pub struct ImagePatternGeometry {
    /// SVG-column-order tileUV→normalized paintUV.
    #[serde(default = "identity")]
    pub transform: [f64; 6],
    #[serde(default = "repeat")]
    pub extend_x: ImageRepeat,
    #[serde(default = "repeat")]
    pub extend_y: ImageRepeat,
    /// SVG pattern viewport clipping. False retains overflowing source copies.
    #[serde(default = "yes")]
    pub clip: bool,
}
impl Default for ImagePatternGeometry {
    fn default() -> Self {
        Self {
            transform: identity(),
            extend_x: ImageRepeat::Repeat,
            extend_y: ImageRepeat::Repeat,
            clip: true,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[cfg_attr(feature = "export-ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "export-ts", ts(export, export_to = "ops.ts"))]
#[serde(rename_all = "snake_case")]
pub enum ImageViewportFit {
    None,
    #[default]
    Meet,
    Slice,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "export-ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "export-ts", ts(export, export_to = "ops.ts"))]
#[serde(rename_all = "camelCase")]
pub struct ImageViewportGeometry {
    pub size: [f64; 2],
    #[serde(default)]
    pub fit: ImageViewportFit,
    #[serde(default = "center")]
    pub align: [f64; 2],
    #[serde(default = "yes")]
    pub clip: bool,
}
fn center() -> [f64; 2] {
    [0.5, 0.5]
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[cfg_attr(feature = "export-ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "export-ts", ts(export, export_to = "ops.ts"))]
#[serde(rename_all = "snake_case")]
pub enum ImageRepeat {
    Once,
    #[default]
    Repeat,
    Reflect,
}
impl ImageRepeat {
    pub fn extension(self) -> ImageExtend {
        match self {
            Self::Once => ImageExtend::Decal,
            Self::Repeat => ImageExtend::Repeat,
            Self::Reflect => ImageExtend::Reflect,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[cfg_attr(feature = "export-ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "export-ts", ts(export, export_to = "ops.ts"))]
#[serde(rename_all = "snake_case")]
pub enum ImageFieldUnits {
    #[default]
    Normalized,
    UserSpace,
    LocalPixels,
}
