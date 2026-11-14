//! Shape shadow inputs, before silhouette spread or displacement is applied.

use crate::{Color, Point2D, Rect};
pub use jian_core::render::shadow::ShadowBlurKernel;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoundRectShadowOptions {
    pub offset: Point2D,
    pub blur: f32,
    pub spread: f32,
    pub color: Color,
    pub kernel: ShadowBlurKernel,
    pub inner: bool,
}

impl RoundRectShadowOptions {
    pub fn silhouette_per_corner(&self, rect: Rect, radii: [f32; 4]) -> Option<(Rect, [f32; 4])> {
        let radii = Self::fitted_radii(rect, radii)?;
        let (shape, _) = self.silhouette(rect, 0.0)?;
        let growth = if self.inner {
            -self.spread
        } else {
            self.spread
        };
        let grown = radii.map(|r| {
            if growth == 0.0 || r <= 0.0 {
                r
            } else {
                (r + growth).max(0.0)
            }
        });
        Some((shape, Self::fitted_radii(shape, grown)?))
    }
    /// Proportionally fit circular corners to all four edges, as an RRect.
    pub fn fitted_radii(rect: Rect, radii: [f32; 4]) -> Option<[f32; 4]> {
        jian_core::render::shadow::fitted_corner_radii([rect.size.x, rect.size.y], radii)
    }
    /// Keep square corners square; spread otherwise shifts and clamps radii.
    pub fn spread_radius(radius: f32, spread: f32, shape: Rect) -> f32 {
        if spread == 0.0 || radius <= 0.0 {
            radius
        } else {
            (radius + spread)
                .max(0.0)
                .min(shape.size.x.min(shape.size.y) * 0.5)
        }
    }

    /// Shape before blur. Insets expand the complementary silhouette when
    /// spread is negative; an empty complement means a fully shaded mask.
    pub fn silhouette(&self, rect: Rect, radius: f32) -> Option<(Rect, f32)> {
        let spread = if self.inner {
            -self.spread
        } else {
            self.spread
        };
        if !spread.is_finite() {
            return None;
        }
        let mut shape = if spread == 0.0 {
            rect
        } else {
            Rect::xywh(
                rect.origin.x - spread,
                rect.origin.y - spread,
                rect.size.x + spread * 2.0,
                rect.size.y + spread * 2.0,
            )
        };
        if shape.size.x <= 0.0 || shape.size.y <= 0.0 {
            return None;
        }
        let radius = Self::spread_radius(radius, spread, shape);
        shape.origin += self.offset;
        [
            shape.origin.x,
            shape.origin.y,
            shape.size.x,
            shape.size.y,
            radius,
        ]
        .iter()
        .all(|value| value.is_finite())
        .then_some((shape, radius))
    }
}
