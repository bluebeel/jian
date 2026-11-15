//! Canvas hit-test over a [`LayoutScene`].
//!
//! These are the input-path geometry queries that used to live as
//! `&Document`-bound helpers (`Document::node_at_doc_point`,
//! `Document::nodes_intersecting_doc_rect`). The reorg moves the
//! editor hosts' input dispatch off the derived `Document` snapshot
//! and onto the layout-resolved [`LayoutScene`], so the hit-test
//! reads the same resolved geometry the painter walks.
//!
//! Hit semantics carry over from `document/walkers.rs`:
//! top-most-first z-order (`children[0]` is frontmost), per-node rotation inverse-transform, the
//! tighter Ellipse / Polygon / Line geometry, the locked-node
//! body-opts-out-children-stay rule, and the hidden-subtree skip.
//!
//! One rule is newer: a Frame / Group that paints no body of its own is
//! not selectable across its empty area (see `paints_body`). Painted
//! containers are unchanged, and a top-level frame stays reachable
//! through its canvas name label, which hit-tests ahead of this walk.

use crate::layout_scene::NodeKind;
use crate::layout_scene::{regular_polygon_points, LayoutScene, SceneNode};
use crate::path_geometry::{flatten_path_points, PathPoints};
use crate::scene_hit_index::SceneHitIndex;
use jian_widgets::geometry::{Point2D, Rect};

impl LayoutScene {
    /// Topmost node id whose geometry contains `point` (doc space)
    /// on the active page. Walks children in front-to-back z-order
    /// (`children[0]` is frontmost). `None` on dead space.
    ///
    /// `zoom` is the live viewport zoom — it scales the Line stroke
    /// hit slack so a thin line stays clickable at any zoom.
    pub fn node_at_doc_point(&self, point: Point2D, zoom: f32) -> Option<String> {
        let zoom = zoom.max(0.0001);
        let page = self.active_page()?;
        for child in &page.children {
            if let Some(hit) = hit_test_walk(child, point, zoom) {
                return Some(hit);
            }
        }
        None
    }

    /// Root-to-deepest structural path for the same topmost hit returned by
    /// [`Self::node_at_doc_point`]. The last id is always the selectable hit;
    /// preceding ids are its scene ancestors in document order.
    ///
    /// Locked ancestors stay in the path when one of their descendants is
    /// hittable, while a locked node's own body is never the terminal hit.
    /// Hidden subtrees are omitted entirely. Rotation and flips are resolved
    /// at every ancestor exactly as in the single-id hit-test.
    pub fn node_path_at_doc_point(&self, point: Point2D, zoom: f32) -> Option<Vec<String>> {
        self.node_path_at_doc_point_impl(point, zoom, false, None)
    }

    /// Like [`Self::node_path_at_doc_point`], but an unpainted container body
    /// still terminates the path. Clicks skip an empty decoration shell so it
    /// cannot swallow the content behind it; an image dropped onto an empty
    /// placeholder box wants exactly that box, so the drop-target resolver
    /// opts back in here rather than falling through to a fresh insertion.
    pub fn node_path_at_doc_point_for_fill(
        &self,
        point: Point2D,
        zoom: f32,
    ) -> Option<Vec<String>> {
        self.node_path_at_doc_point_impl(point, zoom, true, None)
    }

    pub(crate) fn node_path_at_doc_point_impl(
        &self,
        point: Point2D,
        zoom: f32,
        include_empty_body: bool,
        index: Option<&SceneHitIndex>,
    ) -> Option<Vec<String>> {
        let zoom = zoom.max(0.0001);
        let page = self.active_page()?;
        let mut path = Vec::new();
        for child in &page.children {
            if hit_test_path_walk(child, point, zoom, &mut path, include_empty_body, index) {
                return Some(path.into_iter().map(str::to_owned).collect());
            }
            debug_assert!(path.is_empty());
        }
        None
    }

    /// Visible, unlocked top-level node ids whose rotated selection bounds
    /// intersect `rect` (doc space). Backs the marquee rect-select.
    /// Descends only into top-level children — same as the click
    /// hit-test, so the result set selects as a unit.
    pub fn nodes_intersecting_doc_rect(&self, rect: Rect) -> Vec<String> {
        let Some(page) = self.active_page() else {
            return Vec::new();
        };
        let nx = rect.origin.x.min(rect.origin.x + rect.size.x);
        let ny = rect.origin.y.min(rect.origin.y + rect.size.y);
        let nw = rect.size.x.abs();
        let nh = rect.size.y.abs();
        let mut out = Vec::new();
        for child in &page.children {
            if child.hidden || child.locked {
                continue;
            }
            let Some([bx, by, right, bottom]) = selection_aabb(child) else {
                continue;
            };
            // AABB intersection test.
            if right < f64::from(nx)
                || f64::from(nx) + f64::from(nw) < bx
                || bottom < f64::from(ny)
                || f64::from(ny) + f64::from(nh) < by
            {
                continue;
            }
            out.push(child.id.clone());
        }
        out
    }
}

pub(crate) fn selection_aabb(node: &SceneNode) -> Option<[f64; 4]> {
    let aggregate = node.aggregate_bounds();
    let bounds = if matches!(node.kind, NodeKind::Line) {
        node.bounds
    } else {
        aggregate
    };
    let (x, y) = (f64::from(bounds.origin.x), f64::from(bounds.origin.y));
    let (w, h) = (f64::from(bounds.size.x), f64::from(bounds.size.y));
    if ![x, y, w, h].iter().all(|v| v.is_finite()) || (w == 0.0 && h == 0.0) {
        return None;
    }
    let mut out = [x.min(x + w), y.min(y + h), x.max(x + w), y.max(y + h)];
    if node.rotation.abs() > f32::EPSILON {
        let pivot = rotation_pivot(node, aggregate)?;
        let (px, py) = (f64::from(pivot.x), f64::from(pivot.y));
        let (cos, sin) = (
            f64::from(node.rotation.cos()),
            f64::from(node.rotation.sin()),
        );
        out = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for cx in [x, x + w] {
            for cy in [y, y + h] {
                let dx = cx - px;
                let dy = cy - py;
                let rx = px + dx * cos - dy * sin;
                let ry = py + dx * sin + dy * cos;
                out[0] = out[0].min(rx);
                out[1] = out[1].min(ry);
                out[2] = out[2].max(rx);
                out[3] = out[3].max(ry);
            }
        }
    }
    out.iter().all(|v| v.is_finite()).then_some(out)
}

/// Recursive hit-test — returns the topmost id whose geometry
/// contains `point`. When a node carries rotation, the test point
/// is inverse-rotated about the node's pivot BEFORE testing
/// children + self, so the hit area matches the rendered geometry.
fn hit_test_walk(node: &SceneNode, point: Point2D, zoom: f32) -> Option<String> {
    // Hidden nodes skip hit-test entirely — the subtree inherits.
    if node.hidden {
        return None;
    }
    let bounds = node.aggregate_bounds();
    let local = point_in_node_space(node, point, bounds);
    // `visible_children` — not `children` — so hit-test and paint share one
    // rule for which of a tabs node's overlapping panels is live.
    for child in node.visible_children() {
        if let Some(hit) = hit_test_walk(child, local, zoom) {
            return Some(hit);
        }
    }
    // Locked nodes can't be selected via canvas hit, but their
    // children still can — this check runs AFTER the child walk so
    // descendants of a locked Frame remain hittable; only the
    // Frame's own body opts out.
    if node.locked {
        return None;
    }
    // Clicks never terminate on an unpainted container body.
    if point_in_node(node, local, bounds, zoom, false) {
        return Some(node.id.clone());
    }
    None
}

/// Path-preserving counterpart of [`hit_test_walk`]. `path` borrows ids while
/// probing so misses do not clone strings; the public API allocates only the
/// ancestors of the final hit.
fn hit_test_path_walk<'a>(
    node: &'a SceneNode,
    point: Point2D,
    zoom: f32,
    path: &mut Vec<&'a str>,
    include_empty_body: bool,
    index: Option<&SceneHitIndex>,
) -> bool {
    if node.hidden || index.is_some_and(|index| !index.may_hit(node, point, zoom)) {
        return false;
    }
    let bounds = node.aggregate_bounds();
    let local = point_in_node_space(node, point, bounds);
    path.push(node.id.as_str());
    // Same painted-subtree rule as `hit_test_walk`.
    for child in node.visible_children() {
        if hit_test_path_walk(child, local, zoom, path, include_empty_body, index) {
            return true;
        }
    }
    if !node.locked && point_in_node(node, local, bounds, zoom, include_empty_body) {
        return true;
    }
    path.pop();
    false
}

fn point_in_node_space(node: &SceneNode, point: Point2D, bounds: Rect) -> Point2D {
    let local = if node.rotation.abs() > f32::EPSILON {
        if let Some(pivot) = rotation_pivot(node, bounds) {
            let dx = point.x - pivot.x;
            let dy = point.y - pivot.y;
            let cos_t = (-node.rotation).cos();
            let sin_t = (-node.rotation).sin();
            Point2D::new(
                pivot.x + dx * cos_t - dy * sin_t,
                pivot.y + dx * sin_t + dy * cos_t,
            )
        } else {
            point
        }
    } else {
        point
    };
    if node.flip_x || node.flip_y {
        if let Some(pivot) = rotation_pivot(node, bounds) {
            Point2D::new(
                if node.flip_x {
                    2.0 * pivot.x - local.x
                } else {
                    local.x
                },
                if node.flip_y {
                    2.0 * pivot.y - local.y
                } else {
                    local.y
                },
            )
        } else {
            local
        }
    } else {
        local
    }
}

/// Rotation pivot for hit-test. Most kinds rotate around the
/// aggregate-bounds center; Lines need a kind-specific path because
/// a Line with a negative-size dimension collapses its aggregate to
/// `Rect::ZERO` (the segment midpoint is still well defined from
/// `bounds`). Returns `None` when no valid pivot exists.
pub(crate) fn rotation_pivot(node: &SceneNode, bounds: Rect) -> Option<Point2D> {
    if matches!(node.kind, NodeKind::Line) {
        let raw = node.bounds;
        if raw.size.x.abs() < f32::EPSILON && raw.size.y.abs() < f32::EPSILON {
            return None;
        }
        return Some(Point2D::new(
            raw.origin.x + raw.size.x / 2.0,
            raw.origin.y + raw.size.y / 2.0,
        ));
    }
    if bounds.size.x > 0.0 && bounds.size.y > 0.0 {
        return Some(Point2D::new(
            bounds.origin.x + bounds.size.x / 2.0,
            bounds.origin.y + bounds.size.y / 2.0,
        ));
    }
    None
}

/// Per-NodeKind hit-test. Frames / Groups / Rects / Text / Other
/// use the axis-aligned bounds; Ellipse / Polygon / Line use tighter
/// geometry so the click area matches what the painter draws.
fn point_in_node(
    node: &SceneNode,
    local: Point2D,
    bounds: Rect,
    zoom: f32,
    include_empty_body: bool,
) -> bool {
    // Lines get a dedicated path: horizontal / vertical segments
    // have a zero-dimension bounds rect, and negative-size bounds
    // collapse the aggregate to `Rect::ZERO`, so the Line path reads
    // `node.bounds` directly. The distance-to-segment helper is
    // sign-independent.
    if matches!(node.kind, NodeKind::Line) {
        let raw = node.bounds;
        if raw.size.x.abs() < f32::EPSILON && raw.size.y.abs() < f32::EPSILON {
            return false;
        }
        let from = raw.origin;
        let to = Point2D::new(raw.origin.x + raw.size.x, raw.origin.y + raw.size.y);
        let stroke_half = node.stroke.map(|s| s.width / 2.0).unwrap_or(1.0);
        // 4 screen px of slack regardless of zoom — the point is in
        // doc space, so scale by `1/zoom`.
        let screen_slack = 4.0 / zoom.max(0.0001);
        return distance_point_to_segment(local, from, to) <= stroke_half + screen_slack;
    }
    // Paths hit-test against their flattened (bezier-aware) outline,
    // not the bounding box — a curved / thin path otherwise selects
    // empty bbox space, and a zero-height stroked path (which fails
    // the positive-area gate below) stays clickable.
    if matches!(node.kind, NodeKind::Path) {
        // Imported and Boolean paths preserve SVG data rather than editable
        // anchors. Use the same fitted outline the painters consume.
        if node.svg_path.is_some() {
            return crate::svg_path_hit::contains(node, local, zoom);
        }
        let poly = path_hit_points(node);
        let points = poly.as_slice();
        if points.len() < 2 {
            return false;
        }
        let stroke_half = node.stroke.map(|s| s.width / 2.0).unwrap_or(1.0);
        let slack = 4.0 / zoom.max(0.0001);
        for seg in points.windows(2) {
            if distance_point_to_segment(local, seg[0], seg[1]) <= stroke_half + slack {
                return true;
            }
        }
        // A filled closed path is also hittable across its interior.
        return node.path_closed
            && (node.fill.is_some() || !node.fill_layers.is_empty())
            && point_in_path(local, points, node.even_odd_fill);
    }
    // A container that paints nothing does not claim its own body. This
    // runs after the child walk in both walkers, so descendants stay
    // hittable — only the empty body opts out, the same shape as the
    // locked-node rule. Without it a full-bleed transparent decoration
    // layer (the `layout: none` overlay idiom: sparse tape / punch-hole
    // art in a `fill_container` wrapper) is an invisible solid board that
    // swallows every click over the content behind it.
    if !include_empty_body
        && matches!(node.kind, NodeKind::Frame | NodeKind::Group)
        && !paints_body(node)
    {
        return false;
    }
    // Non-line kinds need real positive area on both axes.
    if bounds.size.x <= 0.0 || bounds.size.y <= 0.0 {
        return false;
    }
    let in_bounds = local.x >= bounds.origin.x
        && local.x <= bounds.origin.x + bounds.size.x
        && local.y >= bounds.origin.y
        && local.y <= bounds.origin.y + bounds.size.y;
    if !in_bounds {
        return false;
    }
    let cx = bounds.origin.x + bounds.size.x / 2.0;
    let cy = bounds.origin.y + bounds.size.y / 2.0;
    let rx = (bounds.size.x / 2.0).max(0.0001);
    let ry = (bounds.size.y / 2.0).max(0.0001);
    match node.kind {
        NodeKind::Ellipse => {
            let dx = (local.x - cx) / rx;
            let dy = (local.y - cy) / ry;
            let r2 = dx * dx + dy * dy;
            if r2 > 1.0 {
                return false;
            }
            // Arc ellipse: also require the point inside the pie /
            // donut sector so a missing wedge is not selectable.
            let inner = node.arc_inner_radius.unwrap_or(0.0).clamp(0.0, 1.0);
            let has_arc =
                node.arc_start_angle.is_some() || node.arc_sweep_angle.is_some() || inner > 0.001;
            if !has_arc {
                return true;
            }
            let sweep = node.arc_sweep_angle.unwrap_or(360.0);
            if sweep.abs() < 359.9 {
                let start = node.arc_start_angle.unwrap_or(0.0);
                // Normalise to a forward sweep — a negative sweep
                // covers the angular range `[start + sweep, start]`.
                let (sector_start, span) = if sweep < 0.0 {
                    (start + sweep, -sweep)
                } else {
                    (start, sweep)
                };
                let ang = dy.atan2(dx).to_degrees();
                let rel = (ang - sector_start).rem_euclid(360.0);
                if rel > span {
                    return false;
                }
            }
            r2 >= inner * inner
        }
        NodeKind::Polygon => {
            let points = regular_polygon_points(bounds, node.polygon_sides);
            point_in_polygon(local, &points)
        }
        // Frame, Group, Rect, Text, Other, Path — bounds-only hit.
        _ => true,
    }
}

/// Whether a container draws anything of its own. Children are not
/// consulted — a container whose only ink comes from its descendants
/// still has an empty body, and those descendants are hit-tested on
/// their own geometry.
fn paints_body(node: &SceneNode) -> bool {
    node.fill.is_some()
        || !node.fill_layers.is_empty()
        || node.stroke.is_some()
        || node.image_src.is_some()
        || !node.effects.is_empty()
}

fn path_hit_points(node: &SceneNode) -> PathPoints<'_> {
    flatten_path_points(node)
}

/// Even-odd ray-cast point-in-polygon test over a closed vertex ring.
fn point_in_polygon(p: Point2D, poly: &[Point2D]) -> bool {
    point_in_path(p, poly, true)
}

fn point_in_path(p: Point2D, poly: &[Point2D], even_odd: bool) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut winding = 0_i64;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[j], poly[i]);
        // Rebase to the query before products: world-coordinate offsets do
        // not affect winding precision, including on distant imported pages.
        let ax = f64::from(a.x) - f64::from(p.x);
        let ay = f64::from(a.y) - f64::from(p.y);
        let bx = f64::from(b.x) - f64::from(p.x);
        let by = f64::from(b.y) - f64::from(p.y);
        let cross = ax * by - bx * ay;
        if ay <= 0.0 && by > 0.0 && cross > 0.0 {
            winding += 1;
        } else if ay > 0.0 && by <= 0.0 && cross < 0.0 {
            winding -= 1;
        }
        j = i;
    }
    if even_odd {
        winding % 2 != 0
    } else {
        winding != 0
    }
}

/// Shortest distance from `p` to the segment `a`–`b`.
fn distance_point_to_segment(p: Point2D, a: Point2D, b: Point2D) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq < f32::EPSILON {
        let pdx = p.x - a.x;
        let pdy = p.y - a.y;
        return (pdx * pdx + pdy * pdy).sqrt();
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq).clamp(0.0, 1.0);
    let cx = a.x + t * dx;
    let cy = a.y + t * dy;
    let pdx = p.x - cx;
    let pdy = p.y - cy;
    (pdx * pdx + pdy * pdy).sqrt()
}

#[cfg(test)]
#[path = "layout_scene_hit_tests.rs"]
mod tests;
