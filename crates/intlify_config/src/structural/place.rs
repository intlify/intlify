// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Where each member of an internal path sits in the configuration schema,
//! which Evidence needs before it classifies the member (Decision 015-184).
//!
//! A member is schema-defined when the object schema at its position, or one
//! of that schema's `anyOf` branches, names it under `properties`. A key of
//! the `profiles`, `targetProfiles`, or `deploymentGroups` map is a Profile,
//! Target, or Group ID key, and a key of the `localeNegotiation.aliases` or
//! `messageFallback` map is a locale key; whether the key itself is
//! non-secret is the resolver's decision. Every other member is undefined,
//! and so is every member below an undefined one or below an ID key that does
//! not match the identifier syntax, where the schema describes nothing. An
//! array index has no place: Evidence always keeps it.

use crate::model::valid_identity;

use super::program::{Additional, SchemaId, SchemaProgram};

/// One edge of an internal path from the configuration root: an object
/// member's exact name, or an array element's index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Edge<'a> {
    Member(&'a str),
    Index(u64),
}

/// Where a member sits in the configuration schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    /// A name the schema defines at the member's position.
    Defined,
    /// A key of `profiles`.
    ProfileId,
    /// A key of a declaration's `targetProfiles`.
    TargetId,
    /// A key of a declaration's `deploymentGroups`.
    GroupId,
    /// A key of `localeNegotiation.aliases` or of `messageFallback`.
    LocaleKey,
    /// Any other member.
    Undefined,
}

/// The keyed maps of Decision 015-184, by the schema-defined member that
/// holds each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Map {
    Identifiers(Place),
    Locales,
}

/// The schema node at one position of a path, and the map it is when it is
/// one of those maps.
#[derive(Debug, Clone, Copy)]
struct Position {
    schema: SchemaId,
    map: Option<Map>,
}

/// The place of every edge of `edges`, walked from the root of `program`:
/// `None` for an index, and a member's place otherwise.
pub(super) fn places(program: &SchemaProgram, edges: &[Edge<'_>]) -> Vec<Option<Place>> {
    let mut at = Some(Position {
        schema: program.root(),
        map: None,
    });
    edges
        .iter()
        .map(|edge| {
            let (place, next) = match *edge {
                Edge::Index(_) => (None, at.and_then(|position| items(program, position))),
                Edge::Member(name) => {
                    let (place, next) = at.map_or((Place::Undefined, None), |position| {
                        member(program, position, name)
                    });
                    (Some(place), next)
                }
            };
            at = next;
            place
        })
        .collect()
}

/// The place of the member `name` at `position`, and the position below it.
fn member(program: &SchemaProgram, position: Position, name: &str) -> (Place, Option<Position>) {
    match position.map {
        Some(Map::Identifiers(place)) => {
            let schema = program
                .identity_values(position.schema)
                .filter(|_| valid_identity(name));
            (place, schema.map(|schema| Position { schema, map: None }))
        }
        Some(Map::Locales) => {
            let schema = match program.resolved(position.schema).additional {
                Some(Additional::Schema(schema)) => Some(schema),
                Some(Additional::Forbidden) | None => None,
            };
            (
                Place::LocaleKey,
                schema.map(|schema| Position { schema, map: None }),
            )
        }
        None => {
            defined(program, position.schema, name).map_or((Place::Undefined, None), |schema| {
                let map = held_map(program, schema, name);
                (Place::Defined, Some(Position { schema, map }))
            })
        }
    }
}

/// The schema of the member `name` that `schema` or one of its `anyOf`
/// branches defines.
fn defined(program: &SchemaProgram, schema: SchemaId, name: &str) -> Option<SchemaId> {
    program.property(schema, name).or_else(|| {
        program
            .resolved(schema)
            .any_of
            .as_ref()?
            .iter()
            .find_map(|branch| defined(program, *branch, name))
    })
}

/// The schema of the elements of the array at `position`.
fn items(program: &SchemaProgram, position: Position) -> Option<Position> {
    program
        .resolved(position.schema)
        .items
        .map(|schema| Position { schema, map: None })
}

/// The map that the schema-defined member `name` holds, when its schema is
/// one of the maps of Decision 015-184: an identifier map keys its members by
/// the identifier syntax, and a locale map gives every key one schema.
fn held_map(program: &SchemaProgram, schema: SchemaId, name: &str) -> Option<Map> {
    let node = program.resolved(schema);
    let identifiers = |place| node.identity_values.map(|_| Map::Identifiers(place));
    match name {
        "profiles" => identifiers(Place::ProfileId),
        "targetProfiles" => identifiers(Place::TargetId),
        "deploymentGroups" => identifiers(Place::GroupId),
        "aliases" | "messageFallback" => {
            matches!(node.additional, Some(Additional::Schema(_))).then_some(Map::Locales)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{places, Edge, Place};
    use crate::construction::committed::CONFIGURATION_SCHEMA;
    use crate::structural::program::SchemaProgram;

    use Edge::{Index, Member};
    use Place::{Defined, GroupId, LocaleKey, ProfileId, TargetId, Undefined};

    /// The configuration schema that construction admits.
    fn program() -> SchemaProgram {
        SchemaProgram::compile(&serde_json::from_slice(CONFIGURATION_SCHEMA).unwrap()).unwrap()
    }

    fn of(edges: &[Edge<'_>]) -> Vec<Option<Place>> {
        places(&program(), edges)
    }

    #[test]
    fn schema_defined_names_are_defined() {
        assert_eq!(
            of(&[Member("schemaVersion")]),
            [Some(Defined)],
            "a root field"
        );
        assert_eq!(
            of(&[
                Member("profiles"),
                Member("app"),
                Member("localeNegotiation"),
                Member("aliases"),
            ]),
            [Some(Defined), Some(ProfileId), Some(Defined), Some(Defined)]
        );
        assert_eq!(of(&[]), []);
    }

    /// Each map of Decision 015-184 places its keys, and the schema goes on
    /// below a key.
    #[test]
    fn the_keyed_maps_place_their_keys() {
        let profile = [Member("profiles"), Member("app")];
        let target = [&profile[..], &[Member("targetProfiles"), Member("browser")]].concat();
        assert_eq!(
            of(&[&target[..], &[Member("requestedLocales"), Index(0)]].concat()),
            [
                Some(Defined),
                Some(ProfileId),
                Some(Defined),
                Some(TargetId),
                Some(Defined),
                None,
            ]
        );
        assert_eq!(
            of(&[
                &profile[..],
                &[Member("deploymentGroups"), Member("web"), Member("members")]
            ]
            .concat()),
            [
                Some(Defined),
                Some(ProfileId),
                Some(Defined),
                Some(GroupId),
                Some(Defined)
            ]
        );
        assert_eq!(
            of(&[
                &profile[..],
                &[
                    Member("localeNegotiation"),
                    Member("aliases"),
                    Member("en_US")
                ],
            ]
            .concat()),
            [
                Some(Defined),
                Some(ProfileId),
                Some(Defined),
                Some(Defined),
                Some(LocaleKey)
            ]
        );
    }

    /// A fallback candidate is a string or an object, so `kind` is defined in
    /// the object branch of its `anyOf`.
    #[test]
    fn a_name_of_an_any_of_branch_is_defined() {
        assert_eq!(
            of(&[
                Member("profiles"),
                Member("app"),
                Member("messageFallback"),
                Member("ja"),
                Index(1),
                Member("kind"),
            ]),
            [
                Some(Defined),
                Some(ProfileId),
                Some(Defined),
                Some(LocaleKey),
                None,
                Some(Defined),
            ]
        );
    }

    /// The schema describes nothing below an unknown member, nor below an ID
    /// key outside the identifier syntax, so every member there is undefined,
    /// a schema-defined name included.
    #[test]
    fn nothing_below_an_unknown_member_or_a_malformed_id_is_defined() {
        assert_eq!(
            of(&[
                Member("secret"),
                Member("schemaVersion"),
                Index(0),
                Member("profiles")
            ]),
            [Some(Undefined), Some(Undefined), None, Some(Undefined)]
        );
        assert_eq!(
            of(&[Member("profiles"), Member("App"), Member("targetProfiles")]),
            [Some(Defined), Some(ProfileId), Some(Undefined)]
        );
        assert_eq!(
            of(&[Member("profiles"), Member("app"), Member("unknown")]),
            [Some(Defined), Some(ProfileId), Some(Undefined)]
        );
    }

    /// Only the schema-defined members of 015-184 hold keyed maps: a key
    /// spelled like one of them is a key, and the schema below it goes on.
    #[test]
    fn only_the_named_members_hold_maps() {
        assert_eq!(
            of(&[
                Member("profiles"),
                Member("profiles"),
                Member("targetProfiles"),
                Member("x")
            ]),
            [
                Some(Defined),
                Some(ProfileId),
                Some(Defined),
                Some(TargetId)
            ]
        );
        assert_eq!(
            of(&[
                Member("profiles"),
                Member("app"),
                Member("targetProfiles"),
                Member("profiles"),
                Member("profile"),
            ]),
            [
                Some(Defined),
                Some(ProfileId),
                Some(Defined),
                Some(TargetId),
                Some(Defined)
            ]
        );
    }

    /// A member named like a map holder holds a map only when its schema is
    /// one: an object with fixed members is no identifier or locale map.
    #[test]
    fn a_named_member_whose_schema_is_no_map_holds_none() {
        let fixed = json!({ "type": "object", "properties": { "a": { "type": "string" } } });
        let program = SchemaProgram::compile(&json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": { "profiles": fixed, "aliases": fixed },
        }))
        .unwrap();
        for name in ["profiles", "aliases"] {
            assert_eq!(
                places(&program, &[Member(name), Member("a"), Member("b")]),
                [Some(Defined), Some(Defined), Some(Undefined)],
                "{name}"
            );
        }
    }
}
