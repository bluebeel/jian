//! Scoped point coverage and RGBA8 framebuffer writes for correlated sampling.
//!
//! Ordinary canvas callers retain Skia's native paint behavior. A complete
//! scene camera can opt into one point per physical pixel, then resolve its
//! cameras together after all backdrop, mask and group operations have run.

use skia_safe::{runtime_effect::ChildPtr, BlendMode, Blender, Data, Paint, RuntimeEffect};
use std::{cell::Cell, fmt};

const SOURCE: &str = r#"
uniform blender operation;
float quantize(float value) {
    float scaled = clamp(value, 0.0, 1.0) * 255.0;
    float base = floor(scaled);
    float fraction = scaled - base;
    if (fraction > .5 || (fraction == .5 && mod(base, 2.0) != 0.0)) base += 1.0;
    return base;
}
float4 bytes(float4 value) {
    return float4(quantize(value.r), quantize(value.g), quantize(value.b), quantize(value.a));
}
half4 main(half4 source, half4 destination) {
    // RGBA8 framebuffer loads expand bytes to UNORM16 before float blending.
    // Preserve integer expansion: multiplying by reciprocal255 changes ties.
    float4 backdrop = float4(int4(bytes(float4(destination))) * 257) * (1.0 / 65535.0);
    float4 result = float4(operation.eval(source, half4(backdrop)));
    return half4(bytes(result) / 255.0);
}
"#;

const MODES: &[BlendMode] = &[
    BlendMode::Clear,
    BlendMode::Src,
    BlendMode::Dst,
    BlendMode::SrcOver,
    BlendMode::DstOver,
    BlendMode::SrcIn,
    BlendMode::DstIn,
    BlendMode::SrcOut,
    BlendMode::DstOut,
    BlendMode::SrcATop,
    BlendMode::DstATop,
    BlendMode::Xor,
    BlendMode::Plus,
    BlendMode::Modulate,
    BlendMode::Screen,
    BlendMode::Overlay,
    BlendMode::Darken,
    BlendMode::Lighten,
    BlendMode::ColorDodge,
    BlendMode::ColorBurn,
    BlendMode::HardLight,
    BlendMode::SoftLight,
    BlendMode::Difference,
    BlendMode::Exclusion,
    BlendMode::Multiply,
    BlendMode::Hue,
    BlendMode::Saturation,
    BlendMode::Color,
    BlendMode::Luminosity,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PointSamplingError {
    Compile { message: String },
    CreateBlender { mode: BlendMode },
    Unsupported { feature: &'static str },
}
impl fmt::Display for PointSamplingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compile { message } => write!(f, "point-sampling shader: {message}"),
            Self::CreateBlender { mode } => {
                write!(f, "point-sampling blender unavailable: {mode:?}")
            }
            Self::Unsupported { feature } => write!(f, "point sampling does not support {feature}"),
        }
    }
}
impl std::error::Error for PointSamplingError {}

fn make_blenders() -> Result<Vec<(BlendMode, Blender)>, PointSamplingError> {
    let effect = RuntimeEffect::make_for_blender(SOURCE, None)
        .map_err(|message| PointSamplingError::Compile { message })?;
    MODES
        .iter()
        .map(|&mode| {
            effect
                .make_blender(
                    Data::new_copy(&[]),
                    &[ChildPtr::Blender(Blender::mode(mode))][..],
                )
                .map(|blender| (mode, blender))
                .ok_or(PointSamplingError::CreateBlender { mode })
        })
        .collect()
}

thread_local! {
    static POINT: Cell<bool> = const { Cell::new(false) };
    static UNSUPPORTED: Cell<Option<&'static str>> = const { Cell::new(None) };
    // A finite set of standard modes; no per-scene shader/program cache.
    static BLENDERS: Result<Vec<(BlendMode, Blender)>, PointSamplingError> = make_blenders();
}

pub fn is_point_sampling() -> bool {
    POINT.with(Cell::get)
}

pub fn with_point_sampling<R>(draw: impl FnOnce() -> R) -> Result<R, PointSamplingError> {
    BLENDERS.with(|cache| cache.as_ref().map(|_| ()).map_err(Clone::clone))?;
    struct Restore(bool, Option<&'static str>);
    impl Drop for Restore {
        fn drop(&mut self) {
            POINT.with(|point| point.set(self.0));
            UNSUPPORTED.with(|error| error.set(self.1));
        }
    }
    let restore = Restore(
        POINT.with(|point| point.replace(true)),
        UNSUPPORTED.with(|error| error.replace(None)),
    );
    let result = draw();
    let error = UNSUPPORTED.with(Cell::get);
    drop(restore);
    match error {
        Some(feature) => Err(PointSamplingError::Unsupported { feature }),
        None => Ok(result),
    }
}

pub fn mark_unsupported(feature: &'static str) {
    if is_point_sampling() {
        UNSUPPORTED.with(|error| error.set(Some(feature)));
    }
}

pub fn geometry_antialias() -> bool {
    !is_point_sampling()
}

/// Apply point coverage and a measured floating RGBA8 write to a final draw
/// or saveLayer composite. Existing custom blenders are left intact, including
/// paints this function has already configured during the same camera pass.
pub fn configure_paint(paint: &mut Paint) {
    if !is_point_sampling() {
        return;
    }
    paint.set_anti_alias(false);
    if let Some(mode) = paint.as_blend_mode() {
        BLENDERS.with(|cache| {
            if let Ok(blenders) = cache {
                if let Some((_, blender)) = blenders.iter().find(|(kind, _)| *kind == mode) {
                    paint.set_blender(blender.clone());
                }
            }
        });
    }
}

/// Used where ordinary rendering requests antialiasing.
pub fn configure_geometry(paint: &mut Paint) {
    paint.set_anti_alias(!is_point_sampling());
    configure_paint(paint);
}

/// Preserve the authored blend mode even if it is selected after geometry.
pub fn set_blend_mode(paint: &mut Paint, mode: BlendMode) {
    paint.set_blend_mode(mode);
    configure_paint(paint);
}

#[cfg(test)]
#[path = "raster_sampling_tests.rs"]
mod tests;
