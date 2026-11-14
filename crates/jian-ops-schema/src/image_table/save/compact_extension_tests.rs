use super::*;
use crate::image_table::{write_document_compact_with_extension, write_document_with_extension};
use crate::node::ImageSrc;
use crate::PenDocument;
use serde_json::{json, Value};

#[derive(Serialize)]
struct NumericDocument {
    #[serde(flatten)]
    document: PenDocument,
    single_precision: f32,
    exact: f64,
    negative_zero: f64,
    subnormal: f64,
}

impl SaveImageOrder for NumericDocument {
    fn visit_save_image_sources(&self, visit: &mut dyn FnMut(&ImageSrc)) {
        self.document.visit_save_image_sources(visit);
    }
}

fn fixture() -> NumericDocument {
    let source = format!(
        "data:image/png;base64,compact-extension-{}",
        "A".repeat(4096)
    );
    NumericDocument {
        document: serde_json::from_value(json!({
            "version":"1.0.0",
            "children":[
                {"type":"frame","id":"absent"},
                {"type":"frame","id":"empty","children":[]},
                {"type":"image","id":"image-a","src":source},
                {"type":"image","id":"image-b","src":source}
            ]
        }))
        .unwrap(),
        single_precision: 0.1,
        exact: 11.899999618530273,
        negative_zero: -0.0,
        subnormal: f64::from_bits(1),
    }
}

#[test]
fn compact_extension_matches_pretty_values_number_bits_and_field_order() {
    let document = fixture();
    let meta = json!({"activePageIndex":0,"preserveAuthoredGeometry":true,
        "future":"quoted \" \\ \n🦀", "negativeZero":-0.0});
    let thumbnails = ImageThumbSnapshot::default();
    let (mut pretty, mut compact) = (Vec::new(), Vec::new());
    let before =
        write_document_with_extension(&mut pretty, &document, &thumbnails, "editorMeta", &meta)
            .unwrap();
    let after = write_document_compact_with_extension(
        &mut compact,
        &document,
        &thumbnails,
        "editorMeta",
        &meta,
    )
    .unwrap();
    assert_eq!(before.wrote_images_table, after.wrote_images_table);
    assert_eq!(before.wrote_image_thumbs, after.wrote_image_thumbs);
    let value: Value = serde_json::from_slice(&pretty).unwrap();
    assert_eq!(compact, serde_json::to_vec(&value).unwrap());
    assert!(compact.len() < pretty.len());
    assert_eq!(value["images"].as_object().unwrap().len(), 1);
    assert!(value["children"][0].get("children").is_none());
    assert_eq!(value["children"][1]["children"], json!([]));
    assert_eq!(
        value["negative_zero"].as_f64().unwrap().to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(value["subnormal"].as_f64().unwrap().to_bits(), 1);
    assert_eq!(
        value["single_precision"].as_f64().unwrap().to_bits(),
        0.1f64.to_bits()
    );
}

#[test]
fn compact_extension_writer_failure_releases_image_collection_scope() {
    struct FailAfter(usize);
    impl Write for FailAfter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.0 == 0 {
                return Err(std::io::Error::other("injected compact save failure"));
            }
            let count = bytes.len().min(self.0);
            self.0 -= count;
            Ok(count)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let document = fixture();
    let thumbnails = ImageThumbSnapshot::default();
    assert!(write_document_compact_with_extension(
        &mut FailAfter(128),
        &document,
        &thumbnails,
        "editorMeta",
        &json!({}),
    )
    .is_err());
    let ordinary = serde_json::to_value(&document).unwrap();
    assert!(ordinary["children"][2]["src"]
        .as_str()
        .unwrap()
        .starts_with("data:"));
    let mut output = Vec::new();
    write_document_compact_with_extension(
        &mut output,
        &document,
        &thumbnails,
        "editorMeta",
        &json!({}),
    )
    .unwrap();
    let saved: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(saved["images"].as_object().unwrap().len(), 1);
    assert_eq!(saved["editorMeta"], json!({}));
}
