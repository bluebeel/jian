//! Only canonical styled-text paint slots are image resources. Free-form
//! content on other node kinds remains outside the structural image visitor.
use serde_json::Value;

pub(super) fn visit_mut(content: &mut Value, f: &mut impl FnMut(&mut String)) {
    let Value::Array(segments) = content else {
        return;
    };
    for segment in segments {
        if let Some(fills) = segment
            .as_object_mut()
            .and_then(|segment| segment.get_mut("fillPaints"))
        {
            super::visit_fill_array_mut(fills, f);
        }
    }
}

pub(super) fn visit_ref(content: &Value, f: &mut impl FnMut(&str)) {
    let Value::Array(segments) = content else {
        return;
    };
    for segment in segments {
        if let Some(fills) = segment
            .as_object()
            .and_then(|segment| segment.get("fillPaints"))
        {
            super::visit_fill_array_ref(fills, f);
        }
    }
}
