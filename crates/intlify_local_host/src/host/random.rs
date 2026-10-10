// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Fresh identities from operating-system randomness.
//!
//! Design 017 takes an Intent ID, and a registry identity, from 16 fresh
//! bytes of operating-system cryptographic randomness, spelled as 32
//! lowercase hexadecimal digits. Failing to obtain them is an explicit
//! allocation failure: no timestamp, counter, source digest or other value
//! stands in. A value is drawn once and retained in the plan or the genesis
//! it serves; replaying either never draws again.

use intlify_authoring::{MessageIntentId, Opaque128, OwnerIdentity};
use intlify_authoring_identity::RegistryIdentity;

/// Why fresh bytes could not be drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RandomnessFailure;

/// A source of fresh bytes for new identities.
///
/// The host draws from it only to allocate: a registry identity when it
/// initializes a chain, and an Intent ID for each new declaration of a plan.
pub trait Randomness {
    /// Fill every byte, or fail without a value.
    fn fill(&mut self, bytes: &mut [u8; 16]) -> Result<(), RandomnessFailure>;
}

/// Operating-system cryptographic randomness.
#[derive(Debug, Clone, Copy, Default)]
pub struct OsRandomness;

impl Randomness for OsRandomness {
    fn fill(&mut self, bytes: &mut [u8; 16]) -> Result<(), RandomnessFailure> {
        getrandom::fill(bytes).map_err(|_| RandomnessFailure)
    }
}

fn fresh<R: Randomness + ?Sized>(randomness: &mut R) -> Result<Opaque128, RandomnessFailure> {
    let mut bytes = [0; 16];
    randomness.fill(&mut bytes)?;
    Ok(Opaque128::from_bytes(bytes))
}

/// Draw the identity of a new registry chain.
pub(crate) fn registry_identity<R: Randomness + ?Sized>(
    randomness: &mut R,
) -> Result<RegistryIdentity, RandomnessFailure> {
    fresh(randomness).map(RegistryIdentity::from_value)
}

/// Draw `count` Intent IDs of one owner, in the order a plan uses them. A
/// failure gives no IDs at all, never the ones drawn before it.
pub(crate) fn intent_ids<R: Randomness + ?Sized>(
    owner: &OwnerIdentity,
    count: usize,
    randomness: &mut R,
) -> Result<Vec<MessageIntentId>, RandomnessFailure> {
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        let value = fresh(randomness)?;
        // Spelling 16 bytes always gives a valid value.
        ids.push(
            MessageIntentId::retained(owner.clone(), value.as_str())
                .expect("16 bytes spell a valid Intent ID value"),
        );
    }
    Ok(ids)
}

#[cfg(test)]
pub(crate) mod scripted {
    use super::{Randomness, RandomnessFailure};

    /// Bytes given in advance, then a failure: a test's stand-in for the
    /// operating system, counting what was drawn.
    #[derive(Debug, Default)]
    pub(crate) struct Scripted {
        values: Vec<[u8; 16]>,
        pub(crate) drawn: usize,
    }

    impl Scripted {
        pub(crate) fn new(values: impl IntoIterator<Item = [u8; 16]>) -> Self {
            Self {
                values: values.into_iter().collect(),
                drawn: 0,
            }
        }
    }

    impl Randomness for Scripted {
        fn fill(&mut self, bytes: &mut [u8; 16]) -> Result<(), RandomnessFailure> {
            let value = self.values.get(self.drawn).ok_or(RandomnessFailure)?;
            *bytes = *value;
            self.drawn += 1;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use intlify_authoring::OwnerKind;

    use super::scripted::Scripted;
    use super::*;

    fn owner() -> OwnerIdentity {
        OwnerIdentity::new(OwnerKind::Application, "storefront").unwrap()
    }

    #[test]
    fn the_operating_system_gives_fresh_values_each_time() {
        let mut randomness = OsRandomness;
        let first = registry_identity(&mut randomness).unwrap();
        let second = registry_identity(&mut randomness).unwrap();
        assert_ne!(first, second);
        let ids = intent_ids(&owner(), 2, &mut randomness).unwrap();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        for id in &ids {
            assert_eq!(id.owner(), &owner());
            assert_eq!(id.value().as_str().len(), 32);
        }
    }

    #[test]
    fn drawn_bytes_are_spelled_in_order_for_the_owner() {
        let mut randomness = Scripted::new([[0xab; 16], [0x01; 16], [0x10; 16]]);
        assert_eq!(
            registry_identity(&mut randomness).unwrap().value().as_str(),
            "ab".repeat(16)
        );
        let ids = intent_ids(&owner(), 2, &mut randomness).unwrap();
        let values: Vec<&str> = ids.iter().map(|id| id.value().as_str()).collect();
        assert_eq!(values, ["01".repeat(16), "10".repeat(16)]);
        assert!(ids.iter().all(|id| id.owner() == &owner()));
        // Nothing is drawn for nothing.
        assert_eq!(intent_ids(&owner(), 0, &mut randomness), Ok(vec![]));
        assert_eq!(randomness.drawn, 3);
    }

    #[test]
    fn a_failure_gives_no_value_and_no_part_of_a_list() {
        let mut randomness = Scripted::new([[0x01; 16]]);
        assert_eq!(
            intent_ids(&owner(), 2, &mut randomness),
            Err(RandomnessFailure)
        );
        assert_eq!(registry_identity(&mut randomness), Err(RandomnessFailure));
    }
}
