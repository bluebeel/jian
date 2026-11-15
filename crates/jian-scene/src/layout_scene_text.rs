use super::*;

#[derive(Debug, Clone, PartialEq)]
pub struct SceneTextRun {
    /// Inclusive start byte offset into the node's flat `text`.
    pub start: usize,
    /// Exclusive end byte offset into the node's flat `text`.
    pub end: usize,
    /// Per-run font size in doc-px. `0.0` = inherit the node's.
    pub font_size: f32,
    /// Per-run CSS-style weight (100-900). `0` = inherit the node's.
    pub font_weight: u16,
    /// Per-run fill colour. `None` = inherit the node's resolved fill.
    pub fill: Option<Color>,
    pub fill_disabled: bool,
    /// Optional authored paint stack, front-to-back; an empty stack disables
    /// paint without removing the run or changing its font/layout geometry.
    pub fill_layers: Option<Arc<Vec<SceneFillLayer>>>,
    /// Render this run with an italic (slanted) face.
    pub italic: bool,
    /// Stroke an underline beneath this run's painted slices.
    pub underline: bool,
    /// Stroke a strikethrough across this run's painted slices.
    pub strikethrough: bool,
    pub text_decoration_skip_ink: Option<bool>,
}

/// Video playback metadata carried by an image poster in the render scene.
/// The scene paints only the poster; HTML export consumes the playback fields.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneVideo {
    pub src: Arc<str>,
    pub autoplay: bool,
    pub r#loop: bool,
    pub muted: bool,
    pub hold_last_frame: bool,
    pub click_to_replay: bool,
}
