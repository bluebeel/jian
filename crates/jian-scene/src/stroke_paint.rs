//! Rich stroke layers are optional: ordinary default solid strokes retain the
//! existing Copy descriptor and rendering path without extra paint allocation.

use crate::layout_scene::SceneFillLayer;
pub use jian_widgets::{StrokePaintCap, StrokePaintJoin, StrokePaintStyle};
#[derive(Debug, Clone, PartialEq)]
pub struct SceneStrokePaint {
    pub layers: Vec<SceneFillLayer>,
    pub style: StrokePaintStyle,
}
