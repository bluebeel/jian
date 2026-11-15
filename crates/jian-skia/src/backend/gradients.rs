//! Gradient draw helpers moved intact from the backend spine.

use super::*;

pub(super) fn gradient_endpoints(rect: Rect, angle_deg: f32) -> ((f32, f32), (f32, f32)) {
    // Angle convention matches the .op schema: 0° flows left-to-right,
    // 90° top-to-bottom (clockwise from +x). We pick the two points on
    // the rect's edges where the axis-aligned gradient line meets.
    let cx = rect.min_x() + rect.size.width * 0.5;
    let cy = rect.min_y() + rect.size.height * 0.5;
    let rad = angle_deg.to_radians();
    let dx = rad.cos();
    let dy = rad.sin();
    let half_w = rect.size.width * 0.5;
    let half_h = rect.size.height * 0.5;
    // Project half-diagonal onto the axis so the gradient spans the
    // whole rect along the requested direction.
    let t = (dx.abs() * half_w) + (dy.abs() * half_h);
    let x0 = cx - dx * t;
    let y0 = cy - dy * t;
    let x1 = cx + dx * t;
    let y1 = cy + dy * t;
    ((x0, y0), (x1, y1))
}

#[path = "gradient_reuse.rs"]
mod reuse;

pub(super) fn draw_linear_gradient_rect(
    canvas: &skia_safe::Canvas,
    rect: Rect,
    radii: BorderRadii,
    g: &jian_core::render::LinearGradient,
    stroke: Option<&jian_core::render::StrokeOp>,
) {
    let (p0, p1) = gradient_endpoints(rect, g.angle_deg);
    let shader = reuse::shader(false, [p0.0, p0.1, p1.0, p1.1], &g.stops);
    let paint = gradient_paint(shader, &g.stops, g.opacity);
    draw_gradient_rect(canvas, rect, radii, paint.as_ref(), stroke);
}

pub(super) fn draw_radial_gradient_rect(
    canvas: &skia_safe::Canvas,
    rect: Rect,
    radii: BorderRadii,
    g: &jian_core::render::RadialGradient,
    stroke: Option<&jian_core::render::StrokeOp>,
) {
    // Retain the original local-coordinate arithmetic and radius policy.
    let cx = rect.min_x() + g.cx * rect.size.width;
    let cy = rect.min_y() + g.cy * rect.size.height;
    let r = g.radius * rect.size.width.max(rect.size.height);
    let shader = reuse::shader(true, [cx, cy, r.max(0.0), 0.0], &g.stops);
    let paint = gradient_paint(shader, &g.stops, g.opacity);
    draw_gradient_rect(canvas, rect, radii, paint.as_ref(), stroke);
}

fn gradient_paint(
    shader: Option<skia_safe::Shader>,
    stops: &[jian_core::render::GradientStop],
    opacity: f32,
) -> Option<SkPaint> {
    let first = stops.first()?;
    let mut paint = SkPaint::default();
    crate::raster_sampling::configure_geometry(&mut paint);
    paint.set_style(PaintStyle::Fill);
    if let Some(shader) = shader {
        paint.set_shader(shader);
        paint.set_alpha_f(opacity);
    } else {
        // A refused/single-stop gradient uses its authored color, not the
        // default black Paint. Empty fills stay empty; strokes still draw.
        let mut color = to_sk_color(first.color);
        color.a *= opacity;
        paint.set_color4f(color, None);
    }
    Some(paint)
}

pub(super) fn draw_gradient_rect(
    canvas: &skia_safe::Canvas,
    rect: Rect,
    radii: BorderRadii,
    fill: Option<&SkPaint>,
    stroke: Option<&jian_core::render::StrokeOp>,
) {
    // Reuse the exact same shape preparation for fill and stroke.
    let bounds = to_sk_rect(rect);
    let rounded = (radii != BorderRadii::zero()).then(|| {
        RRect::new_rect_radii(
            bounds,
            &[
                SkPoint::new(radii.tl, radii.tl),
                SkPoint::new(radii.tr, radii.tr),
                SkPoint::new(radii.br, radii.br),
                SkPoint::new(radii.bl, radii.bl),
            ],
        )
    });
    let draw = |paint: &SkPaint| {
        if let Some(rrect) = rounded.as_ref() {
            canvas.draw_rrect(rrect, paint);
        } else {
            canvas.draw_rect(bounds, paint);
        }
    };
    if let Some(paint) = fill {
        draw(paint);
    }
    if let Some(stroke) = stroke {
        let mut paint = SkPaint::new(to_sk_color(stroke.color), None);
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(stroke.width);
        crate::raster_sampling::configure_geometry(&mut paint);
        draw(&paint);
    }
}

pub(super) fn draw_mesh_gradient_rect(
    canvas: &skia_safe::Canvas,
    rect: Rect,
    radii: BorderRadii,
    g: &jian_core::render::MeshGradient,
    stroke: Option<&jian_core::render::StrokeOp>,
) {
    use skia_safe::BlendMode as SkBlendMode;

    let Some(vertices) = crate::mesh_geometry::lattice_vertices(
        [
            rect.min_x(),
            rect.min_y(),
            rect.size.width,
            rect.size.height,
        ],
        g.rows,
        g.cols,
        g.colors.len(),
        |i| {
            let jc = g.colors[i];
            Color::from_argb(jc.a(), jc.r(), jc.g(), jc.b())
        },
    ) else {
        crate::raster_sampling::mark_unsupported("invalid or unrepresentable mesh grid");
        return;
    };

    // Clip to the node's round-rect so the Gouraud fill respects corner
    // radius, then draw the per-vertex-coloured triangles. SrcOver blends
    // the vertex colours straight onto the canvas (Modulate would need a
    // shader to modulate against).
    let is_rounded = radii != BorderRadii::zero();
    let restore_count = canvas.save();
    if is_rounded {
        let sk_rect = to_sk_rect(rect);
        let radii_arr = [
            SkPoint::new(radii.tl, radii.tl),
            SkPoint::new(radii.tr, radii.tr),
            SkPoint::new(radii.br, radii.br),
            SkPoint::new(radii.bl, radii.bl),
        ];
        let rrect = RRect::new_rect_radii(sk_rect, &radii_arr);
        canvas.clip_rrect(
            rrect,
            None,
            Some(crate::raster_sampling::geometry_antialias()),
        );
    } else {
        canvas.clip_rect(
            to_sk_rect(rect),
            None,
            Some(crate::raster_sampling::geometry_antialias()),
        );
    }

    // `draw_vertices` with no shader combines each interpolated vertex
    // colour with the paint's colour via `mode`. A default Paint is opaque
    // BLACK, so seed it white and use `Modulate` (white × vertex == vertex)
    // to pass the vertex colours through unchanged; `opacity` rides in the
    // paint alpha.
    let mut paint = SkPaint::default();
    crate::raster_sampling::configure_geometry(&mut paint);
    paint.set_color4f(Color4f::new(1.0, 1.0, 1.0, g.opacity), None);
    canvas.draw_vertices(&vertices, SkBlendMode::Modulate, &paint);
    canvas.restore_to_count(restore_count);

    if let Some(stroke) = stroke {
        let mut p = SkPaint::new(to_sk_color(stroke.color), None);
        p.set_style(PaintStyle::Stroke);
        p.set_stroke_width(stroke.width);
        crate::raster_sampling::configure_geometry(&mut p);
        if is_rounded {
            let sk_rect = to_sk_rect(rect);
            let radii_arr = [
                SkPoint::new(radii.tl, radii.tl),
                SkPoint::new(radii.tr, radii.tr),
                SkPoint::new(radii.br, radii.br),
                SkPoint::new(radii.bl, radii.bl),
            ];
            let rrect = RRect::new_rect_radii(sk_rect, &radii_arr);
            canvas.draw_rrect(rrect, &p);
        } else {
            canvas.draw_rect(to_sk_rect(rect), &p);
        }
    }
}
