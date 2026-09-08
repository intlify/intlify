// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015-owned structural authoring types, not a checked localization profile.
//!
//! Policy and target references are type parameters; the formal instantiation
//! uses 017's closed reference types, independently from synthetic test inputs.
//! No partial root is returned by deserialization, and semantic normalization
//! (including locale uniqueness, membership, and defaults) is deliberately later.

use std::borrow::Cow;
use std::collections::BTreeMap;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

pub(crate) const CONFIGURATION_SCHEMA_VERSION: &str = "0";
pub(crate) const ID_PATTERN: &str = "^[a-z0-9](?:[a-z0-9._-]*[a-z0-9])?$";

pub(crate) fn valid_identity(value: &str) -> bool {
    let bytes = value.as_bytes();
    let endpoint = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    bytes.first().is_some_and(|byte| endpoint(*byte))
        && bytes.last().is_some_and(|byte| endpoint(*byte))
        && bytes
            .iter()
            .all(|byte| endpoint(*byte) || matches!(byte, b'.' | b'_' | b'-'))
}

macro_rules! identity_type {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
        #[serde(transparent)]
        pub(crate) struct $name(String);

        impl $name {
            pub(crate) fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                if valid_identity(&value) {
                    Ok(Self(value))
                } else {
                    Err(de::Error::custom("invalid configuration identity syntax"))
                }
            }
        }

        impl JsonSchema for $name {
            fn schema_name() -> Cow<'static, str> {
                stringify!($name).into()
            }

            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                schemars::json_schema!({"type": "string", "pattern": ID_PATTERN})
            }
        }
    };
}

// Shared syntax does not make these semantic identity domains interchangeable.
identity_type!(ProfileId);
identity_type!(ProjectId);
identity_type!(SelectionScope);
identity_type!(TargetId);
identity_type!(DeploymentGroupId);

/// Missing and present are distinct; an explicit null still goes through T.
///
/// Unlike Option's Deserialize implementation, this does not admit null unless
/// T itself admits null. Fields opt into omission with serde(default).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) enum Presence<T> {
    #[default]
    Absent,
    Present(T),
}

impl<T> Presence<T> {
    pub(crate) const fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }

    pub(crate) const fn as_option(&self) -> Option<&T> {
        match self {
            Self::Absent => None,
            Self::Present(value) => Some(value),
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Presence<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self::Present)
    }
}

impl<T: Serialize> Serialize for Presence<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Absent => serializer.serialize_none(),
            Self::Present(value) => value.serialize(serializer),
        }
    }
}

impl<T: JsonSchema> JsonSchema for Presence<T> {
    fn schema_name() -> Cow<'static, str> {
        format!("Present_{}", T::schema_name()).into()
    }

    fn schema_id() -> Cow<'static, str> {
        format!("intlify_config::Presence<{}>", T::schema_id()).into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        generator.subschema_for::<T>()
    }
}

/// A required field that permits null. This is intentionally not Option<T>.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub(crate) enum RequiredNullable<T> {
    Reference(T),
    Null,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct NonEmptyVec<T>(Vec<T>);

impl<T> NonEmptyVec<T> {
    pub(crate) fn as_slice(&self) -> &[T] {
        &self.0
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for NonEmptyVec<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let values = Vec::<T>::deserialize(deserializer)?;
        if values.is_empty() {
            Err(de::Error::custom("expected a non-empty array"))
        } else {
            Ok(Self(values))
        }
    }
}

impl<T: JsonSchema> JsonSchema for NonEmptyVec<T> {
    fn schema_name() -> Cow<'static, str> {
        format!("NonEmptyArray_{}", T::schema_name()).into()
    }

    fn schema_id() -> Cow<'static, str> {
        format!("intlify_config::NonEmptyVec<{}>", T::schema_id()).into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let mut schema = Vec::<T>::json_schema(generator);
        schema.insert("minItems".into(), 1.into());
        schema
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct NonEmptyMap<K: Ord, V>(BTreeMap<K, V>);

impl<K: Ord, V> NonEmptyMap<K, V> {
    pub(crate) fn get(&self, key: &K) -> Option<&V> {
        self.0.get(key)
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &V> {
        self.0.values()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.0.iter()
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'de, K: Ord + Deserialize<'de>, V: Deserialize<'de>> Deserialize<'de> for NonEmptyMap<K, V> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let values = BTreeMap::<K, V>::deserialize(deserializer)?;
        if values.is_empty() {
            Err(de::Error::custom("expected a non-empty object"))
        } else {
            Ok(Self(values))
        }
    }
}

impl<K: Ord + JsonSchema, V: JsonSchema> JsonSchema for NonEmptyMap<K, V> {
    fn schema_name() -> Cow<'static, str> {
        format!("NonEmptyMap_{}_{}", K::schema_name(), V::schema_name()).into()
    }

    fn schema_id() -> Cow<'static, str> {
        format!(
            "intlify_config::NonEmptyMap<{},{}>",
            K::schema_id(),
            V::schema_id()
        )
        .into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let mut schema = BTreeMap::<K, V>::json_schema(generator);
        schema.insert("minProperties".into(), 1.into());
        schema
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) enum ConfigurationVersion {
    #[serde(rename = "0")]
    V0,
}

impl ConfigurationVersion {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::V0 => CONFIGURATION_SCHEMA_VERSION,
        }
    }
}

/// Constructible only with the structural stage's sealed complete-root proof.
/// It intentionally does not implement Deserialize or expose field mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct IntlifyConfig<Policy, Target>(AuthoringRoot<Policy, Target>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AuthoringRoot<Policy, Target> {
    #[serde(
        rename = "$schema",
        default,
        skip_serializing_if = "Presence::is_absent"
    )]
    schema: Presence<String>,
    schema_version: ConfigurationVersion,
    profiles: NonEmptyMap<ProfileId, ProfileDeclaration<Policy, Target>>,
}

impl<Policy, Target> IntlifyConfig<Policy, Target> {
    pub(crate) const fn schema(&self) -> &Presence<String> {
        &self.0.schema
    }
    pub(crate) const fn schema_version(&self) -> ConfigurationVersion {
        self.0.schema_version
    }
    pub(crate) fn profiles(&self) -> &NonEmptyMap<ProfileId, ProfileDeclaration<Policy, Target>> {
        &self.0.profiles
    }
}

impl<Policy: de::DeserializeOwned, Target: de::DeserializeOwned> IntlifyConfig<Policy, Target> {
    pub(crate) fn from_complete(
        proof: &crate::structural::CompleteRoot<'_, Policy, Target>,
    ) -> Result<Self, crate::materialize::DecodeError> {
        let doc = proof.document();
        doc.decode(doc.root()).map(Self)
    }
}

impl<Policy: JsonSchema, Target: JsonSchema> JsonSchema for IntlifyConfig<Policy, Target> {
    fn schema_name() -> Cow<'static, str> {
        "IntlifyConfig".into()
    }
    fn schema_id() -> Cow<'static, str> {
        format!(
            "intlify_config::IntlifyConfig<{},{}>",
            Policy::schema_id(),
            Target::schema_id()
        )
        .into()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        // Delegate the one field definition directly, keeping the generated
        // root object shape rather than introducing a transparent-wrapper $ref.
        AuthoringRoot::<Policy, Target>::json_schema(generator)
    }
}

#[cfg(test)]
pub(crate) fn test_root_shape_accepts<
    Policy: de::DeserializeOwned,
    Target: de::DeserializeOwned,
>(
    value: serde_json::Value,
) -> bool {
    serde_json::from_value::<AuthoringRoot<Policy, Target>>(value).is_ok()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProfileDeclaration<Policy, Target> {
    pub(crate) project_id: ProjectId,
    pub(crate) selection_scope: SelectionScope,
    pub(crate) requested_locales: NonEmptyVec<String>,
    pub(crate) default_requested_locale: String,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub(crate) default_source_locale: Presence<String>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub(crate) locale_negotiation: Presence<LocaleNegotiation>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub(crate) message_fallback: Presence<BTreeMap<String, NonEmptyVec<FallbackCandidate>>>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub(crate) coverage: Presence<Coverage>,
    pub(crate) policies: Policies<Policy>,
    pub(crate) target_profiles: NonEmptyMap<TargetId, TargetDeclaration<Target>>,
    pub(crate) deployment_groups: NonEmptyMap<DeploymentGroupId, DeploymentGroup>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub(crate) delivery: Presence<Delivery>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct LocaleNegotiation {
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub(crate) aliases: Presence<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub(crate) enum FallbackCandidate {
    Locale(String),
    IntentSource(IntentSourceCandidate),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct IntentSourceCandidate {
    kind: IntentSourceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) enum IntentSourceKind {
    #[serde(rename = "intent-source-locale")]
    IntentSourceLocale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Coverage {
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub(crate) default_mode: Presence<CoverageMode>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub(crate) rules: Presence<Vec<CoverageRule>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CoverageMode {
    DirectRequired,
    FallbackAllowed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct CoverageRule(CoverageRuleShape);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CoverageRuleShape {
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    requested_locales: Presence<NonEmptyVec<String>>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    intent_surface_classes: Presence<NonEmptyVec<String>>,
    mode: CoverageMode,
}

impl<'de> Deserialize<'de> for CoverageRule {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let rule = CoverageRuleShape::deserialize(deserializer)?;
        if rule.requested_locales.is_absent() && rule.intent_surface_classes.is_absent() {
            Err(de::Error::custom(
                "coverage rule requires at least one selector",
            ))
        } else {
            Ok(Self(rule))
        }
    }
}

impl JsonSchema for CoverageRule {
    fn schema_name() -> Cow<'static, str> {
        "CoverageRule".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let mut schema = CoverageRuleShape::json_schema(generator);
        schema.insert(
            "anyOf".into(),
            serde_json::json!([
                {"required": ["requestedLocales"]},
                {"required": ["intentSurfaceClasses"]}
            ]),
        );
        schema
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Policies<Policy> {
    pub(crate) resource_limits: Policy,
    pub(crate) trust: Policy,
    pub(crate) source_admission: Policy,
    pub(crate) approval: Policy,
    pub(crate) selection: Policy,
    pub(crate) provider_routing: RequiredNullable<Policy>,
    pub(crate) glossary_set: RequiredNullable<Policy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TargetDeclaration<Target> {
    pub(crate) profile: Target,
    pub(crate) requested_locales: NonEmptyVec<String>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub(crate) default_requested_locale: Presence<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DeploymentGroup {
    pub(crate) members: NonEmptyVec<TargetId>,
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub(crate) hydration_relations: Presence<Vec<HydrationRelation>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct HydrationRelation {
    pub(crate) server: TargetId,
    pub(crate) client: TargetId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Delivery {
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub(crate) placement: Presence<DeliveryPlacement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DeliveryPlacement {
    Duplicate,
}
