use super::*;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

fn round_trip<T: DeserializeOwned + Serialize>(source: Value) {
    let typed: T = serde_json::from_str(&source.to_string()).expect("typed style");
    let encoded = serde_json::to_string(&typed).expect("serialize style");
    let after: Value = serde_json::from_str(&encoded).expect("saved style JSON");
    assert_eq!(after, source);
    assert_numeric_bits(&source, &after);
}

fn assert_numeric_bits(before: &Value, after: &Value) {
    match before {
        Value::Number(number) => assert_eq!(
            number.as_f64().unwrap().to_bits(),
            after.as_f64().unwrap().to_bits(),
            "{before} became {after}"
        ),
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                assert_numeric_bits(item, &after[index]);
            }
        }
        Value::Object(items) => {
            for (key, item) in items {
                assert_numeric_bits(item, &after[key]);
            }
        }
        _ => {}
    }
}

#[test]
fn authored_style_numbers_preserve_both_decimal_and_widened_f32_inputs() {
    for number in [
        0.05,
        0.05000000074505806,
        0.10196078568696976,
        0.03365384787321091,
        0.5400000214576721,
        -0.0,
    ] {
        round_trip::<PenFill>(json!({"type":"solid", "color":"#123456", "opacity":number}));
        round_trip::<PenFill>(json!({"type":"linear_gradient", "angle":number,
            "opacity":number, "stops":[{"offset":number,"color":"#123456"}]}));
        round_trip::<PenFill>(json!({"type":"radial_gradient", "cx":number, "cy":number,
            "radius":number,"opacity":number,"stops":[{"offset":number,"color":"#123456"}]}));
        round_trip::<PenStroke>(
            json!({"thickness":number,"dashPattern":[number],"dashOffset":number}),
        );
        round_trip::<PenStroke>(json!({"thickness":[number,number,number,number]}));
        round_trip::<PenStroke>(
            json!({"thickness":{"top":number,"right":number,"bottom":number,"left":number}}),
        );
        round_trip::<PenEffect>(json!({"type":"shadow", "offsetX":number,"offsetY":number,
            "blur":number,"spread":number,"color":"#123456"}));
        round_trip::<PenEffect>(json!({"type":"blur","radius":number}));
        round_trip::<StyledTextSegment>(json!({"text":"authored", "fontSize":number}));
    }
}

#[test]
fn authored_image_and_shader_numbers_remain_exact() {
    let number = 0.10196078568696976;
    round_trip::<PenFill>(json!({"type":"image", "url":"image.png",
        "originalSize":{"width":number,"height":number},
        "transform":{"m00":number,"m01":number,"m02":number,
            "m10":number,"m11":number,"m12":number},
        "rotation":number,"tileScale":number,"opacity":number,
        "exposure":number,"contrast":number,"saturation":number,"temperature":number,
        "tint":number,"highlights":number,"shadows":number}));
    round_trip::<PenFill>(json!({"type":"shader", "opacity":number,
        "uniforms":{"scalar":number,"vector":[number,number]}}));
    round_trip::<PenFill>(json!({"type":"mesh_gradient", "rows":2,"cols":2,
        "stops":[],"opacity":number}));
}

#[test]
fn image_fill_boxing_bounds_the_common_fill_element_size() {
    assert!(std::mem::size_of::<PenFill>() <= 144);
    assert!(std::mem::size_of::<PenFill>() < std::mem::size_of::<ImageFillBody>());
}

#[test]
fn image_boxes_in_node_strokes_and_widget_states_are_charged() {
    let image = json!({"type":"image","url":"image.png"});
    let node: crate::node::PenNode = serde_json::from_value(json!({
        "type":"number_input","id":"node","fill":[image.clone()],
        "stroke":{"thickness":1.0,"fill":[image.clone()]},
        "states":{"hover":{"fill":[image.clone()],
            "stroke":{"thickness":1.0,"fill":[image]}}}
    }))
    .unwrap();
    assert_eq!(
        node.boxed_optional_storage_bytes(),
        4 * std::mem::size_of::<ImageFillBody>()
            + 2 * std::mem::size_of::<PenStrokeBody>()
            + std::mem::size_of::<crate::state_override::WidgetStates>()
    );
    assert_eq!(
        node.clone().boxed_optional_storage_bytes(),
        node.boxed_optional_storage_bytes()
    );
}
