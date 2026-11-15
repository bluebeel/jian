//! Conservative immutable-scene broad phase for repeated headless point queries.
//! The existing precise walker remains authoritative for z-order and hit policy.
use crate::{
    layout_scene::{LayoutScene, NodeKind, SceneNode},
    layout_scene_hit::rotation_pivot,
};
use jian_widgets::geometry::{Point2D, Rect};
use std::{
    collections::HashMap,
    mem::size_of,
    sync::{Arc, Weak},
};

#[derive(Clone, Copy)]
struct Bounds {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}
impl Bounds {
    const UNBOUNDED: Self = Self {
        x0: f64::NEG_INFINITY,
        y0: f64::NEG_INFINITY,
        x1: f64::INFINITY,
        y1: f64::INFINITY,
    };
    fn rect(rect: Rect) -> Self {
        let x = f64::from(rect.origin.x);
        let y = f64::from(rect.origin.y);
        let x1 = x + f64::from(rect.size.x);
        let y1 = y + f64::from(rect.size.y);
        if ![x, y, x1, y1].iter().all(|v| v.is_finite()) {
            return Self::UNBOUNDED;
        }
        Self {
            x0: x.min(x1),
            y0: y.min(y1),
            x1: x.max(x1),
            y1: y.max(y1),
        }
    }
    fn union(&mut self, other: Self) {
        self.x0 = self.x0.min(other.x0);
        self.y0 = self.y0.min(other.y0);
        self.x1 = self.x1.max(other.x1);
        self.y1 = self.y1.max(other.y1);
    }
    fn point(&mut self, point: Point2D) {
        self.union(Self::rect(Rect::xywh(point.x, point.y, 0.0, 0.0)));
    }
    fn inflate(&mut self, amount: f64) {
        self.x0 -= amount;
        self.y0 -= amount;
        self.x1 += amount;
        self.y1 += amount;
    }
    fn transformed(self, node: &SceneNode, aggregate: Rect) -> Self {
        if ![self.x0, self.y0, self.x1, self.y1]
            .iter()
            .all(|v| v.is_finite())
        {
            return Self::UNBOUNDED;
        }
        let mut result = self;
        if let Some(pivot) = rotation_pivot(node, aggregate) {
            let px = f64::from(pivot.x);
            let py = f64::from(pivot.y);
            let rotation = if node.rotation.abs() > f32::EPSILON {
                // Use the walker's f32 trig values; retain a rounding envelope
                // for its inverse transforms at large document coordinates.
                (
                    f64::from(node.rotation.cos()),
                    f64::from(node.rotation.sin()),
                )
            } else {
                (1.0, 0.0)
            };
            if ![px, py, rotation.0, rotation.1]
                .iter()
                .all(|v| v.is_finite())
            {
                return Self::UNBOUNDED;
            }
            result = Self {
                x0: f64::INFINITY,
                y0: f64::INFINITY,
                x1: f64::NEG_INFINITY,
                y1: f64::NEG_INFINITY,
            };
            for x in [self.x0, self.x1] {
                for y in [self.y0, self.y1] {
                    let dx = (x - px) * if node.flip_x { -1.0 } else { 1.0 };
                    let dy = (y - py) * if node.flip_y { -1.0 } else { 1.0 };
                    let x = px + dx * rotation.0 - dy * rotation.1;
                    let y = py + dx * rotation.1 + dy * rotation.0;
                    result.union(Self {
                        x0: x,
                        y0: y,
                        x1: x,
                        y1: y,
                    });
                }
            }
        }
        let magnitude = [
            self.x0, self.y0, self.x1, self.y1, result.x0, result.y0, result.x1, result.y1,
        ]
        .into_iter()
        .fold(1.0_f64, |a, v| a.max(v.abs()));
        result.inflate(magnitude * f64::from(f32::EPSILON) * 8.0);
        result
    }
}

pub struct SceneHitIndex {
    owner: Weak<LayoutScene>,
    bounds: HashMap<usize, Bounds>,
}
impl SceneHitIndex {
    pub fn new(scene: &Arc<LayoutScene>) -> Self {
        let mut bounds = HashMap::new();
        if let Some(page) = scene.active_page() {
            for child in &page.children {
                capture(child, &mut bounds);
            }
        }
        Self {
            owner: Arc::downgrade(scene),
            bounds,
        }
    }
    /// Allocation identity is verified before consulting address-keyed bounds.
    /// Weak ownership prevents reuse, without retaining another complete scene.
    pub fn node_path_at_doc_point(
        &self,
        scene: &Arc<LayoutScene>,
        point: Point2D,
        zoom: f32,
    ) -> Option<Vec<String>> {
        if !Weak::ptr_eq(&self.owner, &Arc::downgrade(scene)) {
            return scene.node_path_at_doc_point(point, zoom);
        }
        scene.node_path_at_doc_point_impl(point, zoom, false, Some(self))
    }
    pub fn storage_bytes(&self) -> usize {
        size_of::<Self>() + self.bounds.capacity() * (size_of::<(usize, Bounds)>() + 16)
    }
    pub(crate) fn may_hit(&self, node: &SceneNode, point: Point2D, zoom: f32) -> bool {
        let Some(bounds) = self.bounds.get(&(node as *const SceneNode as usize)) else {
            return true;
        };
        let zoom = zoom.max(0.0001);
        let slack = (4.0 / f64::from(zoom)).max(f64::from(4.0_f32 / zoom));
        let x = f64::from(point.x);
        let y = f64::from(point.y);
        x >= bounds.x0 - slack
            && x <= bounds.x1 + slack
            && y >= bounds.y0 - slack
            && y <= bounds.y1 + slack
    }
}

fn capture(node: &SceneNode, index: &mut HashMap<usize, Bounds>) -> Bounds {
    let aggregate = node.aggregate_bounds();
    let mut bounds = Bounds::rect(aggregate);
    bounds.union(Bounds::rect(node.bounds));
    if let Some(geometry) = &node.svg_view_box {
        if let Some(ink) = geometry.map_bounds(node.bounds) {
            bounds.union(Bounds::rect(ink));
        } else {
            bounds = Bounds::UNBOUNDED;
        }
    }
    if matches!(node.kind, NodeKind::Path) {
        if node.svg_path.is_some() && (node.bounds.size.x <= 0.0 || node.bounds.size.y <= 0.0) {
            // Degenerate SVG paths translate without fitting; their native
            // outline can extend beyond authored bounds. Keep exact fallback.
            bounds = Bounds::UNBOUNDED;
        } else if node.svg_path.is_some() {
            // Paint leaves near-zero native spans unscaled when fitting.
            bounds.inflate(0.01);
        } else {
            for point in &node.points {
                bounds.point(*point);
            }
            // A Bezier lies inside its control hull, including every sampled
            // segment used by the precise legacy anchor picker.
            for anchor in &node.path_anchors {
                bounds.point(anchor.pos);
                if let Some(point) = anchor.handle_in {
                    bounds.point(point);
                }
                if let Some(point) = anchor.handle_out {
                    bounds.point(point);
                }
            }
        }
    }
    if matches!(node.kind, NodeKind::Path | NodeKind::Line) {
        bounds.inflate(
            node.stroke
                .map_or(1.0, |s| f64::from(s.width) * 0.5)
                .max(0.0),
        );
    }
    for child in &node.children {
        bounds.union(capture(child, index));
    }
    let bounds = bounds.transformed(node, aggregate);
    index.insert(node as *const SceneNode as usize, bounds);
    bounds
}

/// The exact top-level selection AABBs used by marquee, prepared without the
/// per-descendant point-picking index. Same immutable scene identity rule.
pub struct SceneMarqueeIndex {
    owner: Weak<LayoutScene>,
    bounds: Vec<Option<[f64; 4]>>,
}
impl SceneMarqueeIndex {
    pub fn new(scene: &Arc<LayoutScene>) -> Self {
        let bounds = scene.active_page().map_or_else(Vec::new, |page| {
            page.children
                .iter()
                .map(|node| {
                    if node.hidden || node.locked {
                        None
                    } else {
                        crate::layout_scene_hit::selection_aabb(node)
                    }
                })
                .collect()
        });
        Self {
            owner: Arc::downgrade(scene),
            bounds,
        }
    }
    pub fn storage_bytes(&self) -> usize {
        size_of::<Self>() + self.bounds.capacity() * size_of::<Option<[f64; 4]>>()
    }
    pub fn nodes_intersecting_doc_rect(&self, scene: &Arc<LayoutScene>, rect: Rect) -> Vec<String> {
        if !Weak::ptr_eq(&self.owner, &Arc::downgrade(scene)) {
            return scene.nodes_intersecting_doc_rect(rect);
        }
        let Some(page) = scene.active_page() else {
            return Vec::new();
        };
        let nx = rect.origin.x.min(rect.origin.x + rect.size.x);
        let ny = rect.origin.y.min(rect.origin.y + rect.size.y);
        let nw = rect.size.x.abs();
        let nh = rect.size.y.abs();
        page.children
            .iter()
            .zip(&self.bounds)
            .filter_map(|(node, bounds)| {
                let [bx, by, right, bottom] = (*bounds)?;
                if right < f64::from(nx)
                    || f64::from(nx) + f64::from(nw) < bx
                    || bottom < f64::from(ny)
                    || f64::from(ny) + f64::from(nh) < by
                {
                    None
                } else {
                    Some(node.id.clone())
                }
            })
            .collect()
    }
}
