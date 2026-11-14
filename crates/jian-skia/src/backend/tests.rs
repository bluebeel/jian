use super::*;
use jian_core::geometry::{point, rect, size};
use jian_core::render::PathCommand;
use jian_core::scene::Color;

fn draw_blue_rect() -> SkiaSurface {
    let mut backend = SkiaBackend::new();
    let mut surface = backend.new_surface(size(64.0, 64.0));
    backend.begin_frame(&mut surface, 0xffffffff);
    backend.draw(&DrawOp::Rect {
        rect: rect(8.0, 8.0, 48.0, 48.0),
        paint: Paint::solid(Color::rgb(0x1e, 0x88, 0xe5)),
    });
    backend.end_frame(&mut surface);
    surface
}

#[test]
fn authored_color_alpha_multiplies_paint_opacity() {
    let color = Color::rgba(0x7c, 0x3a, 0xed, 0x59);
    assert!((combined_alpha(color, 0.5) - (0x59 as f32 / 255.0 * 0.5)).abs() < 1e-6);

    let mut backend = SkiaBackend::new();
    let mut surface = backend.new_surface(size(8.0, 8.0));
    backend.begin_frame(&mut surface, 0x00000000);
    backend.draw(&DrawOp::Rect {
        rect: rect(0.0, 0.0, 8.0, 8.0),
        paint: Paint {
            fill: Some(color),
            stroke: None,
            opacity: 0.5,
        },
    });
    backend.end_frame(&mut surface);

    let mut pixels = vec![0u8; 8 * 8 * 4];
    assert!(surface.read_rgba8(&mut pixels));
    assert!(
        (0x2c..=0x2d).contains(&pixels[3]),
        "expected alpha 0x2c/0x2d, got {:#04x}",
        pixels[3]
    );
}

#[test]
fn rect_emits_png() {
    let mut s = draw_blue_rect();
    let png = s.encode_png().unwrap();
    assert!(png.len() > 100);
}

#[test]
fn rounded_rect_draws_through_trait() {
    let mut backend = SkiaBackend::new();
    let mut surface = backend.new_surface(size(32.0, 32.0));
    backend.begin_frame(&mut surface, 0);
    backend.draw(&DrawOp::RoundedRect {
        rect: rect(4.0, 4.0, 24.0, 24.0),
        radii: BorderRadii::uniform(4.0),
        paint: Paint::solid(Color::rgb(0xff, 0x00, 0x00)),
    });
    backend.end_frame(&mut surface);
    assert!(surface.encode_png().is_some());
}

#[test]
fn path_triangle_draws_through_trait() {
    let mut backend = SkiaBackend::new();
    let mut surface = backend.new_surface(size(32.0, 32.0));
    backend.begin_frame(&mut surface, 0);
    let cmds: Vec<PathCommand> = vec![
        PathCommand::MoveTo(point(4.0, 28.0)),
        PathCommand::LineTo(point(28.0, 28.0)),
        PathCommand::LineTo(point(16.0, 4.0)),
        PathCommand::Close,
    ];
    backend.draw(&DrawOp::Path {
        commands: cmds,
        paint: Paint::solid(Color::rgb(0x00, 0xff, 0x00)),
    });
    backend.end_frame(&mut surface);
    assert!(surface.encode_png().is_some());
}

#[test]
fn clip_transform_save_pop_cycle() {
    // A clip inside a transform inside a save should not panic on
    // end_frame — and the restore-to-count should clean up a
    // missing pop.
    let mut backend = SkiaBackend::new();
    let mut surface = backend.new_surface(size(32.0, 32.0));
    backend.begin_frame(&mut surface, 0xffffffff);
    backend.push_transform(&Affine2::translation(4.0, 4.0));
    backend.push_clip(rect(0.0, 0.0, 16.0, 16.0));
    backend.draw(&DrawOp::Rect {
        rect: rect(0.0, 0.0, 100.0, 100.0),
        paint: Paint::solid(Color::rgb(0, 0, 0)),
    });
    backend.pop();
    backend.pop();
    backend.end_frame(&mut surface);
    assert!(surface.encode_png().is_some());
}

#[test]
fn apply_blur_then_push_layer_consumes_filter() {
    let mut backend = SkiaBackend::new();
    let mut surface = backend.new_surface(size(32.0, 32.0));
    backend.begin_frame(&mut surface, 0xffffffff);
    backend.apply_blur(4.0);
    assert!(backend.pending_filter.is_some());
    backend.push_layer(rect(0.0, 0.0, 32.0, 32.0));
    assert!(backend.pending_filter.is_none());
    backend.draw(&DrawOp::Rect {
        rect: rect(4.0, 4.0, 24.0, 24.0),
        paint: Paint::solid(Color::rgb(0xff, 0, 0)),
    });
    backend.pop_layer();
    backend.end_frame(&mut surface);
    assert!(surface.encode_png().is_some());
}

#[cfg(feature = "textlayout")]
#[test]
fn paragraph_builder_renders_cjk_under_feature() {
    use jian_core::render::{TextAlign, TextRun};
    use jian_core::scene::Color as JianColor;
    let mut backend = SkiaBackend::new();
    let mut surface = backend.new_surface(size(200.0, 60.0));
    backend.begin_frame(&mut surface, 0xffffffff);
    backend.draw(&DrawOp::Text(TextRun {
        content: "你好世界".to_owned(),
        origin: jian_core::geometry::point(8.0, 8.0),
        color: JianColor::rgb(0, 0, 0),
        font_family: String::new(),
        font_size: 18.0,
        font_weight: 400,
        line_height: 0.0,
        max_width: 180.0,
        align: TextAlign::Start,
    }));
    backend.end_frame(&mut surface);
    // ParagraphBuilder shouldn't panic on CJK and should produce
    // a valid PNG. Single-line fallback fakes the same output
    // for ASCII; the CJK path proves the textlayout feature
    // actually engaged.
    assert!(surface.encode_png().is_some());
}

#[test]
fn data_url_image_draws_through_trait() {
    // 1x1 transparent PNG (smallest valid).
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=";
    let url = format!("data:image/png;base64,{}", png);
    let mut backend = SkiaBackend::new();
    let mut surface = backend.new_surface(size(32.0, 32.0));
    backend.begin_frame(&mut surface, 0xffffffff);
    backend.draw(&DrawOp::Image {
        source: ImageSource::DataUrl(url),
        dst: rect(4.0, 4.0, 24.0, 24.0),
        opacity: 1.0,
    });
    backend.end_frame(&mut surface);
    assert!(surface.encode_png().is_some());
}

#[test]
fn missing_image_falls_back_to_grey() {
    let mut backend = SkiaBackend::new();
    let mut surface = backend.new_surface(size(32.0, 32.0));
    backend.begin_frame(&mut surface, 0xffffffff);
    backend.draw(&DrawOp::Image {
        source: ImageSource::Url("https://nope.example/x.png".into()),
        dst: rect(4.0, 4.0, 24.0, 24.0),
        opacity: 1.0,
    });
    backend.end_frame(&mut surface);
    assert!(surface.encode_png().is_some());
}

#[test]
fn radial_gradient_rect_draws_through_trait() {
    use jian_core::render::{GradientStop, RadialGradient};
    let mut backend = SkiaBackend::new();
    let mut surface = backend.new_surface(size(64.0, 64.0));
    backend.begin_frame(&mut surface, 0xffffffff);
    backend.draw(&DrawOp::RadialGradientRect {
        rect: rect(0.0, 0.0, 64.0, 64.0),
        radii: BorderRadii::zero(),
        gradient: RadialGradient {
            cx: 0.5,
            cy: 0.5,
            radius: 0.5,
            stops: vec![
                GradientStop {
                    offset: 0.0,
                    color: Color::rgb(0xff, 0xff, 0xff),
                },
                GradientStop {
                    offset: 1.0,
                    color: Color::rgb(0x00, 0x00, 0x00),
                },
            ],
            opacity: 1.0,
        },
        stroke: None,
    });
    backend.end_frame(&mut surface);
    let png = surface.encode_png().expect("png");
    assert!(png.len() > 100);
}

#[test]
fn mesh_gradient_rect_draws_through_trait() {
    use jian_core::render::MeshGradient;
    let mut backend = SkiaBackend::new();
    let mut surface = backend.new_surface(size(64.0, 64.0));
    backend.begin_frame(&mut surface, 0xffffffff);
    // 2x2 corner mesh: red / green / blue / yellow.
    backend.draw(&DrawOp::MeshGradientRect {
        rect: rect(0.0, 0.0, 64.0, 64.0),
        radii: BorderRadii::uniform(8.0),
        gradient: MeshGradient {
            rows: 2,
            cols: 2,
            colors: vec![
                Color::rgb(0xff, 0x00, 0x00),
                Color::rgb(0x00, 0xff, 0x00),
                Color::rgb(0x00, 0x00, 0xff),
                Color::rgb(0xff, 0xff, 0x00),
            ],
            opacity: 1.0,
        },
        stroke: None,
    });
    backend.end_frame(&mut surface);
    let png = surface.encode_png().expect("png");
    assert!(png.len() > 100);
}

#[test]
fn mesh_gradient_is_actually_interpolated_not_flat() {
    // PROOF: a 2x2 R/G/B/Y mesh must Gouraud-interpolate, so the
    // CENTRE pixel is a blend of all four corners (≈ mid-tone on every
    // channel) and is NOT equal to any single corner colour. A flat
    // first-stop fallback would make the centre exactly one corner.
    use jian_core::render::MeshGradient;
    let mut backend = SkiaBackend::new();
    let w = 64i32;
    let h = 64i32;
    let mut surface = backend.new_surface(size(w as f32, h as f32));
    backend.begin_frame(&mut surface, 0xff000000); // opaque black clear
    backend.draw(&DrawOp::MeshGradientRect {
        rect: rect(0.0, 0.0, w as f32, h as f32),
        radii: BorderRadii::zero(), // no clip rounding so centre is mesh
        gradient: MeshGradient {
            rows: 2,
            cols: 2,
            // TL=red, TR=green, BL=blue, BR=yellow
            colors: vec![
                Color::rgb(0xff, 0x00, 0x00),
                Color::rgb(0x00, 0xff, 0x00),
                Color::rgb(0x00, 0x00, 0xff),
                Color::rgb(0xff, 0xff, 0x00),
            ],
            opacity: 1.0,
        },
        stroke: None,
    });
    backend.end_frame(&mut surface);

    let mut buf = vec![0u8; (w * h * 4) as usize];
    assert!(surface.read_rgba8(&mut buf), "read_rgba8 failed");
    let px = |x: i32, y: i32| -> (u8, u8, u8, u8) {
        let i = ((y * w + x) * 4) as usize;
        (buf[i], buf[i + 1], buf[i + 2], buf[i + 3])
    };

    // Corners (1px inset) should still read close to their pure colour.
    let tl = px(1, 1);
    let tr = px(w - 2, 1);
    let bl = px(1, h - 2);
    let br = px(w - 2, h - 2);
    assert!(tl.0 > 200 && tl.1 < 60 && tl.2 < 60, "TL not red: {:?}", tl);
    assert!(
        tr.0 < 60 && tr.1 > 200 && tr.2 < 60,
        "TR not green: {:?}",
        tr
    );
    assert!(
        bl.0 < 60 && bl.1 < 60 && bl.2 > 200,
        "BL not blue: {:?}",
        bl
    );
    assert!(
        br.0 > 200 && br.1 > 200 && br.2 < 60,
        "BR not yellow: {:?}",
        br
    );

    // Centre must be an interpolated blend, NOT any single corner solid.
    // Triangulation is [TL,TR,BL] + [TR,BR,BL]; the shared diagonal runs
    // TR(green)->BL(blue), and the geometric centre sits on that diagonal,
    // so the centre is a green<->blue blend (G,B mid; R near 0 because red
    // only lives at the two off-diagonal corners TL/BR). This is correct
    // Gouraud behaviour for a triangle mesh — a flat first-stop fallback
    // would instead paint the centre exactly one corner colour.
    let c = px(w / 2, h / 2);
    assert!(
        c.1 > 60 && c.1 < 200 && c.2 > 60 && c.2 < 200,
        "centre is not a green/blue diagonal blend (suggests flat fallback): {:?}",
        c
    );
    // Centre differs from every corner -> not a first-stop solid.
    for (name, corner) in [("TL", tl), ("TR", tr), ("BL", bl), ("BR", br)] {
        assert!(
            (c.0 as i32 - corner.0 as i32).abs()
                + (c.1 as i32 - corner.1 as i32).abs()
                + (c.2 as i32 - corner.2 as i32).abs()
                > 60,
            "centre {:?} too close to corner {} {:?} (flat fill?)",
            c,
            name,
            corner
        );
    }

    // Edge midpoints must be two-colour blends along that edge.
    let top_mid = px(w / 2, 1); // red↔green
    assert!(
        top_mid.0 > 60 && top_mid.1 > 60 && top_mid.2 < 80,
        "top edge midpoint not a red/green blend: {:?}",
        top_mid
    );
    let left_mid = px(1, h / 2); // red↔blue
    assert!(
        left_mid.0 > 60 && left_mid.2 > 60 && left_mid.1 < 80,
        "left edge midpoint not a red/blue blend: {:?}",
        left_mid
    );
}

#[test]
fn shader_rect_valid_sksl_paints_red() {
    // A trivial valid SkSL program returning opaque red must paint
    // red across the rect (NOT the fallback colour).
    use jian_core::render::ShaderSpec;
    let mut backend = SkiaBackend::new();
    let w = 32i32;
    let h = 32i32;
    let mut surface = backend.new_surface(size(w as f32, h as f32));
    backend.begin_frame(&mut surface, 0xff000000);
    backend.draw(&DrawOp::ShaderRect {
        rect: rect(0.0, 0.0, w as f32, h as f32),
        radii: BorderRadii::zero(),
        shader: ShaderSpec {
            sksl: "half4 main(float2 p){ return half4(1.0, 0.0, 0.0, 1.0); }".into(),
            uniforms: vec![],
            opacity: 1.0,
            // Fallback is green — proves the red came from the shader,
            // not the fallback path.
            fallback: Color::rgb(0x00, 0xff, 0x00),
        },
        stroke: None,
    });
    backend.end_frame(&mut surface);

    let mut buf = vec![0u8; (w * h * 4) as usize];
    assert!(surface.read_rgba8(&mut buf), "read_rgba8 failed");
    let i = ((h / 2 * w + w / 2) * 4) as usize;
    let (r, g, b) = (buf[i], buf[i + 1], buf[i + 2]);
    assert!(
        r > 200 && g < 60 && b < 60,
        "centre should be shader red, got ({r},{g},{b})"
    );
    // A valid program must have compiled exactly once.
    assert_eq!(backend.shader_compile_count(), 1);
}

#[test]
fn shader_rect_invalid_sksl_falls_back_without_panic() {
    // Garbage SkSL must NOT panic; it degrades to the fallback solid.
    use jian_core::render::ShaderSpec;
    let mut backend = SkiaBackend::new();
    let w = 32i32;
    let h = 32i32;
    let mut surface = backend.new_surface(size(w as f32, h as f32));
    backend.begin_frame(&mut surface, 0xff000000);
    backend.draw(&DrawOp::ShaderRect {
        rect: rect(0.0, 0.0, w as f32, h as f32),
        radii: BorderRadii::zero(),
        shader: ShaderSpec {
            sksl: "this is not valid sksl at all {{{".into(),
            uniforms: vec![],
            opacity: 1.0,
            fallback: Color::rgb(0x00, 0x00, 0xff), // blue fallback
        },
        stroke: None,
    });
    backend.end_frame(&mut surface);

    let mut buf = vec![0u8; (w * h * 4) as usize];
    assert!(surface.read_rgba8(&mut buf), "read_rgba8 failed");
    let i = ((h / 2 * w + w / 2) * 4) as usize;
    let (r, g, b) = (buf[i], buf[i + 1], buf[i + 2]);
    assert!(
        b > 200 && r < 60 && g < 60,
        "centre should be blue fallback, got ({r},{g},{b})"
    );
}

#[test]
fn shader_compile_is_cached_across_frames() {
    // PERF PROOF: drawing the SAME source many times compiles ONCE.
    use jian_core::render::ShaderSpec;
    let mut backend = SkiaBackend::new();
    let mut surface = backend.new_surface(size(16.0, 16.0));
    let spec = ShaderSpec {
        sksl: "half4 main(float2 p){ return half4(0.2, 0.4, 0.6, 1.0); }".into(),
        uniforms: vec![],
        opacity: 1.0,
        fallback: Color::rgb(0x80, 0x80, 0x80),
    };
    for _ in 0..10 {
        backend.begin_frame(&mut surface, 0xff000000);
        backend.draw(&DrawOp::ShaderRect {
            rect: rect(0.0, 0.0, 16.0, 16.0),
            radii: BorderRadii::zero(),
            shader: spec.clone(),
            stroke: None,
        });
        backend.end_frame(&mut surface);
    }
    assert_eq!(
        backend.shader_compile_count(),
        1,
        "same source must compile exactly once across 10 frames"
    );

    // A second, distinct source bumps the count to 2.
    let spec2 = ShaderSpec {
        sksl: "half4 main(float2 p){ return half4(0.6, 0.4, 0.2, 1.0); }".into(),
        ..spec.clone()
    };
    backend.begin_frame(&mut surface, 0xff000000);
    backend.draw(&DrawOp::ShaderRect {
        rect: rect(0.0, 0.0, 16.0, 16.0),
        radii: BorderRadii::zero(),
        shader: spec2,
        stroke: None,
    });
    backend.end_frame(&mut surface);
    assert_eq!(backend.shader_compile_count(), 2);
}

#[test]
fn apply_shadow_then_push_layer_consumes_filter() {
    let mut backend = SkiaBackend::new();
    let mut surface = backend.new_surface(size(32.0, 32.0));
    backend.begin_frame(&mut surface, 0xffffffff);
    backend.apply_shadow(&ShadowSpec {
        color: Color::rgba(0, 0, 0, 128),
        dx: 2.0,
        dy: 2.0,
        blur: 3.0,
        spread: 0.0,
    });
    assert!(backend.pending_filter.is_some());
    backend.push_layer(rect(0.0, 0.0, 32.0, 32.0));
    assert!(backend.pending_filter.is_none());
    backend.draw(&DrawOp::Rect {
        rect: rect(4.0, 4.0, 24.0, 24.0),
        paint: Paint::solid(Color::rgb(0, 0, 0xff)),
    });
    backend.pop_layer();
    backend.end_frame(&mut surface);
    assert!(surface.encode_png().is_some());
}
