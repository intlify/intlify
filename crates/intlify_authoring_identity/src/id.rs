// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The identity of one registry chain.

use intlify_authoring::{Opaque128, PrimitiveError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The identity of one owner's registry chain.
///
/// Design 017 spells it like the value of an Intent ID, 32 lowercase
/// hexadecimal digits from 16 random bytes, but gives it a separate role: it
/// names the one chain an owner's identity history lives in, and stays the
/// same for that chain's whole life. Being its own type keeps it from being
/// compared with an Intent ID, or used as one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct RegistryIdentity(Opaque128);

impl RegistryIdentity {
    /// Retain one already generated registry identity.
    ///
    /// This is not a generation. A new chain's identity comes from an
    /// authorized host's explicit initialization, outside this crate.
    pub fn retained(value: &str) -> Result<Self, PrimitiveError> {
        Ok(Self(Opaque128::from_validated(value)?))
    }

    /// Name a chain by a value its initializing host generated.
    #[must_use]
    pub const fn from_value(value: Opaque128) -> Self {
        Self(value)
    }

    /// Borrow the value.
    #[must_use]
    pub const fn value(&self) -> &Opaque128 {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_registry_identity_is_exactly_32_lowercase_hexadecimal_digits() {
        let value = "0123456789abcdef0123456789abcdef";
        let identity = RegistryIdentity::retained(value).unwrap();
        assert_eq!(identity.value().as_str(), value);
        // On the wire it is the bare string, as 017 writes it.
        assert_eq!(serde_json::to_value(&identity).unwrap(), json!(value));
        assert_eq!(
            serde_json::from_value::<RegistryIdentity>(json!(value)).unwrap(),
            identity
        );
        for invalid in [
            "0".repeat(31),
            "0".repeat(33),
            "A".repeat(32),
            "g".repeat(32),
            format!(" {}", "0".repeat(31)),
        ] {
            assert_eq!(
                RegistryIdentity::retained(&invalid),
                Err(PrimitiveError::InvalidToken),
                "{invalid:?}"
            );
            assert!(
                serde_json::from_value::<RegistryIdentity>(json!(invalid)).is_err(),
                "{invalid:?}"
            );
        }
        // A number is not a spelling of one.
        assert!(serde_json::from_value::<RegistryIdentity>(json!(0)).is_err());
    }

    #[test]
    fn a_value_names_a_chain_without_becoming_another_value() {
        let value = Opaque128::from_bytes([7; 16]);
        let identity = RegistryIdentity::from_value(value.clone());
        assert_eq!(identity.value(), &value);
        assert_eq!(
            RegistryIdentity::retained(value.as_str()).unwrap(),
            identity
        );
    }
}
