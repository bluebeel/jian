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

fn reference_source_over() -> Blender {
    let effect = RuntimeEffect::make_for_blender(&format!("{HELPERS}{GENERAL}"), None).unwrap();
    effect
        .make_blender(
            Data::new_copy(&[]),
            &[ChildPtr::Blender(Blender::mode(BlendMode::SrcOver))][..],
        )
        .unwrap()
}
fn painted(
    mut paint: Paint,
    reference: &Blender,
    fast: bool,
    byte_shader: Option<bool>,
) -> Vec<u8> {
    let mut surface = skia_safe::surfaces::raster_n32_premul((12, 12)).unwrap();
    surface
        .canvas()
        .clear(skia_safe::Color::from_argb(173, 27, 74, 133));
    if fast {
        if let Some(binary) = byte_shader {
            configure_final_byte_image(&mut paint, binary);
        } else {
            configure_final_src_over(&mut paint);
        }
    } else {
        paint.set_blender(reference.clone());
    }
    surface
        .canvas()
        .draw_rect(Rect::from_xywh(0.125, 0.375, 10.5, 10.25), &paint);
    surface
        .image_snapshot()
        .peek_pixels()
        .unwrap()
        .bytes()
        .unwrap()
        .to_vec()
}

#[test]
fn correlated_final_solids_keep_framebuffer_ties_and_mask_filter_behavior() {
    let reference = reference_source_over();
    with_point_sampling(|| {
        for value in 0..255 {
            for alpha in [0.0, 0.35, 1.0] {
                let paint = Paint::new(
                    Color4f::new((value as f32 + 0.5) / 255.0, 0.375, 0.8125, alpha),
                    None,
                );
                assert_eq!(
                    painted(paint.clone(), &reference, true, None),
                    painted(paint, &reference, false, None),
                    "byte{value},alpha{alpha}"
                );
            }
        }
        let mut filtered = Paint::new(Color4f::new(0.73125, 0.45625, 0.2175, 1.0), None);
        filtered.set_mask_filter(skia_safe::MaskFilter::blur(
            skia_safe::BlurStyle::Normal,
            1.25,
            false,
        ));
        assert_eq!(
            painted(filtered.clone(), &reference, true, None),
            painted(filtered, &reference, false, None)
        );
    })
    .unwrap();
}

#[test]
fn correlated_byte_image_fast_path_excludes_decal_filter_and_opacity_alpha() {
    let reference = reference_source_over();
    let info = ImageInfo::new((2, 2), ColorType::RGBA8888, AlphaType::Opaque, None);
    let image = skia_safe::images::raster_from_data(
        &info,
        Data::new_copy(&[44, 127, 206, 255].repeat(4)),
        8,
    )
    .unwrap();
    assert!(image.is_opaque());
    with_point_sampling(|| {
        for decal in [false, true] {
            for opacity in [0.35, 1.0] {
                for filter in [false, true] {
                    let mode = if decal {
                        skia_safe::TileMode::Decal
                    } else {
                        skia_safe::TileMode::Clamp
                    };
                    let sampling = if decal {
                        skia_safe::SamplingOptions::new(
                            skia_safe::FilterMode::Linear,
                            skia_safe::MipmapMode::None,
                        )
                    } else {
                        skia_safe::SamplingOptions::default()
                    };
                    let shader = image
                        .to_shader(
                            (mode, mode),
                            sampling,
                            &skia_safe::Matrix::scale((3.0, 3.0)),
                        )
                        .unwrap();
                    let mut paint = Paint::default();
                    paint.set_shader(shader);
                    paint.set_alpha_f(opacity);
                    if filter {
                        let mut matrix = [0.0_f32; 20];
                        matrix[0] = 1.0;
                        matrix[6] = 1.0;
                        matrix[12] = 1.0;
                        matrix[18] = 0.35;
                        paint.set_color_filter(skia_safe::color_filters::matrix_row_major(
                            &matrix, None,
                        ));
                    }
                    // AlphaTypeOpaque alone does not certify filtered Decal output.
                    let binary = !decal;
                    assert_eq!(
                        painted(paint.clone(), &reference, true, Some(binary)),
                        painted(paint, &reference, false, None),
                        "decal{decal},opacity{opacity},filter{filter}"
                    );
                }
            }
        }
    })
    .unwrap();
}
