//! Backend-neutral exact stroke geometry/style for masked paint fields.

use crate::{Point2D, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrokePaintCap {
    #[default]
    Butt,
    Round,
    Square,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrokePaintJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrokePlacement {
    Inside,
    #[default]
    Center,
    Outside,
}
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StrokePaintStyle {
    pub cap: StrokePaintCap,
    pub join: StrokePaintJoin,
    pub dash_pattern: Vec<f32>,
    pub dash_offset: f32,
}
#[derive(Debug, Clone, Copy)]
pub enum StrokeCurve {
    Move(Point2D),
    Line(Point2D),
    Quad(Point2D, Point2D),
    Cubic(Point2D, Point2D, Point2D),
    Close,
}
#[derive(Debug, Clone, Copy)]
pub enum StrokeGeometry<'a> {
    RawSvg {
        data: &'a str,
        even_odd: bool,
    },
    RoundedRect {
        rect: Rect,
        radii: [f32; 4],
    },
    Oval(Rect),
    Svg {
        data: &'a str,
        rect: Rect,
        even_odd: bool,
    },
    Curves {
        elements: &'a [StrokeCurve],
        even_odd: bool,
    },
}
#[derive(Debug, Clone, Copy)]
pub struct StrokeMaskOptions<'a> {
    pub width: f32,
    pub sides: Option<[f32; 4]>,
    pub placement: StrokePlacement,
    pub style: &'a StrokePaintStyle,
}
