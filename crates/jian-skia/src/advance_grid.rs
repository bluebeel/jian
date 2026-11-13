//! Deterministic authored text advances on a 1024-unit em grid.
use skia_safe::{Font, FontHinting, GlyphId, Point};

pub fn quantize_advance(value: f32, font_size: f32) -> f32 {
    if !value.is_finite() || !font_size.is_finite() || font_size <= 0.0 {
        return value;
    }
    (value * 1024.0 / font_size).round() * font_size / 1024.0
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
    let mut correction = 0.0;
    let mut adjusted = Vec::with_capacity(glyphs.len());
    for (index, position) in positions.iter().enumerate() {
        adjusted.push(Point::new(position.x + correction, position.y));
        let end = positions.get(index + 1).map_or(advance_x, |p| p.x);
        let shaped = end - position.x;
        let nominal = widths[index];
        if !shaped.is_finite() || !nominal.is_finite() {
            return None;
        }
        let contextual = shaped - nominal;
        correction += quantize_advance(nominal, font.size())
            + quantize_advance(contextual, font.size())
            - shaped;
    }
    Some(AdjustedGlyphRun {
        positions: adjusted,
        advance_correction: correction,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
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
