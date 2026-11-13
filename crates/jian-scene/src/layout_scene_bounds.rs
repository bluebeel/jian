//! Paint-visible scene bounds kept separate from selection/layout bounds.

use crate::layout_scene::{LayoutScene, SceneNode};
use crate::scene_geometry::rect_has_extent;
use jian_widgets::Rect;

impl SceneNode {
    /// Bounds of paint that can escape this node for viewport framing.
    ///
    /// Selection and layout intentionally keep using `aggregate_bounds`: a
    /// bounded frame reports its authored rectangle. Visual bounds differ only
    /// for open containers, whose descendants may paint beyond that rectangle.
    /// A clipping container stops the walk at its own bounds.
    pub fn visual_bounds(&self) -> Rect {
        Self::compute_visual_bounds(
            self.bounds,
            self.hidden,
            self.clip_content,
            self.children.iter().map(SceneNode::visual_bounds),
        )
    }

    /// Shared computed-geometry rule for paint-visible overflow and clipping.
    pub fn compute_visual_bounds(
        own: Rect,
        hidden: bool,
        clip: bool,
        children: impl IntoIterator<Item = Rect>,
    ) -> Rect {
        if hidden {
            return Rect::ZERO;
        }
        if clip {
            return own;
        }

        let mut bounds = rect_has_extent(own).then_some(own);
        for child_bounds in children {
            if !rect_has_extent(child_bounds) {
                continue;
            }
            bounds = Some(match bounds {
                Some(current) => union_rects(current, child_bounds),
                None => child_bounds,
            });
        }
        bounds.unwrap_or(Rect::ZERO)
    }
}

fn union_rects(a: Rect, b: Rect) -> Rect {
    let min_x = a.origin.x.min(b.origin.x);
    let min_y = a.origin.y.min(b.origin.y);
    let max_x = (a.origin.x + a.size.x).max(b.origin.x + b.size.x);
    let max_y = (a.origin.y + a.size.y).max(b.origin.y + b.size.y);
    Rect::xywh(min_x, min_y, max_x - min_x, max_y - min_y)
}

impl LayoutScene {
    /// Union visible root rects with the same zero/nonfinite behavior as content_bounds.
    pub fn content_bounds_from_rects(roots: impl IntoIterator<Item = Rect>) -> Option<Rect> {
        let mut bounds = None;
        for rect in roots {
            if rect.size.x <= 0.0 && rect.size.y <= 0.0 {
                continue;
            }
            bounds = Some(match bounds {
                None => rect,
                Some(before) => union_rects(before, rect),
            });
        }
        bounds
    }
}
