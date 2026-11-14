//! Heap storage introduced by sparse optional node metadata.
use super::PenNode;
fn storage<T>(value: &Option<Box<T>>) -> usize {
    if value.is_some() {
        std::mem::size_of::<T>()
    } else {
        0
    }
}
fn fill_storage(fills: Option<&[crate::style::PenFill]>) -> usize {
    fills
        .unwrap_or_default()
        .iter()
        .map(|fill| match fill {
            crate::style::PenFill::Image(_) => std::mem::size_of::<crate::style::ImageFillBody>(),
            _ => 0,
        })
        .sum()
}
fn style_storage(
    fills: Option<&[crate::style::PenFill]>,
    stroke: Option<&crate::style::PenStroke>,
) -> usize {
    fill_storage(fills)
        + stroke.map_or(0, |_| std::mem::size_of::<crate::style::PenStrokeBody>())
        + fill_storage(stroke.and_then(|stroke| stroke.fill.as_deref()))
}
fn state_fill_storage(states: &Option<Box<crate::state_override::WidgetStates>>) -> usize {
    let Some(states) = states.as_deref() else {
        return 0;
    };
    [
        &states.hover,
        &states.pressed,
        &states.focused,
        &states.disabled,
    ]
    .into_iter()
    .flatten()
    .map(|style| style_storage(style.fill.as_deref(), style.stroke.as_ref()))
    .sum()
}
impl PenNode {
    /// Bytes in sparse metadata and image-paint boxes, excluding their own nested
    /// buffers/maps. None allocates nothing; Some(empty) retains its allocation.
    /// Memory ledgers add this to existing nested-buffer accounting.
    pub fn boxed_optional_storage_bytes(&self) -> usize {
        let metadata = match self {
            PenNode::Frame(node) => {
                style_storage(
                    node.container.fill.as_deref(),
                    node.container.stroke.as_ref(),
                ) + storage(&node.events)
                    + storage(&node.lifecycle)
                    + storage(&node.semantics)
                    + storage(&node.gestures)
            }
            PenNode::Group(node) => {
                style_storage(
                    node.container.fill.as_deref(),
                    node.container.stroke.as_ref(),
                ) + storage(&node.events)
                    + storage(&node.lifecycle)
                    + storage(&node.semantics)
                    + storage(&node.gestures)
            }
            PenNode::Rectangle(node) => {
                style_storage(
                    node.container.fill.as_deref(),
                    node.container.stroke.as_ref(),
                ) + storage(&node.events)
                    + storage(&node.lifecycle)
                    + storage(&node.semantics)
                    + storage(&node.gestures)
            }
            PenNode::Ellipse(node) => {
                style_storage(node.fill.as_deref(), node.stroke.as_ref())
                    + storage(&node.events)
                    + storage(&node.lifecycle)
                    + storage(&node.semantics)
                    + storage(&node.gestures)
            }
            PenNode::Line(node) => {
                style_storage(None, node.stroke.as_ref())
                    + storage(&node.events)
                    + storage(&node.lifecycle)
                    + storage(&node.semantics)
                    + storage(&node.gestures)
            }
            PenNode::Polygon(node) => {
                style_storage(node.fill.as_deref(), node.stroke.as_ref())
                    + storage(&node.events)
                    + storage(&node.lifecycle)
                    + storage(&node.semantics)
                    + storage(&node.gestures)
            }
            PenNode::Path(node) => {
                style_storage(node.fill.as_deref(), node.stroke.as_ref())
                    + storage(&node.events)
                    + storage(&node.lifecycle)
                    + storage(&node.semantics)
                    + storage(&node.gestures)
            }
            PenNode::Text(node) => {
                style_storage(node.fill.as_deref(), None)
                    + storage(&node.events)
                    + storage(&node.lifecycle)
                    + storage(&node.semantics)
                    + storage(&node.gestures)
            }
            PenNode::TextInput(node) => {
                style_storage(node.fill.as_deref(), node.stroke.as_ref())
                    + storage(&node.events)
                    + storage(&node.lifecycle)
                    + storage(&node.semantics)
                    + storage(&node.gestures)
                    + storage(&node.states)
                    + state_fill_storage(&node.states)
            }
            PenNode::Image(node) => {
                storage(&node.events)
                    + storage(&node.lifecycle)
                    + storage(&node.semantics)
                    + storage(&node.gestures)
            }
            PenNode::IconFont(node) => {
                style_storage(node.fill.as_deref(), node.stroke.as_ref())
                    + storage(&node.events)
                    + storage(&node.lifecycle)
                    + storage(&node.semantics)
                    + storage(&node.gestures)
            }
            PenNode::TextArea(node) => {
                style_storage(node.fill.as_deref(), node.stroke.as_ref())
                    + storage(&node.events)
                    + storage(&node.lifecycle)
                    + storage(&node.semantics)
                    + storage(&node.gestures)
                    + storage(&node.states)
                    + state_fill_storage(&node.states)
            }
            PenNode::Select(node) => {
                style_storage(node.fill.as_deref(), node.stroke.as_ref())
                    + storage(&node.events)
                    + storage(&node.lifecycle)
                    + storage(&node.semantics)
                    + storage(&node.gestures)
                    + storage(&node.states)
                    + state_fill_storage(&node.states)
            }
            PenNode::Switch(node) => {
                style_storage(node.fill.as_deref(), node.stroke.as_ref())
                    + storage(&node.events)
                    + storage(&node.lifecycle)
                    + storage(&node.semantics)
                    + storage(&node.gestures)
                    + storage(&node.states)
                    + state_fill_storage(&node.states)
            }
            PenNode::Checkbox(node) => {
                style_storage(node.fill.as_deref(), node.stroke.as_ref())
                    + storage(&node.events)
                    + storage(&node.lifecycle)
                    + storage(&node.semantics)
                    + storage(&node.gestures)
                    + storage(&node.states)
                    + state_fill_storage(&node.states)
            }
            PenNode::Slider(node) => {
                style_storage(node.fill.as_deref(), node.stroke.as_ref())
                    + storage(&node.events)
                    + storage(&node.lifecycle)
                    + storage(&node.semantics)
                    + storage(&node.gestures)
                    + storage(&node.states)
                    + state_fill_storage(&node.states)
            }
            PenNode::RadioGroup(node) => {
                style_storage(node.fill.as_deref(), node.stroke.as_ref())
                    + storage(&node.events)
                    + storage(&node.lifecycle)
                    + storage(&node.semantics)
                    + storage(&node.gestures)
                    + storage(&node.states)
                    + state_fill_storage(&node.states)
            }
            PenNode::NumberInput(node) => {
                style_storage(node.fill.as_deref(), node.stroke.as_ref())
                    + storage(&node.events)
                    + storage(&node.lifecycle)
                    + storage(&node.semantics)
                    + storage(&node.gestures)
                    + storage(&node.states)
                    + state_fill_storage(&node.states)
            }
            PenNode::Progress(node) => {
                style_storage(node.fill.as_deref(), node.stroke.as_ref())
                    + storage(&node.events)
                    + storage(&node.lifecycle)
                    + storage(&node.semantics)
                    + storage(&node.gestures)
                    + storage(&node.states)
                    + state_fill_storage(&node.states)
            }
            PenNode::Tabs(node) => {
                style_storage(node.fill.as_deref(), node.stroke.as_ref())
                    + storage(&node.events)
                    + storage(&node.lifecycle)
                    + storage(&node.semantics)
                    + storage(&node.gestures)
                    + storage(&node.states)
                    + state_fill_storage(&node.states)
            }
            PenNode::Ref(node) => {
                storage(&node.events)
                    + storage(&node.lifecycle)
                    + storage(&node.semantics)
                    + storage(&node.gestures)
            }
        };
        metadata
            + self
                .motion_declarations()
                .0
                .map_or(0, |_| std::mem::size_of::<crate::motion::Transition>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sparse_boxes_charge_none_empty_and_rich_metadata_exactly() {
        let none: PenNode = serde_json::from_str(r#"{"type":"number_input","id":"n"}"#).unwrap();
        assert_eq!(none.boxed_optional_storage_bytes(), 0);
        let empty:PenNode=serde_json::from_str(r#"{"type":"number_input","id":"n","events":{},"lifecycle":{},"semantics":{},"gestures":{},"states":{}}"#).unwrap();
        let expected = std::mem::size_of::<crate::events::EventHandlers>()
            + std::mem::size_of::<crate::lifecycle::NodeLifecycleHooks>()
            + std::mem::size_of::<crate::semantics::SemanticsMeta>()
            + std::mem::size_of::<crate::gestures::GestureOverrides>()
            + std::mem::size_of::<crate::state_override::WidgetStates>();
        assert_eq!(empty.boxed_optional_storage_bytes(), expected);
        let value = serde_json::to_value(&empty).unwrap();
        for field in ["events", "lifecycle", "semantics", "gestures", "states"] {
            assert_eq!(value[field], serde_json::json!({}))
        }
        let rich:PenNode=serde_json::from_str(r#"{"type":"text","id":"t","content":"x","events":{"future":{"field":[1,2]}},"gestures":{"future":{"neg":-0.0}}}"#).unwrap();
        let clone = rich.clone();
        assert_eq!(
            serde_json::to_vec(&rich).unwrap(),
            serde_json::to_vec(&clone).unwrap()
        );
        assert_eq!(
            rich.boxed_optional_storage_bytes(),
            std::mem::size_of::<crate::events::EventHandlers>()
                + std::mem::size_of::<crate::gestures::GestureOverrides>()
        );
    }
    #[test]
    fn boxed_transition_retains_default_rich_wire_bits_and_accounting() {
        use crate::motion::Transition;
        for source in [
            r#"{}"#,
            r#"{"durationMs":37,"easing":{"cubicBezier":[-0.0,0.25,0.5,1.0]},"properties":[],"future":{"signed":-0.0,"items":[1,"x"]}}"#,
        ] {
            let direct: Transition = serde_json::from_str(source).unwrap();
            let node: PenNode = serde_json::from_str(&format!(
                r#"{{"type":"frame","id":"f","transition":{source}}}"#
            ))
            .unwrap();
            let transition = node.motion_declarations().0.unwrap();
            assert_eq!(
                serde_json::to_vec(transition).unwrap(),
                serde_json::to_vec(&direct).unwrap()
            );
            assert_eq!(
                serde_json::to_vec(&node.clone()).unwrap(),
                serde_json::to_vec(&node).unwrap()
            );
            assert_eq!(
                node.boxed_optional_storage_bytes(),
                std::mem::size_of::<Transition>()
            );
        }
        let absent: PenNode = serde_json::from_str(r#"{"type":"frame","id":"f"}"#).unwrap();
        assert!(absent.motion_declarations().0.is_none());
        assert_eq!(absent.boxed_optional_storage_bytes(), 0);
        assert!(
            std::mem::size_of::<Option<Box<Transition>>>()
                < std::mem::size_of::<Option<Transition>>()
        );
    }
}
