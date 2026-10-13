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

// The generator builds conformance data from CLDR's records. It establishes
// 017's order itself; `new` only checks it.
#[cfg(feature = "generate")]
impl LocaleConformanceData {
    /// Conformance data from parts already in 017's order.
    pub(crate) fn new(
        validity: Vec<ValidityGroup>,
        extension_keys: Vec<ExtensionKey>,
        corrections: Vec<Correction>,
    ) -> Result<Self, ConformanceFailure> {
        let data = Self {
            validity,
            extension_keys,
            corrections,
        };
        data.check()?;
        Ok(data)
    }
}

#[cfg(feature = "generate")]
impl ValidityGroup {
    pub(crate) const fn new(
        field: ValidityField,
        status: ValidityStatus,
        codes: Vec<NonemptyText>,
    ) -> Self {
        Self {
            field,
            status,
            codes,
        }
    }
}

#[cfg(feature = "generate")]
impl ExtensionKey {
    pub(crate) const fn new(
        extension: Extension,
        key: NonemptyText,
        deprecated: bool,
        value_type: ValueType,
        types: Vec<ExtensionType>,
    ) -> Self {
        Self {
            extension,
            key,
            deprecated,
            value_type,
            types,
        }
    }
}

#[cfg(feature = "generate")]
impl Correction {
    pub(crate) const fn new(
        extension: Extension,
        key: NonemptyText,
        from: NonemptyText,
        to: NonemptyText,
    ) -> Self {
        Self {
            extension,
            key,
            from,
            to,
        }
    }
}

// The conformance layer looks values up by binary search, which 017's
// canonical order allows: `read` and `new` both refuse data out of it.
impl LocaleConformanceData {
    /// The status of `code`, in its CLDR spelling and case, in the validity
    /// groups of `field`, if one of them lists it.
    pub(crate) fn status(&self, field: ValidityField, code: &str) -> Option<ValidityStatus> {
        let start = self
            .validity
            .partition_point(|group| group.field.as_str() < field.as_str());
        self.validity[start..]
            .iter()
            .take_while(|group| group.field == field)
            .find(|group| {
                group
                    .codes
                    .binary_search_by(|listed| listed.as_str().cmp(code))
                    .is_ok()
            })
            .map(|group| group.status)
    }

    /// The key `key` of `extension`, if the data lists it.
    pub(crate) fn key(&self, extension: Extension, key: &str) -> Option<&ExtensionKey> {
        self.extension_keys
            .binary_search_by(|listed| {
                (listed.extension.as_str(), listed.key.as_str()).cmp(&(extension.as_str(), key))
            })
            .ok()
            .map(|index| &self.extension_keys[index])
    }

    /// The value a correction of `key` of `extension` maps `from` to, if one
    /// does.
    pub(crate) fn correction(&self, extension: Extension, key: &str, from: &str) -> Option<&str> {
        self.corrections
            .binary_search_by(|listed| {
                (
                    listed.extension.as_str(),
                    listed.key.as_str(),
                    listed.from.as_str(),
                )
                    .cmp(&(extension.as_str(), key, from))
            })
            .ok()
            .map(|index| self.corrections[index].to.as_str())
    }
}

impl ExtensionKey {
    pub(crate) const fn deprecated(&self) -> bool {
        self.deprecated
    }

    pub(crate) const fn value_type(&self) -> ValueType {
        self.value_type
    }

    /// The literal type `name`, if the key lists it.
    pub(crate) fn literal(&self, name: &str) -> Option<&ExtensionType> {
        self.types
            .binary_search_by(|listed| listed.key().cmp(&("literal", name)))
            .ok()
            .map(|index| &self.types[index])
    }

    /// The classes the key lists.
    pub(crate) fn classes(&self) -> impl Iterator<Item = TypeClass> + '_ {
        self.types.iter().filter_map(|listed| match listed {
            ExtensionType::Class { class } => Some(*class),
            ExtensionType::Literal { .. } => None,
        })
    }
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
    pub(crate) const fn as_str(self) -> &'static str {
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
pub(crate) mod tests {
    use intlify_shared_json::decode::DecodeFailure;
    use intlify_shared_json::text::{self, TextFailure};
    use serde_json::{json, Value};

    use super::*;

    pub(crate) const VECTORS: &str = include_str!("../fixtures/dataset/vectors.json");

    pub(crate) fn vectors() -> Value {
        serde_json::from_str(VECTORS).unwrap()
    }

    /// The vectors' conformance value, in canonical order.
    pub(crate) fn sample() -> Value {
        vectors()["conformance"].clone()
    }

    pub(crate) const GENEROUS: ConformanceLimits = ConformanceLimits {
        validity_groups: u64::MAX,
        codes: u64::MAX,
        extension_keys: u64::MAX,
        types: u64::MAX,
        corrections: u64::MAX,
        work: u64::MAX,
    };

    fn read_within(
        value: &Value,
        limits: &ConformanceLimits,
    ) -> Result<LocaleConformanceData, ConformanceFailure> {
        LocaleConformanceData::read(text::write(value).unwrap().as_bytes(), limits)
    }

    fn read(value: &Value) -> Result<LocaleConformanceData, ConformanceFailure> {
        read_within(value, &GENEROUS)
    }

    /// The sample with the value at `pointer` replaced.
    fn edited(pointer: &str, value: Value) -> Value {
        let mut sample = sample();
        *sample.pointer_mut(pointer).unwrap() = value;
        sample
    }

    /// The sample with `member` added to, or removed from, the object at `pointer`.
    fn member(pointer: &str, member: &str, value: Option<Value>) -> Value {
        let mut sample = sample();
        let object = sample
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap();
        match value {
            Some(value) => object.insert(member.into(), value),
            None => object.remove(member),
        };
        sample
    }

    #[test]
    fn the_sample_reads_from_its_canonical_text_and_writes_it_back_exactly() {
        let text = text::write(&sample()).unwrap();
        assert_eq!(vectors()["conformanceText"], json!(text));
        let data = LocaleConformanceData::read(text.as_bytes(), &GENEROUS).unwrap();
        assert_eq!(data.to_text(), text);
        assert_eq!(serde_json::to_value(&data).unwrap(), sample());
    }

    #[test]
    fn every_variant_is_spelled_as_017_names_it() {
        let spelled = |value: Value, expected: &str| assert_eq!(value, json!(expected));
        for field in [
            ValidityField::Language,
            ValidityField::Region,
            ValidityField::Script,
            ValidityField::Subdivision,
            ValidityField::Variant,
        ] {
            spelled(serde_json::to_value(field).unwrap(), field.as_str());
        }
        for status in [
            ValidityStatus::Deprecated,
            ValidityStatus::Macroregion,
            ValidityStatus::PrivateUse,
            ValidityStatus::Regular,
            ValidityStatus::Reserved,
            ValidityStatus::Special,
            ValidityStatus::Unknown,
        ] {
            spelled(serde_json::to_value(status).unwrap(), status.as_str());
        }
        for extension in [Extension::T, Extension::U] {
            spelled(serde_json::to_value(extension).unwrap(), extension.as_str());
        }
        for class in [
            TypeClass::Codepoints,
            TypeClass::PrivateUse,
            TypeClass::ReorderCode,
            TypeClass::RgKeyValue,
            TypeClass::ScriptCode,
            TypeClass::SubdivisionCode,
        ] {
            spelled(serde_json::to_value(class).unwrap(), class.as_str());
        }
        assert_eq!(
            [
                ValueType::Any,
                ValueType::Incremental,
                ValueType::Multiple,
                ValueType::Single
            ]
            .map(|value_type| serde_json::to_value(value_type).unwrap()),
            [
                json!("any"),
                json!("incremental"),
                json!("multiple"),
                json!("single")
            ]
        );
    }

    #[test]
    fn only_the_canonical_text_of_the_value_is_read() {
        let canonical = text::write(&sample()).unwrap();
        let pretty = serde_json::to_string_pretty(&sample()).unwrap();
        // `serde_json` keeps the sample's member order, which is not byte order.
        let unsorted = serde_json::to_string(&sample()).unwrap();
        assert_ne!(unsorted, canonical);
        for text in [pretty, unsorted, format!("{canonical}\n")] {
            assert_eq!(
                LocaleConformanceData::read(text.as_bytes(), &GENEROUS),
                Err(ConformanceFailure::Text(TextFailure::NotCanonical))
            );
        }
        let mut marked = b"\xef\xbb\xbf".to_vec();
        marked.extend_from_slice(canonical.as_bytes());
        for (bytes, failure) in [
            (marked.as_slice(), DecodeFailure::Syntax),
            (
                br#"{"corrections":[],"extensionKeys":[],"validity":1}"#.as_slice(),
                DecodeFailure::Number,
            ),
        ] {
            assert_eq!(
                LocaleConformanceData::read(bytes, &GENEROUS),
                Err(ConformanceFailure::Text(TextFailure::Unreadable(failure)))
            );
        }
    }

    /// Values outside the closed structure, each with what makes it so. The
    /// committed schema refuses every one of them too.
    pub(crate) fn shapes() -> Vec<(&'static str, Value)> {
        vec![
            ("not an object", json!([])),
            ("unknown member", member("", "extra", Some(json!([])))),
            ("missing member", member("", "corrections", None)),
            (
                "group member",
                member("/validity/0", "note", Some(json!("x"))),
            ),
            ("group codes", member("/validity/0", "codes", None)),
            ("field", edited("/validity/0/field", json!("territory"))),
            (
                "CLDR spelling",
                edited("/validity/3/status", json!("private_use")),
            ),
            ("empty code", edited("/validity/0/codes/0", json!(""))),
            ("code type", edited("/validity/0/codes/0", json!(["en"]))),
            (
                "legacy key name",
                member("/extensionKeys/1", "alias", Some(json!("calendar"))),
            ),
            ("value type", member("/extensionKeys/0", "valueType", None)),
            (
                "unknown value type",
                edited("/extensionKeys/0/valueType", json!("several")),
            ),
            (
                "deprecated",
                edited("/extensionKeys/0/deprecated", json!("false")),
            ),
            (
                "extension",
                edited("/extensionKeys/0/extension", json!("x")),
            ),
            (
                "kind",
                edited("/extensionKeys/0/types/0/kind", json!("pattern")),
            ),
            (
                "class",
                edited("/extensionKeys/0/types/0/class", json!("region-code")),
            ),
            (
                "class member",
                member("/extensionKeys/0/types/0", "deprecated", Some(json!(false))),
            ),
            (
                "literal member",
                member("/extensionKeys/1/types/0", "deprecated", None),
            ),
            (
                "literal with a class",
                member(
                    "/extensionKeys/1/types/0",
                    "class",
                    Some(json!("codepoints")),
                ),
            ),
            (
                "empty literal",
                edited("/extensionKeys/1/types/0/type", json!("")),
            ),
            (
                "correction member",
                member("/corrections/0", "preferred", Some(json!("x"))),
            ),
            ("empty correction", edited("/corrections/0/to", json!(""))),
        ]
    }

    #[test]
    fn a_value_outside_the_closed_structure_is_refused_as_a_shape() {
        for (case, value) in shapes() {
            assert_eq!(read(&value), Err(ConformanceFailure::Shape), "{case}");
        }
    }

    #[test]
    fn a_collection_out_of_order_is_unsorted_and_a_repeat_is_repeated() {
        let swapped = |pointer: &str, a: usize, b: usize| {
            let mut sample = sample();
            sample
                .pointer_mut(pointer)
                .unwrap()
                .as_array_mut()
                .unwrap()
                .swap(a, b);
            sample
        };
        for (value, failure) in [
            (
                swapped("/validity", 0, 1),
                ConformanceFailure::Unsorted(Collection::ValidityGroups),
            ),
            // Statuses order by spelling: `macroregion` before `private-use`.
            (
                swapped("/validity", 2, 3),
                ConformanceFailure::Unsorted(Collection::ValidityGroups),
            ),
            (
                edited("/validity/1/status", json!("regular")),
                ConformanceFailure::Repeated(Collection::ValidityGroups),
            ),
            (
                swapped("/validity/0/codes", 0, 1),
                ConformanceFailure::Unsorted(Collection::Codes),
            ),
            (
                edited("/validity/0/codes/1", json!("en")),
                ConformanceFailure::Repeated(Collection::Codes),
            ),
            // Bytes, not case: `hant` is after `Latn`.
            (
                edited("/validity/8/codes", json!(["hant", "Latn"])),
                ConformanceFailure::Unsorted(Collection::Codes),
            ),
            (
                swapped("/extensionKeys", 0, 1),
                ConformanceFailure::Unsorted(Collection::ExtensionKeys),
            ),
            (
                edited("/extensionKeys/2/key", json!("ca")),
                ConformanceFailure::Repeated(Collection::ExtensionKeys),
            ),
            // A class comes before every literal.
            (
                swapped("/extensionKeys/2/types", 0, 1),
                ConformanceFailure::Unsorted(Collection::Types),
            ),
            (
                swapped("/extensionKeys/1/types", 1, 2),
                ConformanceFailure::Unsorted(Collection::Types),
            ),
            // A type listed twice is repeated whatever its deprecation says.
            (
                edited(
                    "/extensionKeys/1/types/1",
                    json!({"kind": "literal", "type": "ethioaa", "deprecated": true}),
                ),
                ConformanceFailure::Repeated(Collection::Types),
            ),
            (
                swapped("/corrections", 0, 1),
                ConformanceFailure::Unsorted(Collection::Corrections),
            ),
            (
                edited(
                    "/corrections/1",
                    json!({"extension": "u", "key": "ca", "from": "ethiopic-amete-alem", "to": "gregory"}),
                ),
                ConformanceFailure::Repeated(Collection::Corrections),
            ),
        ] {
            assert_eq!(read(&value), Err(failure), "{value}");
        }
        // Byte order puts digits before capitals and capitals before lowercase.
        assert!(read(&edited("/validity/8/codes", json!(["Latn", "hant"]))).is_ok());
        assert!(read(&edited("/validity/2/codes", json!(["001", "419", "EU"]))).is_ok());
    }

    #[test]
    fn a_code_may_be_in_one_group_of_its_field_only() {
        for value in [
            edited("/validity/1/codes", json!(["en", "qaa", "qfz"])),
            // The two groups of one field need not be next to each other.
            edited("/validity/6/codes", json!(["001", "ZZ"])),
        ] {
            assert_eq!(
                read(&value),
                Err(ConformanceFailure::CodeInTwoGroups),
                "{value}"
            );
        }
        // A code's field is part of what it is.
        assert!(read(&edited("/validity/8/codes", json!(["Hant", "Latn", "ZZ"]))).is_ok());
    }

    #[test]
    fn a_correction_maps_straight_to_a_value_nothing_corrects_further() {
        let corrections = |value: Value| edited("/corrections", value);
        let chained = corrections(json!([
            {"extension": "u", "key": "ca", "from": "ethioaa", "to": "ethiopic"},
            {"extension": "u", "key": "ca", "from": "ethiopic-amete-alem", "to": "ethioaa"},
        ]));
        let to_itself = corrections(json!([
            {"extension": "u", "key": "ca", "from": "islamicc", "to": "islamicc"},
        ]));
        for value in [chained, to_itself] {
            assert_eq!(
                read(&value),
                Err(ConformanceFailure::Unflattened),
                "{value}"
            );
        }
        // Another key's correction, or another extension's, is not a chain.
        let other_keys = corrections(json!([
            {"extension": "t", "key": "ca", "from": "islamic-civil", "to": "islamic"},
            {"extension": "u", "key": "ca", "from": "islamicc", "to": "islamic-civil"},
            {"extension": "u", "key": "kr", "from": "islamic-civil", "to": "digit"},
        ]));
        assert!(read(&other_keys).is_ok());
    }

    #[test]
    fn each_limit_admits_its_exact_count_and_refuses_the_next() {
        let exact = ConformanceLimits {
            validity_groups: 9,
            codes: 20,
            extension_keys: 4,
            types: 9,
            corrections: 2,
            work: 44,
        };
        assert!(read_within(&sample(), &exact).is_ok());
        for (limits, limited) in [
            (
                ConformanceLimits {
                    validity_groups: 8,
                    ..exact
                },
                Limited::ValidityGroups,
            ),
            (ConformanceLimits { codes: 19, ..exact }, Limited::Codes),
            (
                ConformanceLimits {
                    extension_keys: 3,
                    ..exact
                },
                Limited::ExtensionKeys,
            ),
            (ConformanceLimits { types: 8, ..exact }, Limited::Types),
            (
                ConformanceLimits {
                    corrections: 1,
                    ..exact
                },
                Limited::Corrections,
            ),
            (ConformanceLimits { work: 43, ..exact }, Limited::Work),
        ] {
            assert_eq!(
                read_within(&sample(), &limits),
                Err(ConformanceFailure::Limit(limited))
            );
        }
    }

    #[test]
    fn limits_are_checked_before_the_value_is_projected() {
        let none = ConformanceLimits {
            validity_groups: 0,
            codes: 0,
            extension_keys: 0,
            types: 0,
            corrections: 0,
            work: 0,
        };
        // Two groups that are not groups are still two groups to count.
        let malformed = json!({"corrections": [], "extensionKeys": [], "validity": ["x", "y"]});
        assert_eq!(
            read_within(
                &malformed,
                &ConformanceLimits {
                    validity_groups: 1,
                    ..GENEROUS
                }
            ),
            Err(ConformanceFailure::Limit(Limited::ValidityGroups))
        );
        // A member that is not a list counts nothing, and fails projection.
        for value in [
            json!({"corrections": "x", "extensionKeys": "x", "validity": "x"}),
            json!({"corrections": [], "extensionKeys": [{"types": "x"}], "validity": []}),
            json!({"corrections": [], "extensionKeys": [], "validity": [{"codes": {}}]}),
        ] {
            assert_eq!(
                read_within(
                    &value,
                    &ConformanceLimits {
                        extension_keys: 1,
                        validity_groups: 1,
                        work: 1,
                        ..none
                    }
                ),
                Err(ConformanceFailure::Shape),
                "{value}"
            );
        }
    }

    #[cfg(feature = "generate")]
    #[test]
    fn new_checks_the_parts_are_in_017s_order() {
        let group = |codes: &[&str]| {
            let codes = codes
                .iter()
                .map(|code| NonemptyText::from_validated(code).unwrap())
                .collect();
            ValidityGroup::new(ValidityField::Region, ValidityStatus::Regular, codes)
        };
        assert_eq!(
            LocaleConformanceData::new(vec![group(&["US", "AC"])], Vec::new(), Vec::new()),
            Err(ConformanceFailure::Unsorted(Collection::Codes))
        );
        let data = LocaleConformanceData::new(vec![group(&["AC", "US"])], Vec::new(), Vec::new());
        assert_eq!(
            data.map(|data| data.to_text()).as_deref(),
            Ok(
                r#"{"corrections":[],"extensionKeys":[],"validity":[{"codes":["AC","US"],"field":"region","status":"regular"}]}"#
            )
        );
    }
}
