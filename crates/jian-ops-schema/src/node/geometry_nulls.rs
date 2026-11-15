//! Preserve explicit-null geometry without turning missing coordinates into
//! NaNs or allocating a per-node JSON side table. The canonical node decoder
//! records five presence bits; the serializer appends only still-unset fields.

use super::buf::Buf;
use super::*;
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

const FIELDS: [&str; 5] = ["x", "y", "rotation", "width", "height"];

pub(super) fn from_fields(body: &Buf) -> u8 {
    let Buf::Map(fields) = body else {
        return 0;
    };
    fields.iter().fold(0, |bits, (key, value)| {
        if matches!(value, Buf::Null) {
            bits | FIELDS
                .iter()
                .position(|field| field == key)
                .map_or(0, |i| 1 << i)
        } else {
            bits
        }
    })
}

macro_rules! base_access {
    ($($variant:ident),+ $(,)?) => {
        fn base(node: &PenNode) -> &PenNodeBase {
            match node { $(PenNode::$variant(node) => &node.base,)+ }
        }
        pub(super) fn record(node: &mut PenNode, bits: u8) {
            let bits = if matches!(node, PenNode::Line(_) | PenNode::Ref(_)) { bits & 0b111 } else { bits };
            match node { $(PenNode::$variant(node) => node.base.authored_geometry_nulls = bits,)+ }
        }
    };
}

base_access!(
    Frame,
    Group,
    Rectangle,
    Ellipse,
    Line,
    Polygon,
    Path,
    Text,
    TextInput,
    Image,
    IconFont,
    TextArea,
    Select,
    Switch,
    Checkbox,
    Slider,
    RadioGroup,
    NumberInput,
    Progress,
    Tabs,
    Ref
);

fn remaining(node: &PenNode) -> u8 {
    let base = base(node);
    let mut bits = base.authored_geometry_nulls;
    if bits == 0 {
        return 0;
    }
    for (i, present) in [base.x.is_some(), base.y.is_some(), base.rotation.is_some()]
        .into_iter()
        .enumerate()
    {
        if present {
            bits &= !(1 << i);
        }
    }
    let dimensions = match node {
        PenNode::Frame(n) => Some((&n.container.width, &n.container.height)),
        PenNode::Group(n) => Some((&n.container.width, &n.container.height)),
        PenNode::Rectangle(n) => Some((&n.container.width, &n.container.height)),
        PenNode::Ellipse(n) => Some((&n.width, &n.height)),
        PenNode::Polygon(n) => Some((&n.width, &n.height)),
        PenNode::Path(n) => Some((&n.width, &n.height)),
        PenNode::Text(n) => Some((&n.width, &n.height)),
        PenNode::TextInput(n) => Some((&n.width, &n.height)),
        PenNode::Image(n) => Some((&n.width, &n.height)),
        PenNode::IconFont(n) => Some((&n.width, &n.height)),
        PenNode::TextArea(n) => Some((&n.width, &n.height)),
        PenNode::Select(n) => Some((&n.width, &n.height)),
        PenNode::Switch(n) => Some((&n.width, &n.height)),
        PenNode::Checkbox(n) => Some((&n.width, &n.height)),
        PenNode::Slider(n) => Some((&n.width, &n.height)),
        PenNode::RadioGroup(n) => Some((&n.width, &n.height)),
        PenNode::NumberInput(n) => Some((&n.width, &n.height)),
        PenNode::Progress(n) => Some((&n.width, &n.height)),
        PenNode::Tabs(n) => Some((&n.width, &n.height)),
        PenNode::Line(_) | PenNode::Ref(_) => None,
    };
    if let Some((width, height)) = dimensions {
        if width.is_some() {
            bits &= !(1 << 3);
        }
        if height.is_some() {
            bits &= !(1 << 4);
        }
    } else {
        // Unknown dimensions on these variants remain ignored, as before.
        bits &= 0b111;
    }
    bits
}

#[derive(Clone, Copy)]
struct NullFields(u8);

impl Serialize for NullFields {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.count_ones() as usize))?;
        for (i, key) in FIELDS.iter().enumerate() {
            if self.0 & (1 << i) != 0 {
                map.serialize_entry(key, &())?;
            }
        }
        map.end()
    }
}

#[derive(Serialize)]
struct WithNulls<'a, T: Serialize> {
    #[serde(flatten)]
    node: &'a T,
    #[serde(flatten)]
    nulls: NullFields,
}

macro_rules! node_serializer {
    ($($variant:ident($ty:ty)),+ $(,)?) => {
        #[derive(Serialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum Wire<'a> { $($variant(WithNulls<'a, $ty>),)+ }

        impl Serialize for PenNode {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let nulls = NullFields(remaining(self));
                match self {
                    $(PenNode::$variant(node) => Wire::$variant(WithNulls { node, nulls }).serialize(serializer),)+
                }
            }
        }
    };
}

node_serializer!(
    Frame(FrameNode),
    Group(GroupNode),
    Rectangle(RectangleNode),
    Ellipse(EllipseNode),
    Line(LineNode),
    Polygon(PolygonNode),
    Path(PathNode),
    Text(TextNode),
    TextInput(TextInputNode),
    Image(ImageNode),
    IconFont(IconFontNode),
    TextArea(TextAreaNode),
    Select(SelectNode),
    Switch(SwitchNode),
    Checkbox(CheckboxNode),
    Slider(SliderNode),
    RadioGroup(RadioGroupNode),
    NumberInput(NumberInputNode),
    Progress(ProgressNode),
    Tabs(TabsNode),
    Ref(RefNode)
);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn explicit_null_geometry_round_trips_through_nested_nodes() {
        let value = json!({"type":"group","id":"g","width":null,"height":null,
            "children":[{"type":"path","id":"p","x":null,"y":null,"rotation":null,
                "width":null,"height":null,"d":"M0 0L1 1"}]});
        let node: PenNode = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(node.clone()).unwrap(), value);
        let wire = serde_json::to_string(&node).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&wire).unwrap(), value);
        assert_eq!(serde_json::from_str::<PenNode>(&wire).unwrap(), node);
    }

    #[test]
    fn numeric_edit_overrides_presence_without_duplicate_keys() {
        let mut node: PenNode =
            serde_json::from_str(r#"{"type":"path","id":"p","x":null,"width":null}"#).unwrap();
        let PenNode::Path(path) = &mut node else {
            unreachable!()
        };
        assert_eq!(path.base.x, None);
        assert_eq!(path.width, None);
        path.base.x = Some(-0.0);
        path.width = Some(crate::sizing::SizingBehavior::Number(0.10196078568696976));
        let wire = serde_json::to_string(&node).unwrap();
        assert_eq!(wire.matches("\"x\":").count(), 1);
        assert_eq!(wire.matches("\"width\":").count(), 1);
        let value: Value = serde_json::from_str(&wire).unwrap();
        assert_eq!(value["x"].as_f64().unwrap().to_bits(), (-0.0f64).to_bits());
        assert_eq!(
            value["width"].as_f64().unwrap().to_bits(),
            0.10196078568696976f64.to_bits()
        );
    }

    #[test]
    fn missing_fields_keep_existing_wire_and_unknown_dimensions_stay_ignored() {
        for input in [
            r#"{"type":"group","id":"g"}"#,
            r#"{"type":"path","id":"p","width":17.0}"#,
            r#"{"type":"line","id":"l","x":null}"#,
        ] {
            let node: PenNode = serde_json::from_str(input).unwrap();
            assert_eq!(serde_json::to_string(&node).unwrap(), input);
        }
        let node: PenNode =
            serde_json::from_str(r#"{"type":"line","id":"l","width":null}"#).unwrap();
        assert_eq!(
            serde_json::to_string(&node).unwrap(),
            r#"{"type":"line","id":"l"}"#
        );
    }

    #[test]
    fn document_save_formats_preserve_nulls_and_unquantized_styles() {
        use crate::image_table::{
            write_document, write_document_compact_with_extension, write_document_with_extension,
        };
        let value = json!({"version":"1.0.0","children":[
        {"type":"group","id":"g","width":null,"height":null,"children":[
            {"type":"path","id":"p","x":null,"y":null,"rotation":null,
                "width":null,"height":null,"fill":[{"type":"solid","color":"#fff",
                "opacity":0.05000000074505806}],"stroke":{"thickness":0.5400000214576721}}
        ]}]});
        let document: crate::PenDocument = serde_json::from_value(value.clone()).unwrap();
        let thumbnails = crate::image_thumbs::ImageThumbSnapshot::default();
        for format in 0..3 {
            let mut bytes = Vec::new();
            match format {
                0 => write_document(&mut bytes, &document, &thumbnails),
                1 => write_document_with_extension(
                    &mut bytes,
                    &document,
                    &thumbnails,
                    "editorMeta",
                    &json!({}),
                ),
                _ => write_document_compact_with_extension(
                    &mut bytes,
                    &document,
                    &thumbnails,
                    "editorMeta",
                    &json!({}),
                ),
            }
            .unwrap();
            let mut saved: Value = serde_json::from_slice(&bytes).unwrap();
            saved.as_object_mut().unwrap().remove("editorMeta");
            assert_eq!(saved, value, "format {format}");
            let reloaded: crate::PenDocument = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(serde_json::to_value(reloaded).unwrap(), value);
        }
    }
}
