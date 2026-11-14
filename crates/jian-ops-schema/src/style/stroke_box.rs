//! Sparse canonical strokes retain their wire shape without occupying every node.
use super::PenStrokeBody;
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "export-ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "export-ts", ts(export, export_to = "ops.ts"))]
#[serde(transparent)]
pub struct PenStroke(Box<PenStrokeBody>);

impl From<PenStrokeBody> for PenStroke {
    fn from(body: PenStrokeBody) -> Self {
        Self(Box::new(body))
    }
}

impl Deref for PenStroke {
    type Target = PenStrokeBody;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PenStroke {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl PenStroke {
    /// Recover owned fields when consuming a stroke rather than borrowing it.
    pub fn into_body(self) -> PenStrokeBody {
        *self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::StrokeThickness;

    #[test]
    fn stroke_wrapper_retains_exact_canonical_wire_and_numeric_bits() {
        let source = r##"{"thickness":0.5400000214576721,"align":"inside","dashPattern":[-0.0,0.03365384787321091],"dashOffset":0.05000000074505806,"fill":[{"type":"solid","color":"#123456","opacity":0.10196078568696976}]}"##;
        let body: PenStrokeBody = serde_json::from_str(source).unwrap();
        let stroke: PenStroke = serde_json::from_str(source).unwrap();
        assert_eq!(
            serde_json::to_vec(&stroke).unwrap(),
            serde_json::to_vec(&body).unwrap()
        );
        let again: PenStroke =
            serde_json::from_slice(&serde_json::to_vec(&stroke).unwrap()).unwrap();
        let StrokeThickness::Uniform(value) = &again.thickness else {
            panic!("uniform")
        };
        assert_eq!(value.to_bits(), 0.5400000214576721f64.to_bits());
        assert_eq!(
            again.dash_pattern.as_ref().unwrap()[0].to_bits(),
            (-0.0f64).to_bits()
        );
        assert_eq!(
            again.dash_offset.unwrap().to_bits(),
            0.05000000074505806f64.to_bits()
        );
    }

    #[test]
    fn borrowed_mutation_and_owned_recovery_keep_the_same_fields() {
        let mut stroke: PenStroke = serde_json::from_str(r#"{"thickness":1}"#).unwrap();
        stroke.thickness = StrokeThickness::Uniform(2.125);
        stroke.dash_pattern = Some(vec![3.5, 7.25]);
        let clone = stroke.clone();
        let body = stroke.into_body();
        assert_eq!(body.thickness, StrokeThickness::Uniform(2.125));
        assert_eq!(body.dash_pattern, Some(vec![3.5, 7.25]));
        assert_eq!(
            serde_json::to_vec(&body).unwrap(),
            serde_json::to_vec(&clone).unwrap()
        );
    }

    #[test]
    fn absent_strokes_use_one_pointer_slot_without_an_allocation() {
        assert_eq!(
            std::mem::size_of::<PenStroke>(),
            std::mem::size_of::<usize>()
        );
        assert_eq!(
            std::mem::size_of::<Option<PenStroke>>(),
            std::mem::size_of::<usize>()
        );
        assert!(std::mem::size_of::<PenStroke>() < std::mem::size_of::<PenStrokeBody>());
        let node: crate::node::PenNode =
            serde_json::from_str(r#"{"type":"frame","id":"plain"}"#).unwrap();
        assert_eq!(node.boxed_optional_storage_bytes(), 0);
    }
}
