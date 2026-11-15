//! Borrowed scope extraction for editor and headless snapping.
use super::{SnapIndex, SnapRect, SnapTarget};
use crate::layout_scene::{LayoutScene, SceneNode};

impl SnapIndex {
    /// Index visible direct children in world coordinates. One composed ancestor
    /// transform is reused for the entire scope; no subtree or scene is cloned.
    pub fn for_scene(scene: &LayoutScene, parent: Option<&str>) -> Option<Self> {
        let page = scene.active_page()?;
        let mut ancestors = Vec::new();
        let nodes = match parent {
            None => page.children.as_slice(),
            Some(id) => {
                let node = find(&page.children, id, &mut ancestors)?;
                if node.hidden || ancestors.iter().any(|ancestor| ancestor.hidden) {
                    return Some(Self::new(std::iter::empty::<SnapTarget>()));
                }
                ancestors.push(node);
                node.children.as_slice()
            }
        };
        let parent_transform = ancestors
            .into_iter()
            .fold(Transform::IDENTITY, |world, node| {
                world.child(Transform::for_node(node))
            });
        Some(Self::new(nodes.iter().filter(|node| !node.hidden).map(
            |node| {
                let bounds = node.aggregate_bounds();
                let left = bounds.origin.x;
                let top = bounds.origin.y;
                let right = left + bounds.size.x;
                let bottom = top + bounds.size.y;
                let transform = parent_transform.child(Transform::for_node(node));
                let corners = [(left, top), (right, top), (right, bottom), (left, bottom)]
                    .map(|point| transform.point(point));
                let min_x = corners.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
                let min_y = corners.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
                let max_x = corners
                    .iter()
                    .map(|p| p.0)
                    .fold(f32::NEG_INFINITY, f32::max);
                let max_y = corners
                    .iter()
                    .map(|p| p.1)
                    .fold(f32::NEG_INFINITY, f32::max);
                SnapTarget {
                    id: node.id.clone(),
                    bounds: SnapRect {
                        x: min_x,
                        y: min_y,
                        width: max_x - min_x,
                        height: max_y - min_y,
                    },
                }
            },
        )))
    }
}

fn find<'a>(
    nodes: &'a [SceneNode],
    id: &str,
    ancestors: &mut Vec<&'a SceneNode>,
) -> Option<&'a SceneNode> {
    let mut stack = vec![(nodes, 0usize)];
    while let Some((siblings, next)) = stack.last_mut() {
        if *next == siblings.len() {
            stack.pop();
            ancestors.pop();
            continue;
        }
        let siblings: &'a [SceneNode] = *siblings;
        let node = &siblings[*next];
        *next += 1;
        if node.id == id {
            return Some(node);
        }
        if !node.children.is_empty() {
            ancestors.push(node);
            stack.push((&node.children, 0));
        }
    }
    None
}

#[derive(Clone, Copy)]
struct Transform {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    tx: f32,
    ty: f32,
}
impl Transform {
    const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };
    fn for_node(node: &SceneNode) -> Self {
        if node.rotation.abs() <= f32::EPSILON && !node.flip_x && !node.flip_y {
            return Self::IDENTITY;
        }
        let bounds = node.aggregate_bounds();
        let cx = bounds.origin.x + bounds.size.x * 0.5;
        let cy = bounds.origin.y + bounds.size.y * 0.5;
        let (sin, cos) = node.rotation.sin_cos();
        let sx = if node.flip_x { -1.0 } else { 1.0 };
        let sy = if node.flip_y { -1.0 } else { 1.0 };
        let (a, b, c, d) = (sx * cos, sy * sin, -sx * sin, sy * cos);
        Self {
            a,
            b,
            c,
            d,
            tx: cx - a * cx - c * cy,
            ty: cy - b * cx - d * cy,
        }
    }
    fn child(self, child: Self) -> Self {
        Self {
            a: self.a * child.a + self.c * child.b,
            b: self.b * child.a + self.d * child.b,
            c: self.a * child.c + self.c * child.d,
            d: self.b * child.c + self.d * child.d,
            tx: self.a * child.tx + self.c * child.ty + self.tx,
            ty: self.b * child.tx + self.d * child.ty + self.ty,
        }
    }
    fn point(self, (x, y): (f32, f32)) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.tx,
            self.b * x + self.d * y + self.ty,
        )
    }
}
