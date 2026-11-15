//! Nonuniform circular corners, using the shared four-slice triangle profile.
use skia_safe::{
    runtime_effect::RuntimeShaderBuilder, Canvas, Color4f, Matrix, Paint, Rect, RuntimeEffect,
};

thread_local! {
    static EFFECT: Option<RuntimeEffect> = RuntimeEffect::make_for_shader(
        jian_core::render::shadow::TRIANGULAR_CORNER_SHADOW_SHADER,None,
    ).ok();
}

pub struct CornerShadowDraw {
    pub bounds: Rect,
    pub radii: [f32; 4],
    pub blur: f32,
    pub color: Color4f,
    pub inner: bool,
    pub draw_bounds: Rect,
}
pub fn draw(canvas: &Canvas, shadow: &CornerShadowDraw) -> bool {
    let shader = EFFECT.with(|effect| {
        let mut builder = RuntimeShaderBuilder::new(effect.as_ref()?.clone());
        for (name, values) in [
            (
                "bounds",
                &[
                    shadow.bounds.left,
                    shadow.bounds.top,
                    shadow.bounds.right,
                    shadow.bounds.bottom,
                ][..],
            ),
            ("radii", &shadow.radii[..]),
            ("blur", &[shadow.blur.max(0.0)][..]),
            ("inner", &[if shadow.inner { 1.0 } else { 0.0 }][..]),
            (
                "tint",
                &[
                    shadow.color.r * shadow.color.a,
                    shadow.color.g * shadow.color.a,
                    shadow.color.b * shadow.color.a,
                    shadow.color.a,
                ][..],
            ),
        ] {
            builder.set_uniform_float(name, values).ok()?;
        }
        builder.make_shader(&Matrix::default())
    });
    let Some(shader) = shader else { return false };
    let mut paint = Paint::default();
    paint.set_shader(shader);
    if crate::raster_sampling::configure_framebuffer_blend(&mut paint).is_err() {
        return false;
    }
    canvas.draw_rect(shadow.draw_bounds, &paint);
    true
}
