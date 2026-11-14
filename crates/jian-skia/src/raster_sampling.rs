//! Scoped point coverage and RGBA8 framebuffer writes for correlated sampling.
//!
//! Ordinary canvas callers retain Skia's native paint behavior. A complete
//! scene camera can opt into one point per physical pixel, then resolve its
//! cameras together after all backdrop, mask and group operations have run.

use skia_safe::{runtime_effect::ChildPtr, BlendMode, Blender, Data, Paint, RuntimeEffect};
use std::{cell::Cell, fmt};

const HELPERS: &str = r#"
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
float4 backdrop(half4 destination) {
    // Every destination is an RGBA8 load. Its normalized float is within
    // half a byte of the stored integer, so recovery needs no tie handling.
    int4 stored = int4(clamp(float4(destination), 0.0, 1.0) * 255.0 + .5);
    return float4(stored * 257) * (1.0 / 65535.0);
}
"#;
const GENERAL: &str = r#"
uniform blender operation;
half4 main(half4 source, half4 destination) {
    float4 result = float4(operation.eval(source, half4(backdrop(destination))));
    return half4(bytes(result) / 255.0);
}
"#;
const SOURCE_OVER: &str = r#"
half4 main(half4 source, half4 destination) {
    if (all(equal(source, half4(0.0)))) return destination;
    if (source.a == 1.0) return half4(bytes(float4(source)) / 255.0);
    float4 scaledBackdrop = backdrop(destination) * (1.0 - float(source.a));
    float4 result = float4(source) + scaledBackdrop;
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
    let compile = |main: &str| {
        RuntimeEffect::make_for_blender(&format!("{HELPERS}{main}"), None)
            .map_err(|message| PointSamplingError::Compile { message })
    };
    let general = compile(GENERAL)?;
    let source_over = compile(SOURCE_OVER)?;
    MODES
        .iter()
        .map(|&mode| {
            let blender = if mode == BlendMode::SrcOver {
                source_over.make_blender(Data::new_copy(&[]), None)
            } else {
                general.make_blender(
                    Data::new_copy(&[]),
                    &[ChildPtr::Blender(Blender::mode(mode))][..],
                )
            };
            blender
                .map(|blender| (mode, blender))
                .ok_or(PointSamplingError::CreateBlender { mode })
        })
        .collect()
}

thread_local! {
    static POINT: Cell<bool> = const { Cell::new(false) };
    static EXPORT: Cell<bool> = const { Cell::new(false) };
    static UNSUPPORTED: Cell<Option<&'static str>> = const { Cell::new(None) };
    // A finite set of standard modes; no per-scene shader/program cache.
    static BLENDERS: Result<Vec<(BlendMode, Blender)>, PointSamplingError> = make_blenders();
}

/// Scene exports transform logical geometry through their canvas. Keep this
/// separate from live widgets, whose callers may already scale blur values.
pub fn with_scene_export<R>(draw: impl FnOnce() -> R) -> R {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            EXPORT.with(|export| export.set(self.0));
        }
    }
    let restore = Restore(EXPORT.with(|export| export.replace(true)));
    let result = draw();
    drop(restore);
    result
}

pub fn shadow_respects_ctm() -> bool {
    is_point_sampling() || EXPORT.with(Cell::get)
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

/// Final preparation for a source-over draw. Call only after all shader,
/// filter, color and opacity mutations. An unfiltered constant with alpha0/1
/// has a backdrop-independent result; store its exact byte color once and
/// let Skia use the ordinary skip/fill path instead of a per-pixel program.
pub fn configure_final_src_over(paint: &mut Paint) {
    if !is_point_sampling() {
        return;
    }
    paint.set_anti_alias(false);
    let color = paint.color4f();
    if paint.shader().is_none()
        && paint.color_filter().is_none()
        && paint.mask_filter().is_none()
        && paint.image_filter().is_none()
        && (color.a == 0.0 || color.a == 1.0)
    {
        let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round_ties_even() as u8;
        let color = if color.a == 0.0 {
            skia_safe::Color::TRANSPARENT
        } else {
            skia_safe::Color::from_rgb(byte(color.r), byte(color.g), byte(color.b))
        };
        paint.set_color(color);
        paint.set_blend_mode(BlendMode::SrcOver);
    } else {
        set_blend_mode(paint, BlendMode::SrcOver);
    }
}

/// Use a float RGBA8 source-over write independently of the coverage mode.
/// Analytic translucent sources otherwise lose precision in Skia's packed
/// color path before their coverage and destination are composed.
pub fn configure_framebuffer_blend(paint: &mut Paint) -> Result<(), PointSamplingError> {
    BLENDERS.with(|cache| {
        let blenders = cache.as_ref().map_err(Clone::clone)?;
        let blender = blenders
            .iter()
            .find(|(mode, _)| *mode == BlendMode::SrcOver)
            .ok_or(PointSamplingError::CreateBlender {
                mode: BlendMode::SrcOver,
            })?;
        paint.set_blender(blender.1.clone());
        Ok(())
    })
}

/// A point-image shader may promise byte-normalized colors and binary alpha.
/// With full paint opacity and no filters, ordinary SrcOver then has exactly
/// the same byte result for every backdrop and avoids a redundant quantizer.
pub fn configure_final_byte_image(paint: &mut Paint, binary_alpha: bool) {
    if !is_point_sampling() {
        return;
    }
    paint.set_anti_alias(false);
    if binary_alpha
        && paint.color4f().a == 1.0
        && paint.color_filter().is_none()
        && paint.image_filter().is_none()
        && paint.mask_filter().is_none()
    {
        paint.set_blend_mode(BlendMode::SrcOver);
    } else {
        set_blend_mode(paint, BlendMode::SrcOver);
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
