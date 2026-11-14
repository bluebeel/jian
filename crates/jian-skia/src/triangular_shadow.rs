//! One cached analytic shadow program per render thread, shared by adapters.

use skia_safe::{
    runtime_effect::RuntimeShaderBuilder, Color4f, Matrix, Rect, RuntimeEffect, Shader,
};

thread_local! {
    static EFFECT: Option<RuntimeEffect> = RuntimeEffect::make_for_shader(
        jian_core::render::shadow::TRIANGULAR_SHADOW_SHADER, None,
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
