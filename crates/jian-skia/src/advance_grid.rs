//! Deterministic authored text advances on a 1024-unit em grid.
use skia_safe::{Font, FontHinting, GlyphId, Point};

pub fn quantize_advance(value: f32, font_size: f32) -> f32 {
    if !value.is_finite() || !font_size.is_finite() || font_size <= 0.0 {
        return value;
    }
    (value * 1024.0 / font_size).round() * font_size / 1024.0
}

/// Add authored tracking after the complete coordinate is assembled. The
/// integer cluster ordinal and the original f32 spacing are exact f64 inputs;
/// only the final local coordinate is narrowed to the paint representation.
pub fn tracked_position(position: f32, ordinal: usize, spacing: f32) -> f32 {
    (f64::from(position) + ordinal as f64 * f64::from(spacing)) as f32
}

const MAX_GRID_TICKS: i64 = (1_i64 << 52) - 1;
fn grid_ticks(value: f32, size: f32) -> Option<i64> {
    if !value.is_finite() || !size.is_finite() || size <= 0.0 {
        return None;
    }
    // Preserve the existing f32 quantization and Rust tie rule. Accumulate its
    // integer result, rather than repeatedly canceling raw shaped positions.
    let ticks = (value * 1024.0 / size).round();
    (ticks.is_finite() && f64::from(ticks).abs() <= MAX_GRID_TICKS as f64).then_some(ticks as i64)
}
fn grid_position(ticks: i64, size: f32, origin: f32) -> Option<f32> {
    let position = (f64::from(origin) + ticks as f64 * f64::from(size) / 1024.0) as f32;
    position.is_finite().then_some(position)
}
fn append_ticks(total: &mut i64, ticks: i64) -> Option<()> {
    *total = total.checked_add(ticks)?;
    (total.unsigned_abs() <= MAX_GRID_TICKS as u64).then_some(())
}

pub struct PositionedAdvances {
    pub positions: Vec<Point>,
    pub advance: f32,
}
/// Position a modern unshaped run without accumulating a separate rounding
/// error for every authored tracking step. Its final advance includes the
/// last spacing step so a following fallback-face segment starts correctly.
pub fn position_nominal_advances(
    widths: &[f32],
    size: f32,
    spacing: f32,
    factor: f32,
) -> Option<PositionedAdvances> {
    if !spacing.is_finite() || !factor.is_finite() {
        return None;
    }
    let mut positions = Vec::with_capacity(widths.len());
    let mut ticks = 0;
    for (index, width) in widths.iter().enumerate() {
        let base = grid_position(ticks, size, 0.0)?;
        let x = tracked_position(base, index, spacing);
        if !x.is_finite() {
            return None;
        }
        positions.push(Point::new(x, 0.0));
        append_ticks(&mut ticks, grid_ticks(*width * factor, size)?)?;
    }
    let advance = tracked_position(grid_position(ticks, size, 0.0)?, widths.len(), spacing);
    advance
        .is_finite()
        .then_some(PositionedAdvances { positions, advance })
}

/// Unhinted nominal advances, using instantiated integer TrueType metrics.
/// Variable font backends can expose fractional design-unit advances even
/// though the document's scalable font metrics use OpenType integer rounding.
/// Keep legacy callers on their existing metrics by opting in only alongside
/// the authored advance grid.
pub fn nominal_advances(font: &Font, glyphs: &[GlyphId]) -> Vec<f32> {
    if let Some(widths) = integer_nominal_advances(font, glyphs) {
        return widths;
    }
    let mut font = font.clone();
    font.set_hinting(FontHinting::None);
    font.set_subpixel(true);
    let mut widths = vec![0.0; glyphs.len()];
    font.get_widths(glyphs, &mut widths);
    widths
}

fn integer_nominal_advances(font: &Font, glyphs: &[GlyphId]) -> Option<Vec<f32>> {
    let face = font.typeface();
    let units = face.units_per_em()?;
    if !(16..=16384).contains(&units)
        || !font.size().is_finite()
        || font.size() <= 0.0
        || !font.scale_x().is_finite()
        || font.is_embolden()
        || [*b"head", *b"hmtx", *b"glyf", *b"loca"]
            .iter()
            .any(|tag| face.get_table_size(u32::from_be_bytes(*tag)).is_none())
        || [*b"COLR", *b"CBDT", *b"sbix", *b"SVG "]
            .iter()
            .any(|tag| face.get_table_size(u32::from_be_bytes(*tag)).is_some())
    {
        return None;
    }
    // with_size retains the exact face and its complete variation coordinates.
    // Read at UPEM, rather than undoing the requested size's f32 rounding.
    let mut unit_font = font.with_size(units as f32)?;
    unit_font.set_scale_x(1.0);
    unit_font.set_skew_x(0.0);
    unit_font.set_hinting(FontHinting::None);
    unit_font.set_subpixel(true);
    let mut widths = vec![0.0; glyphs.len()];
    unit_font.get_widths(glyphs, &mut widths);
    for width in &mut widths {
        *width = scale_integer_advance(*width, units, font.size(), font.scale_x())?;
    }
    Some(widths)
}

fn scale_integer_advance(value: f32, units: i32, size: f32, scale_x: f32) -> Option<f32> {
    // OpenType rounds half towards positive infinity, including negative ties.
    let integer = (value + 0.5).floor();
    let scaled = integer * (1.0 / units as f32) * size * scale_x;
    (value.is_finite() && scaled.is_finite()).then_some(scaled)
}

/// Keep joined scripts, marks and join controls on the full shaping path.
pub fn supports_text(text: &str) -> bool {
    text.chars().all(|c| {
        let n = c as u32;
        (n < 0x300 || (0x2000..=0x206f).contains(&n) || (0x20a0..=0x20cf).contains(&n))
            && n != 0x200c
            && n != 0x200d
    })
}

pub struct AdjustedGlyphRun {
    pub positions: Vec<Point>,
    pub advance_correction: f32,
}

/// Quantize nominal advances and contextual positioning independently.
/// Initial placement and vertical glyph offsets are never synthesized.
pub fn adjust_glyph_run(
    font: &Font,
    glyphs: &[GlyphId],
    positions: &[Point],
    clusters: &[u32],
    advance_x: f32,
) -> Option<AdjustedGlyphRun> {
    if glyphs.is_empty()
        || glyphs.len() != positions.len()
        || clusters.len() < glyphs.len()
        || positions[0].x.abs() > 0.00001
        || positions.iter().any(|p| p.y.abs() > 0.00001)
        || clusters[..glyphs.len()]
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        return None;
    }
    let mut nominal_font = font.clone();
    nominal_font.set_hinting(FontHinting::None);
    nominal_font.set_subpixel(true);
    let mut widths = vec![0.0; glyphs.len()];
    nominal_font.get_widths(glyphs, &mut widths);
    let integer_widths = integer_nominal_advances(&nominal_font, glyphs);
    let mut ticks = 0;
    let initial = positions[0].x;
    let mut adjusted = Vec::with_capacity(glyphs.len());
    for (index, position) in positions.iter().enumerate() {
        adjusted.push(Point::new(
            grid_position(ticks, font.size(), initial)?,
            position.y,
        ));
        let end = positions.get(index + 1).map_or(advance_x, |p| p.x);
        let shaped = end - position.x;
        let nominal = widths[index];
        if !shaped.is_finite() || !nominal.is_finite() {
            return None;
        }
        let contextual = shaped - nominal;
        let grid_nominal = integer_widths
            .as_ref()
            .map_or(nominal, |widths| widths[index]);
        append_ticks(&mut ticks, grid_ticks(grid_nominal, font.size())?)?;
        append_ticks(&mut ticks, grid_ticks(contextual, font.size())?)?;
    }
    let correction = ((f64::from(initial) + ticks as f64 * f64::from(font.size()) / 1024.0)
        - f64::from(advance_x)) as f32;
    correction.is_finite().then_some(AdjustedGlyphRun {
        positions: adjusted,
        advance_correction: correction,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nominal_advance_rounds_design_units_before_the_em_grid() {
        for (input, integer) in [
            (642.49, 642.0),
            (642.5, 643.0),
            (438.5, 439.0),
            (-0.49, 0.0),
            (-0.5, 0.0),
            (-1.5, -1.0),
        ] {
            let actual = scale_integer_advance(input, 1000, 56.0, 1.0).unwrap();
            assert_eq!(actual, integer * 0.001 * 56.0);
        }
        let nominal = scale_integer_advance(642.49, 1000, 56.0, 1.0).unwrap();
        assert_eq!(quantize_advance(nominal, 56.0), 35.9296875);
        assert_ne!(
            quantize_advance(nominal, 56.0),
            quantize_advance(642.49 * 0.001 * 56.0, 56.0)
        );
        assert_eq!(
            scale_integer_advance(642.49, 1000, 56.0, -1.25),
            Some(nominal * -1.25)
        );
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(scale_integer_advance(value, 1000, 56.0, 1.0).is_none());
        }
    }

    #[test]
    fn nominal_and_contextual_advances_round_separately_on_the_em_grid() {
        let scale = 16.0 / 2048.0;
        // Even positive nominal plus an odd negative pair adjustment.
        let nominal = 1230.0 * scale;
        let kern = -65.0 * scale;
        let result = quantize_advance(nominal, 16.0) + quantize_advance(kern, 16.0);
        assert_eq!(result, (1230.0 - 66.0) * scale);
        assert_ne!(result, quantize_advance(nominal + kern, 16.0));
        assert!(!supports_text("a\u{301}"));
        assert!(!supports_text("a\u{200d}b"));
    }
}
