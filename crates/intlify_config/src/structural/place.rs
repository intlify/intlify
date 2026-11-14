// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Where each member of an internal path sits in the configuration schema,
//! which Evidence needs before it classifies the member (Decision 015-184).
//!
//! A member is schema-defined when the object schema at its position, or one
//! of that schema's `anyOf` branches, names it under `properties`. The keyed
//! maps are the members at the positions 015 gives them: a key of the root's
//! `profiles` map (D15:292), or of a profile declaration's `targetProfiles`
//! or `deploymentGroups` map (D15:469), is a Profile, Target, or Group ID
//! key, and a key of a declaration's `localeNegotiation.aliases` or
//! `messageFallback` map is a locale key; whether the key itself is
//! non-secret is the resolver's decision. A member of the same name at any
//! other position holds no keyed map. Every other member is undefined, and so
//! is every member below an undefined one, where the schema describes nothing.
//! Below an ID key, the entry's schema goes on whether or not the key matches
//! the identifier syntax: structural admission evaluates the entry's value
//! under it (D15:2184). An array index has no place: Evidence always keeps it.

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
    /// A key of the root's `profiles`.
    ProfileId,
    /// A key of a declaration's `targetProfiles`.
    TargetId,
    /// A key of a declaration's `deploymentGroups`.
    GroupId,
    /// A key of a declaration's `localeNegotiation.aliases` or
    /// `messageFallback`.
    LocaleKey,
    /// Any other member.
    Undefined,
}

/// What the schema node at one position of a path is to Decision 015-184:
/// an object that holds one of its keyed maps, or one of those maps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// The configuration root, which holds `profiles`.
    Root,
    /// A profile declaration, which holds `targetProfiles`,
    /// `deploymentGroups`, `messageFallback`, and `localeNegotiation`.
    Declaration,
    /// A declaration's `localeNegotiation`, which holds `aliases`.
    Negotiation,
    /// A map keyed by identifiers, whose keys have this place.
    Identifiers(Place),
    /// A map keyed by locales.
    Locales,
}

/// The schema node at one position of a path, and its role when it has one.
#[derive(Debug, Clone, Copy)]
struct Position {
    schema: SchemaId,
    role: Option<Role>,
}

/// The place of every edge of `edges`, walked from the root of `program`:
/// `None` for an index, and a member's place otherwise.
pub(super) fn places(program: &SchemaProgram, edges: &[Edge<'_>]) -> Vec<Option<Place>> {
    let mut at = Some(Position {
        schema: program.root(),
        role: Some(Role::Root),
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
    match position.role {
        Some(Role::Identifiers(place)) => {
            let schema = program.identity_values(position.schema);
            // The value of a Profile ID key is a profile declaration.
            let role = (place == Place::ProfileId).then_some(Role::Declaration);
            (place, schema.map(|schema| Position { schema, role }))
        }
        Some(Role::Locales) => {
            let schema = match program.resolved(position.schema).additional {
                Some(Additional::Schema(schema)) => Some(schema),
                Some(Additional::Forbidden) | None => None,
            };
            (
                Place::LocaleKey,
                schema.map(|schema| Position { schema, role: None }),
            )
        }
        holder => {
            defined(program, position.schema, name).map_or((Place::Undefined, None), |schema| {
                let role = holder.and_then(|holder| held(program, holder, schema, name));
                (Place::Defined, Some(Position { schema, role }))
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
        .map(|schema| Position { schema, role: None })
}

/// The role of the schema-defined member `name` of `holder`, whose schema is
/// `schema`: a declaration's `localeNegotiation`, or one of the maps of
/// Decision 015-184 when the member is at that map's position and its schema
/// is one, where an identifier map keys its members by the identifier syntax
/// and a locale map gives every key one schema.
fn held(program: &SchemaProgram, holder: Role, schema: SchemaId, name: &str) -> Option<Role> {
    let node = program.resolved(schema);
    let identifiers = |place| node.identity_values.map(|_| Role::Identifiers(place));
    let locales = matches!(node.additional, Some(Additional::Schema(_))).then_some(Role::Locales);
    match (holder, name) {
        (Role::Root, "profiles") => identifiers(Place::ProfileId),
        (Role::Declaration, "targetProfiles") => identifiers(Place::TargetId),
        (Role::Declaration, "deploymentGroups") => identifiers(Place::GroupId),
        (Role::Declaration, "messageFallback") | (Role::Negotiation, "aliases") => locales,
        (Role::Declaration, "localeNegotiation") => Some(Role::Negotiation),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{places, Edge, Place};
    use crate::construction::committed::CONFIGURATION_SCHEMA;
    use crate::model::ID_PATTERN;
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

    /// The schema describes nothing below an unknown member, so every member
    /// there is undefined, a schema-defined name included.
    #[test]
    fn nothing_below_an_unknown_member_is_defined() {
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
            of(&[Member("profiles"), Member("app"), Member("unknown")]),
            [Some(Defined), Some(ProfileId), Some(Undefined)]
        );
    }

    /// Below an ID key outside the identifier syntax, the entry's schema goes
    /// on, as structural admission evaluates the entry's value (D15:2184):
    /// its names are defined, and its maps place their keys.
    #[test]
    fn the_entry_schema_goes_on_below_a_malformed_id() {
        assert_eq!(
            of(&[
                Member("profiles"),
                Member("App"),
                Member("targetProfiles"),
                Member("Web Target"),
                Member("requestedLocales"),
                Index(0),
            ]),
            [
                Some(Defined),
                Some(ProfileId),
                Some(Defined),
                Some(TargetId),
                Some(Defined),
                None,
            ]
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

    /// A map keyed by the identifier syntax, with `values` for every key.
    fn identifiers(values: &Value) -> Value {
        json!({ "type": "object", "patternProperties": { ID_PATTERN: values } })
    }

    /// A map with `values` for every key.
    fn open(values: &Value) -> Value {
        json!({ "type": "object", "additionalProperties": values })
    }

    /// A schema whose root object has `properties`.
    fn compile(properties: &Value) -> SchemaProgram {
        SchemaProgram::compile(&json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": properties,
        }))
        .unwrap()
    }

    /// Below `holder`, each of the members `names` is defined and a key below
    /// them is undefined: no member of `names` holds a keyed map there.
    fn assert_no_map(program: &SchemaProgram, holder: &[Edge<'_>], names: &[&str]) {
        let below: Vec<_> = names.iter().map(|&name| Member(name)).collect();
        let edges = [holder, &below, &[Member("key")]].concat();
        let mut expected = places(program, holder);
        expected.extend(names.iter().map(|_| Some(Defined)));
        expected.push(Some(Undefined));
        assert_eq!(places(program, &edges), expected, "{edges:?}");
    }

    /// A member at a map's position holds the map only when its schema is
    /// one: an object with fixed members is no identifier or locale map.
    #[test]
    fn a_member_whose_schema_is_no_map_holds_none() {
        let fixed = json!({ "type": "object", "properties": { "a": { "type": "string" } } });
        assert_no_map(&compile(&json!({ "profiles": fixed })), &[], &["profiles"]);
        let declaration = json!({
            "type": "object",
            "properties": {
                "targetProfiles": fixed,
                "deploymentGroups": fixed,
                "messageFallback": fixed,
                "localeNegotiation": { "type": "object", "properties": { "aliases": fixed } },
            },
        });
        let program = compile(&json!({ "profiles": identifiers(&declaration) }));
        let app = [Member("profiles"), Member("app")];
        assert_eq!(places(&program, &app), [Some(Defined), Some(ProfileId)]);
        for names in [
            &["targetProfiles"][..],
            &["deploymentGroups"],
            &["messageFallback"],
            &["localeNegotiation", "aliases"],
        ] {
            assert_no_map(&program, &app, names);
        }
    }

    /// The keyed maps of 015-184 sit only where 015 gives them: a member of
    /// the same name and shape at the root, in a declaration, below a key of
    /// each map, or in an array element holds no keyed map.
    #[test]
    fn only_the_designated_positions_hold_maps() {
        let string = json!({ "type": "string" });
        let negotiation = |aliases: &Value| json!({ "type": "object", "properties": { "aliases": open(aliases) } });
        // Every map name of 015-184 with the schema of its map.
        let stray = json!({
            "type": "object",
            "properties": {
                "profiles": identifiers(&string),
                "targetProfiles": identifiers(&string),
                "deploymentGroups": identifiers(&string),
                "messageFallback": open(&string),
                "aliases": open(&string),
                "localeNegotiation": negotiation(&string),
            },
        });
        let mut localization = negotiation(&stray);
        localization["properties"]["messageFallback"] = open(&string);
        let declaration = json!({
            "type": "object",
            "properties": {
                "profiles": identifiers(&string),
                "aliases": open(&string),
                "targetProfiles": identifiers(&stray),
                "deploymentGroups": identifiers(&stray),
                "messageFallback": open(&stray),
                "localeNegotiation": localization,
                "entries": { "type": "array", "items": stray },
            },
        });
        let mut root = stray["properties"].clone();
        root["profiles"] = identifiers(&declaration);
        let program = compile(&root);

        let app = [Member("profiles"), Member("app")];
        let target = [&app[..], &[Member("targetProfiles"), Member("web")]].concat();
        let group = [&app[..], &[Member("deploymentGroups"), Member("web")]].concat();
        let fallback = [&app[..], &[Member("messageFallback"), Member("en")]].concat();
        let alias = [
            &app[..],
            &[Member("localeNegotiation"), Member("aliases"), Member("fr")],
        ]
        .concat();
        let entry = [&app[..], &[Member("entries"), Index(0)]].concat();
        let keyed = |key| [Some(Defined), Some(ProfileId), Some(Defined), Some(key)];
        assert_eq!(places(&program, &target), keyed(TargetId));
        assert_eq!(places(&program, &group), keyed(GroupId));
        assert_eq!(places(&program, &fallback), keyed(LocaleKey));
        assert_eq!(
            places(&program, &alias),
            [
                Some(Defined),
                Some(ProfileId),
                Some(Defined),
                Some(Defined),
                Some(LocaleKey)
            ]
        );
        assert_eq!(
            places(&program, &entry),
            [Some(Defined), Some(ProfileId), Some(Defined), None]
        );

        let strays: [&[&str]; 6] = [
            &["profiles"],
            &["targetProfiles"],
            &["deploymentGroups"],
            &["messageFallback"],
            &["aliases"],
            &["localeNegotiation", "aliases"],
        ];
        for names in &strays[1..] {
            assert_no_map(&program, &[], names);
        }
        for names in [
            &["profiles"][..],
            &["aliases"],
            &["localeNegotiation", "messageFallback"],
        ] {
            assert_no_map(&program, &app, names);
        }
        for holder in [&target, &group, &fallback, &alias, &entry] {
            for names in strays {
                assert_no_map(&program, holder, names);
            }
        }
    }
}
