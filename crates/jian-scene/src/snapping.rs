//! Request-scoped, indexed edge/center snapping in document coordinates.
use std::collections::HashSet;

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
    kind: usize,
}

/// Three anchors per axis; batches reuse this index without rewalking a page.
pub struct SnapIndex {
    targets: Vec<SnapTarget>,
    x: Vec<Anchor>,
    y: Vec<Anchor>,
}
impl SnapIndex {
    pub fn new(targets: impl IntoIterator<Item = SnapTarget>) -> Self {
        let mut index = Self {
            targets: Vec::new(),
            x: Vec::new(),
            y: Vec::new(),
        };
        for mut target in targets {
            let Some(bounds) = target.bounds.normalized() else {
                continue;
            };
            target.bounds = bounds;
            let ordinal = index.targets.len();
            for (kind, value) in positions(bounds.x, bounds.width).into_iter().enumerate() {
                index.x.push(Anchor {
                    value,
                    target: ordinal,
                    kind,
                });
            }
            for (kind, value) in positions(bounds.y, bounds.height).into_iter().enumerate() {
                index.y.push(Anchor {
                    value,
                    target: ordinal,
                    kind,
                });
            }
            index.targets.push(target);
        }
        for anchors in [&mut index.x, &mut index.y] {
            anchors.sort_unstable_by(|a, b| {
                a.value
                    .total_cmp(&b.value)
                    .then(a.target.cmp(&b.target))
                    .then(a.kind.cmp(&b.kind))
            });
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
            positions(proposed.x, proposed.width),
            threshold,
            &excluded,
        );
        let y = self.nearest(
            &self.y,
            positions(proposed.y, proposed.height),
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
        Some(SnapResult {
            dx: adjusted_dx,
            dy: adjusted_dy,
            guides,
        })
    }

    fn nearest(
        &self,
        anchors: &[Anchor],
        moving: [f32; 3],
        threshold: f32,
        excluded: &HashSet<&str>,
    ) -> Option<Candidate> {
        let mut best: Option<Candidate> = None;
        // Center-center wins an exact tie, then scene order makes ties stable.
        for kind in [1, 0, 2] {
            let value = moving[kind];
            let start = anchors.partition_point(|a| a.value < value - threshold);
            for anchor in anchors[start..]
                .iter()
                .take_while(|a| a.value <= value + threshold)
            {
                if excluded.contains(self.targets[anchor.target].id.as_str()) {
                    continue;
                }
                let candidate = Candidate {
                    anchor: *anchor,
                    offset: anchor.value - value,
                    moving_kind: kind,
                };
                if !candidate.offset.is_finite() || candidate.offset.abs() > threshold {
                    continue;
                }
                if best.is_none_or(|current| candidate.better(current)) {
                    best = Some(candidate);
                }
                if candidate.offset == 0.0 && kind == 1 && anchor.kind == 1 {
                    return best;
                }
            }
        }
        best
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
}
impl Candidate {
    fn better(self, other: Self) -> bool {
        let rank = |c: Self| {
            (
                usize::from(c.moving_kind != c.anchor.kind),
                usize::from(c.moving_kind != 1),
                c.anchor.target,
                c.anchor.kind,
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
