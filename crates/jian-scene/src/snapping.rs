//! Request-scoped, indexed edge/center snapping in document coordinates.
use std::collections::HashSet;

mod scene;

#[derive(Clone, Copy, Debug)]
pub struct SnapRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl SnapRect {
    pub fn normalized(self) -> Option<Self> {
        let right = self.x + self.width;
        let bottom = self.y + self.height;
        if ![self.x, self.y, right, bottom]
            .iter()
            .all(|v| v.is_finite())
        {
            return None;
        }
        Some(Self {
            x: self.x.min(right),
            y: self.y.min(bottom),
            width: self.width.abs(),
            height: self.height.abs(),
        })
    }
}

/// The fixed opposite edge is preserved when snapping a resize handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeHandle {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}
impl ResizeHandle {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "left" => Self::Left,
            "right" => Self::Right,
            "top" => Self::Top,
            "bottom" => Self::Bottom,
            "top_left" => Self::TopLeft,
            "top_right" => Self::TopRight,
            "bottom_left" => Self::BottomLeft,
            "bottom_right" => Self::BottomRight,
            _ => return None,
        })
    }
    fn x_kind(self) -> Option<usize> {
        match self {
            Self::Left | Self::TopLeft | Self::BottomLeft => Some(0),
            Self::Right | Self::TopRight | Self::BottomRight => Some(2),
            _ => None,
        }
    }
    fn y_kind(self) -> Option<usize> {
        match self {
            Self::Top | Self::TopLeft | Self::TopRight => Some(0),
            Self::Bottom | Self::BottomLeft | Self::BottomRight => Some(2),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SnapTarget {
    pub id: String,
    pub bounds: SnapRect,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapAxis {
    X,
    Y,
}
#[derive(Clone, Debug)]
pub struct SnapGuide {
    pub axis: SnapAxis,
    pub position: f32,
    pub start: f32,
    pub end: f32,
    pub target_id: String,
}
#[derive(Clone, Debug)]
pub struct SnapResult {
    pub dx: f32,
    pub dy: f32,
    pub guides: Vec<SnapGuide>,
}

#[derive(Clone, Copy)]
struct Anchor {
    value: f32,
    target: usize,
}

/// Three anchors per axis; batches reuse this index without rewalking a page.
pub struct SnapIndex {
    targets: Vec<SnapTarget>,
    x: [Vec<Anchor>; 3],
    y: [Vec<Anchor>; 3],
}
impl SnapIndex {
    /// Owned index capacity, including unused Vec slots and target IDs.
    pub fn storage_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            .saturating_add(
                self.targets
                    .capacity()
                    .saturating_mul(std::mem::size_of::<SnapTarget>()),
            )
            .saturating_add(
                self.targets
                    .iter()
                    .map(|target| target.id.capacity())
                    .sum::<usize>(),
            )
            .saturating_add(
                self.x
                    .iter()
                    .chain(&self.y)
                    .map(|anchors| {
                        anchors
                            .capacity()
                            .saturating_mul(std::mem::size_of::<Anchor>())
                    })
                    .sum::<usize>(),
            )
    }

    pub fn new(targets: impl IntoIterator<Item = SnapTarget>) -> Self {
        let mut index = Self {
            targets: Vec::new(),
            x: std::array::from_fn(|_| Vec::new()),
            y: std::array::from_fn(|_| Vec::new()),
        };
        for mut target in targets {
            let Some(bounds) = target.bounds.normalized() else {
                continue;
            };
            target.bounds = bounds;
            let ordinal = index.targets.len();
            for (kind, value) in positions(bounds.x, bounds.width).into_iter().enumerate() {
                index.x[kind].push(Anchor {
                    value,
                    target: ordinal,
                });
            }
            for (kind, value) in positions(bounds.y, bounds.height).into_iter().enumerate() {
                index.y[kind].push(Anchor {
                    value,
                    target: ordinal,
                });
            }
            index.targets.push(target);
        }
        for axis in [&mut index.x, &mut index.y] {
            for anchors in axis {
                anchors.sort_unstable_by(|a, b| {
                    a.value.total_cmp(&b.value).then(a.target.cmp(&b.target))
                });
            }
        }
        index
    }

    pub fn query(
        &self,
        bounds: SnapRect,
        dx: f32,
        dy: f32,
        zoom: f32,
        threshold_px: f32,
        excluded: &[String],
    ) -> Option<SnapResult> {
        let original = bounds.normalized()?;
        if ![dx, dy, zoom, threshold_px].iter().all(|v| v.is_finite())
            || zoom <= 0.0
            || threshold_px < 0.0
        {
            return None;
        }
        let proposed = SnapRect {
            x: original.x + dx,
            y: original.y + dy,
            ..original
        }
        .normalized()?;
        let threshold = threshold_px / zoom;
        if !threshold.is_finite() {
            return None;
        }
        let excluded: HashSet<&str> = excluded.iter().map(String::as_str).collect();
        let x = self.nearest(
            &self.x,
            positions(proposed.x, proposed.width).map(Some),
            threshold,
            &excluded,
        );
        let y = self.nearest(
            &self.y,
            positions(proposed.y, proposed.height).map(Some),
            threshold,
            &excluded,
        );
        let correction_x = x.map_or(0.0, |c| c.offset);
        let correction_y = y.map_or(0.0, |c| c.offset);
        let adjusted_dx = dx + correction_x;
        let adjusted_dy = dy + correction_y;
        if !adjusted_dx.is_finite() || !adjusted_dy.is_finite() {
            return None;
        }
        let final_bounds = SnapRect {
            x: proposed.x + correction_x,
            y: proposed.y + correction_y,
            ..proposed
        }
        .normalized()?;
        let guides = self.guides(final_bounds, x, y);
        Some(SnapResult {
            dx: adjusted_dx,
            dy: adjusted_dy,
            guides,
        })
    }

    /// Snap only the moving edges; stationary edges and an untouched axis
    /// cannot attract the cursor. A nonpositive/crossed result is refused.
    pub fn query_resize(
        &self,
        bounds: SnapRect,
        handle: ResizeHandle,
        dx: f32,
        dy: f32,
        zoom: f32,
        threshold_px: f32,
        excluded: &[String],
    ) -> Option<(SnapRect, SnapResult)> {
        let original = bounds.normalized()?;
        if ![dx, dy, zoom, threshold_px].iter().all(|v| v.is_finite())
            || zoom <= 0.0
            || threshold_px < 0.0
        {
            return None;
        }
        let threshold = threshold_px / zoom;
        if !threshold.is_finite() {
            return None;
        }
        let mut moving_x = [None; 3];
        let mut moving_y = [None; 3];
        if let Some(kind) = handle.x_kind() {
            moving_x[kind] = Some(positions(original.x, original.width)[kind] + dx);
        }
        if let Some(kind) = handle.y_kind() {
            moving_y[kind] = Some(positions(original.y, original.height)[kind] + dy);
        }
        if moving_x
            .iter()
            .chain(&moving_y)
            .flatten()
            .any(|v| !v.is_finite())
        {
            return None;
        }
        let excluded = excluded.iter().map(String::as_str).collect();
        let x = self.nearest(&self.x, moving_x, threshold, &excluded);
        let y = self.nearest(&self.y, moving_y, threshold, &excluded);
        let dx = if handle.x_kind().is_some() {
            dx + x.map_or(0.0, |c| c.offset)
        } else {
            0.0
        };
        let dy = if handle.y_kind().is_some() {
            dy + y.map_or(0.0, |c| c.offset)
        } else {
            0.0
        };
        let mut resized = original;
        match handle.x_kind() {
            Some(0) => {
                resized.x += dx;
                resized.width -= dx;
            }
            Some(2) => resized.width += dx,
            _ => {}
        }
        match handle.y_kind() {
            Some(0) => {
                resized.y += dy;
                resized.height -= dy;
            }
            Some(2) => resized.height += dy,
            _ => {}
        }
        if resized.width <= 0.0 || resized.height <= 0.0 {
            return None;
        }
        let resized = resized.normalized()?;
        let guides = self.guides(resized, x, y);
        Some((resized, SnapResult { dx, dy, guides }))
    }

    fn guides(
        &self,
        final_bounds: SnapRect,
        x: Option<Candidate>,
        y: Option<Candidate>,
    ) -> Vec<SnapGuide> {
        let mut guides = Vec::with_capacity(2);
        if let Some(candidate) = x {
            let target = &self.targets[candidate.anchor.target];
            guides.push(SnapGuide {
                axis: SnapAxis::X,
                position: candidate.anchor.value,
                start: final_bounds.y.min(target.bounds.y),
                end: (final_bounds.y + final_bounds.height)
                    .max(target.bounds.y + target.bounds.height),
                target_id: target.id.clone(),
            });
        }
        if let Some(candidate) = y {
            let target = &self.targets[candidate.anchor.target];
            guides.push(SnapGuide {
                axis: SnapAxis::Y,
                position: candidate.anchor.value,
                start: final_bounds.x.min(target.bounds.x),
                end: (final_bounds.x + final_bounds.width)
                    .max(target.bounds.x + target.bounds.width),
                target_id: target.id.clone(),
            });
        }
        guides
    }

    fn nearest(
        &self,
        axis: &[Vec<Anchor>; 3],
        moving: [Option<f32>; 3],
        threshold: f32,
        excluded: &HashSet<&str>,
    ) -> Option<Candidate> {
        let mut best: Option<Candidate> = None;
        // Separate target kinds let coincident edges/centers be resolved without
        // scanning every sibling inside the screen-pixel threshold. Each probe
        // visits only the nearest permitted coordinate on either side.
        for moving_kind in [1, 0, 2] {
            let Some(value) = moving[moving_kind] else {
                continue;
            };
            for target_kind in [1, 0, 2] {
                let anchors = &axis[target_kind];
                let split = anchors.partition_point(|a| a.value < value);
                for anchor in self.neighbors(anchors, split, value, threshold, excluded) {
                    let candidate = Candidate {
                        anchor,
                        offset: anchor.value - value,
                        moving_kind,
                        target_kind,
                    };
                    if best.is_none_or(|current| candidate.better(current)) {
                        best = Some(candidate);
                    }
                    if candidate.offset == 0.0 && moving_kind == 1 && target_kind == 1 {
                        return best;
                    }
                }
            }
        }
        best
    }

    fn neighbors(
        &self,
        anchors: &[Anchor],
        split: usize,
        value: f32,
        threshold: f32,
        excluded: &HashSet<&str>,
    ) -> impl Iterator<Item = Anchor> {
        let allowed =
            |anchor: &&Anchor| !excluded.contains(self.targets[anchor.target].id.as_str());
        let mut left = None;
        let mut end = split;
        while end > 0 {
            let coordinate = anchors[end - 1].value;
            if value - coordinate > threshold {
                break;
            }
            let start = anchors[..end].partition_point(|a| a.value < coordinate);
            if let Some(anchor) = anchors[start..end].iter().find(allowed) {
                left = Some(*anchor);
                break;
            }
            end = start;
        }
        let mut right = None;
        let mut start = split;
        while start < anchors.len() {
            let coordinate = anchors[start].value;
            if coordinate - value > threshold {
                break;
            }
            let end = start + anchors[start..].partition_point(|a| a.value == coordinate);
            if let Some(anchor) = anchors[start..end].iter().find(allowed) {
                right = Some(*anchor);
                break;
            }
            start = end;
        }
        [left, right].into_iter().flatten()
    }
}

fn positions(origin: f32, extent: f32) -> [f32; 3] {
    [origin, origin + extent * 0.5, origin + extent]
        .map(|value| if value == 0.0 { 0.0 } else { value })
}
#[derive(Clone, Copy)]
struct Candidate {
    anchor: Anchor,
    offset: f32,
    moving_kind: usize,
    target_kind: usize,
}
impl Candidate {
    fn better(self, other: Self) -> bool {
        let rank = |c: Self| {
            (
                usize::from(c.moving_kind != c.target_kind),
                usize::from(c.moving_kind != 1),
                c.anchor.target,
                c.target_kind,
                c.moving_kind,
            )
        };
        self.offset
            .abs()
            .total_cmp(&other.offset.abs())
            .then_with(|| rank(self).cmp(&rank(other)))
            .is_lt()
    }
}
