//! Canonical root maps/structs append fields through serde's streaming flatten
//! adapter, avoiding a one-byte tail adapter around every serializer write.
use super::{
    order, with_collector, SaveCollector, SaveImageOrder, SaveTableWriteStats, SaveTables,
};
use crate::image_thumbs::ImageThumbSnapshot;
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use std::{cell::Cell, io::Write};

/// Compact canonical-object save with the same typed numbers, declaration
/// order, image ordering and extension as the ordinary compact host writer.
/// The two flattened maps stream directly to `writer`: no Value/document tree,
/// final JSON string or per-write root-tail copying is created. Root values
/// must be serde maps/structs; generic legacy writers retain their old surface.
pub fn write_object_compact_with_extension<
    W: Write,
    D: Serialize + SaveImageOrder + ?Sized,
    T: Serialize,
>(
    writer: &mut W,
    document: &D,
    thumbnails: &ImageThumbSnapshot,
    extension_name: &str,
    extension: &T,
) -> serde_json::Result<SaveTableWriteStats> {
    let mut collector = SaveCollector::default();
    order::prepare(document, &mut collector);
    with_collector(collector, thumbnails, |tables| {
        let stats = Cell::new(SaveTableWriteStats::default());
        serde_json::to_writer(
            writer,
            &Appended {
                document,
                extra: Extra {
                    tables,
                    extension_name,
                    extension,
                    stats: &stats,
                },
            },
        )?;
        Ok(stats.get())
    })
}
#[derive(Serialize)]
struct Appended<'a, 't, D: ?Sized, T> {
    #[serde(flatten)]
    document: &'a D,
    #[serde(flatten)]
    extra: Extra<'a, 't, T>,
}
struct Extra<'a, 't, T> {
    tables: &'a SaveTables<'t>,
    extension_name: &'a str,
    extension: &'a T,
    stats: &'a Cell<SaveTableWriteStats>,
}
impl<T: Serialize> Serialize for Extra<'_, '_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wrote_images_table = !self.tables.images_is_empty();
        let thumbs = self.tables.image_thumbs();
        let wrote_image_thumbs = !thumbs.is_empty();
        let mut map = serializer.serialize_map(Some(
            1 + usize::from(wrote_images_table) + usize::from(wrote_image_thumbs),
        ))?;
        if wrote_images_table {
            map.serialize_entry("images", &self.tables.images())?;
        }
        if wrote_image_thumbs {
            map.serialize_entry("imageThumbs", &thumbs)?;
        }
        map.serialize_entry(self.extension_name, self.extension)?;
        self.stats.set(SaveTableWriteStats {
            wrote_images_table,
            wrote_image_thumbs,
        });
        map.end()
    }
}
