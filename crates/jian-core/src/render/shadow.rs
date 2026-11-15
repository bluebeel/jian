//! Portable shadow profiles; host painters compile the shared SkSL when needed.

pub use jian_ops_schema::style::ShadowBlurKernel;

/// Convolution with a normalized separable triangular kernel. Rounded corners
/// use four weighted midpoint slices and an analytic interval integral on the
/// other axis; square corners have an exact closed form. This source is shared
/// by native Skia and CanvasKit and has no framebuffer-specific noise phase.
pub const TRIANGULAR_SHADOW_SHADER: &str = include_str!("triangular_shadow.sksl");
pub const TRIANGULAR_CORNER_SHADOW_SHADER: &str = include_str!("triangular_corner_shadow.sksl");

/// Proportionally fit circular corners to all four silhouette edges.
pub fn fitted_corner_radii(size: [f32; 2], radii: [f32; 4]) -> Option<[f32; 4]> {
    let w = f64::from(size[0]);
    let h = f64::from(size[1]);
    if !(w > 0.0 && h > 0.0)
        || ![w, h].iter().all(|v| v.is_finite())
        || !radii.iter().all(|v| v.is_finite())
    {
        return None;
    }
    let r = radii.map(|v| f64::from(v.max(0.0)));
    let mut scale = 1.0_f64;
    for (sum, extent) in [
        (r[0] + r[1], w),
        (r[3] + r[2], w),
        (r[0] + r[3], h),
        (r[1] + r[2], h),
    ] {
        if sum > extent {
            scale = scale.min(extent / sum);
        }
    }
    Some(r.map(|v| (v * scale) as f32))
}

/// Preserve functional color precision alongside the legacy packed fallback.
/// Hex callers keep exactly the old packed-color path.
pub fn parse_shadow_color(source: &str) -> Option<(crate::scene::Color, Option<[f32; 4]>)> {
    if let Some(color) = crate::scene::Color::from_hex(source) {
        return Some((color, None));
    }
    let source = source.trim();
    let body = source
        .strip_prefix("rgba(")
        .or_else(|| source.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let parts: Vec<_> = body.split(',').map(str::trim).collect();
    if parts.len() != 3 && parts.len() != 4 {
        return None;
    }
    let channel = |index: usize| -> Option<f32> {
        let value = parts[index].parse::<f64>().ok()? / 255.0;
        value.is_finite().then_some(value.clamp(0.0, 1.0) as f32)
    };
    let alpha = parts
        .get(3)
        .map_or(Some(1.0), |value| value.parse::<f32>().ok())?;
    if !alpha.is_finite() {
        return None;
    }
    let rgba = [channel(0)?, channel(1)?, channel(2)?, alpha.clamp(0.0, 1.0)];
    let byte = |value: f32| (value * 255.0).round() as u8;
    Some((
        crate::scene::Color::rgba(byte(rgba[0]), byte(rgba[1]), byte(rgba[2]), byte(rgba[3])),
        Some(rgba),
    ))
}

pub(crate) fn kernel_from_json(value: &serde_json::Value) -> ShadowBlurKernel {
    match value.get("blurKernel").and_then(serde_json::Value::as_str) {
        Some("triangular") => ShadowBlurKernel::Triangular,
        _ => ShadowBlurKernel::Gaussian,
    }
}

/// Known shape semantics carried through the generic effect-layer API.
#[derive(Debug, Clone, Copy)]
pub struct RoundedShadowSilhouette {
    pub bounds: crate::geometry::Rect,
    pub radii: super::BorderRadii,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shadow_functional_color_preserves_source_bits_and_hex_keeps_its_legacy_path() {
        let value = f32::from_bits(0x3e808180);
        let source = format!("rgba({},10,20,0.05)", f64::from(value) * 255.0);
        let (_, rgba) = parse_shadow_color(&source).unwrap();
        let rgba = rgba.unwrap();
        assert_eq!(rgba[0].to_bits(), value.to_bits());
        assert_eq!(rgba[3].to_bits(), 0.05f32.to_bits());
        for byte in 0..=255 {
            let source = format!("#{byte:02x}{byte:02x}{byte:02x}");
            let (color, float) = parse_shadow_color(&source).unwrap();
            assert_eq!(color.r(), byte);
            assert!(float.is_none());
        }
    }
    #[test]
    fn shadow_flat_draws_keep_profile_and_functional_color() {
        let json = serde_json::json!({"type":"rectangle","id":"a","width":20,"height":10,
            "effects":[{"type":"shadow","blur":8,"blurKernel":"triangular","offsetX":0,"offsetY":1,"spread":0,"color":"rgba(10,20,30,0.05)"}]});
        let mut ops = Vec::new();
        let mut text = Vec::new();
        super::super::scene::emit_for_node(
            crate::geometry::rect(0.0, 0.0, 20.0, 10.0),
            &json,
            false,
            None,
            &mut ops,
            &mut text,
            true,
        );
        let super::super::DrawOp::ShadowedRect { shadow, .. } = &ops[0] else {
            panic!("shadow")
        };
        assert_eq!(shadow.blur_kernel, ShadowBlurKernel::Triangular);
        assert_eq!(shadow.float_color.unwrap()[3].to_bits(), 0.05f32.to_bits());
    }
}
