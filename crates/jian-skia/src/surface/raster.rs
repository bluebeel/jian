//! `SkiaSurface` — owns a Skia `Surface` + provides PNG encoding for tests.
//!
//! Three creation modes are supported:
//! 1. [`SkiaSurface::new_raster`] — CPU raster; default for tests and
//!    headless rendering. Works everywhere without a GPU context.
//! 2. `SkiaSurface::from_backend_surface` (future, Plan 8) — wraps a
//!    host-provided `SkSurface` backed by Metal / D3D12 / GL / Vulkan.
//! 3. WASM path (Plan 12) — same as above, backed by WebGPU/WebGL.

use skia_safe::{EncodedImageFormat, Surface};

pub struct SkiaSurface {
    pub(crate) inner: Surface,
}

impl SkiaSurface {
    /// Fallible raster allocation for FFI callers that must report OOM.
    pub fn try_new_raster(width: i32, height: i32) -> Option<Self> {
        let inner = skia_safe::surfaces::raster_n32_premul((width, height))?;
        Some(Self { inner })
    }

    /// Build a CPU raster surface at `width × height` logical pixels.
    pub fn new_raster(width: i32, height: i32) -> Self {
        Self::try_new_raster(width, height).expect("skia raster surface allocation failed")
    }

    /// Snapshot the surface into a PNG byte vector. Useful for golden
    /// tests and for debugging.
    pub fn encode_png(&mut self) -> Option<Vec<u8>> {
        let image = self.inner.image_snapshot();
        let mut ctx = self.inner.direct_context();
        let data = image.encode(ctx.as_mut(), EncodedImageFormat::PNG, None)?;
        Some(data.as_bytes().to_vec())
    }

    /// Access the underlying canvas. Re-borrow on every frame.
    pub fn canvas(&mut self) -> &skia_safe::Canvas {
        self.inner.canvas()
    }

    pub fn width(&self) -> i32 {
        self.inner.width()
    }
    pub fn height(&self) -> i32 {
        self.inner.height()
    }

    /// Copy the surface contents into `buf` in straight (un-premultiplied)
    /// RGBA8888. `buf.len()` must equal `width * height * 4`. Returns
    /// `true` on success.
    pub fn read_rgba8(&mut self, buf: &mut [u8]) -> bool {
        let Ok(width) = usize::try_from(self.inner.width()) else {
            return false;
        };
        let Some(row_bytes) = width.checked_mul(4) else {
            return false;
        };
        self.read_rgba8_strided(buf, row_bytes)
    }

    /// Copy straight RGBA8888 directly into caller-owned strided storage.
    /// Padding is not pixel data and remains untouched. The slice must cover
    /// `(height - 1) * row_bytes + width * 4`; trailing padding is optional.
    /// Invalid/overflowing extents return false before entering Skia.
    pub fn read_rgba8_strided(&mut self, out: &mut [u8], row_bytes: usize) -> bool {
        use skia_safe::{image::CachingHint, AlphaType, ColorType, ISize, ImageInfo};
        let (w, h) = (self.inner.width(), self.inner.height());
        let (Ok(width), Ok(height)) = (usize::try_from(w), usize::try_from(h)) else {
            return false;
        };
        if width == 0 || height == 0 {
            return false;
        }
        let Some(tight_row) = width.checked_mul(4) else {
            return false;
        };
        if row_bytes < tight_row {
            return false;
        }
        let Some(required) = (height - 1)
            .checked_mul(row_bytes)
            .and_then(|rows| rows.checked_add(tight_row))
        else {
            return false;
        };
        if out.len() < required {
            return false;
        }
        let info = ImageInfo::new(
            ISize::new(w, h),
            ColorType::RGBA8888,
            AlphaType::Unpremul,
            None,
        );
        let image = self.inner.image_snapshot();
        if info.valid_row_bytes(row_bytes) {
            return image.read_pixels(&info, out, row_bytes, (0, 0), CachingHint::Allow);
        }
        // The existing FFI accepts arbitrary byte padding, whereas Skia's
        // RGBA strided API requires a pixel-aligned row size. Read each row
        // directly for that uncommon case instead of allocating a full tight
        // frame and copying it back into the caller's padded storage.
        let row_info = ImageInfo::new(
            ISize::new(w, 1),
            ColorType::RGBA8888,
            AlphaType::Unpremul,
            None,
        );
        for row in 0..height {
            let start = row * row_bytes;
            if !image.read_pixels(
                &row_info,
                &mut out[start..start + tight_row],
                tight_row,
                (0, row as i32),
                CachingHint::Allow,
            ) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_surface_has_requested_size() {
        let s = SkiaSurface::new_raster(200, 100);
        assert_eq!(s.width(), 200);
        assert_eq!(s.height(), 100);
    }

    #[test]
    fn blank_surface_encodes_to_png() {
        let mut s = SkiaSurface::new_raster(16, 16);
        let png = s.encode_png().expect("encode");
        // Minimum PNG signature + IHDR is 8 + 25 bytes.
        assert!(png.len() > 33);
        assert_eq!(&png[0..8], b"\x89PNG\r\n\x1a\n");
    }
}
