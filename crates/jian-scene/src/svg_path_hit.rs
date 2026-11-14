//! Backend-independent picking for the preserved SVG outline used by paint.
use crate::layout_scene::SceneNode;
use jian_widgets::geometry::Point2D;
use kurbo::{Affine, BezPath, ParamCurveNearest, PathEl, Point, Shape};
use std::{
    collections::{HashMap, VecDeque},
    mem::size_of,
    sync::{Arc, Mutex, OnceLock},
};

const CACHE_BYTES: usize = 8 * 1024 * 1024;
const CACHE_ENTRIES: usize = 256;
const MAX_PENDING: usize = 16;
const PENDING_KEY_BYTES: usize = 4 * 1024 * 1024;
type Pending = Arc<OnceLock<Option<Arc<Geometry>>>>;

struct Geometry {
    outline: BezPath,
    fill: BezPath,
    bounds: kurbo::Rect,
}

#[derive(Default)]
struct Cache {
    entries: HashMap<Arc<str>, (Option<Arc<Geometry>>, usize)>,
    order: VecDeque<Arc<str>>,
    bytes: usize,
    pending: HashMap<Arc<str>, Pending>,
    pending_bytes: usize,
}

fn geometry(d: &str) -> Option<Arc<Geometry>> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let pending = {
        let mut cache = cache.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((value, _)) = cache.entries.get(d) {
            return value.clone();
        }
        if let Some(value) = cache.pending.get(d) {
            Some(value.clone())
        } else if cache.pending.len() < MAX_PENDING
            && d.len() <= PENDING_KEY_BYTES.saturating_sub(cache.pending_bytes)
        {
            let value = Arc::new(OnceLock::new());
            cache.pending.insert(Arc::from(d), value.clone());
            cache.pending_bytes += d.len();
            Some(value)
        } else {
            None
        }
    };
    // Parse outside the cache lock: unrelated documents never wait on SVG
    // decoding. All hit arithmetic stays local and f64, even on distant pages.
    let value = pending
        .as_ref()
        .map_or_else(|| parse(d), |slot| slot.get_or_init(|| parse(d)).clone());
    let charge = d.len().saturating_add(value.as_ref().map_or(0, |g| {
        (g.outline.elements().len() + g.fill.elements().len())
            .saturating_mul(size_of::<PathEl>())
            .saturating_mul(2)
    }));
    let mut cache = cache.lock().unwrap_or_else(|p| p.into_inner());
    if pending.as_ref().is_some_and(|slot| {
        cache
            .pending
            .get(d)
            .is_some_and(|live| Arc::ptr_eq(slot, live))
    }) {
        cache.pending.remove(d);
        cache.pending_bytes = cache.pending_bytes.saturating_sub(d.len());
    }
    if charge > CACHE_BYTES {
        return value;
    }
    if let Some((existing, _)) = cache.entries.get(d) {
        return existing.clone();
    }
    while cache.entries.len() >= CACHE_ENTRIES || cache.bytes + charge > CACHE_BYTES {
        let Some(key) = cache.order.pop_front() else {
            break;
        };
        if let Some((_, bytes)) = cache.entries.remove(&key) {
            cache.bytes = cache.bytes.saturating_sub(bytes);
        }
    }
    let key: Arc<str> = Arc::from(d);
    cache.order.push_back(key.clone());
    cache.entries.insert(key, (value.clone(), charge));
    cache.bytes += charge;
    value
}

fn parse(d: &str) -> Option<Arc<Geometry>> {
    let outline = BezPath::from_svg(d).ok().filter(BezPath::is_finite)?;
    let bounds = outline.bounding_box();
    if !bounds.is_finite() {
        return None;
    }
    // SVG fills implicitly close every subpath; strokes retain authored closure.
    let mut fill = BezPath::new();
    let mut open = false;
    for element in outline.iter() {
        if matches!(element, PathEl::MoveTo(_)) && open {
            fill.close_path();
        }
        open = !matches!(element, PathEl::ClosePath);
        fill.push(element);
    }
    if open {
        fill.close_path();
    }
    Some(Arc::new(Geometry {
        outline,
        fill,
        bounds,
    }))
}

pub(super) fn contains(node: &SceneNode, point: Point2D, zoom: f32) -> bool {
    let Some(d) = node.svg_path.as_deref() else {
        return false;
    };
    let bounds = node.bounds;
    let slack = 4.0 / f64::from(zoom.max(0.0001));
    let stroke = node.stroke.map_or(0.0, |s| f64::from(s.width) * 0.5);
    let tolerance = stroke + slack;
    let local = Point::new(
        f64::from(point.x) - f64::from(bounds.origin.x),
        f64::from(point.y) - f64::from(bounds.origin.y),
    );
    let size = (f64::from(bounds.size.x), f64::from(bounds.size.y));
    // Positive fitted bounds contain every curve extremum. Reject misses
    // before parsing, cache lookup, or nearest-curve work.
    let fitted = size.0 > 0.0 && size.1 > 0.0 && size.0.is_finite() && size.1.is_finite();
    // The painter leaves a native span <= .01 unscaled. Such an outline
    // can extend slightly beyond a smaller positive authored dimension.
    let broad_slack = tolerance + 0.01;
    if fitted
        && (local.x < -broad_slack
            || local.y < -broad_slack
            || local.x > size.0 + broad_slack
            || local.y > size.1 + broad_slack)
    {
        return false;
    }
    let Some(geometry) = geometry(d) else {
        return false;
    };
    // Match native/Web paint: fit tight curve bounds for positive authored
    // dimensions, otherwise translate the original geometry unchanged.
    let (sx, sy, x0, y0) = if fitted {
        let native = geometry.bounds;
        (
            if native.width().abs() > 0.01 {
                size.0 / native.width()
            } else {
                1.0
            },
            if native.height().abs() > 0.01 {
                size.1 / native.height()
            } else {
                1.0
            },
            native.x0,
            native.y0,
        )
    } else {
        (1.0, 1.0, 0.0, 0.0)
    };
    let authored = Point::new(local.x / sx + x0, local.y / sy + y0);
    if node.fill.is_some() || !node.fill_layers.is_empty() {
        let winding = geometry.fill.winding(authored);
        if if node.even_odd_fill {
            winding % 2 != 0
        } else {
            winding != 0
        } {
            return true;
        }
    }
    // Preserve the editor's screen-pixel edge slack and evaluate it after
    // nonuniform fitting, without flattening curves or joining subpaths.
    let transform = Affine::new([sx, 0.0, 0.0, sy, -x0 * sx, -y0 * sy]);
    let hit = geometry.outline.segments().any(|segment| {
        (transform * segment).nearest(local, 0.01).distance_sq <= tolerance * tolerance
    });
    hit
}
