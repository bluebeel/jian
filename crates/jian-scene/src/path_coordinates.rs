//! Explicit document-path coordinates; absent metadata keeps legacy tight fit.
use jian_widgets::{Point2D, Rect};
#[derive(Clone, Debug, PartialEq)]
pub struct ScenePathGeometry {
    pub view_box: [f32; 4],
    pub local_bounds: Option<Rect>,
}
impl ScenePathGeometry {
    pub fn new(view_box: [f64; 4], local_bounds: Option<(f32, f32, f32, f32)>) -> Option<Self> {
        let view_box = view_box.map(|v| v as f32);
        if !view_box.iter().all(|v| v.is_finite()) || view_box[2] <= 0.0 || view_box[3] <= 0.0 {
            return None;
        }
        Some(Self {
            view_box,
            local_bounds: local_bounds.map(|(x, y, w, h)| Rect::xywh(x, y, w, h)),
        })
    }
    pub fn map_bounds(&self, destination: Rect) -> Option<Rect> {
        let b = self.local_bounds?;
        let [x, y, w, h] = self.view_box;
        let sx = destination.size.x / w;
        let sy = destination.size.y / h;
        let origin = Point2D::new(
            destination.origin.x + (b.origin.x - x) * sx,
            destination.origin.y + (b.origin.y - y) * sy,
        );
        let size = Point2D::new(b.size.x * sx, b.size.y * sy);
        [origin.x, origin.y, size.x, size.y]
            .iter()
            .all(|v| v.is_finite())
            .then_some(Rect { origin, size })
    }
}
