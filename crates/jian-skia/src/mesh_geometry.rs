//! Checked lattice preparation shared by native mesh paint adapters.
//!
//! Small grids retain their exact indexed triangle order. Larger grids use a
//! single non-indexed draw rather than truncating vertex identities to u16.

use skia_safe::{vertices::VertexMode, Color, Point, Vertices};

fn buffer<T>(len: usize) -> Option<Vec<T>> {
    let mut values = Vec::new();
    values.try_reserve_exact(len).ok()?;
    Some(values)
}

/// Construct the existing row-major two-triangle cell topology without changing
/// interpolation, coordinates or paint alpha. `color` is called once per source
/// vertex. Temporary CPU storage is released before the returned native draw.
pub fn lattice_vertices(
    bounds: [f32; 4],
    rows: u32,
    cols: u32,
    color_count: usize,
    mut color: impl FnMut(usize) -> Color,
) -> Option<Vertices> {
    let rows = usize::try_from(rows.max(2)).ok()?;
    let cols = usize::try_from(cols.max(2)).ok()?;
    let count = rows.checked_mul(cols)?;
    if color_count != count {
        return None;
    }
    let triangles = (rows - 1).checked_mul(cols - 1)?.checked_mul(6)?;
    // Both counts cross the native signed-int ABI (including index count).
    i32::try_from(triangles).ok()?;
    let indexed = count <= usize::from(u16::MAX) + 1;
    let draw_count = if indexed { count } else { triangles };
    i32::try_from(draw_count).ok()?;

    let mut xs = buffer(cols)?;
    let mut ys = buffer(rows)?;
    let mut colors = buffer(count)?;
    for c in 0..cols {
        xs.push(bounds[0] + (c as f32 / (cols - 1) as f32) * bounds[2]);
    }
    for r in 0..rows {
        ys.push(bounds[1] + (r as f32 / (rows - 1) as f32) * bounds[3]);
    }
    for i in 0..count {
        colors.push(color(i));
    }
    let mut positions = buffer(draw_count)?;
    if indexed {
        for &y in &ys {
            for &x in &xs {
                positions.push(Point::new(x, y));
            }
        }
        let mut indices = buffer(triangles)?;
        for r in 0..rows - 1 {
            for c in 0..cols - 1 {
                let tl = (r * cols + c) as u16;
                let tr = tl + 1;
                let bl = ((r + 1) * cols + c) as u16;
                let br = bl + 1;
                indices.extend_from_slice(&[tl, tr, bl, tr, br, bl]);
            }
        }
        Some(Vertices::new_copy(
            VertexMode::Triangles,
            &positions,
            &positions,
            &colors,
            Some(&indices),
        ))
    } else {
        let mut expanded_colors = buffer(draw_count)?;
        for r in 0..rows - 1 {
            for c in 0..cols - 1 {
                let tl = r * cols + c;
                let tr = tl + 1;
                let bl = tl + cols;
                let br = bl + 1;
                for i in [tl, tr, bl, tr, br, bl] {
                    positions.push(Point::new(xs[i % cols], ys[i / cols]));
                    expanded_colors.push(colors[i]);
                }
            }
        }
        Some(Vertices::new_copy(
            VertexMode::Triangles,
            &positions,
            &positions,
            &expanded_colors,
            None,
        ))
    }
}
