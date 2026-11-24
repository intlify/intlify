// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The selected declaration as the semantic phases read it: each member's
//! value once every candidate at and below it passed, or the causes of those
//! that did not, as [`ValueAdmissions`] kept them (D15:393, 2367, 3340). The
//! checks decide: a member they admitted and the analysis did not, or a
//! required one they admitted though it is absent, is a resolver bug.

use std::collections::BTreeMap;

use serde::de::value::{BorrowedStrDeserializer, Error as KeyError};
use serde::de::DeserializeOwned;

use crate::materialize::{MaterializedDocument, NodeId, NodeKind, ObjectMember};
use crate::model::{
    CoverageMode, DeploymentGroupId, FallbackCandidate, RequiredNullable, TargetId,
};
use crate::references::{PolicyReference, TargetProfileReference};
use crate::structural::{AdmissionInvariant, Member, StructuralAnalysis};

use super::coverage::RuleContent;
use super::evaluation::{and, Admitted};
use super::hydration::{RelationContent, RelationKey};
use super::locales::joined;
use super::resource_policy::{PolicySlot, ReferenceField, ReferenceOccurrence};
use super::structural_admission::StructuralInvariant;
use super::tally::{total, Tally};
use super::value_admissions::ValueAdmissions;

/// The members of an object, by name.
type Members = BTreeMap<String, ObjectMember>;

/// The `policies` object slot by slot: each slot's reference or why it has
/// none, so that one that failed holds back no other, with no entry for an
/// admitted `null`, the explicit absence of a reference (D15:2137, 2367); and
/// whether the object itself passed apart from its slots. An undeclared
/// member is a failure of the object and in no slot, so it stops no
/// reference, while both semantic groups of the object wait on it (D15:2184,
/// 2367; plan 025 interpretation #346).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PolicySlots<Policy> {
    pub(crate) slots: Vec<(PolicySlot, Admitted<Policy>)>,
    pub(crate) object: Admitted<()>,
}

/// One entry of `targetProfiles` whose key structural admission admitted:
/// its reference, and each locale-bearing field's value, element by element,
/// or why it has none. Its key names the subjects of its fields, so one field
/// that failed holds back neither the other nor another entry (D15:2184,
/// 2367).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TargetEntry<Target> {
    pub(crate) id: TargetId,
    pub(crate) profile: Admitted<Target>,
    pub(crate) requested_locales: Admitted<Vec<Admitted<String>>>,
    pub(crate) default_requested_locale: Admitted<Option<String>>,
}

/// The Target ID map entry by entry: how many entries it holds, every one
/// counted whatever its key (D15:1930), each entry with an admitted key, in
/// the map's order, whether every key was admitted, which decides that a value
/// naming none of them names no declared Target ID, and whether everything at
/// and below the map passed, which what needs every target waits on. An entry
/// whose key failed has no later subject (D15:2184, 3354).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TargetEntries<Target> {
    pub(crate) submitted: usize,
    pub(crate) entries: Vec<TargetEntry<Target>>,
    pub(crate) keys: Admitted<()>,
    pub(crate) whole: Admitted<()>,
}

/// One object of a group's `hydrationRelations`: what names it, its identity
/// once both endpoints were admitted and otherwise its content, and each
/// endpoint or why it has none (D15:2181, 2367).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RelationEntry {
    pub(crate) key: RelationKey,
    pub(crate) server: Admitted<TargetId>,
    pub(crate) client: Admitted<TargetId>,
}

/// One entry of `deploymentGroups` whose key structural admission admitted:
/// its `members`, element by element, and its `hydrationRelations`, each
/// element a relation or why it is none, each or why it has none to read
/// (D15:2184, 2367).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GroupEntry {
    pub(crate) id: DeploymentGroupId,
    pub(crate) members: Admitted<Vec<Admitted<TargetId>>>,
    pub(crate) relations: Admitted<Vec<Admitted<RelationEntry>>>,
}

/// The Group ID map entry by entry: how many entries it holds and how many
/// member and relation occurrences their arrays hold, each counted whatever
/// its key or its value (D15:1932, 1935, 1936), whether every key was admitted,
/// each entry with an admitted key, in the map's order, and whether everything
/// at and below the map passed, which what needs every relation waits on. An
/// entry whose key failed has no later subject (D15:2184, 3354).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GroupEntries {
    pub(crate) submitted: usize,
    pub(crate) memberships: Admitted<usize>,
    pub(crate) relations: Admitted<usize>,
    pub(crate) keys: Admitted<()>,
    pub(crate) entries: Vec<GroupEntry>,
    pub(crate) whole: Admitted<()>,
}

/// One entry of `localeNegotiation.aliases`: its key, a spelling whatever its
/// value is, and its destination or why it has none (D15:1477, 2367).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AliasEntry {
    pub(crate) key: String,
    pub(crate) value: Admitted<String>,
}

/// The alias map entry by entry, in the map's order, and whether everything
/// at and below `localeNegotiation` passed, which the profile built from
/// every entry waits on: an undeclared member beside `aliases` holds back no
/// entry (D15:2184, 2367, 2418).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AliasEntries {
    pub(crate) entries: Vec<AliasEntry>,
    pub(crate) whole: Admitted<()>,
}

/// One entry of `messageFallback`: its key, a spelling whatever its sequence
/// is, and its sequence candidate by candidate, or why it has none (D15:1519,
/// 2367).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FallbackEntry {
    pub(crate) key: String,
    pub(crate) sequence: Admitted<Vec<Admitted<FallbackCandidate>>>,
}

/// A selector of a coverage rule: absent, why it has no item to read, or each
/// item's value or why it has none (D15:2367).
pub(crate) type SelectorEntry = Admitted<Option<Vec<Admitted<String>>>>;

/// One object of `coverage.rules`: the content naming it, whatever failed in
/// it, and what structural admission left of its members (D15:2190, 2367). A
/// rule with no selector has neither, for the failed alternative that
/// requires one (D15:516).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuleEntry {
    pub(crate) content: RuleContent,
    pub(crate) requested_locales: SelectorEntry,
    pub(crate) intent_surface_classes: SelectorEntry,
    pub(crate) mode: Admitted<CoverageMode>,
}

/// The declaration's `coverage`, rule by rule: its default mode, each element
/// as a rule or why it is none, the raw items of each selector kind, and
/// whether everything at and below `coverage` passed, which the policy built
/// from all of it waits on (D15:1902, 1903, 2426).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CoverageEntries {
    pub(crate) default_mode: Admitted<Option<CoverageMode>>,
    pub(crate) rules: Admitted<Vec<Admitted<RuleEntry>>>,
    pub(crate) locale_items: Admitted<usize>,
    pub(crate) surface_items: Admitted<usize>,
    pub(crate) whole: Admitted<()>,
}

impl Default for CoverageEntries {
    /// The `coverage` of a declaration that omits it: no rule, no default.
    fn default() -> Self {
        Self {
            default_mode: Ok(None),
            rules: Ok(Vec::new()),
            locale_items: Ok(0),
            surface_items: Ok(0),
            whole: Ok(()),
        }
    }
}

/// The declaration `id` of an analysis, with what structural admission left
/// at its values.
pub(crate) struct DeclarationFragments<'r, Policy, Target> {
    values: &'r ValueAdmissions,
    analysis: &'r StructuralAnalysis<Policy, Target>,
    document: &'r MaterializedDocument,
    declaration: NodeId,
    id: &'r str,
}

impl<'r, Policy, Target> DeclarationFragments<'r, Policy, Target> {
    /// The declaration `id` of `analysis`.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a declaration the analysis does not hold.
    pub(crate) fn new(
        values: &'r ValueAdmissions,
        analysis: &'r StructuralAnalysis<Policy, Target>,
        id: &'r str,
    ) -> Result<Self, StructuralInvariant> {
        let (document, declaration) = analysis
            .declaration_value(id)
            .ok_or(StructuralInvariant::UnreportedFailure)?;
        Ok(Self {
            values,
            analysis,
            document,
            declaration,
            id,
        })
    }

    /// The optional member `name`: its value once the checks at and below it
    /// admitted it, absent when it is, or the causes.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a member the checks admitted and the
    /// analysis did not.
    pub(crate) fn optional<T: DeserializeOwned>(
        &self,
        name: &str,
    ) -> Result<Admitted<Option<T>>, StructuralInvariant> {
        let member = self
            .analysis
            .profile_member(self.id, name)
            .map_err(StructuralInvariant::Admission)?;
        let admission = self.values.member(self.document, self.declaration, name);
        match (admission, member) {
            (Err(causes), _) => Ok(Err(causes)),
            (Ok(()), Member::Admitted(value)) => Ok(Ok(Some(value))),
            (Ok(()), Member::Absent) => Ok(Ok(None)),
            // The checks decide, so a member they admitted and the analysis
            // did not is a resolver bug.
            (Ok(()), Member::Unadmitted) => Err(StructuralInvariant::UnreportedFailure),
        }
    }

    /// The required member `name`, whose absence its required check reports.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a member the checks admitted and the
    /// analysis did not, or of one they admitted though it is absent.
    pub(crate) fn required<T: DeserializeOwned>(
        &self,
        name: &str,
    ) -> Result<Admitted<T>, StructuralInvariant> {
        match self.optional(name)? {
            Ok(Some(value)) => Ok(Ok(value)),
            Ok(None) => Err(StructuralInvariant::UnreportedFailure),
            Err(causes) => Ok(Err(causes)),
        }
    }

    /// The required array of strings `name`, element by element: each one's
    /// value or why it has none, so that one that failed holds back no other
    /// (D15:2367). The array itself failing leaves no element to read.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of an array the checks admitted that is absent
    /// or no array, or of an element they admitted that is no string.
    pub(crate) fn string_items(
        &self,
        name: &str,
    ) -> Result<Admitted<Vec<Admitted<String>>>, StructuralInvariant> {
        self.strings_at(self.declaration, name)
    }

    /// The `policies` object slot by slot, and whether it passed apart from
    /// its slots.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a declaration without the object, of a
    /// slot the checks admitted that is absent, or of one its type cannot
    /// read.
    pub(crate) fn policy_slots(&self) -> Result<PolicySlots<Policy>, StructuralInvariant>
    where
        Policy: DeserializeOwned,
    {
        let policies = self.policies()?;
        let mut slots = Vec::with_capacity(PolicySlot::ALL.len());
        for slot in PolicySlot::ALL {
            if let Some(reference) = self.slot_reference(policies, slot)? {
                slots.push((slot, reference));
            }
        }
        Ok(PolicySlots {
            slots,
            object: self.values.value(policies),
        })
    }

    /// The value of the declaration's `policies`, which a selected declaration
    /// holds (D15:2131).
    fn policies(&self) -> Result<NodeId, StructuralInvariant> {
        self.member_value(self.declaration, "policies")
            .ok_or(StructuralInvariant::UnreportedFailure)
    }

    /// The reference at `slot` of the object `policies`, why it has none, or
    /// nothing for an admitted `null`.
    fn slot_reference<T: DeserializeOwned>(
        &self,
        policies: NodeId,
        slot: PolicySlot,
    ) -> Result<Option<Admitted<T>>, StructuralInvariant> {
        if let Err(causes) = self.values.member(self.document, policies, slot.name()) {
            return Ok(Some(Err(causes)));
        }
        let value = self
            .member_value(policies, slot.name())
            .ok_or(StructuralInvariant::UnreportedFailure)?;
        Ok(match self.decoded::<RequiredNullable<T>>(value)? {
            RequiredNullable::Reference(reference) => Some(Ok(reference)),
            RequiredNullable::Null => None,
        })
    }

    /// The Target ID map, entry by entry, or why it has no entry to read.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a map the checks admitted that is absent
    /// or no object, of a key they admitted that is no Target ID, or of a
    /// value they admitted that its type cannot read.
    pub(crate) fn target_entries(
        &self,
    ) -> Result<Admitted<TargetEntries<Target>>, StructuralInvariant>
    where
        Target: DeserializeOwned,
    {
        let members = match self.required_map("targetProfiles")? {
            Ok(members) => members,
            Err(causes) => return Ok(Err(causes)),
        };
        let mut entries = Vec::with_capacity(members.len());
        let mut keys = Ok(());
        for (key, member) in members {
            let entry = member.value();
            match self.values.key(entry) {
                Ok(()) => entries.push(TargetEntry {
                    id: identity(key)?,
                    profile: self.required_at(entry, "profile")?,
                    requested_locales: self.strings_at(entry, "requestedLocales")?,
                    default_requested_locale: self
                        .string_at_member(entry, "defaultRequestedLocale")?,
                }),
                Err(causes) => keys = and(keys, Err(causes)),
            }
        }
        Ok(Ok(TargetEntries {
            submitted: members.len(),
            entries,
            keys,
            whole: self
                .values
                .member(self.document, self.declaration, "targetProfiles"),
        }))
    }

    /// The Group ID map, entry by entry, or why it has no entry to read. Each
    /// total counts the elements of every array of its member, of an entry
    /// whose key failed too, since a value that is none holds no occurrence;
    /// it waits on why only when no entry holds one (D15:1935, 1936, 2184,
    /// 3354). An omitted `hydrationRelations` is no value, while an empty one
    /// is a value that holds no occurrence.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a map the checks admitted that is absent
    /// or no object, of a key they admitted that is no Group ID, or of a
    /// member or endpoint they admitted that is no Target ID.
    pub(crate) fn group_entries(&self) -> Result<Admitted<GroupEntries>, StructuralInvariant> {
        let members = match self.required_map("deploymentGroups")? {
            Ok(members) => members,
            Err(causes) => return Ok(Err(causes)),
        };
        let mut entries = Vec::with_capacity(members.len());
        let mut keys = Ok(());
        let mut held = 0;
        let mut excluded = None;
        let mut relations = 0;
        let mut relation_array = false;
        let mut unread = None;
        for (key, member) in members {
            let entry = member.value();
            let items = self.items_at::<TargetId>(entry, "members")?;
            match &items {
                Ok(items) => held += items.len(),
                Err(causes) => excluded = Some(joined(excluded, causes)),
            }
            let nodes = self.relation_nodes(entry)?;
            match &nodes {
                Ok(Some(nodes)) => {
                    relations += nodes.len();
                    relation_array = true;
                }
                Ok(None) => {}
                Err(causes) => unread = Some(joined(unread, causes)),
            }
            match self.values.key(entry) {
                Ok(()) => entries.push(GroupEntry {
                    id: identity(key)?,
                    members: items,
                    relations: match nodes {
                        Ok(nodes) => Ok(nodes
                            .unwrap_or_default()
                            .iter()
                            .map(|&relation| self.relation_entry(relation))
                            .collect::<Result<_, _>>()?),
                        Err(causes) => Err(causes),
                    },
                }),
                Err(causes) => keys = and(keys, Err(causes)),
            }
        }
        Ok(Ok(GroupEntries {
            submitted: members.len(),
            memberships: match excluded {
                Some(causes) if held == 0 => Err(causes),
                _ => Ok(held),
            },
            relations: match unread {
                Some(causes) if !relation_array => Err(causes),
                _ => Ok(relations),
            },
            keys,
            entries,
            whole: self
                .values
                .member(self.document, self.declaration, "deploymentGroups"),
        }))
    }

    /// The elements of the `hydrationRelations` of the group value `entry`:
    /// none when it is omitted, or why it has none to read. A value that is
    /// no object has no member to read.
    fn relation_nodes(
        &self,
        entry: NodeId,
    ) -> Result<Admitted<Option<&'r [NodeId]>>, StructuralInvariant> {
        if matches!(self.document.node(entry).kind(), NodeKind::Object(_)) {
            self.array_at(entry, "hydrationRelations")
        } else {
            self.failed(entry)
        }
    }

    /// The element `relation` of a group's `hydrationRelations`, or why it
    /// is no relation. Both endpoints admitted are its identity; otherwise
    /// its content keys it (D15:2181, 2190, 2203).
    fn relation_entry(
        &self,
        relation: NodeId,
    ) -> Result<Admitted<RelationEntry>, StructuralInvariant> {
        let NodeKind::Object(members) = self.document.node(relation).kind() else {
            return self.failed(relation);
        };
        let server = self.required_at::<TargetId>(relation, "server")?;
        let client = self.required_at::<TargetId>(relation, "client")?;
        let key = match (&server, &client) {
            (Ok(server), Ok(client)) => RelationKey::Pair(server.clone(), client.clone()),
            _ => RelationKey::Content(RelationContent::read(self.document, members)),
        };
        Ok(Ok(RelationEntry {
            key,
            server,
            client,
        }))
    }

    /// The required map `name` of the declaration and its members, or why it
    /// has none to read.
    fn required_map(&self, name: &str) -> Result<Admitted<&'r Members>, StructuralInvariant> {
        let document: &'r MaterializedDocument = self.document;
        let map = self.member_value(self.declaration, name);
        match map.map(|map| (map, document.node(map).kind())) {
            Some((map, NodeKind::Object(members))) if self.values.value(map).is_ok() => {
                Ok(Ok(members))
            }
            _ => match self.values.member(document, self.declaration, name) {
                Err(causes) => Ok(Err(causes)),
                Ok(()) => Err(StructuralInvariant::UnreportedFailure),
            },
        }
    }

    /// The alias map, entry by entry, or why it has no entry to read. An
    /// absent `localeNegotiation` or `aliases` holds none.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a value the checks admitted that is of
    /// another type than its schema's.
    pub(crate) fn alias_entries(&self) -> Result<Admitted<AliasEntries>, StructuralInvariant> {
        let members = match self.member_value(self.declaration, "localeNegotiation") {
            Some(negotiation) => match self.map_at(negotiation, "aliases")? {
                Ok(map) => map.map(|(_, members)| members),
                Err(causes) => return Ok(Err(causes)),
            },
            None => None,
        };
        let entries = members
            .into_iter()
            .flatten()
            .map(|(key, member)| {
                Ok(AliasEntry {
                    key: key.clone(),
                    value: self.value_at(member.value())?,
                })
            })
            .collect::<Result<_, StructuralInvariant>>()?;
        Ok(Ok(AliasEntries {
            entries,
            whole: self
                .values
                .member(self.document, self.declaration, "localeNegotiation"),
        }))
    }

    /// The fallback map, entry by entry, or why it has no entry to read. An
    /// absent `messageFallback` holds none.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a value the checks admitted that is of
    /// another type than its schema's.
    pub(crate) fn fallback_entries(
        &self,
    ) -> Result<Admitted<Vec<FallbackEntry>>, StructuralInvariant> {
        let map = match self.map_at(self.declaration, "messageFallback")? {
            Ok(map) => map,
            Err(causes) => return Ok(Err(causes)),
        };
        map.into_iter()
            .flat_map(|(map, members)| members.keys().map(move |key| (map, key)))
            .map(|(map, key)| {
                Ok(FallbackEntry {
                    key: key.clone(),
                    sequence: self.items_at(map, key)?,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Ok)
    }

    /// The declaration's `coverage`, rule by rule, or why it has no member to
    /// read. An absent `coverage` holds no rule.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a value the checks admitted that is of
    /// another type than its schema's.
    pub(crate) fn coverage_entries(
        &self,
    ) -> Result<Admitted<CoverageEntries>, StructuralInvariant> {
        let Some(coverage) = self.member_value(self.declaration, "coverage") else {
            return Ok(Ok(CoverageEntries::default()));
        };
        if !matches!(self.document.node(coverage).kind(), NodeKind::Object(_)) {
            return self.failed(coverage);
        }
        let default_mode = match self.member_value(coverage, "defaultMode") {
            Some(mode) => self.value_at(mode)?.map(Some),
            None => Ok(None),
        };
        let elements = self.array_at(coverage, "rules")?;
        let (rules, locale_items, surface_items) = match elements {
            Ok(rules) => {
                let rules = rules.unwrap_or_default();
                (
                    Ok(rules
                        .iter()
                        .map(|&rule| self.rule_entry(rule))
                        .collect::<Result<Vec<_>, _>>()?),
                    self.selector_items(rules, "requestedLocales")?,
                    self.selector_items(rules, "intentSurfaceClasses")?,
                )
            }
            Err(causes) => (Err(causes.clone()), Err(causes.clone()), Err(causes)),
        };
        Ok(Ok(CoverageEntries {
            default_mode,
            rules,
            locale_items,
            surface_items,
            whole: self
                .values
                .member(self.document, self.declaration, "coverage"),
        }))
    }

    /// How many raw items the selectors `name` of `rules` hold: every element
    /// of a selector that is an array, whatever the element is (D15:1902,
    /// 1903). A rule that is no object, or a selector that failed, has no item
    /// to count, so the count waits on why only when no selector holds one
    /// (D15:2184, 3354).
    fn selector_items(
        &self,
        rules: &[NodeId],
        name: &str,
    ) -> Result<Admitted<usize>, StructuralInvariant> {
        let mut held = 0;
        let mut excluded = None;
        for &rule in rules {
            let items = if matches!(self.document.node(rule).kind(), NodeKind::Object(_)) {
                self.array_at(rule, name)?
            } else {
                self.failed(rule)?
            };
            match items {
                Ok(Some(items)) => held += items.len(),
                Ok(None) => {}
                Err(causes) => excluded = Some(joined(excluded, &causes)),
            }
        }
        Ok(match excluded {
            Some(causes) if held == 0 => Err(causes),
            _ => Ok(held),
        })
    }

    /// The element `rule` of `coverage.rules`, or why it is no rule: a value
    /// that is no object has no member to read (D15:2184).
    fn rule_entry(&self, rule: NodeId) -> Result<Admitted<RuleEntry>, StructuralInvariant> {
        let NodeKind::Object(members) = self.document.node(rule).kind() else {
            return self.failed(rule);
        };
        let mut requested_locales = self.selector_at(rule, "requestedLocales")?;
        let mut intent_surface_classes = self.selector_at(rule, "intentSurfaceClasses")?;
        // Neither selector is the failed alternative of the rule object, the
        // value that check looked at.
        if matches!(
            (&requested_locales, &intent_surface_classes),
            (Ok(None), Ok(None))
        ) {
            let unconstrained = self.values.value(rule).map(|()| None);
            requested_locales.clone_from(&unconstrained);
            intent_surface_classes = unconstrained;
        }
        Ok(Ok(RuleEntry {
            content: RuleContent::read(self.document, members),
            requested_locales,
            intent_surface_classes,
            mode: self.required_at(rule, "mode")?,
        }))
    }

    /// The optional selector `name` of the rule `object`, item by item.
    fn selector_at(
        &self,
        object: NodeId,
        name: &str,
    ) -> Result<SelectorEntry, StructuralInvariant> {
        match self.array_at(object, name)? {
            Ok(Some(items)) => Ok(Ok(Some(
                items
                    .iter()
                    .map(|&item| self.value_at(item))
                    .collect::<Result<_, _>>()?,
            ))),
            Ok(None) => Ok(Ok(None)),
            Err(causes) => Ok(Err(causes)),
        }
    }

    /// Why the value at `node`, of another type than its schema's, has nothing
    /// to read.
    fn failed<T>(&self, node: NodeId) -> Result<Admitted<T>, StructuralInvariant> {
        match self.values.value(node) {
            Err(causes) => Ok(Err(causes)),
            Ok(()) => Err(StructuralInvariant::UnreportedFailure),
        }
    }

    /// The optional array `name` of `object` and its elements: none when it
    /// is absent, or why it has none to read.
    fn array_at(
        &self,
        object: NodeId,
        name: &str,
    ) -> Result<Admitted<Option<&'r [NodeId]>>, StructuralInvariant> {
        let document: &'r MaterializedDocument = self.document;
        let array = self.member_value(object, name);
        match array.map(|array| (array, document.node(array).kind())) {
            None => Ok(Ok(None)),
            Some((array, NodeKind::Array(items))) if self.values.value(array).is_ok() => {
                Ok(Ok(Some(items)))
            }
            _ => match self.values.member(document, object, name) {
                Err(causes) => Ok(Err(causes)),
                Ok(()) => Err(StructuralInvariant::UnreportedFailure),
            },
        }
    }

    /// The optional map `name` of `object` and its members: none when it is
    /// absent, or why it has none to read.
    fn map_at(
        &self,
        object: NodeId,
        name: &str,
    ) -> Result<Admitted<Option<(NodeId, &'r Members)>>, StructuralInvariant> {
        let document: &'r MaterializedDocument = self.document;
        let map = self.member_value(object, name);
        match map.map(|map| (map, document.node(map).kind())) {
            Some((map, NodeKind::Object(members))) if self.values.value(map).is_ok() => {
                Ok(Ok(Some((map, members))))
            }
            None if matches!(document.node(object).kind(), NodeKind::Object(_)) => Ok(Ok(None)),
            _ => match self.values.member(document, object, name) {
                Err(causes) => Ok(Err(causes)),
                Ok(()) => Err(StructuralInvariant::UnreportedFailure),
            },
        }
    }

    /// The value at `node` as `T`, or why it has none: only what looked at
    /// `node` itself decides, so a sibling's failure holds back nothing here.
    fn value_at<T: DeserializeOwned>(
        &self,
        node: NodeId,
    ) -> Result<Admitted<T>, StructuralInvariant> {
        match self.values.value(node) {
            Err(causes) => Ok(Err(causes)),
            Ok(()) => self.decoded(node).map(Ok),
        }
    }

    /// The required member `name` of `object` as `T`, or why it has none.
    fn required_at<T: DeserializeOwned>(
        &self,
        object: NodeId,
        name: &str,
    ) -> Result<Admitted<T>, StructuralInvariant> {
        if let Err(causes) = self.values.member(self.document, object, name) {
            return Ok(Err(causes));
        }
        let value = self
            .member_value(object, name)
            .ok_or(StructuralInvariant::UnreportedFailure)?;
        self.decoded(value).map(Ok)
    }

    /// The admitted value at `node`, as `T`.
    fn decoded<T: DeserializeOwned>(&self, node: NodeId) -> Result<T, StructuralInvariant> {
        self.document
            .decode(node)
            .map_err(|_| StructuralInvariant::Admission(AdmissionInvariant::AuthoringTypeMismatch))
    }

    /// The optional string member `name` of `object`, absent when it is, or
    /// why it has none.
    fn string_at_member(
        &self,
        object: NodeId,
        name: &str,
    ) -> Result<Admitted<Option<String>>, StructuralInvariant> {
        if let Err(causes) = self.values.member(self.document, object, name) {
            return Ok(Err(causes));
        }
        match self
            .member_value(object, name)
            .map(|value| self.document.node(value).kind())
        {
            None => Ok(Ok(None)),
            Some(NodeKind::String(text)) => Ok(Ok(Some(text.clone()))),
            Some(_) => Err(StructuralInvariant::Admission(
                AdmissionInvariant::AuthoringTypeMismatch,
            )),
        }
    }

    /// The required array of strings `name` of `object`, element by element.
    fn strings_at(
        &self,
        object: NodeId,
        name: &str,
    ) -> Result<Admitted<Vec<Admitted<String>>>, StructuralInvariant> {
        self.items_at(object, name)
    }

    /// The required array `name` of `object`, each element as `T` or why it
    /// has none. The array itself failing leaves no element to read.
    fn items_at<T: DeserializeOwned>(
        &self,
        object: NodeId,
        name: &str,
    ) -> Result<Admitted<Vec<Admitted<T>>>, StructuralInvariant> {
        let value = self.member_value(object, name);
        let items = match value.map(|array| (array, self.document.node(array).kind())) {
            Some((array, NodeKind::Array(items))) if self.values.value(array).is_ok() => items,
            _ => {
                return match self.values.member(self.document, object, name) {
                    Err(causes) => Ok(Err(causes)),
                    Ok(()) => Err(StructuralInvariant::UnreportedFailure),
                };
            }
        };
        items
            .iter()
            .map(|&item| self.value_at(item))
            .collect::<Result<Vec<_>, _>>()
            .map(Ok)
    }

    /// Every locale spelling of the declaration, as `maxLocaleOccurrences`
    /// counts them, whatever structural admission made of the fields that hold
    /// them: each string at a locale role and each key of a map keyed by
    /// locale. A value that is no string is no spelling, and an invalid string
    /// is one (D15:1896, 1907, 2184, 3348).
    pub(crate) fn locale_spellings(&self) -> Tally {
        let declaration = self.declaration;
        let member = |object, name| self.member_value(object, name);
        let aliases = member(declaration, "localeNegotiation")
            .and_then(|negotiation| member(negotiation, "aliases"));
        let rules = member(declaration, "coverage").and_then(|coverage| member(coverage, "rules"));
        let selectors = match rules.map(|rules| self.document.node(rules).kind()) {
            Some(NodeKind::Array(rules)) => total(
                rules
                    .iter()
                    .map(|&rule| self.strings_in(member(rule, "requestedLocales"))),
            ),
            _ => Tally::of(0),
        };
        total([
            self.strings_in(member(declaration, "requestedLocales")),
            self.string_at(member(declaration, "defaultRequestedLocale")),
            self.string_at(member(declaration, "defaultSourceLocale")),
            total(
                self.entries(aliases)
                    .map(|value| Tally::of(1).plus(self.string_at(Some(value)))),
            ),
            total(
                self.entries(member(declaration, "messageFallback"))
                    .map(|sequence| Tally::of(1).plus(self.strings_in(Some(sequence)))),
            ),
            selectors,
            total(
                self.entries(member(declaration, "targetProfiles"))
                    .map(|target| {
                        self.strings_in(member(target, "requestedLocales"))
                            .plus(self.string_at(member(target, "defaultRequestedLocale")))
                    }),
            ),
        ])
    }

    /// One when `value` is a string.
    fn string_at(&self, value: Option<NodeId>) -> Tally {
        Tally::of(usize::from(value.is_some_and(|value| {
            matches!(self.document.node(value).kind(), NodeKind::String(_))
        })))
    }

    /// The strings among the elements of `array`, when it is one.
    fn strings_in(&self, array: Option<NodeId>) -> Tally {
        match array.map(|array| self.document.node(array).kind()) {
            Some(NodeKind::Array(items)) => {
                total(items.iter().map(|&item| self.string_at(Some(item))))
            }
            _ => Tally::of(0),
        }
    }

    /// The value of each entry of `object`, when it is one.
    fn entries(&self, object: Option<NodeId>) -> impl Iterator<Item = NodeId> + '_ {
        object
            .map(|object| self.document.node(object).kind())
            .into_iter()
            .filter_map(|kind| match kind {
                NodeKind::Object(members) => Some(members),
                _ => None,
            })
            .flat_map(|members| members.values().map(ObjectMember::value))
    }

    /// The value of the member `name` of `object`, when it holds one.
    fn member_value(&self, object: NodeId, name: &str) -> Option<NodeId> {
        match self.document.node(object).kind() {
            NodeKind::Object(members) => members.get(name).map(ObjectMember::value),
            _ => None,
        }
    }
}

impl DeclarationFragments<'_, PolicyReference, TargetProfileReference> {
    /// Every reference of the declaration, whether or not structural
    /// admission admitted it, with what Phase 4's bounds read of each: each
    /// `policies` slot but an admitted `null`, then each entry's `profile`
    /// (D15:1918, 1924).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a reference the checks admitted that is
    /// absent or that its type cannot read.
    pub(crate) fn reference_occurrences(
        &self,
    ) -> Result<Vec<ReferenceOccurrence>, StructuralInvariant> {
        let policies = self.policies()?;
        let mut references = Vec::with_capacity(PolicySlot::ALL.len());
        for slot in PolicySlot::ALL {
            let Some(reference) = self.slot_reference::<PolicyReference>(policies, slot)? else {
                continue;
            };
            references.push(ReferenceOccurrence {
                field: Some(ReferenceField::Policy(slot)),
                occurs: self.member_value(policies, slot.name()).is_some(),
                bytes: reference.map(|reference| reference.canonical_bytes()),
            });
        }
        if let Some(targets) = self.member_value(self.declaration, "targetProfiles") {
            if let NodeKind::Object(entries) = self.document.node(targets).kind() {
                for (key, entry) in entries {
                    references.extend(self.target_occurrence(key, entry.value())?);
                }
            }
        }
        Ok(references)
    }

    /// The `profile` of the entry `key` of `targetProfiles`. An entry whose key
    /// failed has no later subject, so one without a `profile` has nothing
    /// to report here (D15:2184).
    fn target_occurrence(
        &self,
        key: &str,
        entry: NodeId,
    ) -> Result<Option<ReferenceOccurrence>, StructuralInvariant> {
        let field = match self.values.key(entry) {
            Ok(()) => Some(ReferenceField::TargetProfile(identity(key)?)),
            Err(_) => None,
        };
        let value = self.member_value(entry, "profile");
        let bytes = match (self.values.member(self.document, entry, "profile"), value) {
            (Err(_), None) if field.is_none() => return Ok(None),
            (Err(causes), _) => Err(causes),
            (Ok(()), Some(value)) => Ok(self
                .decoded::<TargetProfileReference>(value)?
                .canonical_bytes()),
            (Ok(()), None) => return Err(StructuralInvariant::UnreportedFailure),
        };
        Ok(Some(ReferenceOccurrence {
            field,
            occurs: value.is_some(),
            bytes,
        }))
    }
}

/// The identity an admitted key spells.
fn identity<T: DeserializeOwned>(key: &str) -> Result<T, StructuralInvariant> {
    T::deserialize(BorrowedStrDeserializer::<KeyError>::new(key))
        .map_err(|_| StructuralInvariant::UnreportedFailure)
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use crate::fixtures::{
        complete_config, minimal_config, FixturePolicyReference, FixtureTargetReference,
    };
    use crate::model::{CoverageMode, CoverageRule, ProfileDeclaration};
    use crate::references::{PolicyReference, TargetProfileReference};
    use crate::resolver::coverage::RuleContent;
    use crate::resolver::evaluation::{Admitted, Check, Evaluation, Subject, Unit};
    use crate::resolver::hydration::RelationKey;
    use crate::resolver::locales::fixtures::declaration;
    use crate::resolver::locales::LocaleFields;
    use crate::resolver::profile_resolution::tests::Inputs;
    use crate::resolver::resource_policy::{
        DeclarationReferences, PolicyReferences, PolicySlot, ReferenceField, ReferenceOccurrence,
        TargetReferences,
    };
    use crate::resolver::structural_admission::{
        EntryKey, StructuralInvariant, StructuralSubject as S,
    };
    use crate::resolver::value_admissions::tests::{
        structured_as, structured_in, Analysis, FIELDS,
    };
    use crate::resolver::value_admissions::ValueAdmissions;
    use crate::structural::{AdmissionInvariant, StructuralAnalysis};

    use super::{
        AliasEntries, CoverageEntries, DeclarationFragments, FallbackEntry, GroupEntries,
        RuleEntry, TargetEntries,
    };

    /// One way to break the declaration, by its name.
    type Edit = (&'static str, fn(&mut Value));

    /// Whether `unit` lies under the field `name` of the declaration `app`,
    /// the way the field's units are named: its path from the declaration, or
    /// a key or a field of an entry of the map the field holds.
    fn under(unit: &Unit, name: &str) -> bool {
        let app = |key: &EntryKey| matches!(key, EntryKey::Identity(id) if &**id == "app");
        match &unit.subject {
            Subject::Structural(S::ProfileField(profile, field)) => {
                app(profile) && field.path.first().is_some_and(|first| &**first == name)
            }
            Subject::Structural(S::TargetProfileKey(profile, _) | S::TargetField(profile, ..)) => {
                app(profile) && name == "targetProfiles"
            }
            Subject::Structural(S::DeploymentGroupKey(profile, _) | S::GroupField(profile, ..)) => {
                app(profile) && name == "deploymentGroups"
            }
            _ => false,
        }
    }

    /// What the units under each field say of it: their blocking results,
    /// and the causes of each not evaluated.
    fn by_units(evaluation: &Evaluation, name: &str) -> Admitted<()> {
        evaluation.admitted_within(|unit| under(unit, name))
    }

    /// Each field's fragment holds exactly what the units under it found.
    fn agrees(evaluation: &Evaluation, values: &ValueAdmissions, analysis: &Analysis, case: &str) {
        let fragments = DeclarationFragments::new(values, analysis, "app").unwrap();
        for name in FIELDS {
            let read = fragments.optional::<Value>(name).unwrap().map(|_| ());
            assert_eq!(read, by_units(evaluation, name), "{case}: {name}");
        }
    }

    /// Each kind of failure at each depth holds back the fields whose units
    /// found it, and no other.
    #[test]
    fn a_fragment_holds_what_the_units_of_its_field_found() {
        let edits: [Edit; 14] = [
            ("complete", |_| {}),
            ("scalar", |app| app["projectId"] = json!("Store Front")),
            ("element", |app| app["requestedLocales"] = json!(["en", 7])),
            ("container", |app| app["requestedLocales"] = json!("en")),
            ("missing", |app| {
                app.as_object_mut().unwrap().remove("selectionScope");
            }),
            ("undeclared", |app| app["extra"] = json!(1)),
            ("alias", |app| {
                app["localeNegotiation"] = json!({"aliases": {"fr": 1}, "x": 1});
            }),
            ("fallback", |app| {
                app["messageFallback"] = json!({"ja": [1]});
            }),
            ("rule", |app| {
                app["coverage"] = json!({"rules": [{"mode": "x"}]});
            }),
            ("slot", |app| app["policies"]["trust"] = json!(1)),
            ("target key", |app| {
                let target = app["targetProfiles"]["browser"].clone();
                app["targetProfiles"]["Web App"] = target;
            }),
            ("target field", |app| {
                app["targetProfiles"]["browser"]["requestedLocales"] = json!([1]);
            }),
            ("group", |app| {
                let groups = app["deploymentGroups"].as_object_mut().unwrap();
                let group = groups.values_mut().next().unwrap();
                group["hydrationRelations"] = json!([{"server": 1}]);
            }),
            ("delivery", |app| {
                app["delivery"] = json!({"placement": "hoist"});
            }),
        ];
        for (case, edit) in edits {
            let mut config = complete_config();
            edit(&mut config["profiles"]["app"]);
            let (evaluation, values, analysis) = structured_in(Evaluation::default(), &config);
            agrees(&evaluation, &values, &analysis, case);
        }
    }

    /// Whichever structural unit of a complete configuration blocks, each
    /// fragment holds what the units of its field say: the checks decide. A
    /// unit before the scope levels leaves no analysis to read.
    #[test]
    fn a_blocked_unit_holds_back_the_fragments_of_its_field() {
        let config = complete_config();
        let (complete, _, _) = structured_in(Evaluation::default(), &config);
        let scope_levels = complete
            .candidates()
            .map(|(unit, _)| unit)
            .filter(|unit| matches!(unit.subject, Subject::Structural(_)));
        for unit in scope_levels {
            let blocking = Evaluation::blocking(unit.clone());
            let (evaluation, values, analysis) = structured_in(blocking, &config);
            agrees(&evaluation, &values, &analysis, &format!("{unit:?}"));
        }
    }

    /// A declaration the analysis does not hold is a resolver bug.
    #[test]
    fn a_declaration_the_analysis_lacks_is_a_resolver_bug() {
        let (_, values, analysis) = structured_in(Evaluation::default(), &minimal_config());
        assert_eq!(
            DeclarationFragments::new(&values, &analysis, "absent").err(),
            Some(StructuralInvariant::UnreportedFailure)
        );
    }

    /// The project's `requestedLocales` once `edit` changed the declaration.
    fn items(
        admissions: Option<ValueAdmissions>,
        edit: fn(&mut Value),
    ) -> Result<Admitted<Vec<Admitted<String>>>, StructuralInvariant> {
        let mut config = complete_config();
        edit(&mut config["profiles"]["app"]);
        let (_, values, analysis) = structured_in(Evaluation::default(), &config);
        let values = admissions.unwrap_or(values);
        DeclarationFragments::new(&values, &analysis, "app")?.string_items("requestedLocales")
    }

    /// Each element is read on its own: one that failed holds back no other,
    /// and an array that failed itself has no element to read (D15:2367).
    #[test]
    fn an_array_of_strings_is_read_element_by_element() {
        assert_eq!(items(None, |_| {}), Ok(Ok(vec![Ok("en".to_owned())])));
        let read = items(None, |app| app["requestedLocales"] = json!(["ja", 7, "zz"]));
        let read = read.unwrap().unwrap();
        assert_eq!(read[0], Ok("ja".to_owned()));
        assert!(read[1].is_err());
        assert_eq!(read[2], Ok("zz".to_owned()));
        let whole: [fn(&mut Value); 3] = [
            |app| app["requestedLocales"] = json!("en"),
            |app| app["requestedLocales"] = json!([]),
            |app| lacking(app, &[], "requestedLocales"),
        ];
        for edit in whole {
            assert!(items(None, edit).unwrap().is_err());
        }
    }

    /// An array the checks admitted that is not one, and an element they
    /// admitted that is no string, are resolver bugs.
    #[test]
    fn an_array_the_checks_admit_and_the_value_lacks_is_a_resolver_bug() {
        let bug = |edit| items(Some(ValueAdmissions::default()), edit).err();
        let unreported: [fn(&mut Value); 2] = [
            |app| app["requestedLocales"] = json!("en"),
            |app| lacking(app, &[], "requestedLocales"),
        ];
        for edit in unreported {
            assert_eq!(bug(edit), Some(StructuralInvariant::UnreportedFailure));
        }
        assert_eq!(
            bug(|app| app["requestedLocales"] = json!([7])),
            Some(StructuralInvariant::Admission(
                AdmissionInvariant::AuthoringTypeMismatch
            ))
        );
    }

    /// The `policies` slots once `edit` changed the declaration.
    fn slots(
        admissions: Option<ValueAdmissions>,
        edit: fn(&mut Value),
    ) -> Result<Vec<(PolicySlot, Admitted<FixturePolicyReference>)>, StructuralInvariant> {
        let mut config = complete_config();
        edit(&mut config["profiles"]["app"]);
        let (_, values, analysis) = structured_in(Evaluation::default(), &config);
        let values = admissions.unwrap_or(values);
        let read = DeclarationFragments::new(&values, &analysis, "app")?.policy_slots()?;
        Ok(read.slots)
    }

    /// Each slot is read on its own: one that failed or is missing holds back
    /// no other, an admitted `null` has no entry, and an object that failed
    /// leaves every slot with why (D15:2137, 2367).
    #[test]
    fn the_policies_are_read_slot_by_slot() {
        let held =
            |read: &[(PolicySlot, Admitted<FixturePolicyReference>)]| -> Vec<(PolicySlot, bool)> {
                read.iter()
                    .map(|(slot, each)| (*slot, each.is_ok()))
                    .collect()
            };
        let every = |failed: Option<PolicySlot>| -> Vec<(PolicySlot, bool)> {
            PolicySlot::ALL
                .map(|slot| (slot, Some(slot) != failed))
                .into()
        };
        assert_eq!(held(&slots(None, |_| {}).unwrap()), every(None));
        for edit in [
            (|app| app["policies"]["trust"] = json!(42)) as fn(&mut Value),
            |app| lacking(app, &["policies"], "trust"),
        ] {
            assert_eq!(
                held(&slots(None, edit).unwrap()),
                every(Some(PolicySlot::Trust))
            );
        }
        let nulled = slots(None, |app| app["policies"]["glossarySet"] = Value::Null).unwrap();
        let mut expected = every(None);
        expected.retain(|(slot, _)| *slot != PolicySlot::GlossarySet);
        assert_eq!(held(&nulled), expected);
        let object = slots(None, |app| app["policies"] = json!(7)).unwrap();
        assert_eq!(object.len(), PolicySlot::ALL.len());
        assert!(object.iter().all(|(_, each)| each.is_err()));
    }

    /// Whether the `policies` object passed once `edit` changed the
    /// declaration.
    fn policy_object(edit: fn(&mut Value)) -> Admitted<()> {
        let mut config = complete_config();
        edit(&mut config["profiles"]["app"]);
        let (_, values, analysis) = structured_in(Evaluation::default(), &config);
        DeclarationFragments::<FixturePolicyReference, FixtureTargetReference>::new(
            &values, &analysis, "app",
        )
        .unwrap()
        .policy_slots()
        .unwrap()
        .object
    }

    /// An undeclared member is the object's own failure, in no slot, while a
    /// slot that failed or is missing is that slot's alone (D15:2184, 2367).
    #[test]
    fn the_policies_object_passes_apart_from_its_slots() {
        assert_eq!(policy_object(|_| {}), Ok(()));
        assert!(policy_object(|app| app["policies"]["extra"] = json!(1)).is_err());
        let undeclared = slots(None, |app| app["policies"]["extra"] = json!(1)).unwrap();
        assert_eq!(undeclared.len(), PolicySlot::ALL.len());
        assert!(undeclared.iter().all(|(_, each)| each.is_ok()));
        let of_a_slot: [fn(&mut Value); 2] = [
            |app| app["policies"]["trust"] = json!(42),
            |app| lacking(app, &["policies"], "trust"),
        ];
        for edit in of_a_slot {
            assert_eq!(policy_object(edit), Ok(()));
        }
    }

    /// A declaration without `policies`, a slot the checks admitted that is
    /// absent, and one its type cannot read are resolver bugs.
    #[test]
    fn a_slot_the_checks_admit_and_the_value_lacks_is_a_resolver_bug() {
        let bug = |edit| slots(Some(ValueAdmissions::default()), edit).err();
        let unreported: [fn(&mut Value); 2] = [
            |app| lacking(app, &[], "policies"),
            |app| lacking(app, &["policies"], "trust"),
        ];
        for edit in unreported {
            assert_eq!(bug(edit), Some(StructuralInvariant::UnreportedFailure));
        }
        assert_eq!(
            bug(|app| app["policies"]["trust"] = json!(42)),
            Some(StructuralInvariant::Admission(
                AdmissionInvariant::AuthoringTypeMismatch
            ))
        );
    }

    /// The Target ID map once `edit` changed the declaration.
    fn entries(
        admissions: Option<ValueAdmissions>,
        edit: fn(&mut Value),
    ) -> Result<Admitted<TargetEntries<FixtureTargetReference>>, StructuralInvariant> {
        let mut config = complete_config();
        edit(&mut config["profiles"]["app"]);
        let (_, values, analysis) = structured_in(Evaluation::default(), &config);
        let values = admissions.unwrap_or(values);
        DeclarationFragments::new(&values, &analysis, "app")?.target_entries()
    }

    /// Each entry is read on its own: a field that failed holds back neither
    /// its sibling field nor another entry, an entry that is no object keeps
    /// its key's subject, and one whose key failed counts without one
    /// (D15:1930, 2184, 2367, 3354).
    #[test]
    fn the_target_map_is_read_entry_by_entry() {
        let whole = entries(None, |_| {}).unwrap().unwrap();
        assert_eq!(whole.submitted, 2);
        let ids: Vec<&str> = whole.entries.iter().map(|each| each.id.as_str()).collect();
        assert_eq!(ids, ["browser", "server"]);
        assert_eq!(
            whole.entries[0].requested_locales,
            Ok(vec![Ok("en".to_owned())])
        );
        assert_eq!(
            whole.entries[0].default_requested_locale,
            Ok(Some("en".to_owned()))
        );
        assert_eq!(whole.entries[1].default_requested_locale, Ok(None));
        assert!(whole.entries.iter().all(|each| each.profile.is_ok()));
        assert_eq!((whole.keys, whole.whole), (Ok(()), Ok(())));

        let read = entries(None, |app| {
            app["targetProfiles"]["browser"]["requestedLocales"] = json!(["ja", 7]);
            app["targetProfiles"]["server"]["defaultRequestedLocale"] = json!(7);
        });
        let read = read.unwrap().unwrap();
        let items = read.entries[0].requested_locales.as_ref().unwrap();
        assert_eq!(items[0], Ok("ja".to_owned()));
        assert!(items[1].is_err());
        assert_eq!(
            read.entries[0].default_requested_locale,
            Ok(Some("en".to_owned()))
        );
        assert_eq!(
            read.entries[1].requested_locales,
            Ok(vec![Ok("en".to_owned())])
        );
        assert!(read.entries[1].default_requested_locale.is_err());
        assert!(read.whole.is_err());

        let read = entries(None, |app| {
            app["targetProfiles"]["server"]["profile"] = json!(7);
        });
        let read = read.unwrap().unwrap();
        assert!(read.entries[0].profile.is_ok() && read.entries[1].profile.is_err());
        assert!(read.entries[1].requested_locales.is_ok());

        let read = entries(None, |app| app["targetProfiles"]["server"] = json!(7));
        let read = read.unwrap().unwrap();
        assert_eq!(read.submitted, 2);
        assert!(read.entries[1].requested_locales.is_err());
        assert!(read.entries[1].default_requested_locale.is_err());
        assert!(read.entries[0].requested_locales.is_ok());

        let read = entries(None, |app| {
            let copy = app["targetProfiles"]["server"].clone();
            app["targetProfiles"]["Web App"] = copy;
        });
        let read = read.unwrap().unwrap();
        assert_eq!(read.submitted, 3);
        let ids: Vec<&str> = read.entries.iter().map(|each| each.id.as_str()).collect();
        assert_eq!(ids, ["browser", "server"]);
        assert!(read.keys.is_err());
    }

    /// A map that failed itself, or is missing, has no entry to read.
    #[test]
    fn a_target_map_that_failed_has_no_entries() {
        let whole: [fn(&mut Value); 3] = [
            |app| app["targetProfiles"] = json!(7),
            |app| app["targetProfiles"] = json!({}),
            |app| lacking(app, &[], "targetProfiles"),
        ];
        for edit in whole {
            assert!(entries(None, edit).unwrap().is_err());
        }
    }

    /// What the checks admitted and the document does not hold is a resolver
    /// bug: a map that is no object or is absent, a key that is no Target ID,
    /// and a default that is no string.
    #[test]
    fn a_target_map_the_checks_admit_and_the_value_lacks_is_a_resolver_bug() {
        let bug = |edit| entries(Some(ValueAdmissions::default()), edit).err();
        let unreported: [fn(&mut Value); 4] = [
            |app| app["targetProfiles"] = json!(7),
            |app| lacking(app, &[], "targetProfiles"),
            |app| {
                let copy = app["targetProfiles"]["server"].clone();
                app["targetProfiles"]["Web App"] = copy;
            },
            |app| lacking(app, &["targetProfiles", "server"], "profile"),
        ];
        for edit in unreported {
            assert_eq!(bug(edit), Some(StructuralInvariant::UnreportedFailure));
        }
        let mismatched: [fn(&mut Value); 2] = [
            |app| app["targetProfiles"]["server"]["defaultRequestedLocale"] = json!(7),
            |app| app["targetProfiles"]["server"]["profile"] = json!(7),
        ];
        for edit in mismatched {
            assert_eq!(
                bug(edit),
                Some(StructuralInvariant::Admission(
                    AdmissionInvariant::AuthoringTypeMismatch
                ))
            );
        }
    }

    /// The Group ID map once `edit` changed the declaration.
    fn groups(
        admissions: Option<ValueAdmissions>,
        edit: fn(&mut Value),
    ) -> Result<Admitted<GroupEntries>, StructuralInvariant> {
        let mut config = complete_config();
        edit(&mut config["profiles"]["app"]);
        let (_, values, analysis) = structured_in(Evaluation::default(), &config);
        let values = admissions.unwrap_or(values);
        DeclarationFragments::new(&values, &analysis, "app")?.group_entries()
    }

    /// Each entry is read on its own, and each element of its `members`: the
    /// counts take what was submitted, an entry whose key failed has no
    /// subject, and the total waits only when no entry holds an array
    /// (D15:1932, 1935, 2184, 2367, 3354).
    #[test]
    fn the_group_map_is_read_entry_by_entry() {
        let whole = groups(None, |_| {}).unwrap().unwrap();
        assert_eq!(
            (whole.submitted, whole.memberships, whole.keys),
            (1, Ok(2), Ok(()))
        );
        assert_eq!(whole.entries[0].id.as_str(), "web");
        let members = whole.entries[0].members.as_ref().unwrap();
        let spelled: Vec<&str> = members
            .iter()
            .map(|each| each.as_ref().unwrap().as_str())
            .collect();
        assert_eq!(spelled, ["browser", "server"]);

        let read = groups(None, |app| {
            app["deploymentGroups"]["web"]["members"] = json!(["browser", 7, "server"]);
        });
        let read = read.unwrap().unwrap();
        assert_eq!(read.memberships, Ok(3));
        let items = read.entries[0].members.as_ref().unwrap();
        assert!(items[0].is_ok() && items[1].is_err() && items[2].is_ok());

        let read = groups(None, |app| {
            app["deploymentGroups"]["Web App"] = json!({"members": ["worker"]});
        });
        let read = read.unwrap().unwrap();
        assert_eq!((read.submitted, read.memberships), (2, Ok(3)));
        assert!(read.keys.is_err());
        let ids: Vec<&str> = read.entries.iter().map(|each| each.id.as_str()).collect();
        assert_eq!(ids, ["web"]);

        let read = groups(None, |app| {
            app["deploymentGroups"]["jobs"] = json!({"members": 7});
        });
        let read = read.unwrap().unwrap();
        assert_eq!(read.memberships, Ok(2));
        assert!(read.entries[0].members.is_err() && read.entries[1].members.is_ok());

        let read = groups(None, |app| {
            app["deploymentGroups"]["web"]["members"] = json!(7);
            app["deploymentGroups"]["jobs"] = json!({"members": []});
        });
        let causes = read.unwrap().unwrap().memberships.unwrap_err();
        assert_eq!(causes.keys().count(), 2);
    }

    /// Each relation is read on its own: an element that is no object has
    /// no relation, and one with an endpoint that failed keeps the other under
    /// its content. The total takes every element of every array, of an entry
    /// whose key failed too (D15:1936, 2181, 2184, 2367).
    #[test]
    fn the_relations_of_a_group_are_read_element_by_element() {
        let read = groups(None, |app| {
            app["deploymentGroups"]["web"]["hydrationRelations"] = json!([
                {"server": "server", "client": "browser"},
                7,
                {"server": "server", "client": 8},
            ]);
            app["deploymentGroups"]["Web App"] =
                json!({"members": ["browser"], "hydrationRelations": [7]});
        });
        let read = read.unwrap().unwrap();
        assert_eq!(read.relations, Ok(4));
        let items = read.entries[0].relations.as_ref().unwrap();
        let pair = items[0].as_ref().unwrap();
        assert!(matches!(pair.key, RelationKey::Pair(..)));
        assert!(pair.server.is_ok() && pair.client.is_ok());
        assert!(items[1].is_err());
        let partial = items[2].as_ref().unwrap();
        assert!(matches!(partial.key, RelationKey::Content(_)));
        assert!(partial.server.is_ok() && partial.client.is_err());
    }

    /// A relation's content tells raw relations apart, a nested array in its
    /// order, and is never shown (D15:2186, 2190).
    #[test]
    fn a_relation_without_an_identity_is_named_by_its_content() {
        let read = groups(None, |app| {
            app["deploymentGroups"]["web"]["hydrationRelations"] = json!([
                {"server": "secret", "client": 7, "undeclared": [1, 2]},
                {"server": "secret", "client": 7, "undeclared": [2, 1]},
                {"server": "secret", "client": 7, "undeclared": [1, 2]},
            ]);
        });
        let read = read.unwrap().unwrap();
        let items = read.entries[0].relations.as_ref().unwrap();
        let key = |at: usize| &items[at].as_ref().unwrap().key;
        assert_ne!(key(0), key(1));
        assert_eq!(key(0), key(2));
        let shown = format!("{:?}", key(0));
        assert!(shown.starts_with("Content(RelationContent(") && shown.ends_with(" bytes))"));
    }

    /// The total waits only when no array can be read: an omitted array is
    /// no value, an empty one holds no occurrence, and a group that is no
    /// object has none to read (D15:569, 1936, 2184, 3354).
    #[test]
    fn the_relation_total_waits_only_when_no_array_can_be_read() {
        let total = |edit| groups(None, edit).unwrap().unwrap().relations;
        assert_eq!(
            total(|app| lacking(app, &["deploymentGroups", "web"], "hydrationRelations")),
            Ok(0)
        );
        let failed: fn(&mut Value) = |app| {
            app["deploymentGroups"]["web"]["hydrationRelations"] = json!(7);
        };
        assert!(total(failed).is_err());
        let read = groups(None, failed).unwrap().unwrap();
        assert!(read.entries[0].relations.is_err());
        assert_eq!(
            total(|app| {
                app["deploymentGroups"]["web"]["hydrationRelations"] = json!(7);
                app["deploymentGroups"]["jobs"] =
                    json!({"members": ["worker"], "hydrationRelations": []});
            }),
            Ok(0)
        );
        assert!(total(|app| app["deploymentGroups"]["web"] = json!(7)).is_err());
    }

    /// A map that failed itself, or is missing, has no entry to read.
    #[test]
    fn a_group_map_that_failed_has_no_entries() {
        let whole: [fn(&mut Value); 3] = [
            |app| app["deploymentGroups"] = json!(7),
            |app| app["deploymentGroups"] = json!({}),
            |app| lacking(app, &[], "deploymentGroups"),
        ];
        for edit in whole {
            assert!(groups(None, edit).unwrap().is_err());
        }
    }

    /// What the checks admitted and the document does not hold is a resolver
    /// bug: a map that is no object or is absent, a key that is no Group ID,
    /// members that are absent, and a member that is no Target ID.
    #[test]
    fn a_group_map_the_checks_admit_and_the_value_lacks_is_a_resolver_bug() {
        let bug = |edit| groups(Some(ValueAdmissions::default()), edit).err();
        let unreported: [fn(&mut Value); 7] = [
            |app| app["deploymentGroups"] = json!(7),
            |app| lacking(app, &[], "deploymentGroups"),
            |app| app["deploymentGroups"]["Web App"] = json!({"members": ["browser"]}),
            |app| lacking(app, &["deploymentGroups", "web"], "members"),
            |app| app["deploymentGroups"]["web"]["hydrationRelations"] = json!(7),
            |app| app["deploymentGroups"]["web"]["hydrationRelations"] = json!([7]),
            |app| {
                app["deploymentGroups"]["web"]["hydrationRelations"] =
                    json!([{"server": "server"}]);
            },
        ];
        for edit in unreported {
            assert_eq!(bug(edit), Some(StructuralInvariant::UnreportedFailure));
        }
        let mistyped: [fn(&mut Value); 2] = [
            |app| app["deploymentGroups"]["web"]["members"] = json!([7]),
            |app| {
                app["deploymentGroups"]["web"]["hydrationRelations"] =
                    json!([{"server": 7, "client": "browser"}]);
            },
        ];
        for edit in mistyped {
            assert_eq!(
                bug(edit),
                Some(StructuralInvariant::Admission(
                    AdmissionInvariant::AuthoringTypeMismatch
                ))
            );
        }
    }

    /// The alias map once `edit` changed the declaration.
    fn aliases(
        admissions: Option<ValueAdmissions>,
        edit: fn(&mut Value),
    ) -> Result<Admitted<AliasEntries>, StructuralInvariant> {
        let mut config = complete_config();
        edit(&mut config["profiles"]["app"]);
        let (_, values, analysis) = structured_in(Evaluation::default(), &config);
        let values = admissions.unwrap_or(values);
        DeclarationFragments::new(&values, &analysis, "app")?.alias_entries()
    }

    /// Each entry is read on its own: a destination that failed holds back
    /// no other, and an undeclared member beside the map holds back no entry
    /// while the whole waits on it (D15:1477, 2367, 3354).
    #[test]
    fn the_alias_map_is_read_entry_by_entry() {
        let whole = aliases(None, |_| {}).unwrap().unwrap();
        assert_eq!(whole.entries[0].key, "fr");
        assert_eq!(whole.entries[0].value, Ok("fr-FR".to_owned()));
        assert_eq!(whole.whole, Ok(()));

        let read = aliases(None, |app| {
            app["localeNegotiation"]["aliases"] = json!({"de": 7, "fr": "ja"});
        });
        let read = read.unwrap().unwrap();
        assert_eq!(read.entries.len(), 2);
        assert_eq!(read.entries[0].key, "de");
        assert!(read.entries[0].value.is_err());
        assert_eq!(read.entries[1].value, Ok("ja".to_owned()));
        assert!(read.whole.is_err());

        let read = aliases(None, |app| app["localeNegotiation"]["extra"] = json!(1));
        let read = read.unwrap().unwrap();
        assert_eq!(read.entries[0].value, Ok("fr-FR".to_owned()));
        assert!(read.whole.is_err());

        let empty: [fn(&mut Value); 2] = [
            |app| lacking(app, &[], "localeNegotiation"),
            |app| app["localeNegotiation"] = json!({}),
        ];
        for edit in empty {
            let read = aliases(None, edit).unwrap().unwrap();
            assert_eq!((read.entries.len(), read.whole), (0, Ok(())));
        }
        let failed: [fn(&mut Value); 2] = [
            |app| app["localeNegotiation"] = json!(7),
            |app| app["localeNegotiation"]["aliases"] = json!(7),
        ];
        for edit in failed {
            assert!(aliases(None, edit).unwrap().is_err());
        }
    }

    /// The checks decide: a map whose own check did not pass has no entry to
    /// read, even though the value is an object.
    #[test]
    fn an_alias_map_whose_own_check_did_not_pass_has_no_entries() {
        let config = complete_config();
        let (complete, _, _) = structured_in(Evaluation::default(), &config);
        let own = complete
            .candidates()
            .map(|(unit, _)| unit)
            .find(|unit| {
                unit.check == Check::FieldType
                    && matches!(&unit.subject, Subject::Structural(S::ProfileField(_, field))
                        if field.path.iter().map(|step| &**step)
                            .eq(["localeNegotiation", "aliases"]))
            })
            .unwrap();
        let (_, values, analysis) = structured_in(Evaluation::blocking(own.clone()), &config);
        let fragments = DeclarationFragments::new(&values, &analysis, "app").unwrap();
        assert!(fragments.alias_entries().unwrap().is_err());
    }

    /// What the checks admitted and the document does not hold is a resolver
    /// bug: an object or a map that is neither, and a destination that is no
    /// string.
    #[test]
    fn an_alias_map_the_checks_admit_and_the_value_lacks_is_a_resolver_bug() {
        let bug = |edit| aliases(Some(ValueAdmissions::default()), edit).err();
        let unreported: [fn(&mut Value); 2] = [
            |app| app["localeNegotiation"] = json!(7),
            |app| app["localeNegotiation"]["aliases"] = json!(7),
        ];
        for edit in unreported {
            assert_eq!(bug(edit), Some(StructuralInvariant::UnreportedFailure));
        }
        assert_eq!(
            bug(|app| app["localeNegotiation"]["aliases"] = json!({"de": 7})),
            Some(StructuralInvariant::Admission(
                AdmissionInvariant::AuthoringTypeMismatch
            ))
        );
    }

    /// The fallback map once `edit` changed the declaration.
    fn fallback(
        admissions: Option<ValueAdmissions>,
        edit: fn(&mut Value),
    ) -> Result<Admitted<Vec<FallbackEntry>>, StructuralInvariant> {
        let mut config = complete_config();
        edit(&mut config["profiles"]["app"]);
        let (_, values, analysis) = structured_in(Evaluation::default(), &config);
        let values = admissions.unwrap_or(values);
        DeclarationFragments::new(&values, &analysis, "app")?.fallback_entries()
    }

    /// Each entry and each candidate is read on its own: a sequence that
    /// failed holds back no other, and a candidate that failed none beside it
    /// (D15:1519, 2367).
    #[test]
    fn the_fallback_map_is_read_entry_by_entry() {
        let whole = fallback(None, |_| {}).unwrap().unwrap();
        assert_eq!(whole[0].key, "ja");
        let candidates = whole[0].sequence.as_ref().unwrap();
        assert_eq!(candidates.len(), 2);
        assert!(candidates.iter().all(Result::is_ok));

        let read = fallback(None, |app| {
            app["messageFallback"] = json!({"de": 7, "en": [], "ja": ["en", 42]});
        });
        let read = read.unwrap().unwrap();
        assert_eq!(read.len(), 3);
        assert!(read[0].sequence.is_err() && read[1].sequence.is_err());
        let candidates = read[2].sequence.as_ref().unwrap();
        assert!(candidates[0].is_ok() && candidates[1].is_err());

        let absent = fallback(None, |app| lacking(app, &[], "messageFallback"));
        assert_eq!(absent, Ok(Ok(Vec::new())));
        let failed = fallback(None, |app| app["messageFallback"] = json!(7));
        assert!(failed.unwrap().is_err());
    }

    /// What the checks admitted and the document does not hold is a resolver
    /// bug: a map or a sequence of another type, and a candidate of neither
    /// kind.
    #[test]
    fn a_fallback_map_the_checks_admit_and_the_value_lacks_is_a_resolver_bug() {
        let bug = |edit| fallback(Some(ValueAdmissions::default()), edit).err();
        let unreported: [fn(&mut Value); 2] = [
            |app| app["messageFallback"] = json!(7),
            |app| app["messageFallback"]["ja"] = json!(7),
        ];
        for edit in unreported {
            assert_eq!(bug(edit), Some(StructuralInvariant::UnreportedFailure));
        }
        assert_eq!(
            bug(|app| app["messageFallback"]["ja"] = json!([7])),
            Some(StructuralInvariant::Admission(
                AdmissionInvariant::AuthoringTypeMismatch
            ))
        );
    }

    fn coverage(
        admissions: Option<ValueAdmissions>,
        edit: impl FnOnce(&mut Value),
    ) -> Result<Admitted<CoverageEntries>, StructuralInvariant> {
        let mut config = complete_config();
        edit(&mut config["profiles"]["app"]);
        let (_, values, analysis) = structured_in(Evaluation::default(), &config);
        let values = admissions.unwrap_or(values);
        DeclarationFragments::new(&values, &analysis, "app")?.coverage_entries()
    }

    /// The entries of `rules` under `coverage`.
    fn rules(rules: &Value) -> CoverageEntries {
        coverage(None, |app| app["coverage"] = json!({ "rules": rules }))
            .unwrap()
            .unwrap()
    }

    /// The element at `at` of what `entries` read as a rule.
    fn rule(entries: &CoverageEntries, at: usize) -> &RuleEntry {
        entries.rules.as_ref().unwrap()[at].as_ref().unwrap()
    }

    /// Each rule is read on its own, each member and item as structural
    /// admission left it, and only what is at or below `coverage` holds back
    /// the whole (D15:2184, 2190, 2367, 2426).
    #[test]
    fn the_coverage_is_read_rule_by_rule() {
        let whole = coverage(None, |_| {}).unwrap().unwrap();
        assert_eq!(whole.default_mode, Ok(Some(CoverageMode::DirectRequired)));
        assert_eq!(whole.rules.as_ref().unwrap().len(), 3);
        assert_eq!((whole.locale_items, whole.surface_items), (Ok(2), Ok(2)));
        assert_eq!(whole.whole, Ok(()));
        assert_eq!(
            coverage(None, |app| lacking(app, &[], "coverage")),
            Ok(Ok(CoverageEntries::default()))
        );
        let omitted = coverage(None, |app| app["coverage"] = json!({})).unwrap();
        assert_eq!(omitted, Ok(CoverageEntries::default()));
        assert!(coverage(None, |app| app["coverage"] = json!(7))
            .unwrap()
            .is_err());

        let unread = coverage(None, |app| {
            app["coverage"] = json!({"defaultMode": "fallback-allowed", "rules": 7});
        });
        let unread = unread.unwrap().unwrap();
        assert_eq!(unread.default_mode, Ok(Some(CoverageMode::FallbackAllowed)));
        assert!(unread.rules.is_err() && unread.whole.is_err());
        assert!(unread.locale_items.is_err() && unread.surface_items.is_err());

        let failed = coverage(None, |app| app["coverage"]["defaultMode"] = json!(7));
        let failed = failed.unwrap().unwrap();
        assert!(failed.default_mode.is_err() && failed.whole.is_err());
        assert_eq!(failed.rules.as_ref().unwrap().len(), 3);

        let read = rules(&json!([
            7,
            {"requestedLocales": ["ja", 7], "intentSurfaceClasses": 7, "mode": "direct-required"},
            {"requestedLocales": ["ja"], "mode": 7},
            {"mode": "direct-required"},
            {"intentSurfaceClasses": ["nav"], "mode": "direct-required", "extra": 1},
        ]));
        assert!(read.rules.as_ref().unwrap()[0].is_err());
        let items = rule(&read, 1).requested_locales.as_ref().unwrap();
        let items = items.as_ref().unwrap();
        assert!(items[0].is_ok() && items[1].is_err());
        assert!(rule(&read, 1).intent_surface_classes.is_err());
        assert_eq!(rule(&read, 1).mode, Ok(CoverageMode::DirectRequired));
        assert_eq!(rule(&read, 2).intent_surface_classes, Ok(None));
        assert!(rule(&read, 2).mode.is_err());
        // Neither selector is the alternative the rule failed.
        assert!(rule(&read, 3).requested_locales.is_err());
        assert!(rule(&read, 3).intent_surface_classes.is_err());
        // An undeclared member holds back neither its selector nor its mode.
        assert!(matches!(rule(&read, 4).intent_surface_classes, Ok(Some(_))));
        assert_eq!(rule(&read, 4).mode, Ok(CoverageMode::DirectRequired));
        assert!(read.whole.is_err());
    }

    /// A selector count takes every element of a selector that is an array,
    /// and waits only when no selector holds one (D15:1902, 1903, 2184, 3354).
    #[test]
    fn a_selector_count_takes_every_item_of_what_is_an_array() {
        let read = rules(&json!([
            7,
            {"requestedLocales": ["ja", 7, true], "intentSurfaceClasses": 7, "mode": 7},
            {"requestedLocales": "ja", "intentSurfaceClasses": ["nav"], "mode": 7},
        ]));
        assert_eq!((read.locale_items, read.surface_items), (Ok(3), Ok(1)));
        let none = rules(&json!([7, {"requestedLocales": 7, "mode": 7}]));
        assert!(none.locale_items.is_err());
        assert!(none.surface_items.is_err());
        let beside = rules(&json!([{"requestedLocales": 7, "mode": 7}]));
        assert!(beside.locale_items.is_err());
        assert_eq!(beside.surface_items, Ok(0));
        // An empty selector failed its own check, so it has no item to count.
        let empty = rules(&json!([{"intentSurfaceClasses": [], "mode": 7}]));
        assert!(empty.surface_items.is_err());
        // The causes join every value excluded.
        let distinct = rules(&json!([7, {"requestedLocales": 7, "mode": "direct-required"}]));
        assert_eq!(distinct.locale_items.unwrap_err().keys().count(), 2);
    }

    /// A rule of the admitted form reads as the typed rule's content, and any
    /// other is told apart by what it holds: the selectors as sets with every
    /// repeat, any other array in order (D15:2186, 2190).
    #[test]
    fn a_rule_is_named_by_the_content_it_was_authored_with() {
        let content = |value: Value| rule(&rules(&json!([value])), 0).content.clone();
        for value in [
            json!({"requestedLocales": ["ja", "en", "ja"], "mode": "direct-required"}),
            json!({"intentSurfaceClasses": ["nav"], "mode": "fallback-allowed"}),
            json!({
                "intentSurfaceClasses": ["nav", "checkout"],
                "mode": "direct-required",
                "requestedLocales": ["en"]
            }),
        ] {
            let typed: CoverageRule = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(content(value), RuleContent::of(&typed));
        }
        assert_eq!(
            content(json!({"requestedLocales": ["b", 7, "a"], "mode": 7})),
            content(json!({"requestedLocales": [7, "a", "b"], "mode": 7}))
        );
        assert_eq!(
            content(json!({"intentSurfaceClasses": ["b", 7], "mode": 7})),
            content(json!({"intentSurfaceClasses": [7, "b"], "mode": 7}))
        );
        assert_ne!(
            content(json!({"requestedLocales": ["a", "a", 7], "mode": 7})),
            content(json!({"requestedLocales": ["a", 7], "mode": 7}))
        );
        assert_ne!(
            content(json!({"requestedLocales": [[1, 2]], "mode": 7})),
            content(json!({"requestedLocales": [[2, 1]], "mode": 7}))
        );
        assert_ne!(
            content(json!({"requestedLocales": ["a"], "mode": 7, "extra": [1, 2]})),
            content(json!({"requestedLocales": ["a"], "mode": 7, "extra": [2, 1]}))
        );
        assert_ne!(
            content(json!({"requestedLocales": [{"nested": [1, 2]}], "mode": 7})),
            content(json!({"requestedLocales": [{"nested": [2, 1]}], "mode": 7}))
        );
        let shown = format!(
            "{:?}",
            content(json!({"requestedLocales": ["secret"], "mode": 7}))
        );
        assert!(!shown.contains("secret"), "{shown}");
    }

    /// Contents order as 015's identity does: by type, then within it, a
    /// shorter exact prefix first and members by name (D15:2186).
    #[test]
    fn a_rule_content_orders_as_its_canonical_identity() {
        let content = |mode: Value| rule(&rules(&json!([{"mode": mode}])), 0).content.clone();
        let ordered = [
            json!(null),
            json!(false),
            json!(true),
            json!(-2.5),
            json!(-1),
            json!(0),
            json!(0.5),
            json!(2),
            json!(""),
            json!("a"),
            json!("a\0"),
            json!("a\0b"),
            json!("ab"),
            json!([]),
            json!([null]),
            json!([null, 1]),
            json!([1]),
            // What follows an element never overtakes the element itself.
            json!(["a", {}]),
            json!(["a\0"]),
            json!([[1], 2]),
            json!([[1, 2]]),
            json!([{}, 2]),
            json!([{"": 1}]),
            json!({}),
            json!({"": 1}),
            json!({"a": 1}),
            json!({"a": 1, "b": 1}),
            json!({"a": 2}),
            json!({"a": {}, "b": 1}),
            json!({"a": {"b": 1}}),
            json!({"b": 0}),
        ]
        .map(content);
        assert!(ordered.windows(2).all(|pair| pair[0] < pair[1]));
    }

    /// What the checks admitted and the document does not hold is a resolver
    /// bug: a `coverage`, rule, or selector of another type, and an item or a
    /// mode that its type cannot read.
    #[test]
    fn a_coverage_the_checks_admit_and_the_value_lacks_is_a_resolver_bug() {
        let bug = |edit: fn(&mut Value)| coverage(Some(ValueAdmissions::default()), edit).err();
        let unreported: [fn(&mut Value); 4] = [
            |app| app["coverage"] = json!(7),
            |app| app["coverage"]["rules"] = json!(7),
            |app| app["coverage"]["rules"][0] = json!(7),
            |app| app["coverage"]["rules"][0]["requestedLocales"] = json!(7),
        ];
        for edit in unreported {
            assert_eq!(bug(edit), Some(StructuralInvariant::UnreportedFailure));
        }
        let mismatched: [fn(&mut Value); 3] = [
            |app| app["coverage"]["defaultMode"] = json!(7),
            |app| app["coverage"]["rules"][0]["requestedLocales"] = json!([7]),
            |app| app["coverage"]["rules"][0]["mode"] = json!(7),
        ];
        for edit in mismatched {
            assert_eq!(
                bug(edit),
                Some(StructuralInvariant::Admission(
                    AdmissionInvariant::AuthoringTypeMismatch
                ))
            );
        }
    }

    /// What the document holds of a declaration admitted whole is what its
    /// type counts, every role included (D15:1907).
    #[test]
    fn the_spellings_of_a_declaration_admitted_whole_are_those_its_type_counts() {
        for config in [minimal_config(), complete_config()] {
            let (_, values, analysis) = structured_in(Evaluation::default(), &config);
            let fragments = DeclarationFragments::new(&values, &analysis, "app").unwrap();
            let declared = declaration(&config, |_| {});
            assert_eq!(
                LocaleFields::declared(&declared).spellings,
                Ok(fragments.locale_spellings())
            );
        }
    }

    type Referencing = StructuralAnalysis<PolicyReference, TargetProfileReference>;

    /// The complete example's configuration once `edit` changed its
    /// declaration, and Phase 2 over it.
    fn analyzed(edit: impl FnOnce(&mut Value)) -> (ValueAdmissions, Referencing) {
        let mut app = Inputs::complete().declaration().clone();
        edit(&mut app);
        let config = json!({"schemaVersion": "0", "profiles": {"app": app}});
        let (_, values, analysis) = structured_as(Evaluation::default(), &config);
        (values, analysis)
    }

    fn occurrences(edit: impl FnOnce(&mut Value)) -> Vec<ReferenceOccurrence> {
        let (values, analysis) = analyzed(edit);
        DeclarationFragments::new(&values, &analysis, "app")
            .unwrap()
            .reference_occurrences()
            .unwrap()
    }

    /// Whether the reference at `field` occurs, and whether it has bytes.
    fn at(references: &[ReferenceOccurrence], field: &ReferenceField) -> (bool, bool) {
        let found = references
            .iter()
            .find(|each| each.field.as_ref() == Some(field));
        let found = found.unwrap();
        (found.occurs, found.bytes.is_ok())
    }

    fn server() -> ReferenceField {
        ReferenceField::TargetProfile(serde_json::from_value(json!("server")).unwrap())
    }

    fn lacking(app: &mut Value, object: &[&str], name: &str) {
        let held = object.iter().fold(app, |value, step| &mut value[*step]);
        held.as_object_mut().unwrap().remove(name);
    }

    /// Each slot of a complete declaration has its bytes. A value that failed
    /// occurs without them, a required member that is missing does not occur,
    /// and an admitted `null` is no reference (D15:1918, 1924).
    #[test]
    fn each_slot_occurs_as_structural_admission_left_it() {
        let app = Inputs::complete().declaration().clone();
        let declared: ProfileDeclaration<_, _> = serde_json::from_value(app).unwrap();
        let typed = DeclarationReferences {
            policies: PolicyReferences::declared(&declared.policies),
            target_profiles: Ok(TargetReferences::declared(&declared.target_profiles)),
        };
        assert_eq!(Ok(occurrences(|_| {})), typed.occurrences());
        let trust = ReferenceField::Policy(PolicySlot::Trust);
        let references = occurrences(|app| app["policies"]["trust"] = json!(42));
        assert_eq!(at(&references, &trust), (true, false));
        let references = occurrences(|app| app["policies"]["trust"] = Value::Null);
        assert_eq!(at(&references, &trust), (true, false));
        let references = occurrences(|app| lacking(app, &["policies"], "trust"));
        assert_eq!(at(&references, &trust), (false, false));
        let routing = Some(ReferenceField::Policy(PolicySlot::ProviderRouting));
        let references = occurrences(|app| app["policies"]["providerRouting"] = Value::Null);
        assert_eq!(references.len(), 8);
        assert!(!references.iter().any(|each| each.field == routing));
    }

    /// A target's `profile` occurs as a slot does; one under a key that
    /// failed has no subject but still counts with its bytes; and a map of
    /// the wrong type holds none (D15:1918, 1924, 2184).
    #[test]
    fn each_target_profile_occurs_as_structural_admission_left_it() {
        let references = occurrences(|app| app["targetProfiles"]["server"]["profile"] = json!(1));
        assert_eq!(at(&references, &server()), (true, false));
        let references = occurrences(|app| lacking(app, &["targetProfiles", "server"], "profile"));
        assert_eq!(at(&references, &server()), (false, false));
        let references = occurrences(|app| app["targetProfiles"]["server"] = json!(7));
        assert_eq!(at(&references, &server()), (false, false));
        let references = occurrences(|app| {
            app["targetProfiles"]["Web App"] = app["targetProfiles"]["server"].clone();
        });
        assert_eq!(references.len(), 10);
        let unnamed = references.iter().find(|each| each.field.is_none()).unwrap();
        assert!(unnamed.occurs && unnamed.bytes.is_ok());
        let failed = occurrences(|app| app["targetProfiles"]["Web App"] = json!({"profile": 1}));
        let unnamed = failed.iter().find(|each| each.field.is_none()).unwrap();
        assert!(unnamed.occurs && unnamed.bytes.is_err());
        let absent = occurrences(|app| app["targetProfiles"]["Web App"] = json!({}));
        assert_eq!(absent.len(), 9);
        assert_eq!(occurrences(|app| app["targetProfiles"] = json!(7)).len(), 7);
    }

    /// What the checks admitted and the value does not hold is a resolver
    /// bug.
    #[test]
    fn a_reference_the_checks_admit_and_the_value_lacks_is_a_resolver_bug() {
        let bug = |edit: fn(&mut Value)| {
            let (_, analysis) = analyzed(edit);
            DeclarationFragments::new(&ValueAdmissions::default(), &analysis, "app")
                .unwrap()
                .reference_occurrences()
                .err()
        };
        let unreported: [fn(&mut Value); 4] = [
            |app| lacking(app, &[], "policies"),
            |app| lacking(app, &["policies"], "trust"),
            |app| lacking(app, &["targetProfiles", "server"], "profile"),
            |app| app["targetProfiles"]["Web App"] = app["targetProfiles"]["server"].clone(),
        ];
        for edit in unreported {
            assert_eq!(bug(edit), Some(StructuralInvariant::UnreportedFailure));
        }
        let mismatched: [fn(&mut Value); 2] = [
            |app| app["policies"]["trust"] = json!(42),
            |app| app["targetProfiles"]["server"]["profile"] = json!(42),
        ];
        for edit in mismatched {
            assert_eq!(
                bug(edit),
                Some(StructuralInvariant::Admission(
                    AdmissionInvariant::AuthoringTypeMismatch
                ))
            );
        }
    }
}
