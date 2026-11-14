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

pub(super) fn draw_linear_gradient_rect(
    canvas: &skia_safe::Canvas,
    rect: Rect,
    radii: BorderRadii,
    g: &jian_core::render::LinearGradient,
    stroke: Option<&jian_core::render::StrokeOp>,
) {
    use skia_safe::{gradient, Shader, TileMode};
    let (p0, p1) = gradient_endpoints(rect, g.angle_deg);
    let colors: Vec<Color4f> = g.stops.iter().map(|s| to_sk_color(s.color)).collect();
    let offsets: Vec<f32> = g.stops.iter().map(|s| s.offset.clamp(0.0, 1.0)).collect();
    // skia-safe 0.97 replaced `gradient_shader::linear` with `gradient::shaders::linear_gradient`
    // taking a `gradient::Gradient { colors, interpolation }` description.
    let gradient_colors =
        gradient::Colors::new(&colors, Some(offsets.as_slice()), TileMode::Clamp, None);
    let gradient_obj = gradient::Gradient::new(gradient_colors, gradient::Interpolation::default());
    let shader: Option<Shader> = gradient::shaders::linear_gradient(
        (SkPoint::new(p0.0, p0.1), SkPoint::new(p1.0, p1.1)),
        &gradient_obj,
        None,
    );
    let mut paint = SkPaint::default();
    crate::raster_sampling::configure_geometry(&mut paint);
    paint.set_style(PaintStyle::Fill);
    paint.set_alpha_f(g.opacity);
    if let Some(s) = shader {
        paint.set_shader(s as Shader);
    }

    let is_rounded = radii != BorderRadii::zero();
    if is_rounded {
        let sk_rect = to_sk_rect(rect);
        let radii_arr = [
            SkPoint::new(radii.tl, radii.tl),
            SkPoint::new(radii.tr, radii.tr),
            SkPoint::new(radii.br, radii.br),
            SkPoint::new(radii.bl, radii.bl),
        ];
        let rrect = RRect::new_rect_radii(sk_rect, &radii_arr);
        canvas.draw_rrect(rrect, &paint);
    } else {
        canvas.draw_rect(to_sk_rect(rect), &paint);
    }

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

pub(super) fn draw_radial_gradient_rect(
    canvas: &skia_safe::Canvas,
    rect: Rect,
    radii: BorderRadii,
    g: &jian_core::render::RadialGradient,
    stroke: Option<&jian_core::render::StrokeOp>,
) {
    use skia_safe::{gradient, Shader, TileMode};

    // cx/cy ∈ [0,1] within the rect; radius ∈ [0,1] of max(w, h).
    let cx = rect.min_x() + g.cx * rect.size.width;
    let cy = rect.min_y() + g.cy * rect.size.height;
    let r = g.radius * rect.size.width.max(rect.size.height);

    let colors: Vec<Color4f> = g.stops.iter().map(|s| to_sk_color(s.color)).collect();
    let offsets: Vec<f32> = g.stops.iter().map(|s| s.offset.clamp(0.0, 1.0)).collect();
    // skia-safe 0.97 replaced `gradient_shader::radial` with `gradient::shaders::radial_gradient`
    // taking `(center, radius)` and a `gradient::Gradient` description.
    let gradient_colors =
        gradient::Colors::new(&colors, Some(offsets.as_slice()), TileMode::Clamp, None);
    let gradient_obj = gradient::Gradient::new(gradient_colors, gradient::Interpolation::default());
    let shader: Option<Shader> =
        gradient::shaders::radial_gradient((SkPoint::new(cx, cy), r.max(0.0)), &gradient_obj, None);
    let mut paint = SkPaint::default();
    crate::raster_sampling::configure_geometry(&mut paint);
    paint.set_style(PaintStyle::Fill);
    paint.set_alpha_f(g.opacity);
    if let Some(s) = shader {
        paint.set_shader(s as Shader);
    }

    let is_rounded = radii != BorderRadii::zero();
    if is_rounded {
        let sk_rect = to_sk_rect(rect);
        let radii_arr = [
            SkPoint::new(radii.tl, radii.tl),
            SkPoint::new(radii.tr, radii.tr),
            SkPoint::new(radii.br, radii.br),
            SkPoint::new(radii.bl, radii.bl),
        ];
        let rrect = RRect::new_rect_radii(sk_rect, &radii_arr);
        canvas.draw_rrect(rrect, &paint);
    } else {
        canvas.draw_rect(to_sk_rect(rect), &paint);
    }

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

pub(super) fn draw_mesh_gradient_rect(
    canvas: &skia_safe::Canvas,
    rect: Rect,
    radii: BorderRadii,
    g: &jian_core::render::MeshGradient,
    stroke: Option<&jian_core::render::StrokeOp>,
) {
    use skia_safe::{vertices::VertexMode, BlendMode as SkBlendMode, Vertices};

    let rows = g.rows.max(2);
    let cols = g.cols.max(2);
    let vcount = (rows * cols) as usize;
    if g.colors.len() != vcount {
        return;
    }

    // Lattice vertex positions across the rect: vertex (r, c) maps to
    // (c/(cols-1), r/(rows-1)) of the rect. Colours come straight from
    // the row-major grid; `texs` is unused (no shader) but must match
    // the vertex count, so we reuse `positions`.
    let mut positions: Vec<SkPoint> = Vec::with_capacity(vcount);
    let mut colors: Vec<Color> = Vec::with_capacity(vcount);
    let denom_c = (cols - 1) as f32;
    let denom_r = (rows - 1) as f32;
    for r in 0..rows {
        for c in 0..cols {
            let fx = rect.min_x() + (c as f32 / denom_c) * rect.size.width;
            let fy = rect.min_y() + (r as f32 / denom_r) * rect.size.height;
            positions.push(SkPoint::new(fx, fy));
            let jc = g.colors[(r * cols + c) as usize];
            colors.push(Color::from_argb(jc.a(), jc.r(), jc.g(), jc.b()));
        }
    }

    // Triangulate each grid cell into two triangles (CCW). Index of
    // vertex (r, c) is `r * cols + c`.
    let mut indices: Vec<u16> = Vec::with_capacity(((rows - 1) * (cols - 1) * 6) as usize);
    for r in 0..(rows - 1) {
        for c in 0..(cols - 1) {
            let tl = (r * cols + c) as u16;
            let tr = (r * cols + c + 1) as u16;
            let bl = ((r + 1) * cols + c) as u16;
            let br = ((r + 1) * cols + c + 1) as u16;
            indices.extend_from_slice(&[tl, tr, bl, tr, br, bl]);
        }
    }

    let vertices = Vertices::new_copy(
        VertexMode::Triangles,
        &positions,
        &positions,
        &colors,
        Some(&indices),
    );

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
