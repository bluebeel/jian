use super::*;
use crate::image_table::externalize_images;
use crate::node::{ImageSrc, PenNode};
use crate::PenDocument;
use serde_json::{json, Value};

fn source() -> String {
    format!("data:image/png;base64,stream-compact-{}", "A".repeat(4096))
}

fn document() -> PenDocument {
    serde_json::from_value(json!({
        "version":"1.0.0",
        "pages":[{"id":"page", "name":"Quoted \"page\"\n🦀", "children":[
            {"type":"image", "id":"p1", "src":source()}
        ]}],
        "children":[
            {"type":"image", "id":"shared", "src":source()},
            {"type":"rectangle", "id":"fill", "name":"\\ \" } { \u{0000}", "fill":[
                {"type":"image", "url":source(), "thumbnailUrl":source(), "opacity":0.35, "rotation":0.1},
                {"type":"image", "url":"data:image/png;base64,tiny"}
            ]},
            {"type":"image", "id":"remote", "src":"https://example.invalid/image.png"}
        ]
    }))
    .unwrap()
}

fn legacy<D: Serialize>(doc: &D) -> Value {
    let mut value = serde_json::to_value(doc).unwrap();
    externalize_images(&mut value);
    value
}

fn check_compact_matches_legacy_json_and_deduplicates_equal_and_shared_images() {
    let mut document = document();
    let PenNode::Image(first) = &document.children[0] else {
        panic!()
    };
    let shared = first.src.clone();
    document
        .children
        .push(PenNode::Image(crate::node::ImageNode {
            nearest_sampling: None,
            geometry: None,
            src: shared,
            ..match document.children[0].clone() {
                PenNode::Image(image) => image,
                _ => unreachable!(),
            }
        }));
    let before = serde_json::to_value(&document).unwrap();
    let mut output = Vec::new();
    let stats = write_document(&mut output, &document, &ImageThumbSnapshot::default()).unwrap();
    let actual: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(actual, legacy(&document));
    assert!(stats.wrote_images_table);
    assert_eq!(actual["images"].as_object().unwrap().len(), 1);
    assert!(actual.get("editorMeta").is_none());
    assert!(actual["children"][1]["fill"][1]["url"]
        .as_str()
        .unwrap()
        .starts_with("data:"));
    assert_eq!(
        serde_json::to_value(&document).unwrap(),
        before,
        "save does not mutate the document"
    );
    assert_eq!(
        String::from_utf8(output).unwrap(),
        actual.to_string(),
        "retain compact legacy field/table order"
    );
}

#[derive(Serialize)]
struct ExtraDocument<'a> {
    #[serde(flatten)]
    document: &'a PenDocument,
    future: &'a Value,
}
impl SaveImageOrder for ExtraDocument<'_> {
    fn visit_save_image_sources(&self, visit: &mut dyn FnMut(&ImageSrc)) {
        self.document.visit_save_image_sources(visit);
    }
}

fn check_compact_keeps_unknown_json_payloads_and_escaped_text_unchanged() {
    let document = document();
    let unknown = json!({"future":"quoted \" \\ \n\t🦀", "literal":"{}", "nodeLike":{
        "type":"image", "src":source(), "fill":[{"type":"image", "url":source()}]
    }, "array":[null, 5, {"newFeature":true}]});
    let extended = ExtraDocument {
        document: &document,
        future: &unknown,
    };
    let mut output = Vec::new();
    write_document(&mut output, &extended, &ImageThumbSnapshot::default()).unwrap();
    let actual: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(actual, legacy(&extended));
    assert_eq!(actual["future"], unknown);
}

#[test]
fn compact_preserves_referenced_thumbnails_without_host_metadata() {
    crate::image_thumbs::with_test_serialized(|| {
        crate::image_thumbs::clear_registry();
        let document = document();
        let paint_id = crate::node::image_src::paint_image_id(&source());
        crate::image_thumbs::store_thumb(paint_id, vec![0xff, 0xd8, 0xff, 0xd9]);
        let mut output = Vec::new();
        let stats = write_document(
            &mut output,
            &document,
            &crate::image_thumbs::capture_snapshot(),
        )
        .unwrap();
        let actual: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(actual, legacy(&document));
        assert!(stats.wrote_image_thumbs);
        assert_eq!(
            actual["imageThumbs"],
            json!({paint_id.to_string():"/9j/2Q=="})
        );
        assert!(actual.get("editorMeta").is_none());
        crate::image_thumbs::clear_registry();
    });
}

fn check_compact_handles_no_images_and_empty_serialized_objects() {
    for value in [
        json!({}),
        json!({"version":"1.0.0", "children":[], "escaped":"\\\"}\n"}),
    ] {
        struct Empty<'a>(&'a Value);
        impl Serialize for Empty<'_> {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                self.0.serialize(s)
            }
        }
        impl SaveImageOrder for Empty<'_> {
            fn visit_save_image_sources(&self, _: &mut dyn FnMut(&ImageSrc)) {}
        }
        let mut output = Vec::new();
        assert_eq!(
            write_document(&mut output, &Empty(&value), &ImageThumbSnapshot::default()).unwrap(),
            SaveTableWriteStats::default()
        );
        assert_eq!(serde_json::from_slice::<Value>(&output).unwrap(), value);
    }
}

fn check_compact_propagates_short_writes_and_errors_and_restores_image_scope() {
    struct Limited {
        remaining: usize,
        bytes: Vec<u8>,
        fail: bool,
    }
    impl Write for Limited {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.remaining == 0 && self.fail {
                return Err(Error::other("test disk full"));
            }
            let len = bytes.len().min(3).min(self.remaining);
            self.bytes.extend_from_slice(&bytes[..len]);
            self.remaining -= len;
            Ok(len)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let document = document();
    let mut successful = Limited {
        remaining: usize::MAX,
        bytes: Vec::new(),
        fail: false,
    };
    write_document(&mut successful, &document, &ImageThumbSnapshot::default()).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&successful.bytes).unwrap(),
        legacy(&document)
    );
    for limit in [5, successful.bytes.len() - 20] {
        let mut failed = Limited {
            remaining: limit,
            bytes: Vec::new(),
            fail: true,
        };
        let error =
            write_document(&mut failed, &document, &ImageThumbSnapshot::default()).unwrap_err();
        assert!(error.is_io());
        assert_eq!(error.io_error_kind(), Some(ErrorKind::Other));
        assert!(serde_json::to_string(&ImageSrc::from(source()))
            .unwrap()
            .contains("data:image/png"));
    }
}

#[test]
fn compact_matches_legacy_json_and_deduplicates_equal_and_shared_images() {
    crate::image_thumbs::with_test_serialized(|| {
        crate::image_thumbs::clear_registry();
        check_compact_matches_legacy_json_and_deduplicates_equal_and_shared_images();
        crate::image_thumbs::clear_registry();
    });
}

#[test]
fn compact_keeps_unknown_json_payloads_and_escaped_text_unchanged() {
    crate::image_thumbs::with_test_serialized(|| {
        crate::image_thumbs::clear_registry();
        check_compact_keeps_unknown_json_payloads_and_escaped_text_unchanged();
        crate::image_thumbs::clear_registry();
    });
}

#[test]
fn compact_handles_no_images_and_empty_serialized_objects() {
    crate::image_thumbs::with_test_serialized(|| {
        crate::image_thumbs::clear_registry();
        check_compact_handles_no_images_and_empty_serialized_objects();
        crate::image_thumbs::clear_registry();
    });
}

#[test]
fn compact_propagates_short_writes_and_errors_and_restores_image_scope() {
    crate::image_thumbs::with_test_serialized(|| {
        crate::image_thumbs::clear_registry();
        check_compact_propagates_short_writes_and_errors_and_restores_image_scope();
        crate::image_thumbs::clear_registry();
    });
}

#[test]
fn compact_preserves_legacy_f32_numbers_and_float_key_strings() {
    struct FloatKeys;
    impl Serialize for FloatKeys {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeMap;
            let mut map = serializer.serialize_map(Some(2))?;
            map.serialize_entry(&0.1f32, &0.35f32)?;
            map.serialize_entry(&-0.0f32, &f32::INFINITY)?;
            map.end()
        }
    }
    #[derive(Serialize)]
    struct Numeric {
        values: [f32; 6],
        keys: FloatKeys,
    }
    impl SaveImageOrder for Numeric {
        fn visit_save_image_sources(&self, _: &mut dyn FnMut(&ImageSrc)) {}
    }
    let document = Numeric {
        values: [
            0.1,
            -0.0,
            f32::MIN_POSITIVE,
            f32::MAX,
            f32::NAN,
            f32::NEG_INFINITY,
        ],
        keys: FloatKeys,
    };
    let expected = serde_json::to_value(&document).unwrap();
    let mut actual = Vec::new();
    write_document(&mut actual, &document, &ImageThumbSnapshot::default()).unwrap();
    assert_eq!(String::from_utf8(actual).unwrap(), expected.to_string());
}
