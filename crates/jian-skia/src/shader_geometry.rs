//! Canonical node-local RuntimeEffect coordinates shared by host adapters.
//! The optional exact float2 `size` packet scales authored shader dimensions;
//! other programs retain unscaled local coordinates at the node origin.
use skia_safe::Matrix;
pub fn local_matrix<'a>(
    bounds: [f32; 4],
    uniforms: impl IntoIterator<Item = (&'a str, &'a [f32])>,
) -> Matrix {
    let [x, y, w, h] = bounds;
    let size = uniforms
        .into_iter()
        .find(|(name, values)| *name == "size" && values.len() == 2)
        .map(|(_, values)| (values[0], values[1]));
    let mut matrix = Matrix::new_identity();
    if let Some((width, height)) = size {
        if width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0 {
            matrix.set_scale_translate((w / width, h / height), (x, y));
            return matrix;
        }
    }
    matrix.set_translate((x, y));
    matrix
}
