// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's resolver construction, starting from its built-in root.
//!
//! The Resolver Construction Root Package is the finite root every
//! construction is validated under. Its five members are written from the
//! values in these modules and checked in below `conformance/construction/`;
//! the implementation compiles them in and checks, on first use, that they are
//! the package whose two digests it pins.
//!
//! The authorities a construction is given are checked in below
//! `conformance/authorities/`, written from the values in these modules too:
//! the Profile Specification Schema and the Profile Specification
//! ([`profile`]), the Configuration Schema Authority Set with its mapped
//! schema ([`authority_set`]), and the Resolver Capability Specification
//! ([`capability`]). The implementation compiles in only their digests, which
//! construction compares with the authorities it is given.
//!
//! The Registry Package's six members and its container are checked in
//! below `conformance/registries/` in the same way ([`registry`]).
//!
//! Construction checks the authorities it is given in the order of the
//! construction-failure precedence, each check refusing with its row: the
//! checks every component starts with ([`component`]), against the members of
//! the built-in root ([`root`]), and then each authority's own checks, in the
//! authority's module. The construction flow ([`flow`]) first applies the
//! construction-root bounds to the whole input ([`bounds`]), then takes the
//! five components in that order, then applies the cross-authority rules
//! ([`rules`]), and ends by checking that every admitted authority is the one
//! the resolver is compiled from.

pub(crate) mod admission;
pub(crate) mod authority_set;
pub(crate) mod bounds;
pub(crate) mod capability;
pub(crate) mod component;
pub(crate) mod envelope;
pub(crate) mod failure;
pub(crate) mod flow;
pub(crate) mod meta_schema;
pub(crate) mod profile;
pub(crate) mod registry;
pub(crate) mod root;
pub(crate) mod rules;
pub(crate) mod scan;
pub(crate) mod validator;

#[cfg(any(test, feature = "vectors"))]
pub mod generate;
