//! `SkiaBackend` — the `RenderBackend` impl.
//!
//! The `RenderBackend` trait from Plan 2 is canvas-less: `draw`,
//! `push_clip`, `push_transform`, `push_layer`, `apply_blur`, and
//! `apply_shadow` don't carry the surface. To honour the full contract,
//! `SkiaBackend` accumulates a `Vec<Cmd>` between `begin_frame` and
//! `end_frame`, then replays it onto the surface's canvas. This means a
//! caller written against the generic trait gets correct output even
//! though the trait itself never sees the canvas.
//!
//! The canvas-aware entry point `SkiaBackend::draw_on(surface, op)`
//! remains available for host adapters that want to draw without
//! going through the frame-scoped command buffer.

use crate::color::to_sk_color;
use crate::convert::{to_sk_matrix, to_sk_rect};
use crate::image::ImageCache;
use crate::path::to_sk_path;
use crate::shader_cache::ShaderCache;
use crate::surface::SkiaSurface;
use jian_core::geometry::{Affine2, Rect, Size};
use jian_core::render::{BorderRadii, DrawOp, ImageSource, Paint, RenderBackend, ShadowSpec};
use skia_safe::{
    canvas::SaveLayerRec, image_filters, BlurStyle, Color, Color4f, ImageFilter, MaskFilter,
    Paint as SkPaint, PaintStyle, Point as SkPoint, RRect, Rect as SkRect,
};

#[path = "backend/gradients.rs"]
mod gradients;
use gradients::{draw_linear_gradient_rect, draw_mesh_gradient_rect, draw_radial_gradient_rect};

#[path = "backend/shadows.rs"]
mod shadows;
use shadows::draw_shadowed_rect;

#[path = "backend/text.rs"]
mod text;
use text::draw_text;

/// Buffered command — one-to-one with a trait-level `RenderBackend` call.
#[derive(Clone)]
enum Cmd {
    PushClip(Rect),
    PushTransform(Affine2),
    Pop,
    PushLayer {
        bounds: Rect,
        filter: Option<ImageFilter>,
    },
    PopLayer,
    Draw(DrawOp),
}

struct FrameRestore<'a> {
    canvas: &'a skia_safe::Canvas,
    count: usize,
}
impl Drop for FrameRestore<'_> {
    fn drop(&mut self) {
        self.canvas.restore_to_count(self.count);
    }
}

pub struct SkiaBackend {
    /// Pending image filter accumulated by `apply_blur` / `apply_shadow`.
    /// Consumed by the next `push_layer` so the layer is drawn with the
    /// filter applied.
    pending_filter: Option<ImageFilter>,
    pending_analytic_shadow: Option<ShadowSpec>,
    /// Recorded commands for the current frame. Drained by `end_frame`.
    cmds: Vec<Cmd>,
    frame_restore_count: Option<usize>,
    /// Decoded `sk_Image` cache keyed by `ImageSource::cache_key`.
    /// Reused across frames so scrolling lists don't repeat decode.
    image_cache: ImageCache,
    /// Compiled-SkSL `RuntimeEffect` cache keyed by source hash —
    /// compile-once-per-unique-source so per-frame repaints reuse the
    /// program instead of recompiling.
    shader_cache: ShaderCache,
}

impl SkiaBackend {
    pub fn new() -> Self {
        Self {
            pending_filter: None,
            pending_analytic_shadow: None,
            cmds: Vec::new(),
            frame_restore_count: None,
            image_cache: ImageCache::new(),
            shader_cache: ShaderCache::new(),
        }
    }

    /// Number of distinct SkSL programs compiled so far — exposed for
    /// the compile-cache proof (per-frame repaints must NOT grow this).
    pub fn shader_compile_count(&self) -> u64 {
        self.shader_cache.compile_count()
    }

    fn record(&mut self, cmd: Cmd) {
        self.cmds.push(cmd);
    }
}

impl Default for SkiaBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderBackend for SkiaBackend {
    type Surface = SkiaSurface;

    fn new_surface(&mut self, size: Size) -> Self::Surface {
        SkiaSurface::new_raster(size.width.max(1.0) as i32, size.height.max(1.0) as i32)
    }

    fn begin_frame(&mut self, surface: &mut Self::Surface, clear: u32) {
        let canvas = surface.canvas();
        canvas.clear(Color::new(clear));
        self.frame_restore_count = Some(canvas.save());
        self.cmds.clear();
        self.pending_filter = None;
        self.pending_analytic_shadow = None;
    }

    fn end_frame(&mut self, surface: &mut Self::Surface) {
        let canvas = surface.canvas();
        let _restore = FrameRestore {
            canvas,
            count: self.frame_restore_count.take().unwrap_or(1),
        };
        let shadow_basis = canvas.local_to_device_as_3x3();
        for cmd in self.cmds.drain(..) {
            match cmd {
                Cmd::PushClip(r) => {
                    canvas.save();
                    canvas.clip_rect(
                        to_sk_rect(r),
                        None,
                        crate::raster_sampling::geometry_antialias(),
                    );
                }
                Cmd::PushTransform(m) => {
                    canvas.save();
                    canvas.concat(&to_sk_matrix(&m));
                }
                Cmd::Pop => {
                    canvas.restore();
                }
                Cmd::PushLayer { bounds, filter } => {
                    let sk_bounds = to_sk_rect(bounds);
                    let has_filter = filter.is_some();
                    let mut paint = SkPaint::default();
                    if let Some(filter) = filter {
                        paint.set_image_filter(filter);
                    }
                    crate::raster_sampling::configure_paint(&mut paint);
                    let mut rec = SaveLayerRec::default().bounds(&sk_bounds);
                    if has_filter || crate::raster_sampling::is_point_sampling() {
                        rec = rec.paint(&paint);
                    }
                    canvas.save_layer(&rec);
                }
                Cmd::PopLayer => {
                    canvas.restore();
                }
                Cmd::Draw(op) => {
                    draw_canvas(
                        canvas,
                        &op,
                        &mut self.image_cache,
                        &mut self.shader_cache,
                        Some(&shadow_basis),
                    );
                }
            }
        }
        // Retain normal display-list storage, not its dropped DrawOp payloads.
        // A single exceptional frame must not pin an unbounded command backing.
        if self
            .cmds
            .capacity()
            .saturating_mul(std::mem::size_of::<Cmd>())
            > 8 * 1024 * 1024
        {
            self.cmds = Vec::new();
        }
    }

    fn push_clip(&mut self, rect: Rect) {
        self.record(Cmd::PushClip(rect));
    }

    fn push_transform(&mut self, m: &Affine2) {
        self.record(Cmd::PushTransform(*m));
    }

    fn pop(&mut self) {
        self.record(Cmd::Pop);
    }

    fn push_layer(&mut self, bounds: Rect) {
        if let Some(shadow) = self.pending_analytic_shadow.take() {
            if let Some(shape) = shadow.silhouette {
                self.record(Cmd::Draw(DrawOp::ShadowedRect {
                    rect: shape.bounds,
                    radii: shape.radii,
                    shadow,
                }));
            }
        }
        let filter = self.pending_filter.take();
        self.record(Cmd::PushLayer { bounds, filter });
    }

    fn pop_layer(&mut self) {
        self.record(Cmd::PopLayer);
    }

    fn apply_blur(&mut self, sigma: f32) {
        self.pending_analytic_shadow = None;
        self.pending_filter = image_filters::blur((sigma, sigma), None, None, None);
    }

    fn apply_shadow(&mut self, shadow: &ShadowSpec) {
        self.pending_analytic_shadow = None;
        if shadow.blur_kernel == jian_core::render::shadow::ShadowBlurKernel::Triangular {
            if shadow.silhouette.is_some() {
                self.pending_filter = None;
                self.pending_analytic_shadow = Some(shadow.clone());
                return;
            }
            crate::raster_sampling::mark_unsupported(
                "triangular shadow of an arbitrary layer mask",
            );
        }
        let color = to_sk_color(shadow.color);
        let packed = Color::from_argb(
            (color.a * 255.0) as u8,
            (color.r * 255.0) as u8,
            (color.g * 255.0) as u8,
            (color.b * 255.0) as u8,
        );
        let rgba: Color4f = shadow
            .float_color
            .map(|c| Color4f::new(c[0], c[1], c[2], c[3]))
            .unwrap_or_else(|| packed.into());
        self.pending_filter = image_filters::drop_shadow(
            (shadow.dx, shadow.dy),
            (shadow.blur, shadow.blur),
            rgba,
            None,
            None,
            None,
        );
    }

    fn draw(&mut self, op: &DrawOp) {
        self.record(Cmd::Draw(op.clone()));
    }
}

impl SkiaBackend {
    /// Canvas-aware entry point — bypasses the command buffer and draws
    /// directly. Host adapters that already have the surface in hand can
    /// use this to avoid the buffer-and-replay round-trip.
    pub fn draw_on(&mut self, surface: &mut SkiaSurface, op: &DrawOp) {
        draw_canvas(
            surface.canvas(),
            op,
            &mut self.image_cache,
            &mut self.shader_cache,
            None,
        );
    }

    /// GPU-canvas adapter — same as `draw_on` but accepts a borrowed
    /// `&skia_safe::Canvas` directly. Used by hosts that own a GPU
    /// `DirectContext` + `Surface` outside of `SkiaBackend` (e.g.
    /// OpenPencil's `SharedSkiaContext`) and only need the per-`DrawOp`
    /// canvas dispatch from this crate.
    pub fn draw_on_canvas(&mut self, canvas: &skia_safe::Canvas, op: &DrawOp) {
        draw_canvas(
            canvas,
            op,
            &mut self.image_cache,
            &mut self.shader_cache,
            None,
        );
    }
}

fn draw_canvas(
    canvas: &skia_safe::Canvas,
    op: &DrawOp,
    image_cache: &mut ImageCache,
    shader_cache: &mut ShaderCache,
    shadow_basis: Option<&skia_safe::Matrix>,
) {
    match op {
        DrawOp::Rect { rect, paint } => draw_rect(canvas, *rect, paint),
        DrawOp::RoundedRect { rect, radii, paint } => draw_rrect(canvas, *rect, *radii, paint),
        DrawOp::Path { commands, paint } => {
            let path = to_sk_path(commands);
            if let Some(fill_color) = paint.fill {
                let mut p = SkPaint::new(to_sk_color(fill_color), None);
                p.set_alpha_f(combined_alpha(fill_color, paint.opacity));
                crate::raster_sampling::configure_geometry(&mut p);
                p.set_style(PaintStyle::Fill);
                canvas.draw_path(&path, &p);
            }
            if let Some(ref stroke) = paint.stroke {
                let mut p = SkPaint::new(to_sk_color(stroke.color), None);
                p.set_style(PaintStyle::Stroke);
                p.set_stroke_width(stroke.width);
                crate::raster_sampling::configure_geometry(&mut p);
                p.set_alpha_f(combined_alpha(stroke.color, paint.opacity));
                canvas.draw_path(&path, &p);
            }
        }
        DrawOp::Image {
            source,
            dst,
            opacity,
        } => draw_image(canvas, image_cache, source, *dst, *opacity),
        DrawOp::Text(run) => draw_text(canvas, run),
        DrawOp::LinearGradientRect {
            rect,
            radii,
            gradient,
            stroke,
        } => draw_linear_gradient_rect(canvas, *rect, *radii, gradient, stroke.as_ref()),
        DrawOp::RadialGradientRect {
            rect,
            radii,
            gradient,
            stroke,
        } => draw_radial_gradient_rect(canvas, *rect, *radii, gradient, stroke.as_ref()),
        DrawOp::MeshGradientRect {
            rect,
            radii,
            gradient,
            stroke,
        } => draw_mesh_gradient_rect(canvas, *rect, *radii, gradient, stroke.as_ref()),
        DrawOp::ShaderRect {
            rect,
            radii,
            shader,
            stroke,
        } => draw_shader_rect(canvas, shader_cache, *rect, *radii, shader, stroke.as_ref()),
        DrawOp::ShadowedRect {
            rect,
            radii,
            shadow,
        } => {
            draw_shadowed_rect(canvas, *rect, *radii, shadow, shadow_basis);
        }
        DrawOp::Icon {
            rect,
            name,
            family: _,
            color,
        } => draw_icon(canvas, *rect, name, *color),
    }
}

fn draw_icon(canvas: &skia_safe::Canvas, r: Rect, name: &str, color: jian_core::scene::Color) {
    use skia_safe::Matrix;
    let Some(d) = crate::icons::lookup(name) else {
        // Unknown glyph — fall back to a small filled square so the
        // missing icon is visible for debugging.
        let mut p = SkPaint::new(to_sk_color(color), None);
        crate::raster_sampling::configure_geometry(&mut p);
        p.set_style(PaintStyle::Fill);
        canvas.draw_rect(to_sk_rect(r), &p);
        return;
    };
    let Some(path) = icon_path(d) else {
        return;
    };
    // Lucide icons are authored in a 24×24 viewBox. Scale + translate
    // into the target rect, preserving aspect ratio.
    let scale_x = r.size.width / 24.0;
    let scale_y = r.size.height / 24.0;
    let mut m = Matrix::new_identity();
    m.pre_translate((r.min_x(), r.min_y()));
    m.pre_scale((scale_x, scale_y), None);
    // skia-safe 0.97 replaced the mutating `Path::transform(&Matrix)` with
    // `Path::with_transform(&Matrix) -> Path` returning a fresh path.
    let path = path.with_transform(&m);

    let mut sp = SkPaint::new(to_sk_color(color), None);
    crate::raster_sampling::configure_geometry(&mut sp);
    sp.set_style(PaintStyle::Stroke);
    // Lucide's native stroke-width is 2 at 24px. Normalise so the
    // visual weight is consistent across icon sizes.
    let stroke_w = 2.0 * scale_x.min(scale_y).max(0.1);
    sp.set_stroke_width(stroke_w);
    sp.set_stroke_cap(skia_safe::paint::Cap::Round);
    sp.set_stroke_join(skia_safe::paint::Join::Round);
    canvas.draw_path(&path, &sp);
}

// This source is an actual bundled static path returned by icons::lookup;
// cache entries cannot outlive/recycle that immutable lifetime.
fn icon_path(source: &'static str) -> Option<skia_safe::Path> {
    use std::{cell::RefCell, collections::HashMap};
    thread_local! {
        static ICONS: RefCell<(usize, HashMap<&'static str, skia_safe::Path>)> =
            RefCell::new((0, HashMap::new()));
    }
    if let Some(path) = ICONS.with(|slot| slot.borrow().1.get(source).cloned()) {
        return Some(path);
    }
    let path = skia_safe::utils::parse_path::from_svg(source)?;
    let bytes = source
        .len()
        .saturating_add(path.approximate_bytes_used())
        .saturating_add(256);
    if bytes <= 1024 * 1024 {
        ICONS.with(|slot| {
            let mut cache = slot.borrow_mut();
            if cache.1.len() >= 128 || cache.0 > 1024 * 1024 - bytes {
                cache.0 = 0;
                cache.1.clear();
            }
            cache.0 += bytes;
            cache.1.insert(source, path.clone());
        });
    }
    Some(path)
}

fn draw_image(
    canvas: &skia_safe::Canvas,
    cache: &mut ImageCache,
    source: &ImageSource,
    dst: Rect,
    opacity: f32,
) {
    use skia_safe::{canvas::SrcRectConstraint, SamplingOptions};
    let dst_rect = to_sk_rect(dst);
    if let Some(img) = cache.get_or_decode(source) {
        let mut p = SkPaint::default();
        crate::raster_sampling::configure_geometry(&mut p);
        p.set_alpha_f(opacity);
        // object-fit: cover — parity with TS `node-renderer.ts` drawImage,
        // whose default (`objectFit ?? 'fill'`) branch scales by `Math.max`.
        // Center-crop the SOURCE to the destination's aspect ratio so the
        // image fills its slot without distorting: a 4:3 photo dropped into a
        // wide card header is cropped top/bottom, not squashed horizontally.
        // The previous full-source → dst mapping stretched every image.
        let iw = img.width() as f32;
        let ih = img.height() as f32;
        let dw = dst_rect.width().max(1.0);
        let dh = dst_rect.height().max(1.0);
        let src_rect = if iw > 0.0 && ih > 0.0 {
            let src_aspect = iw / ih;
            let dst_aspect = dw / dh;
            let (mut cw, mut ch) = (iw, ih);
            if src_aspect > dst_aspect {
                cw = ih * dst_aspect; // source too wide → crop left/right
            } else {
                ch = iw / dst_aspect; // source too tall → crop top/bottom
            }
            let cx = (iw - cw) * 0.5;
            let cy = (ih - ch) * 0.5;
            SkRect::from_xywh(cx, cy, cw, ch)
        } else {
            SkRect::from_iwh(img.width(), img.height())
        };
        canvas.draw_image_rect_with_sampling_options(
            img,
            Some((&src_rect, SrcRectConstraint::Strict)),
            dst_rect,
            SamplingOptions::default(),
            &p,
        );
    } else {
        // Decode failed (or remote URL with no resolver) — fall back to
        // a grey placeholder so the slot is visible at the right size.
        let mut p = SkPaint::new(Color4f::new(0.5, 0.5, 0.5, opacity), None);
        crate::raster_sampling::configure_geometry(&mut p);
        canvas.draw_rect(dst_rect, &p);
    }
}

/// Fill a (round-)rect with a native SkSL shader. The program is
/// compiled once and cached on `shader_cache` (keyed by source hash);
/// per-frame repaints reuse the `RuntimeEffect`. Uniforms are bound
/// through `RuntimeShaderBuilder`. On ANY compile failure (or a failed
/// `make_shader`) we degrade to `shader.fallback` solid — never panic,
/// since the SkSL source is untrusted.
fn draw_shader_rect(
    canvas: &skia_safe::Canvas,
    shader_cache: &mut ShaderCache,
    rect: Rect,
    radii: BorderRadii,
    spec: &jian_core::render::ShaderSpec,
    stroke: Option<&jian_core::render::StrokeOp>,
) {
    let shader = build_shader(shader_cache, spec, rect);
    let mut paint = SkPaint::default();
    crate::raster_sampling::configure_geometry(&mut paint);
    paint.set_style(PaintStyle::Fill);
    paint.set_alpha_f(spec.opacity.clamp(0.0, 1.0));
    match shader {
        Some(s) => {
            paint.set_shader(s);
        }
        None => {
            // Compile failed (or no shader produced) — visible solid
            // fallback so the node still paints.
            let mut c = to_sk_color(spec.fallback);
            c.a = combined_alpha(spec.fallback, spec.opacity);
            paint.set_color4f(c, None);
        }
    }
    gradients::draw_gradient_rect(canvas, rect, radii, Some(&paint), stroke);
}

/// Compile + bind a shader spec into a `skia_safe::Shader`, or `None` on
/// any failure. Cached compile lives in `shader_cache`; uniform binding
/// errors (a name the program doesn't declare, or an arity mismatch) are
/// tolerated — a best-effort bind still yields a usable shader.
fn build_shader(
    shader_cache: &mut ShaderCache,
    spec: &jian_core::render::ShaderSpec,
    rect: Rect,
) -> Option<skia_safe::Shader> {
    use skia_safe::runtime_effect::RuntimeShaderBuilder;
    // `RuntimeEffect` is an RCHandle — this clone is a refcount bump, not
    // a recompile.
    let effect = shader_cache.get_or_compile(&spec.sksl)?;
    let mut builder = RuntimeShaderBuilder::new(effect);
    for u in &spec.uniforms {
        // Ignore bind errors (undeclared name / wrong arity) so one bad
        // uniform doesn't sink the whole fill.
        let _ = builder.set_uniform_float(&u.name, &u.values);
    }
    let local = crate::shader_geometry::local_matrix(
        [
            rect.min_x(),
            rect.min_y(),
            rect.size.width,
            rect.size.height,
        ],
        spec.uniforms
            .iter()
            .map(|uniform| (uniform.name.as_str(), uniform.values.as_slice())),
    );
    builder.make_shader(&local)
}

fn draw_rect(canvas: &skia_safe::Canvas, r: Rect, paint: &Paint) {
    let rr: SkRect = to_sk_rect(r);
    if let Some(fill) = paint.fill {
        let mut p = SkPaint::new(to_sk_color(fill), None);
        p.set_alpha_f(combined_alpha(fill, paint.opacity));
        crate::raster_sampling::configure_geometry(&mut p);
        p.set_style(PaintStyle::Fill);
        canvas.draw_rect(rr, &p);
    }
    if let Some(ref stroke) = paint.stroke {
        let mut p = SkPaint::new(to_sk_color(stroke.color), None);
        p.set_style(PaintStyle::Stroke);
        p.set_stroke_width(stroke.width);
        p.set_alpha_f(combined_alpha(stroke.color, paint.opacity));
        crate::raster_sampling::configure_geometry(&mut p);
        canvas.draw_rect(rr, &p);
    }
}

fn draw_rrect(canvas: &skia_safe::Canvas, r: Rect, radii: BorderRadii, paint: &Paint) {
    let sk_rect = to_sk_rect(r);
    let radii_arr = [
        SkPoint::new(radii.tl, radii.tl),
        SkPoint::new(radii.tr, radii.tr),
        SkPoint::new(radii.br, radii.br),
        SkPoint::new(radii.bl, radii.bl),
    ];
    let rrect = RRect::new_rect_radii(sk_rect, &radii_arr);
    if let Some(fill) = paint.fill {
        let mut p = SkPaint::new(to_sk_color(fill), None);
        p.set_alpha_f(combined_alpha(fill, paint.opacity));
        crate::raster_sampling::configure_geometry(&mut p);
        p.set_style(PaintStyle::Fill);
        canvas.draw_rrect(rrect, &p);
    }
    if let Some(ref stroke) = paint.stroke {
        let mut p = SkPaint::new(to_sk_color(stroke.color), None);
        p.set_style(PaintStyle::Stroke);
        p.set_stroke_width(stroke.width);
        p.set_alpha_f(combined_alpha(stroke.color, paint.opacity));
        crate::raster_sampling::configure_geometry(&mut p);
        canvas.draw_rrect(rrect, &p);
    }
}

fn combined_alpha(color: jian_core::scene::Color, opacity: f32) -> f32 {
    (f32::from(color.a()) / 255.0) * opacity.clamp(0.0, 1.0)
}

// Convenience: keeps `BlurStyle` / `MaskFilter` imported for future
// stroke-expansion work without triggering an unused-import warning.
#[allow(dead_code)]
fn _unused_keeping_imports() {
    let _ = BlurStyle::Normal;
    let _ = MaskFilter::blur(BlurStyle::Normal, 1.0, None);
}

#[cfg(test)]
#[path = "backend/tests.rs"]
mod tests;
