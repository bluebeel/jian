use crate::node::PenNode;
#[test]
fn decoration_metadata_roundtrips_false_and_keeps_legacy_absence() {
    let node: PenNode = serde_json::from_value(serde_json::json!({
        "id":"text", "type":"text", "textDecorationSkipInk":false,
        "content":[{"text":"A", "textDecorationSkipInk":true}, {"text":"é"}]
    }))
    .unwrap();
    let value = serde_json::to_value(&node).unwrap();
    assert_eq!(value["textDecorationSkipInk"], false);
    assert_eq!(value["content"][0]["textDecorationSkipInk"], true);
    assert!(value["content"][1].get("textDecorationSkipInk").is_none());
    let legacy: PenNode =
        serde_json::from_value(serde_json::json!({"id":"old","type":"text","content":"legacy"}))
            .unwrap();
    assert!(serde_json::to_value(legacy)
        .unwrap()
        .get("textDecorationSkipInk")
        .is_none());
}
