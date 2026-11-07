// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's construction staleness: how two constructions compare before
//! their invocations do.
//!
//! Revision `"0"` evaluates staleness as one function of two retained input
//! sets, never of a stored key, and compares the construction contexts first
//! (D15:2583). Unequal Resolver Construction Identities make the earlier
//! construction `construction-stale`. Equal identities over authorities that
//! differ in what the identity commits are construction-authority corruption,
//! `corrupt`, which forbids reuse (D15:2575): the bytes of every artifact the
//! identity commits by its file digest, and the Profile Specification's body
//! value, which it commits by its semantic digest. Bytes the identity does not
//! commit leave one construction: a Profile Specification written with other
//! bytes but an equal body value, and a schema the Configuration Schema
//! Authority Set does not map. Under one construction, the invocations are
//! compared next.

use super::component::Artifact;
use super::flow::{AdmittedAuthorities, ConstructionInput};
use super::identity;
use super::registry::RegistryRole;
use super::root::ConstructionRoot;

/// One construction as staleness compares it: the input it was given and the
/// authorities it admitted.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ConstructionContext<'a> {
    pub(crate) input: &'a ConstructionInput,
    pub(crate) admitted: &'a AdmittedAuthorities,
}

/// How a later construction compares with an earlier one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConstructionComparison {
    /// One construction: the invocations are compared next.
    Same,
    /// `construction-stale`: the identities differ, so every outcome of the
    /// earlier construction is stale and the resolver is constructed again.
    Stale,
    /// `corrupt`: the identities are equal but what they commit differs,
    /// which forbids reuse.
    Corrupt,
}

/// Compare two constructions, both constructed under `root`.
pub(crate) fn compare(
    root: &ConstructionRoot,
    earlier: ConstructionContext<'_>,
    later: ConstructionContext<'_>,
) -> ConstructionComparison {
    if identity::derive(root, earlier.admitted) != identity::derive(root, later.admitted) {
        return ConstructionComparison::Stale;
    }
    if committed_bytes(earlier) != committed_bytes(later)
        || earlier.admitted.profile_specification.body != later.admitted.profile_specification.body
    {
        return ConstructionComparison::Corrupt;
    }
    ConstructionComparison::Same
}

/// The bytes of every artifact the identity commits by its file digest, in
/// the order of its fields: the Profile Specification Schema, the set's
/// container and the schemas it maps, the Capability Specification, and the
/// Registry Package's container and members by role. A component not given
/// has no bytes.
fn committed_bytes(context: ConstructionContext<'_>) -> Vec<Option<&[u8]>> {
    let input = context.input;
    let set = input.configuration_schema_authority_set.as_ref();
    let package = input.registry_package.as_ref();
    let mut bytes = vec![
        input
            .profile_specification_schema
            .as_ref()
            .map(|component| component.artifact.bytes()),
        set.map(|set| set.component.artifact.bytes()),
    ];
    // Only the schemas the admitted set maps: the identity commits no other.
    let mapped = &context.admitted.configuration_schema_authority_set.members;
    bytes.extend(mapped.keys().map(|version| {
        set.and_then(|set| set.members.get(version))
            .map(Artifact::bytes)
    }));
    bytes.push(
        input
            .resolver_capability_specification
            .as_ref()
            .map(|component| component.artifact.bytes()),
    );
    bytes.push(package.map(|package| package.component.artifact.bytes()));
    bytes.extend(RegistryRole::ALL.into_iter().map(|role| {
        package
            .and_then(|package| package.members.get(&role))
            .map(Artifact::bytes)
    }));
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::committed::{self, profile_specification, PROFILE_SPECIFICATION};
    use crate::construction::flow;
    use crate::construction::flow::testing::{compact, value};
    use crate::construction::resolver::{self, ConstructedResolver};

    fn root() -> &'static ConstructionRoot {
        ConstructionRoot::built_in().unwrap()
    }

    fn built(input: ConstructionInput) -> ConstructedResolver {
        resolver::build(root(), input).unwrap()
    }

    fn context<'a>(
        input: &'a ConstructionInput,
        admitted: &'a AdmittedAuthorities,
    ) -> ConstructionContext<'a> {
        ConstructionContext { input, admitted }
    }

    /// Other bytes for the same value: one more space at the end.
    fn respaced(artifact: &Artifact) -> Artifact {
        let mut bytes = artifact.bytes().to_vec();
        bytes.push(b' ');
        Artifact::read(bytes)
    }

    fn respace_member(input: &mut ConstructionInput, role: RegistryRole) {
        let package = input.registry_package.as_mut().unwrap();
        let member = package.members.get_mut(&role).unwrap();
        *member = respaced(member);
    }

    #[test]
    fn one_input_constructed_twice_is_one_construction() {
        let earlier = built(committed::input());
        let later = built(committed::input());
        assert_eq!(
            compare(root(), earlier.context(), later.context()),
            ConstructionComparison::Same
        );
    }

    /// The identity commits the Profile Specification's body value, not its
    /// bytes, and no schema the set does not map (D15:2583).
    #[test]
    fn bytes_the_identity_does_not_commit_leave_one_construction() {
        let earlier = built(committed::input());
        let mut input = committed::input();
        input.profile_specification = Some(profile_specification(&compact(&value(
            PROFILE_SPECIFICATION,
        ))));
        let later = built(input);
        assert_eq!(
            compare(root(), earlier.context(), later.context()),
            ConstructionComparison::Same
        );
        let mut input = committed::input();
        input
            .configuration_schema_authority_set
            .as_mut()
            .unwrap()
            .members
            .insert("1".to_owned(), Artifact::read(b"{}".to_vec()));
        let later = built(input);
        assert_eq!(
            compare(root(), earlier.context(), later.context()),
            ConstructionComparison::Same
        );
    }

    /// Revision `"0"` admits one set of authorities, so another identity
    /// comes from authorities this crate does not implement: the admitted
    /// values are changed directly.
    #[test]
    fn another_identity_is_construction_stale() {
        let input = committed::input();
        let earlier = flow::construct(root(), &input).unwrap();
        let mut later = flow::construct(root(), &input).unwrap();
        later.registry_package.declared.revision = "1".to_owned();
        assert_eq!(
            compare(root(), context(&input, &earlier), context(&input, &later)),
            ConstructionComparison::Stale
        );
        assert_eq!(
            compare(root(), context(&input, &later), context(&input, &earlier)),
            ConstructionComparison::Stale
        );
    }

    /// Under one identity, other bytes in any artifact the identity commits
    /// by its file digest are corruption. Construction would refuse such
    /// bytes, so the later input is changed after its authorities were
    /// admitted.
    #[test]
    fn other_committed_bytes_under_one_identity_are_corrupt() {
        let earlier_input = committed::input();
        let earlier = flow::construct(root(), &earlier_input).unwrap();
        let changes: [fn(&mut ConstructionInput); 11] = [
            |input| {
                let schema = input.profile_specification_schema.as_mut().unwrap();
                schema.artifact = respaced(&schema.artifact);
            },
            |input| {
                let set = input.configuration_schema_authority_set.as_mut().unwrap();
                set.component.artifact = respaced(&set.component.artifact);
            },
            |input| {
                let set = input.configuration_schema_authority_set.as_mut().unwrap();
                let member = set.members.values_mut().next().unwrap();
                *member = respaced(member);
            },
            |input| {
                let capability = input.resolver_capability_specification.as_mut().unwrap();
                capability.artifact = respaced(&capability.artifact);
            },
            |input| {
                let package = input.registry_package.as_mut().unwrap();
                package.component.artifact = respaced(&package.component.artifact);
            },
            |input| respace_member(input, RegistryRole::ALL[0]),
            |input| respace_member(input, RegistryRole::ALL[1]),
            |input| respace_member(input, RegistryRole::ALL[2]),
            |input| respace_member(input, RegistryRole::ALL[3]),
            |input| respace_member(input, RegistryRole::ALL[4]),
            |input| respace_member(input, RegistryRole::ALL[5]),
        ];
        for (index, change) in changes.into_iter().enumerate() {
            let mut later_input = committed::input();
            let later = flow::construct(root(), &later_input).unwrap();
            change(&mut later_input);
            assert_eq!(
                compare(
                    root(),
                    context(&earlier_input, &earlier),
                    context(&later_input, &later)
                ),
                ConstructionComparison::Corrupt,
                "change {index}"
            );
        }
    }

    /// Under one identity, another Profile Specification body value is
    /// corruption, though its bytes are not compared.
    #[test]
    fn another_profile_specification_body_under_one_identity_is_corrupt() {
        let input = committed::input();
        let earlier = flow::construct(root(), &input).unwrap();
        let mut later = flow::construct(root(), &input).unwrap();
        later.profile_specification.body["identity"] = serde_json::json!("other");
        assert_eq!(
            compare(root(), context(&input, &earlier), context(&input, &later)),
            ConstructionComparison::Corrupt
        );
    }
}
