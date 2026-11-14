//! Shape-shadow dispatch, preserving the legacy Gaussian branch.

use super::*;

pub(super) fn draw_shadowed_rect(
    canvas: &skia_safe::Canvas,
    rect: Rect,
    radii: BorderRadii,
    shadow: &jian_core::render::ShadowSpec,
    basis: Option<&skia_safe::Matrix>,
) {
    // Draw a blurred copy of the shape in `shadow.color` offset by
    // (dx, dy). The foreground fill lands via a subsequent draw-op.
    let c4 = shadow
        .float_color
        .map(|c| Color4f::new(c[0], c[1], c[2], c[3]))
        .unwrap_or_else(|| to_sk_color(shadow.color));
    if shadow.blur_kernel == jian_core::render::shadow::ShadowBlurKernel::Triangular
        && shadow.blur > 0.0
    {
        if triangular(canvas, rect, radii, shadow, basis, c4) {
            return;
        }
        crate::raster_sampling::mark_unsupported(
            "triangular shadow with nonuniform radii or unavailable shader",
        );
    }
    let mut paint = SkPaint::new(c4, None);
    crate::raster_sampling::configure_geometry(&mut paint);
    paint.set_style(PaintStyle::Fill);
    // Skia's blur filter uses sigma; CSS-ish `blur` is ~= sigma * 2.
    let sigma = (shadow.blur * 0.5).max(0.0);
    if sigma > 0.0 {
        paint.set_mask_filter(skia_safe::MaskFilter::blur(BlurStyle::Normal, sigma, None));
    }
    let offset_rect = Rect::new(
        jian_core::geometry::point(rect.min_x() + shadow.dx, rect.min_y() + shadow.dy),
        rect.size,
    );
    let is_rounded = radii != BorderRadii::zero();
    if is_rounded {
        let sk_rect = to_sk_rect(offset_rect);
        let radii_arr = [
            SkPoint::new(radii.tl, radii.tl),
            SkPoint::new(radii.tr, radii.tr),
            SkPoint::new(radii.br, radii.br),
            SkPoint::new(radii.bl, radii.bl),
        ];
        let rrect = RRect::new_rect_radii(sk_rect, &radii_arr);
        canvas.draw_rrect(rrect, &paint);
    } else {
        canvas.draw_rect(to_sk_rect(offset_rect), &paint);
    }
}

fn triangular(
    canvas: &skia_safe::Canvas,
    rect: Rect,
    radii: BorderRadii,
    shadow: &ShadowSpec,
    basis: Option<&skia_safe::Matrix>,
    color: Color4f,
) -> bool {
    if radii.tl != radii.tr || radii.tl != radii.br || radii.tl != radii.bl {
        return false;
    }
    let mut bounds = to_sk_rect(rect).with_outset((shadow.spread, shadow.spread));
    if bounds.is_empty() {
        return true;
    }
    let radius = if radii.tl <= 0.0 {
        0.0
    } else {
        (radii.tl + shadow.spread)
            .max(0.0)
            .min(bounds.width().min(bounds.height()) * 0.5)
    };
    let offset = match (basis, canvas.local_to_device_as_3x3().invert()) {
        (Some(basis), Some(inverse)) => {
            inverse.map_vector(basis.map_vector((shadow.dx, shadow.dy)))
        }
        _ => SkPoint::new(shadow.dx, shadow.dy),
    };
    bounds.offset(offset);
    let Some(shader) = crate::triangular_shadow::shader(bounds, radius, shadow.blur, color, false)
    else {
        return false;
    };
    let mut paint = SkPaint::default();
    paint.set_shader(shader);
    if crate::raster_sampling::configure_framebuffer_blend(&mut paint).is_err() {
        return false;
    }
    canvas.draw_rect(bounds.with_outset((shadow.blur, shadow.blur)), &paint);
    true
}
