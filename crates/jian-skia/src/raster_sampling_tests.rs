use super::*;
use skia_safe::{AlphaType, Color4f, ColorType, ImageInfo, Rect};

fn quantize(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round_ties_even() as u8
}

#[test]
fn correlated_source_writes_match_float32_rgba8_blending() {
    let mut seed = 12345_u32;
    let mut differing_native = 0;
    with_point_sampling(|| {
        for i in 0..1024 {
            let mut next = || {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                (seed >> 24) as u8
            };
            let alpha = if i < 256 { i as u8 } else { next() };
            let fg = [
                f32::from(next()) / 255.0,
                f32::from(next()) / 255.0,
                f32::from(next()) / 255.0,
                f32::from(alpha) / 255.0,
            ];
            let ba = if i % 3 == 0 { 255 } else { next() };
            let bg = [next().min(ba), next().min(ba), next().min(ba), ba];
            let info = ImageInfo::new((1, 1), ColorType::RGBA8888, AlphaType::Premul, None);
            let mut actual = bg;
            let mut native = bg;
            for (bytes, point) in [(&mut actual, true), (&mut native, false)] {
                let mut surface = skia_safe::surfaces::wrap_pixels(&info, bytes, 4, None).unwrap();
                let mut paint = Paint::new(Color4f::new(fg[0], fg[1], fg[2], fg[3]), None);
                if point {
                    configure_paint(&mut paint);
                }
                surface.canvas().draw_rect(Rect::from_wh(1.0, 1.0), &paint);
            }
            differing_native += usize::from(actual != native);
            let expected = std::array::from_fn::<_, 4, _>(|channel| {
                let source = if channel == 3 {
                    fg[3]
                } else {
                    fg[channel] * fg[3]
                };
                let backdrop = (u32::from(bg[channel]) * 257) as f32 * (1.0_f32 / 65535.0);
                quantize(source + backdrop * (1.0 - fg[3]))
            });
            assert_eq!(actual, expected, "case{i}");
        }
    })
    .unwrap();
    assert!(
        differing_native > 500,
        "the integer source-over path is a meaningful control"
    );
    assert!(!is_point_sampling());
}

#[test]
fn correlated_scope_restores_after_panic_and_reports_unsupported() {
    let result = std::panic::catch_unwind(|| {
        let _ = with_point_sampling(|| {
            assert!(is_point_sampling());
            panic!("test unwind");
        });
    });
    assert!(result.is_err());
    assert!(!is_point_sampling());
    assert_eq!(
        with_point_sampling(|| mark_unsupported("test feature")),
        Err(PointSamplingError::Unsupported {
            feature: "test feature"
        })
    );
    assert!(!is_point_sampling());
    assert!(with_point_sampling(|| ()).is_ok());
}

#[test]
fn correlated_mode_cache_is_finite_and_all_standard_blends_are_available() {
    with_point_sampling(|| {
        for mode in MODES {
            let mut paint = Paint::default();
            set_blend_mode(&mut paint, *mode);
            assert!(paint.as_blend_mode().is_none());
            assert!(!paint.is_anti_alias());
        }
    })
    .unwrap();
    BLENDERS.with(|cache| assert_eq!(cache.as_ref().unwrap().len(), 29));
    let mut legacy = Paint::default();
    configure_geometry(&mut legacy);
    assert!(legacy.is_anti_alias());
    assert_eq!(legacy.as_blend_mode(), Some(BlendMode::SrcOver));
}
