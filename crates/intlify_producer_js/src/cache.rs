// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Producer-owned semantic cache identity and opaque storage boundary.
//!
//! Cache keys use domain-separated, tagged, length-prefixed canonical framing.
//! Values repeat that exact frame and protect canonical artifact bytes with a
//! BLAKE3 digest before contract decoding. Storage failures and rejected values
//! remain optimization misses; this module never turns a cache into source
//! authority.

use std::error::Error;

use intlify_contract::{ArtifactVersion, DeliveryUnitId, ProducerIdentity, SourceDocumentIdentity};

use crate::{
    JsGrammarProfile, JsKeySyntax, JsPhysicalSourceGroup, JsRecognizerCallKind, JsRecognizerSet,
    JsSelectedSourceGoal, JsSourceGoal, JsSourceProfile,
};

const CACHE_SCHEMA_REVISION: u32 = 1;
const CACHE_KEY_DOMAIN: &[u8] = b"dev.intlify/js-reference/cache-key\0";
const CACHE_ENTRY_DOMAIN: &[u8] = b"dev.intlify/js-reference/cache-entry\0";

/// Storage-layer error intentionally excluded from producer semantic outcomes.
pub type JsProducerCacheIoError = Box<dyn Error + Send + Sync + 'static>;

/// Opaque producer cache storage used through immutable shared access.
///
/// Implementations may persist entries by [`JsProducerCacheKey::lookup_digest`].
/// The supplied value is a producer-owned opaque envelope and must be returned
/// byte-for-byte. Implementations use interior synchronization when shared
/// across caller-owned worker threads.
pub trait JsProducerCache: Send + Sync {
    /// Load one opaque entry, returning `None` when the physical key is absent.
    fn load(&self, key: &JsProducerCacheKey) -> Result<Option<Box<[u8]>>, JsProducerCacheIoError>;

    /// Store or replace one complete opaque entry.
    fn store(&self, key: &JsProducerCacheKey, value: &[u8]) -> Result<(), JsProducerCacheIoError>;
}

/// Canonical semantic cache key with a BLAKE3 physical lookup digest.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JsProducerCacheKey {
    frame: Box<[u8]>,
    lookup_digest: [u8; 32],
}

impl JsProducerCacheKey {
    /// Return the initial producer-owned cache schema revision.
    #[must_use]
    pub const fn schema_revision(&self) -> u32 {
        CACHE_SCHEMA_REVISION
    }

    /// Return the unkeyed BLAKE3-256 digest of the canonical semantic frame.
    #[must_use]
    pub const fn lookup_digest(&self) -> &[u8; 32] {
        &self.lookup_digest
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SourceFingerprint {
    length: u64,
    digest: [u8; 32],
}

impl SourceFingerprint {
    pub(crate) fn for_bytes(source_bytes: &[u8]) -> Self {
        Self {
            length: u64::try_from(source_bytes.len())
                .expect("an admitted JavaScript source length fits into u64"),
            digest: *blake3::hash(source_bytes).as_bytes(),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct CacheKeyContext<'a> {
    pub(crate) version: ArtifactVersion,
    pub(crate) producer: &'a ProducerIdentity,
    pub(crate) group: &'a JsPhysicalSourceGroup,
    pub(crate) profile: JsSourceProfile,
    pub(crate) selected_goal: JsSelectedSourceGoal,
    pub(crate) recognizers: &'a JsRecognizerSet,
    pub(crate) delivery_unit: &'a DeliveryUnitId,
    pub(crate) fingerprint: SourceFingerprint,
}

pub(crate) fn key_for_group(context: CacheKeyContext<'_>) -> JsProducerCacheKey {
    let mut frame = Vec::new();
    frame.extend_from_slice(CACHE_KEY_DOMAIN);
    write_field(0x01, &CACHE_SCHEMA_REVISION.to_be_bytes(), &mut frame);
    write_field(0x02, &encode_artifact_version(context.version), &mut frame);
    write_field(0x03, &encode_producer(context.producer), &mut frame);
    write_field(
        0x04,
        &encode_sequence(context.group.all_sources().map(encode_source)),
        &mut frame,
    );
    write_field(
        0x05,
        &[
            grammar_tag(context.profile.grammar()),
            source_goal_tag(context.profile.goal()),
            selected_goal_tag(context.selected_goal),
        ],
        &mut frame,
    );
    write_field(
        0x06,
        &encode_sequence(context.recognizers.bindings().iter().map(|binding| {
            let mut record = Vec::new();
            write_field(0x01, binding.callee().as_bytes(), &mut record);
            write_field(0x02, &[call_kind_tag(binding.kind())], &mut record);
            write_field(
                0x03,
                binding.scope().namespace().as_str().as_bytes(),
                &mut record,
            );
            write_field(
                0x04,
                binding.scope().name().as_str().as_bytes(),
                &mut record,
            );
            write_field(0x05, binding.domain().as_str().as_bytes(), &mut record);
            write_field(0x06, &[key_syntax_tag(binding.key_syntax())], &mut record);
            record
        })),
        &mut frame,
    );
    let mut source_fingerprint = Vec::with_capacity(40);
    source_fingerprint.extend_from_slice(&context.fingerprint.length.to_be_bytes());
    source_fingerprint.extend_from_slice(&context.fingerprint.digest);
    write_field(0x07, &source_fingerprint, &mut frame);
    write_field(
        0x08,
        &encode_sequence(
            context
                .delivery_unit
                .segments()
                .iter()
                .map(|segment| segment.as_str().as_bytes().to_vec()),
        ),
        &mut frame,
    );

    let lookup_digest = *blake3::hash(&frame).as_bytes();
    JsProducerCacheKey {
        frame: frame.into_boxed_slice(),
        lookup_digest,
    }
}

pub(crate) fn encode_cache_entry(key: &JsProducerCacheKey, artifact_bytes: &[u8]) -> Box<[u8]> {
    let mut output =
        Vec::with_capacity(CACHE_ENTRY_DOMAIN.len() + key.frame.len() + artifact_bytes.len() + 59);
    output.extend_from_slice(CACHE_ENTRY_DOMAIN);
    write_field(0x01, &key.frame, &mut output);
    write_field(0x02, artifact_bytes, &mut output);
    write_field(0x03, blake3::hash(artifact_bytes).as_bytes(), &mut output);
    output.into_boxed_slice()
}

pub(crate) fn decode_cache_entry<'a>(
    key: &JsProducerCacheKey,
    entry: &'a [u8],
) -> Option<&'a [u8]> {
    let mut cursor = CACHE_ENTRY_DOMAIN.len();
    if !entry.starts_with(CACHE_ENTRY_DOMAIN) {
        return None;
    }
    let framed_key = read_field(entry, &mut cursor, 0x01)?;
    if framed_key != key.frame.as_ref() {
        return None;
    }
    let artifact_bytes = read_field(entry, &mut cursor, 0x02)?;
    let artifact_digest = read_field(entry, &mut cursor, 0x03)?;
    if cursor != entry.len()
        || artifact_digest.len() != 32
        || artifact_digest != blake3::hash(artifact_bytes).as_bytes()
    {
        return None;
    }
    Some(artifact_bytes)
}

fn encode_artifact_version(version: ArtifactVersion) -> [u8; 4] {
    let mut output = [0_u8; 4];
    output[..2].copy_from_slice(&version.major().to_be_bytes());
    output[2..].copy_from_slice(&version.minor().to_be_bytes());
    output
}

fn encode_producer(producer: &ProducerIdentity) -> Vec<u8> {
    let mut output = Vec::new();
    write_field(0x01, producer.id().as_str().as_bytes(), &mut output);
    write_field(0x02, producer.revision().as_str().as_bytes(), &mut output);
    output
}

fn encode_source(source: &SourceDocumentIdentity) -> Vec<u8> {
    let mut output = Vec::new();
    write_field(0x01, source.namespace().as_str().as_bytes(), &mut output);
    write_field(
        0x02,
        &encode_sequence(
            source
                .path()
                .segments()
                .iter()
                .map(|segment| segment.as_str().as_bytes().to_vec()),
        ),
        &mut output,
    );
    output
}

fn encode_sequence(items: impl IntoIterator<Item = Vec<u8>>) -> Vec<u8> {
    let items = items.into_iter().collect::<Vec<_>>();
    let mut output = Vec::new();
    output.extend_from_slice(&(items.len() as u64).to_be_bytes());
    for item in items {
        output.extend_from_slice(&(item.len() as u64).to_be_bytes());
        output.extend_from_slice(&item);
    }
    output
}

fn write_field(tag: u8, value: &[u8], output: &mut Vec<u8>) {
    output.push(tag);
    output.extend_from_slice(&(value.len() as u64).to_be_bytes());
    output.extend_from_slice(value);
}

fn read_field<'a>(input: &'a [u8], cursor: &mut usize, expected_tag: u8) -> Option<&'a [u8]> {
    if *input.get(*cursor)? != expected_tag {
        return None;
    }
    *cursor = cursor.checked_add(1)?;
    let length_bytes: [u8; 8] = input
        .get(*cursor..cursor.checked_add(8)?)?
        .try_into()
        .ok()?;
    *cursor = cursor.checked_add(8)?;
    let length = usize::try_from(u64::from_be_bytes(length_bytes)).ok()?;
    let end = cursor.checked_add(length)?;
    let value = input.get(*cursor..end)?;
    *cursor = end;
    Some(value)
}

const fn grammar_tag(value: JsGrammarProfile) -> u8 {
    match value {
        JsGrammarProfile::JavaScript => 0,
        JsGrammarProfile::JavaScriptJsx => 1,
        JsGrammarProfile::TypeScript => 2,
        JsGrammarProfile::TypeScriptJsx => 3,
        JsGrammarProfile::TypeScriptDeclaration => 4,
    }
}

const fn source_goal_tag(value: JsSourceGoal) -> u8 {
    match value {
        JsSourceGoal::BoundedUnambiguous => 0,
        JsSourceGoal::Module => 1,
        JsSourceGoal::CommonJs => 2,
    }
}

const fn selected_goal_tag(value: JsSelectedSourceGoal) -> u8 {
    match value {
        JsSelectedSourceGoal::Module => 0,
        JsSelectedSourceGoal::Script => 1,
    }
}

const fn call_kind_tag(value: JsRecognizerCallKind) -> u8 {
    match value {
        JsRecognizerCallKind::Lookup => 0,
        JsRecognizerCallKind::Set => 1,
    }
}

const fn key_syntax_tag(value: JsKeySyntax) -> u8 {
    match value {
        JsKeySyntax::Canonical => 0,
        JsKeySyntax::Literal => 1,
        JsKeySyntax::DotPath => 2,
    }
}

#[cfg(test)]
mod tests {
    use intlify_contract::{
        ArtifactNamespace, ArtifactVersion, CatalogKeyDomain, CatalogScopeId, CatalogScopeName,
        DeliveryUnitId, DeliveryUnitSegment, PortablePathSegment, PortableRelativePath, ProducerId,
        ProducerIdentity, ProducerRevision, SourceDocumentIdentity,
    };

    use super::{
        decode_cache_entry, encode_cache_entry, key_for_group, SourceFingerprint,
        CACHE_SCHEMA_REVISION,
    };
    use crate::{
        JsKeySyntax, JsPhysicalSourceGroup, JsRecognizerBinding, JsRecognizerCallKind,
        JsRecognizerSet, JsSelectedSourceGoal, JsSourceProfile,
    };

    fn source(name: &str) -> SourceDocumentIdentity {
        SourceDocumentIdentity::new(
            ArtifactNamespace::Project,
            PortableRelativePath::try_new(vec![
                PortablePathSegment::try_new("src").unwrap(),
                PortablePathSegment::try_new(name).unwrap(),
            ])
            .unwrap(),
        )
    }

    fn producer(revision: &str) -> ProducerIdentity {
        ProducerIdentity::new(
            ProducerId::try_new("dev.intlify/js-reference").unwrap(),
            ProducerRevision::try_new(revision).unwrap(),
        )
    }

    fn recognizers(callee: &str) -> JsRecognizerSet {
        JsRecognizerSet::try_new(vec![JsRecognizerBinding::try_new(
            callee,
            JsRecognizerCallKind::Lookup,
            CatalogScopeId::new(
                ArtifactNamespace::Project,
                CatalogScopeName::try_new("app").unwrap(),
            ),
            CatalogKeyDomain::JsonPointer,
            JsKeySyntax::Literal,
        )
        .unwrap()])
        .unwrap()
    }

    fn cache_key(
        bytes: &[u8],
        revision: &str,
        version: ArtifactVersion,
        callee: &str,
    ) -> super::JsProducerCacheKey {
        cache_key_with_profile_goal_and_delivery(
            bytes,
            revision,
            version,
            callee,
            JsSourceProfile::from_filename("app.ts").unwrap(),
            JsSelectedSourceGoal::Module,
            &DeliveryUnitId::main(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn cache_key_with_profile_goal_and_delivery(
        bytes: &[u8],
        revision: &str,
        version: ArtifactVersion,
        callee: &str,
        profile: JsSourceProfile,
        selected_goal: JsSelectedSourceGoal,
        delivery_unit: &DeliveryUnitId,
    ) -> super::JsProducerCacheKey {
        let group = JsPhysicalSourceGroup::try_new(vec![source("app.ts")], bytes.to_vec()).unwrap();
        key_for_group(super::CacheKeyContext {
            version,
            producer: &producer(revision),
            group: &group,
            profile,
            selected_goal,
            recognizers: &recognizers(callee),
            delivery_unit,
            fingerprint: SourceFingerprint::for_bytes(bytes),
        })
    }

    #[test]
    fn every_output_affecting_component_changes_cache_identity() {
        let base = cache_key(b"t('a')", "1.0.0+src.aaa", ArtifactVersion::DRAFT_V0_1, "t");
        assert_eq!(base.schema_revision(), CACHE_SCHEMA_REVISION);
        assert_eq!(
            base,
            cache_key(b"t('a')", "1.0.0+src.aaa", ArtifactVersion::DRAFT_V0_1, "t")
        );
        assert_ne!(
            base,
            cache_key(b"t('b')", "1.0.0+src.aaa", ArtifactVersion::DRAFT_V0_1, "t")
        );
        assert_ne!(
            base,
            cache_key(b"t('a')", "1.0.0+src.bbb", ArtifactVersion::DRAFT_V0_1, "t")
        );
        assert_ne!(
            base,
            cache_key(b"t('a')", "1.0.0+src.aaa", ArtifactVersion::new(0, 2), "t")
        );
        assert_ne!(
            base,
            cache_key(
                b"t('a')",
                "1.0.0+src.aaa",
                ArtifactVersion::DRAFT_V0_1,
                "translate"
            )
        );

        let alias_group = JsPhysicalSourceGroup::try_new(
            vec![source("app.ts"), source("linked.ts")],
            b"t('a')".to_vec(),
        )
        .unwrap();
        let alias_key = key_for_group(super::CacheKeyContext {
            version: ArtifactVersion::DRAFT_V0_1,
            producer: &producer("1.0.0+src.aaa"),
            group: &alias_group,
            profile: JsSourceProfile::from_source(alias_group.primary()).unwrap(),
            selected_goal: JsSelectedSourceGoal::Module,
            recognizers: &recognizers("t"),
            delivery_unit: &DeliveryUnitId::main(),
            fingerprint: SourceFingerprint::for_bytes(b"t('a')"),
        });
        assert_ne!(base, alias_key);

        let javascript_profile = JsSourceProfile::from_filename("app.js").unwrap();
        assert_ne!(
            base,
            cache_key_with_profile_goal_and_delivery(
                b"t('a')",
                "1.0.0+src.aaa",
                ArtifactVersion::DRAFT_V0_1,
                "t",
                javascript_profile,
                JsSelectedSourceGoal::Module,
                &DeliveryUnitId::main(),
            )
        );

        let fixed_module_profile = JsSourceProfile::from_filename("app.mts").unwrap();
        assert_ne!(
            base,
            cache_key_with_profile_goal_and_delivery(
                b"t('a')",
                "1.0.0+src.aaa",
                ArtifactVersion::DRAFT_V0_1,
                "t",
                fixed_module_profile,
                JsSelectedSourceGoal::Module,
                &DeliveryUnitId::main(),
            )
        );
        assert_ne!(
            base,
            cache_key_with_profile_goal_and_delivery(
                b"t('a')",
                "1.0.0+src.aaa",
                ArtifactVersion::DRAFT_V0_1,
                "t",
                JsSourceProfile::from_filename("app.ts").unwrap(),
                JsSelectedSourceGoal::Script,
                &DeliveryUnitId::main(),
            )
        );

        let secondary_delivery_unit =
            DeliveryUnitId::try_new(vec![DeliveryUnitSegment::try_new("secondary").unwrap()])
                .unwrap();
        assert_ne!(
            base,
            cache_key_with_profile_goal_and_delivery(
                b"t('a')",
                "1.0.0+src.aaa",
                ArtifactVersion::DRAFT_V0_1,
                "t",
                JsSourceProfile::from_filename("app.ts").unwrap(),
                JsSelectedSourceGoal::Module,
                &secondary_delivery_unit,
            )
        );
    }

    #[test]
    fn entry_rejects_corruption_key_mismatch_and_trailing_bytes() {
        let key = cache_key(b"t('a')", "1.0.0+src.aaa", ArtifactVersion::DRAFT_V0_1, "t");
        let other = cache_key(b"t('b')", "1.0.0+src.aaa", ArtifactVersion::DRAFT_V0_1, "t");
        let entry = encode_cache_entry(&key, b"artifact");
        assert_eq!(
            decode_cache_entry(&key, &entry),
            Some(b"artifact".as_slice())
        );
        assert_eq!(decode_cache_entry(&other, &entry), None);

        let mut corrupted = entry.to_vec();
        let last = corrupted.last_mut().unwrap();
        *last ^= 1;
        assert_eq!(decode_cache_entry(&key, &corrupted), None);

        let mut trailing = entry.to_vec();
        trailing.push(0);
        assert_eq!(decode_cache_entry(&key, &trailing), None);
    }
}
