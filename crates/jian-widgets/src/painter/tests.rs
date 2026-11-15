use super::*;
use crate::test_support::{CapturePainter, PaintOp};

#[test]
fn fill_svg_path_in_rect_forwards_destination_extent_to_scalar_fallback() {
    let mut p = CapturePainter::default();
    let rect = Rect::xywh(2.0, 3.0, 12.0, 18.0);

    p.fill_svg_path_in_rect("M0 0h1v1z", rect, Color::RED);

    assert!(matches!(
        p.ops.as_slice(),
        [PaintOp::FillSvgPath {
            top_left,
            size,
            viewbox,
            color,
            ..
        }] if *top_left == rect.origin
            && *size == rect.size.y
            && *viewbox == 1.0
            && *color == Color::RED
    ));
}

#[test]
fn fill_rule_variant_defaults_to_existing_path_fill() {
    let mut p = CapturePainter::default();
    let rect = Rect::xywh(2.0, 3.0, 12.0, 18.0);

    p.fill_svg_path_in_rect_with_fill_rule("M0 0h1v1z", rect, Color::RED, true);

    assert!(matches!(p.ops.as_slice(), [PaintOp::FillSvgPath { .. }]));
}

#[test]
fn image_thumb_hook_defaults_to_a_no_op() {
    let mut p = CapturePainter::default();

    p.draw_image_thumb(Rect::xywh(0.0, 0.0, 20.0, 10.0), 7, b"jpeg");

    assert!(p.ops.is_empty());
}
