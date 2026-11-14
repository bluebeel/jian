//! Two fixed shadow programs per render thread, shared by adapters.

use skia_safe::{
    runtime_effect::RuntimeShaderBuilder, Canvas, ClipOp, Color4f, Matrix, Paint, Rect,
    RuntimeEffect, Shader,
};

thread_local! {
    static EFFECT: Option<RuntimeEffect> = RuntimeEffect::make_for_shader(
        jian_core::render::shadow::TRIANGULAR_SHADOW_SHADER, None,
    ).ok();
    static SOLID: Option<RuntimeEffect> = RuntimeEffect::make_for_shader(
        "uniform float4 tint; half4 main(float2 p) { return half4(tint); }", None,
    ).ok();
}

pub fn shader(bounds: Rect, radius: f32, blur: f32, color: Color4f, inner: bool) -> Option<Shader> {
    EFFECT.with(|effect| {
        let mut builder = RuntimeShaderBuilder::new(effect.as_ref()?.clone());
        let color = [
            color.r * color.a,
            color.g * color.a,
            color.b * color.a,
            color.a,
        ];
        for (name, values) in [
            (
                "bounds",
                &[bounds.left, bounds.top, bounds.right, bounds.bottom][..],
            ),
            ("radius", &[radius][..]),
            ("blur", &[blur.max(0.0)][..]),
            ("inner", &[if inner { 1.0 } else { 0.0 }][..]),
            ("tint", &color[..]),
        ] {
            builder.set_uniform_float(name, values).ok()?;
        }
        builder.make_shader(&Matrix::default())
    })
}

/// An analytic profile plus the caller's already-selected raster bounds.
/// No offscreen surface or persistent per-shape resource is allocated.
pub struct RoundedShadowDraw {
    pub bounds: Rect,
    pub radius: f32,
    pub blur: f32,
    pub color: Color4f,
    pub inner: bool,
    pub draw_bounds: Rect,
}

/// Split only a conservative region where the existing shader returns exactly
/// one (outer) or zero (inset). Both regions use the same framebuffer blender;
/// an AA-disabled clip and its complement cover the original raster once.
pub fn draw(canvas: &Canvas, shadow: &RoundedShadowDraw) -> bool {
    let Some(shader) = shader(
        shadow.bounds,
        shadow.radius,
        shadow.blur,
        shadow.color,
        shadow.inner,
    ) else {
        return false;
    };
    let mut paint = Paint::default();
    paint.set_shader(shader);
    if crate::raster_sampling::configure_framebuffer_blend(&mut paint).is_err() {
        return false;
    }
    let interior = (!canvas.local_to_device_as_3x3().has_perspective())
        .then(|| constant_interior(canvas, shadow.bounds, shadow.radius, shadow.blur))
        .flatten();
    let solid = if interior.is_some() && !shadow.inner {
        solid_shader(shadow.color)
    } else {
        None
    };
    if let Some(interior) = interior.filter(|_| shadow.inner || solid.is_some()) {
        let saved = canvas.save();
        canvas.clip_rect(interior, ClipOp::Difference, false);
        canvas.draw_rect(shadow.draw_bounds, &paint);
        canvas.restore_to_count(saved);
        if let Some(solid) = solid {
            let saved = canvas.save();
            canvas.clip_rect(interior, ClipOp::Intersect, false);
            paint.set_shader(solid);
            canvas.draw_rect(shadow.draw_bounds, &paint);
            canvas.restore_to_count(saved);
        }
    } else {
        canvas.draw_rect(shadow.draw_bounds, &paint);
    }
    true
}

fn solid_shader(color: Color4f) -> Option<Shader> {
    SOLID.with(|effect| {
        let mut builder = RuntimeShaderBuilder::new(effect.as_ref()?.clone());
        builder
            .set_uniform_float(
                "tint",
                &[
                    color.r * color.a,
                    color.g * color.a,
                    color.b * color.a,
                    color.a,
                ],
            )
            .ok()?;
        builder.make_shader(&Matrix::default())
    })
}

fn constant_interior(canvas: &Canvas, bounds: Rect, radius: f32, blur: f32) -> Option<Rect> {
    if !(blur > 0.0 && blur.is_finite() && radius >= 0.0 && radius.is_finite()) {
        return None;
    }
    // Keep the partition away from the strict normalized shader boundary.
    // The guard grows with coordinate ULPs; ill-conditioned/overflowing inputs
    // simply retain the original single draw.
    let magnitude = [
        bounds.left,
        bounds.top,
        bounds.right,
        bounds.bottom,
        radius,
        blur,
    ]
    .into_iter()
    .map(f32::abs)
    .fold(0.0f32, f32::max);
    let inverse = canvas.local_to_device_as_3x3().invert()?;
    let inverse_norm = (inverse.scale_x().abs() + inverse.skew_x().abs())
        .max(inverse.skew_y().abs() + inverse.scale_y().abs());
    let clip = canvas.device_clip_bounds()?;
    let device_extent = [clip.left, clip.top, clip.right, clip.bottom]
        .into_iter()
        .map(|value| (value as f32).abs())
        .fold(1.0f32, f32::max);
    let translation = inverse.translate_x().abs().max(inverse.translate_y().abs());
    // Two device pixels also dominate non-AA clip scan-conversion rounding.
    // Mapping this guard back to local units makes zoom-out and ill-conditioned
    // affine matrices shrink the fast region, or disable it entirely.
    let guard = (magnitude * (16.0 * f32::EPSILON))
        .max(1.0)
        .max(2.0 * inverse_norm)
        .max((inverse_norm * device_extent + translation) * (16.0 * f32::EPSILON));
    let pad = radius + blur + guard;
    let interior = bounds.with_inset((pad, pad));
    if !interior.is_finite()
        || interior.is_empty()
        || ![
            bounds.width() / blur,
            bounds.height() / blur,
            radius / blur,
            (bounds.left + bounds.right) * 0.5,
            (bounds.top + bounds.bottom) * 0.5,
            1.0 / blur,
        ]
        .into_iter()
        .all(f32::is_finite)
        || f64::from(interior.width()) * f64::from(interior.height())
            < f64::from(bounds.width()) * f64::from(bounds.height()) * 0.25
    {
        None
    } else {
        Some(interior)
    }
}
