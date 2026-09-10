// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Borrow the already normalized flat value tree during owned deserialization.
//! Raw source is never re-parsed and no intermediate `serde_json::Value` is cloned.
//! This helper establishes a Rust type, not schema/version/root admission.

use std::fmt;

use serde::de::{self, DeserializeOwned, IntoDeserializer, Visitor};
use serde::{forward_to_deserialize_any, Deserializer};

use super::{MaterializedDocument, NodeId, NodeKind, PortableNumber};

/// Deliberately content-free: serde's default errors can interpolate rejected
/// values, enum variants, and map keys. Structural diagnostics are separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DecodeError;

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("materialized value does not match the authoring type")
    }
}

impl std::error::Error for DecodeError {}

impl de::Error for DecodeError {
    fn custom<T: fmt::Display>(_: T) -> Self {
        Self
    }
}

pub(super) fn decode<T: DeserializeOwned>(
    doc: &MaterializedDocument,
    id: NodeId,
) -> Result<T, DecodeError> {
    T::deserialize(NodeRef { doc, id })
}

#[derive(Clone, Copy)]
struct NodeRef<'doc> {
    doc: &'doc MaterializedDocument,
    id: NodeId,
}

impl<'de> IntoDeserializer<'de, DecodeError> for NodeRef<'de> {
    type Deserializer = Self;
    fn into_deserializer(self) -> Self {
        self
    }
}

impl<'de> Deserializer<'de> for NodeRef<'de> {
    type Error = DecodeError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        match self.doc.node(self.id).kind() {
            NodeKind::Null => visitor.visit_unit(),
            NodeKind::Boolean(value) => visitor.visit_bool(*value),
            NodeKind::Number(value) => visit_number(*value, visitor),
            NodeKind::String(value) => visitor.visit_borrowed_str(value),
            NodeKind::Array(items) => {
                let values = items.iter().map(|&id| NodeRef { doc: self.doc, id });
                let mut access = de::value::SeqDeserializer::new(values);
                let value = visitor.visit_seq(&mut access)?;
                access.end()?;
                Ok(value)
            }
            NodeKind::Object(members) => {
                let values = members.iter().map(|(name, member)| {
                    (
                        de::value::BorrowedStrDeserializer::<DecodeError>::new(name),
                        NodeRef {
                            doc: self.doc,
                            id: member.value(),
                        },
                    )
                });
                let mut access = de::value::MapDeserializer::new(values);
                let value = visitor.visit_map(&mut access)?;
                access.end()?;
                Ok(value)
            }
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        if matches!(self.doc.node(self.id).kind(), NodeKind::Null) {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        match self.doc.node(self.id).kind() {
            NodeKind::String(value) => visitor.visit_enum(de::value::BorrowedStrDeserializer::<
                DecodeError,
            >::new(value)),
            // Compound authoring variants are untagged and use deserialize_any.
            // No external tagged-map enum is part of the current closed model.
            _ => Err(DecodeError),
        }
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        // Strict materialization already validated the whole subtree. Ignoring
        // a Rust field cannot trigger re-traversal, coercion, or a raw parse.
        visitor.visit_unit()
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct seq tuple tuple_struct map struct identifier
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "integral portable binary64 is exactly within the checked integer range"
)]
fn visit_number<'de, V: Visitor<'de>>(
    value: PortableNumber,
    visitor: V,
) -> Result<V::Value, DecodeError> {
    let value = value.get();
    if value.fract() == 0.0 {
        // PortableNumber is already finite, normalized, and |value| <= 2^53-1.
        if value >= 0.0 {
            visitor.visit_u64(value as u64)
        } else {
            visitor.visit_i64(value as i64)
        }
    } else {
        visitor.visit_f64(value)
    }
}

#[cfg(test)]
mod tests {
    use crate::fixtures::{complete_config, minimal_config};
    use crate::materialize::materialize_file;
    use crate::materialize_tests::limits;
    use serde::Deserialize;
    use std::sync::Arc;

    fn doc(source: &[u8]) -> super::MaterializedDocument {
        materialize_file(Arc::from(source), limits()).unwrap()
    }

    #[test]
    fn complete_owned_authoring_types_decode_from_normalized_values() {
        for value in [minimal_config(), complete_config()] {
            let analysis = crate::structural::admission_tests::analyze_fixture(&value);
            let typed = analysis.construct().unwrap().unwrap();
            drop(analysis);
            assert_eq!(serde_json::to_value(typed).unwrap(), value);
        }
    }

    #[test]
    fn integer_and_float_targets_use_normalized_binary64_not_raw_tokens() {
        for source in ["-0", "-0.0", "-1e-999"] {
            let materialized = doc(source.as_bytes());
            assert_eq!(
                materialized
                    .decode::<f64>(materialized.root())
                    .unwrap()
                    .to_bits(),
                0.0_f64.to_bits()
            );
            assert_eq!(materialized.decode::<u64>(materialized.root()).unwrap(), 0);
        }
        let materialized = doc(b"9007199254740991.1");
        assert_eq!(
            materialized.decode::<u64>(materialized.root()).unwrap(),
            9_007_199_254_740_991
        );
        let fractional = doc(b"1.5");
        assert!(fractional.decode::<i64>(fractional.root()).is_err());
        assert_eq!(
            fractional
                .decode::<f64>(fractional.root())
                .unwrap()
                .to_bits(),
            1.5_f64.to_bits()
        );
    }

    #[test]
    fn errors_do_not_interpolate_rejected_input() {
        #[derive(Debug, Deserialize)]
        enum Finite {
            Allowed,
        }
        let input = doc(br#""private-token""#);
        let error = input.decode::<Finite>(input.root()).unwrap_err();
        assert!(!format!("{error:?} {error}").contains("private-token"));
    }

    #[test]
    fn type_failure_does_not_produce_partial_owned_configuration() {
        let mut value = complete_config();
        value["profiles"]["app"]["defaultRequestedLocale"] = false.into();
        let analysis = crate::structural::admission_tests::analyze_fixture(&value);
        assert!(analysis.construct().unwrap().is_none());
    }

    #[test]
    fn a_tuple_cannot_silently_ignore_remaining_array_values() {
        let materialized = doc(b"[1,2,3]");
        assert!(materialized
            .decode::<(u64, u64)>(materialized.root())
            .is_err());
        assert_eq!(
            materialized
                .decode::<(u64, u64, u64)>(materialized.root())
                .unwrap(),
            (1, 2, 3)
        );
    }
}
