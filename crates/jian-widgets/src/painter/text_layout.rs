//! Text layout and baseline requests; public paths remain in `painter`.
use crate::geometry::{Color, Point2D};
use jian_core::render::{TextAlign, TextRun};

/// Text layout passed through to a backend-owned text renderer.
#[derive(Debug, Clone)]
pub struct TextLayout {
    runs: Vec<TextRun>,
    source_color: Option<Color>,
    italic: bool,
    kerning: Option<bool>,
    letter_spacing: f32,
    font_features: std::collections::BTreeMap<String, u32>,
}

/// Inputs used to locate the first alphabetic baseline inside a text line.
///
/// `line_height` is the authored multiplier (`0` means backend default).
/// Including the actual line text lets a backend resolve the same fallback
/// face used for CJK/emoji paint instead of sampling an unrelated Latin glyph.
#[derive(Debug, Clone, Copy)]
pub struct TextBaselineRequest<'a> {
    pub text: &'a str,
    pub font_family: &'a str,
    pub font_size: f32,
    pub font_weight: u16,
    pub italic: bool,
    pub line_height: f32,
}

impl TextLayout {
    pub fn single_run(
        content: &str,
        font_family: &str,
        font_size: f32,
        color: jian_core::scene::Color,
        origin: Point2D,
    ) -> Self {
        let run = TextRun {
            content: content.to_string(),
            font_family: font_family.to_string(),
            font_size,
            font_weight: 400,
            color,
            origin: jian_core::geometry::Point::new(origin.x, origin.y),
            max_width: 0.0,
            align: TextAlign::Start,
            line_height: 0.0,
        };
        Self {
            runs: vec![run],
            source_color: None,
            italic: false,
            kerning: None,
            letter_spacing: 0.0,
            font_features: Default::default(),
        }
    }

    /// Original float paint for hosts whose correlated source stage must
    /// avoid narrowing a color through the legacy packed TextRun contract.
    pub fn with_source_color(mut self, color: Color) -> Self {
        self.source_color = Some(color);
        self
    }

    pub fn source_color(&self) -> Option<Color> {
        self.source_color
    }

    pub fn runs(&self) -> &[TextRun] {
        &self.runs
    }

    pub fn with_font_weight(mut self, weight: u16) -> Self {
        for r in &mut self.runs {
            r.font_weight = weight;
        }
        self
    }

    pub fn with_italic(mut self, italic: bool) -> Self {
        self.italic = italic;
        self
    }

    pub fn italic(&self) -> bool {
        self.italic
    }

    pub fn with_kerning(mut self, kerning: Option<bool>) -> Self {
        self.kerning = kerning;
        self
    }

    pub fn kerning(&self) -> Option<bool> {
        self.kerning
    }

    pub fn with_letter_spacing(mut self, spacing: f32) -> Self {
        self.letter_spacing = spacing;
        self
    }

    pub fn letter_spacing(&self) -> f32 {
        self.letter_spacing
    }

    pub fn with_font_features(
        mut self,
        features: &std::collections::BTreeMap<String, u32>,
    ) -> Self {
        self.font_features = features.clone();
        self
    }

    pub fn font_features(&self) -> &std::collections::BTreeMap<String, u32> {
        &self.font_features
    }

    pub fn translated(&self, offset: Point2D) -> Self {
        let runs = self
            .runs
            .iter()
            .map(|r| {
                let mut r2 = r.clone();
                r2.origin =
                    jian_core::geometry::Point::new(r.origin.x + offset.x, r.origin.y + offset.y);
                r2
            })
            .collect();
        Self {
            runs,
            source_color: self.source_color,
            italic: self.italic,
            kerning: self.kerning,
            letter_spacing: self.letter_spacing,
            font_features: self.font_features.clone(),
        }
    }
}
