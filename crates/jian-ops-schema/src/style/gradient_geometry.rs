//! Optional exact geometry for imported gradient fields. Legacy fills omit it.

use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[cfg_attr(feature = "export-ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum GradientUnits {
    #[default]
    ObjectBoundingBox,
    UserSpaceOnUse,
}
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[cfg_attr(feature = "export-ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum GradientSpread {
    #[default]
    Pad,
    Repeat,
    Reflect,
}
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[cfg_attr(feature = "export-ts", derive(ts_rs::TS))]
pub enum GradientInterpolation {
    #[default]
    #[serde(rename = "sRGB")]
    Srgb,
    #[serde(rename = "linearRGB")]
    LinearRgb,
}
/// Geometry and alpha interpolation are independent source contracts.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[cfg_attr(feature = "export-ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum GradientAlphaInterpolation {
    #[default]
    Premultiplied,
    Straight,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "export-ts", derive(ts_rs::TS))]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum GradientShape {
    Linear {
        start: [f64; 2],
        end: [f64; 2],
    },
    Radial {
        center: [f64; 2],
        radius: f64,
        focal: [f64; 2],
        focal_radius: f64,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "export-ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct GradientGeometry {
    pub shape: GradientShape,
    /// Column-vector matrix [a,b,c,d,e,f]: gradient coordinates to the
    /// node's normalized local bounds. Includes resolved units/ancestry.
    /// This normalized representation follows canonical node resizing.
    pub transform: [f64; 6],
    /// Retained source units; the matrix already resolves their source basis.
    #[serde(default)]
    pub units: GradientUnits,
    #[serde(default)]
    pub spread: GradientSpread,
    #[serde(default)]
    pub interpolation: GradientInterpolation,
    #[serde(default)]
    pub alpha_interpolation: GradientAlphaInterpolation,
}
/// Resolved paint metadata retained beside an executable scene shader, so SVG
/// export can emit the original gradient field instead of a solid fallback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "export-ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct GradientPaint {
    pub geometry: GradientGeometry,
    pub stops: Vec<super::GradientStop>,
    pub opacity: f64,
}
