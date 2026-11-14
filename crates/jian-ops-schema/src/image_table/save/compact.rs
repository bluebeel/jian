//! Compact, allocation-bounded canonical saves without host extensions.

use super::{order, with_collector, SaveCollector, SaveImageOrder, SaveTableWriteStats};
use crate::image_thumbs::ImageThumbSnapshot;
use serde::Serialize;
use std::io::{Error, ErrorKind, Write};

/// Write the complete typed document and shared image tables directly to `writer`.
///
/// This is the compact equivalent of serializing to a JSON value, calling
/// `externalize_images`, and serializing that value. It adds no host metadata.
/// Payloads remain shared through `Arc`; only image identities and references
/// are collected. The writer is not flushed, matching `serde_json::to_writer`.
pub fn write_document<W: Write, D: Serialize + SaveImageOrder + ?Sized>(
    writer: &mut W,
    document: &D,
    thumbnails: &ImageThumbSnapshot,
) -> serde_json::Result<SaveTableWriteStats> {
    let mut collector = SaveCollector::default();
    order::prepare(document, &mut collector);
    with_collector(collector, thumbnails, |tables| {
        let mut root = ObjectTail::new(writer);
        document.serialize(&mut serde_json::Serializer::with_formatter(
            &mut root,
            LegacyValueNumbers::default(),
        ))?;
        let mut has_fields = root.finish()?;
        let wrote_images_table = !tables.images_is_empty();
        if wrote_images_table {
            field(writer, &mut has_fields, "images", &tables.images())?;
        }
        let image_thumbs = tables.image_thumbs();
        let wrote_image_thumbs = !image_thumbs.is_empty();
        if wrote_image_thumbs {
            field(writer, &mut has_fields, "imageThumbs", &image_thumbs)?;
        }
        writer.write_all(b"}").map_err(serde_json::Error::io)?;
        Ok(SaveTableWriteStats {
            wrote_images_table,
            wrote_image_thumbs,
        })
    })
}

fn field(
    writer: &mut impl Write,
    has_fields: &mut bool,
    name: &str,
    value: &impl Serialize,
) -> serde_json::Result<()> {
    if *has_fields {
        writer.write_all(b",").map_err(serde_json::Error::io)?;
    }
    serde_json::to_writer(&mut *writer, name)?;
    writer.write_all(b":").map_err(serde_json::Error::io)?;
    serde_json::to_writer(writer, value)?;
    *has_fields = true;
    Ok(())
}

/// Hold only the final byte so the root's closing brace can be replaced with
/// optional tables. Nested objects and escaped braces pass through untouched.
struct ObjectTail<'a, W> {
    inner: &'a mut W,
    last: Option<u8>,
    len: usize,
}

impl<'a, W: Write> ObjectTail<'a, W> {
    fn new(inner: &'a mut W) -> Self {
        Self {
            inner,
            last: None,
            len: 0,
        }
    }

    fn finish(self) -> serde_json::Result<bool> {
        if self.last != Some(b'}') || self.len < 2 {
            return Err(serde_json::Error::io(Error::new(
                ErrorKind::InvalidData,
                "document must serialize as an object",
            )));
        }
        Ok(self.len > 2)
    }
}

impl<W: Write> Write for ObjectTail<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        if self.len == 0 && bytes[0] != b'{' {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "document must serialize as an object",
            ));
        }
        if let Some(last) = self.last {
            self.inner.write_all(&[last])?;
        }
        self.inner.write_all(&bytes[..bytes.len() - 1])?;
        self.last = bytes.last().copied();
        self.len += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// `to_value` stores finite f32 values as f64 JSON numbers. Keep their exact
/// numerical value instead of shortening them to a merely f32-roundtrippable
/// decimal. Float map keys remain strings with the ordinary f32 formatting.
#[derive(Default)]
struct LegacyValueNumbers {
    in_string: bool,
}
impl serde_json::ser::Formatter for LegacyValueNumbers {
    fn begin_string<W: Write + ?Sized>(&mut self, writer: &mut W) -> std::io::Result<()> {
        self.in_string = true;
        writer.write_all(b"\"")
    }
    fn end_string<W: Write + ?Sized>(&mut self, writer: &mut W) -> std::io::Result<()> {
        self.in_string = false;
        writer.write_all(b"\"")
    }
    fn write_f32<W: Write + ?Sized>(&mut self, writer: &mut W, value: f32) -> std::io::Result<()> {
        if self.in_string {
            serde_json::ser::Formatter::write_f32(
                &mut serde_json::ser::CompactFormatter,
                writer,
                value,
            )
        } else {
            serde_json::ser::Formatter::write_f64(
                &mut serde_json::ser::CompactFormatter,
                writer,
                value as f64,
            )
        }
    }
}

#[cfg(test)]
#[path = "compact_tests.rs"]
mod tests;
