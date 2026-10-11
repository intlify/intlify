// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 017's `LocaleConformanceData`: the pinned CLDR validity records,
//! Unicode BCP 47 extension keys and types, and flattened corrections that
//! design 015's conformance layer uses. It travels as its own payload, written
//! as canonical JSON text revision "0", and the dataset's semantic digest
//! covers it.
//!
//! Reading checks the closed structure and the canonical order 017 fixes, so
//! the same records have one representation and one digest. Whether they are
//! the records a specification revision admits is for that digest to decide,
//! not this module.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use intlify_shared_json::text::{self, TextFailure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::primitives::NonemptyText;

/// The validity records, extension keys and types, and corrections that the
/// conformance layer of a Locale Canonicalization Specification uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocaleConformanceData {
    /// One group per field and status, sorted by field and then status.
    validity: Vec<ValidityGroup>,
    /// Every `u` and `t` key, sorted by extension and then key.
    extension_keys: Vec<ExtensionKey>,
    /// The flattened corrections, sorted by extension, key, and then `from`.
    corrections: Vec<Correction>,
}

/// The codes CLDR gives one status for one field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidityGroup {
    field: ValidityField,
    status: ValidityStatus,
    /// Expanded from CLDR range notation, in CLDR spelling and case, sorted by
    /// unsigned UTF-8 bytes without duplicates. A code is in at most one group
    /// of its field.
    codes: Vec<NonemptyText>,
}

/// A locale identifier field that CLDR validity data covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ValidityField {
    Language,
    Region,
    Script,
    Subdivision,
    Variant,
}

/// A CLDR validity status; `private-use` is CLDR's `private_use`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ValidityStatus {
    Deprecated,
    Macroregion,
    PrivateUse,
    Regular,
    Reserved,
    Special,
    Unknown,
}

/// One `u` or `t` key from the pinned CLDR `common/bcp47` records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionKey {
    extension: Extension,
    key: NonemptyText,
    deprecated: bool,
    /// `single` when the source omits it.
    value_type: ValueType,
    /// Sorted by kind and then by class or type, without duplicates.
    types: Vec<ExtensionType>,
}

/// The extension a key belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Extension {
    T,
    U,
}

/// CLDR's `valueType`: how a key's value is made of its types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ValueType {
    Any,
    Incremental,
    Multiple,
    Single,
}

/// One type a key admits: a class whose value grammar design 015 defines, or
/// one literal type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum ExtensionType {
    Class {
        class: TypeClass,
    },
    Literal {
        #[serde(rename = "type")]
        name: NonemptyText,
        deprecated: bool,
    },
}

/// A CLDR special type name, in lowercase with `-` in place of `_`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TypeClass {
    Codepoints,
    PrivateUse,
    ReorderCode,
    RgKeyValue,
    ScriptCode,
    SubdivisionCode,
}

/// A deprecated value mapped directly to its final replacement, where ICU4X
/// does not already realize the mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Correction {
    extension: Extension,
    key: NonemptyText,
    from: NonemptyText,
    to: NonemptyText,
}

/// The caller's finite limits on conformance data. They are checked on the
/// decoded value before any of it is projected, so a value over a limit is
/// refused whole rather than read in part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConformanceLimits {
    pub validity_groups: u64,
    pub codes: u64,
    pub extension_keys: u64,
    pub types: u64,
    pub corrections: u64,
    /// Validation work: every group, code, key, type, and correction, each
    /// counted once.
    pub work: u64,
}

/// One of the counts [`ConformanceLimits`] bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limited {
    ValidityGroups,
    Codes,
    ExtensionKeys,
    Types,
    Corrections,
    Work,
}

/// One ordered collection of the conformance data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Collection {
    ValidityGroups,
    Codes,
    ExtensionKeys,
    Types,
    Corrections,
}

/// Why bytes are not one conformance data value. No variant keeps any of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConformanceFailure {
    /// The bytes are not canonical JSON text revision "0".
    Text(TextFailure),
    /// A count is over the caller's limit.
    Limit(Limited),
    /// The value is not the closed structure: a member is missing, unknown,
    /// or of another type, or a field, status, value type, class, or kind is
    /// not one 017 names.
    Shape,
    /// A collection is out of its canonical order.
    Unsorted(Collection),
    /// A collection repeats what its order makes unique.
    Repeated(Collection),
    /// A code is in two groups of the same field.
    CodeInTwoGroups,
    /// A correction maps to a value that a correction of the same key maps
    /// from, so it is not flattened; this includes a value mapped to itself.
    Unflattened,
}

impl LocaleConformanceData {
    /// Read a conformance payload: canonical JSON text of the closed structure
    /// in canonical order.
    pub fn read(bytes: &[u8], limits: &ConformanceLimits) -> Result<Self, ConformanceFailure> {
        let value = text::read(bytes).map_err(ConformanceFailure::Text)?;
        Counts::of(&value).within(limits)?;
        let data: Self = serde_json::from_value(value).map_err(|_| ConformanceFailure::Shape)?;
        data.check()?;
        Ok(data)
    }

    /// The payload's exact bytes: this value as canonical JSON text.
    pub fn to_text(&self) -> String {
        let value = serde_json::to_value(self).expect("conformance data serializes to JSON");
        text::write(&value).expect("conformance data holds no number and nests shallowly")
    }

    /// Whether the value is in 017's canonical order, which makes its
    /// representation, and so its digest, unique.
    pub(crate) fn check(&self) -> Result<(), ConformanceFailure> {
        self.check_validity()?;
        self.check_extension_keys()?;
        self.check_corrections()
    }

    fn check_validity(&self) -> Result<(), ConformanceFailure> {
        ascending(
            &self.validity,
            ValidityGroup::order,
            Collection::ValidityGroups,
        )?;
        for group in &self.validity {
            ascending(&group.codes, text_order, Collection::Codes)?;
        }
        // Sorting by field puts every group of one field together.
        for groups in self.validity.chunk_by(|a, b| a.field == b.field) {
            let mut seen = BTreeSet::new();
            for code in groups.iter().flat_map(|group| &group.codes) {
                if !seen.insert(code.as_str()) {
                    return Err(ConformanceFailure::CodeInTwoGroups);
                }
            }
        }
        Ok(())
    }

    fn check_extension_keys(&self) -> Result<(), ConformanceFailure> {
        ascending(
            &self.extension_keys,
            ExtensionKey::order,
            Collection::ExtensionKeys,
        )?;
        for key in &self.extension_keys {
            ascending(&key.types, ExtensionType::order, Collection::Types)?;
        }
        Ok(())
    }

    fn check_corrections(&self) -> Result<(), ConformanceFailure> {
        ascending(
            &self.corrections,
            Correction::order,
            Collection::Corrections,
        )?;
        // Each key's corrections are together and sorted by `from`.
        for corrections in self
            .corrections
            .chunk_by(|a, b| (a.extension, &a.key) == (b.extension, &b.key))
        {
            let corrected = |value: &str| {
                corrections
                    .binary_search_by(|correction| correction.from.as_str().cmp(value))
                    .is_ok()
            };
            if corrections
                .iter()
                .any(|correction| corrected(correction.to.as_str()))
            {
                return Err(ConformanceFailure::Unflattened);
            }
        }
        Ok(())
    }
}

/// Whether each item is strictly after the one before it.
fn ascending<T>(
    items: &[T],
    order: impl Fn(&T, &T) -> Ordering,
    collection: Collection,
) -> Result<(), ConformanceFailure> {
    for pair in items.windows(2) {
        match order(&pair[0], &pair[1]) {
            Ordering::Less => {}
            Ordering::Equal => return Err(ConformanceFailure::Repeated(collection)),
            Ordering::Greater => return Err(ConformanceFailure::Unsorted(collection)),
        }
    }
    Ok(())
}

// Every order here compares spellings by unsigned UTF-8 bytes, which is how
// `str` orders.

fn text_order(a: &NonemptyText, b: &NonemptyText) -> Ordering {
    a.as_str().cmp(b.as_str())
}

impl ValidityGroup {
    fn order(&self, other: &Self) -> Ordering {
        (self.field.as_str(), self.status.as_str())
            .cmp(&(other.field.as_str(), other.status.as_str()))
    }
}

impl ExtensionKey {
    fn order(&self, other: &Self) -> Ordering {
        (self.extension.as_str(), self.key.as_str())
            .cmp(&(other.extension.as_str(), other.key.as_str()))
    }
}

impl ExtensionType {
    /// The kind, then the class or type, which is all that orders a type: a
    /// literal listed twice is repeated whatever its `deprecated` says.
    fn key(&self) -> (&'static str, &str) {
        match self {
            Self::Class { class } => ("class", class.as_str()),
            Self::Literal { name, .. } => ("literal", name.as_str()),
        }
    }

    fn order(&self, other: &Self) -> Ordering {
        self.key().cmp(&other.key())
    }
}

impl Correction {
    fn order(&self, other: &Self) -> Ordering {
        (
            self.extension.as_str(),
            self.key.as_str(),
            self.from.as_str(),
        )
            .cmp(&(
                other.extension.as_str(),
                other.key.as_str(),
                other.from.as_str(),
            ))
    }
}

impl ValidityField {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Language => "language",
            Self::Region => "region",
            Self::Script => "script",
            Self::Subdivision => "subdivision",
            Self::Variant => "variant",
        }
    }
}

impl ValidityStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Deprecated => "deprecated",
            Self::Macroregion => "macroregion",
            Self::PrivateUse => "private-use",
            Self::Regular => "regular",
            Self::Reserved => "reserved",
            Self::Special => "special",
            Self::Unknown => "unknown",
        }
    }
}

impl Extension {
    const fn as_str(self) -> &'static str {
        match self {
            Self::T => "t",
            Self::U => "u",
        }
    }
}

impl TypeClass {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Codepoints => "codepoints",
            Self::PrivateUse => "private-use",
            Self::ReorderCode => "reorder-code",
            Self::RgKeyValue => "rg-key-value",
            Self::ScriptCode => "script-code",
            Self::SubdivisionCode => "subdivision-code",
        }
    }
}

/// The counts the limits bound, taken from the decoded value. A member that is
/// missing or of another type counts nothing here and fails projection next.
struct Counts {
    validity_groups: u64,
    codes: u64,
    extension_keys: u64,
    types: u64,
    corrections: u64,
}

impl Counts {
    fn of(value: &Value) -> Self {
        let groups = items(value.get("validity"));
        let keys = items(value.get("extensionKeys"));
        Self {
            validity_groups: count(groups.len()),
            codes: total(groups, "codes"),
            extension_keys: count(keys.len()),
            types: total(keys, "types"),
            corrections: count(items(value.get("corrections")).len()),
        }
    }

    fn within(&self, limits: &ConformanceLimits) -> Result<(), ConformanceFailure> {
        let work = [
            self.validity_groups,
            self.codes,
            self.extension_keys,
            self.types,
            self.corrections,
        ]
        .into_iter()
        .fold(0_u64, u64::saturating_add);
        for (actual, limit, limited) in [
            (
                self.validity_groups,
                limits.validity_groups,
                Limited::ValidityGroups,
            ),
            (self.codes, limits.codes, Limited::Codes),
            (
                self.extension_keys,
                limits.extension_keys,
                Limited::ExtensionKeys,
            ),
            (self.types, limits.types, Limited::Types),
            (self.corrections, limits.corrections, Limited::Corrections),
            (work, limits.work, Limited::Work),
        ] {
            if actual > limit {
                return Err(ConformanceFailure::Limit(limited));
            }
        }
        Ok(())
    }
}

fn items(value: Option<&Value>) -> &[Value] {
    value.and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn count(length: usize) -> u64 {
    u64::try_from(length).unwrap_or(u64::MAX)
}

/// The total length of one array member across `entries`.
fn total(entries: &[Value], member: &str) -> u64 {
    entries
        .iter()
        .map(|entry| count(items(entry.get(member)).len()))
        .fold(0, u64::saturating_add)
}

#[cfg(test)]
pub(crate) mod tests;
