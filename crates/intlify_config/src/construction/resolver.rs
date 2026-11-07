// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's immutable resolver: what one successful construction built.
//!
//! Construction is given its input and keeps it: every component's declared
//! tuple and exact bytes stay beside the authorities construction admitted,
//! since staleness compares two retained inputs by value and replay needs
//! the complete input again (D15:2575, 2583). The resolver takes the input
//! rather than copying it, so construction holds no second copy of what the
//! caller acquired (D15:2796). Nothing changes the resolver after
//! construction.
//!
//! What the resolver supports in each specification slot comes from the
//! Profile Specification it admitted, which the implemented authority fixes
//! to the one the resolver is compiled from. The resolver's Resolver
//! Construction Identity is derived from it as a step of its own.

#[cfg(any(test, feature = "benchmark"))]
use std::sync::OnceLock;

use intlify_shared_json::decode;

use super::component::Refusal;
use super::flow::{self, AdmittedAuthorities, ConstructionInput};
use super::identity::{self, ConstructionIdentity};
use super::profile::{Pin, ProjectProfileSpecification, SlotValue};
use super::root::ConstructionRoot;
use crate::locale::{DatasetPin, SemanticPins, Supported, VersionedIdentity};
use crate::resolver::specification::SupportedSpecifications;

/// A resolver one construction built: the input it was given, the
/// authorities it admitted, and the Profile Specification's body as its type.
#[derive(Debug)]
pub(crate) struct ConstructedResolver {
    input: ConstructionInput,
    admitted: AdmittedAuthorities,
    profile: ProjectProfileSpecification,
}

/// Construct a resolver from the input, which it keeps: the 43 rows of the
/// construction-failure precedence, refusing with the first that fails.
pub(crate) fn build(
    root: &ConstructionRoot,
    input: ConstructionInput,
) -> Result<ConstructedResolver, Refusal> {
    let admitted = flow::construct(root, &input)?;
    // The implemented authority admits only the Profile Specification this
    // crate writes, so its body reads as the type it is written from.
    let profile =
        decode::typed::<ProjectProfileSpecification>(admitted.profile_specification.body.clone())
            .expect("the implemented Profile Specification reads as its type");
    Ok(ConstructedResolver {
        input,
        admitted,
        profile,
    })
}

/// The specification a slot pins.
///
/// # Panics
///
/// Panics for a slot the invocation asserts, which no caller reads this way.
fn pinned(slot: &SlotValue) -> VersionedIdentity<String> {
    match slot {
        SlotValue::Pinned(Pin { identity, revision }) => VersionedIdentity {
            identity: identity.clone(),
            revision: revision.clone(),
        },
        SlotValue::Asserted(_) => unreachable!("only the vocabulary slot is asserted"),
    }
}

impl ConstructedResolver {
    /// The input construction was given.
    pub(crate) const fn input(&self) -> &ConstructionInput {
        &self.input
    }

    /// The authorities construction admitted.
    pub(crate) const fn admitted(&self) -> &AdmittedAuthorities {
        &self.admitted
    }

    /// The resolver's Resolver Construction Identity.
    pub(crate) fn identity(&self, root: &ConstructionRoot) -> ConstructionIdentity {
        identity::derive(root, &self.admitted)
    }

    /// The canonicalization specification the resolver supports, with the
    /// one dataset it admits.
    pub(crate) fn locale_canonicalization(&self) -> Supported<String> {
        let dataset = &self.profile.locale_canonicalization_dataset;
        Supported::new(vec![SemanticPins {
            specification: pinned(&self.profile.specification_slots.locale_canonicalization),
            dataset: DatasetPin {
                identity: dataset.identity.clone(),
                semantic_digest: dataset.semantic_digest.clone(),
            },
        }])
        .expect("one entry lists its specification once")
    }

    /// What the resolver supports in each specification slot.
    pub(crate) fn supported(&self) -> SupportedSpecifications<String> {
        let slots = &self.profile.specification_slots;
        SupportedSpecifications::new(
            pinned(&slots.project_profile),
            self.locale_canonicalization(),
            pinned(&slots.locale_negotiation),
            pinned(&slots.message_locale_fallback),
            pinned(&slots.coverage),
            pinned(&slots.delivery_graph),
            pinned(&slots.delivery_placement),
        )
    }
}

/// The resolver the committed input builds, built once.
///
/// # Panics
///
/// Panics when the committed input does not construct, which the tests rule
/// out.
#[cfg(any(test, feature = "benchmark"))]
pub(crate) fn committed() -> &'static ConstructedResolver {
    static COMMITTED: OnceLock<ConstructedResolver> = OnceLock::new();
    COMMITTED.get_or_init(|| {
        let root = ConstructionRoot::built_in().expect("the built-in Root Package verifies");
        build(root, super::committed::input()).expect("the committed input constructs")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::committed;
    use crate::construction::component::testing::row;
    use crate::construction::flow::testing::value;
    use crate::construction::profile;

    const VECTORS: &[u8] =
        include_bytes!("../../conformance/construction/construction-vectors-v0.json");

    fn root() -> &'static ConstructionRoot {
        ConstructionRoot::built_in().unwrap()
    }

    #[test]
    fn a_resolver_keeps_the_input_it_was_built_from() {
        let input = committed::input();
        let resolver = build(root(), input.clone()).unwrap();
        assert_eq!(resolver.input(), &input);
        assert_eq!(
            resolver
                .admitted()
                .profile_specification
                .semantic_digest
                .as_str(),
            profile::SEMANTIC_DIGEST
        );
        assert_eq!(
            resolver.identity(root()).text(),
            value(VECTORS)["constructionIdentity"]["text"]
        );
        assert_eq!(committed().identity(root()), resolver.identity(root()));
    }

    #[test]
    fn a_refused_input_builds_no_resolver() {
        let mut input = committed::input();
        input.profile_specification_schema = None;
        assert_eq!(row(build(root(), input)), Some(2));
    }

    /// The values the resolver tests write by hand are the ones a resolver
    /// built from the committed input supports.
    #[test]
    fn the_supported_values_are_the_profile_specifications() {
        let resolver = committed();
        assert_eq!(
            resolver.locale_canonicalization(),
            Supported::revision_zero()
        );
        let project_profile = VersionedIdentity {
            identity: profile::IDENTITY.to_owned(),
            revision: profile::REVISION.to_owned(),
        };
        assert_eq!(
            resolver.supported(),
            SupportedSpecifications::revision_zero(project_profile, Supported::revision_zero())
        );
    }
}
