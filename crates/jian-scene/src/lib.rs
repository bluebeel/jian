//! Canonical render-ready scene for jian-based design tools.
//!
//! `layout_scene` is the layout-resolved, variable-resolved render tree built
//! from a document; it is the form the `RenderBackend` painters consume. Moved
//! out of OpenPencil's `op-editor-ui` so it can be shared cross-product.
pub mod layout_scene;
pub mod layout_scene_binding;
mod layout_scene_bounds;
mod layout_scene_fill;
pub mod layout_scene_hit;
pub mod layout_scene_motion;
pub mod path_coordinates;
pub mod path_geometry;
mod scene_geometry;
pub mod scene_hit_index;
mod svg_path_hit;
pub use layout_scene::{SceneTextAlign, SceneTextVerticalAlign};

pub mod snapping;

mod stroke_paint;
pub use stroke_paint::{SceneStrokePaint, StrokePaintCap, StrokePaintJoin, StrokePaintStyle};

pub use jian_ops_schema::style::{
    ImageExtend, ImageFieldGeometry, ImageFieldUnits, ImagePatternGeometry, ImageRepeat,
    ImageViewportFit, ImageViewportGeometry,
};
