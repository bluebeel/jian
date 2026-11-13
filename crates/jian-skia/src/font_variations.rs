//! Apply authored numeric weights to variable font outlines, not synthetic ink.

use skia_safe::{
    font_arguments::{variation_position::Coordinate, VariationPosition},
    textlayout::TextStyle,
    FontArguments, FourByteTag, Typeface,
};

const WGHT: FourByteTag = FourByteTag::from_chars('w', 'g', 'h', 't');

/// Match a resolved variable face to the requested weight. Font managers may
/// select a variable font's default instance without applying its `wght` axis.
/// Preserve the selected face's other coordinates (for example optical size).
pub(crate) fn typeface_at_weight(typeface: Typeface, weight: u16) -> Typeface {
    let Some(axes) = typeface.variation_design_parameters() else {
        return typeface;
    };
    let Some(axis) = axes.iter().find(|axis| axis.tag == WGHT) else {
        return typeface;
    };
    let target = f32::from(weight).clamp(axis.min, axis.max);
    let mut coordinates = typeface.variation_design_position().unwrap_or_default();
    if let Some(coordinate) = coordinates
        .iter_mut()
        .find(|coordinate| coordinate.axis == WGHT)
    {
        if (coordinate.value - target).abs() < f32::EPSILON {
            return typeface;
        }
        coordinate.value = target;
    } else {
        coordinates.push(Coordinate {
            axis: WGHT,
            value: target,
        });
    }
    let arguments = FontArguments::new().set_variation_design_position(VariationPosition {
        coordinates: &coordinates,
    });
    typeface
        .clone_with_arguments(&arguments)
        .unwrap_or(typeface)
}

/// Supply the same numeric weight to Paragraph shaping. Skia ignores `wght`
/// for static fonts and clamps it to the supported range for variable fonts.
/// The style still needs its FontStyle for family matching and static faces.
pub fn set_paragraph_weight(style: &mut TextStyle, weight: u16) {
    let coordinates = [Coordinate {
        axis: WGHT,
        value: f32::from(weight),
    }];
    let arguments = FontArguments::new().set_variation_design_position(VariationPosition {
        coordinates: &coordinates,
    });
    style.set_font_arguments(&arguments);
}
