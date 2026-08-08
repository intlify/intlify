// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Strict ownership-manifest decoding and canonical encoding.
//!
//! The manifest is an integration-owned wire contract rather than a serde
//! snapshot of Rust values. This module rejects duplicate and unknown members,
//! reconstructs common artifact types, enforces the independent wire ceiling,
//! and keeps payload and manifest fingerprints in separate typed values.

use std::fmt;

use intlify_export::{
    ExportArtifact, ExportArtifactFormatVersion, ExportArtifactKind, ExportArtifactMetadata,
    ExportArtifactPath, ExportArtifactPayload, ExportArtifactRelationship,
    ExportArtifactRelationshipKind, ExportArtifactSet, ExportError, ExportErrorKind,
    ExportMediaType, OutputLimitCounter,
};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::Value;

use super::super::delivery::BuiltInExporterId;

pub(super) const OUTPUT_MANIFEST_BASENAME: &str = ".intlify-output-manifest.json";
pub(super) const OUTPUT_MANIFEST_WIRE_LIMIT: usize = 16_777_216;

const MANIFEST_SCHEMA_MAJOR: u16 = 0;
const MANIFEST_SCHEMA_MINOR: u16 = 1;
pub(super) const BLAKE3_ALGORITHM: &str = "blake3-256";
pub(super) const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";
pub(super) const ESM_MODULE_KIND: &str = "dev.intlify/esm-module";
pub(super) const ESM_LOADER_KIND: &str = "dev.intlify/loader-map";
pub(super) const ESM_ACCESSOR_KIND: &str = "dev.intlify/typescript-accessor";
const ESM_MODULE_LIMIT: usize = 1_024;
const ESM_LOADER_LIMIT: usize = 1;
const ESM_ACCESSOR_LIMIT: usize = 4_096;
const ESM_ARTIFACT_LIMIT: usize = ESM_MODULE_LIMIT + ESM_LOADER_LIMIT + ESM_ACCESSOR_LIMIT;
const ESM_LOADER_RELATIONSHIP_LIMIT: usize = 1_024;

/// Checked semantic manifest together with its exact canonical wire bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CheckedOutputManifest {
    manifest: OutputManifest,
    canonical_bytes: Box<[u8]>,
    canonical_fingerprint: Blake3Fingerprint,
}

impl CheckedOutputManifest {
    /// Construct the expected manifest from one complete checked artifact set.
    pub(super) fn from_artifact_set(
        exporter: BuiltInExporterId,
        artifacts: &ExportArtifactSet,
    ) -> Result<Self, ManifestCodecError> {
        let mut records = Vec::with_capacity(artifacts.artifacts().len());
        for artifact in artifacts.artifacts() {
            let payload_bytes = u64::try_from(artifact.payload().as_bytes().len())
                .map_err(|_| ManifestCodecError::limit())?;
            records.push(ManifestArtifact {
                path: artifact.logical_path().clone(),
                kind: artifact.kind().clone(),
                format_version: artifact.format_version(),
                media_type: artifact.metadata().media_type().cloned(),
                relationships: artifact
                    .metadata()
                    .relationships()
                    .to_vec()
                    .into_boxed_slice(),
                payload_bytes,
                payload_fingerprint: Blake3Fingerprint::of(artifact.payload().as_bytes()),
            });
        }
        Self::finish(OutputManifest {
            exporter,
            artifacts: records.into_boxed_slice(),
        })
    }

    /// Strictly decode one complete supported manifest wire document.
    pub(super) fn decode(
        input: &[u8],
        expected_exporter: BuiltInExporterId,
    ) -> Result<Self, ManifestCodecError> {
        if input.len() > OUTPUT_MANIFEST_WIRE_LIMIT {
            return Err(ManifestCodecError::limit());
        }
        if input.starts_with(&[0xef, 0xbb, 0xbf]) {
            return Err(ManifestCodecError::invalid());
        }
        let source = std::str::from_utf8(input).map_err(|_| ManifestCodecError::invalid())?;
        let value = parse_unique_json(source)?;
        let manifest = decode_manifest(&value, expected_exporter)?;
        Self::finish(manifest)
    }

    fn finish(manifest: OutputManifest) -> Result<Self, ManifestCodecError> {
        validate_exporter_route_limits(&manifest)?;
        let canonical_bytes = encode_manifest(&manifest)?;
        let canonical_fingerprint = Blake3Fingerprint::of(&canonical_bytes);
        Ok(Self {
            manifest,
            canonical_bytes,
            canonical_fingerprint,
        })
    }

    /// Return the checked semantic manifest.
    pub(super) const fn manifest(&self) -> &OutputManifest {
        &self.manifest
    }

    /// Return canonical UTF-8 bytes including the one final LF.
    pub(super) const fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }

    /// Return the fingerprint of the complete canonical manifest bytes.
    pub(super) const fn canonical_fingerprint(&self) -> Blake3Fingerprint {
        self.canonical_fingerprint
    }
}

/// Checked semantic ownership manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OutputManifest {
    exporter: BuiltInExporterId,
    artifacts: Box<[ManifestArtifact]>,
}

impl OutputManifest {
    pub(super) const fn exporter(&self) -> BuiltInExporterId {
        self.exporter
    }

    pub(super) const fn artifacts(&self) -> &[ManifestArtifact] {
        &self.artifacts
    }

    pub(super) fn artifact(&self, path: &ExportArtifactPath) -> Option<&ManifestArtifact> {
        self.artifacts
            .binary_search_by(|artifact| artifact.path.cmp(path))
            .ok()
            .map(|index| &self.artifacts[index])
    }
}

/// One canonical manifest artifact record without embedded payload bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ManifestArtifact {
    path: ExportArtifactPath,
    kind: ExportArtifactKind,
    format_version: ExportArtifactFormatVersion,
    media_type: Option<ExportMediaType>,
    relationships: Box<[ExportArtifactRelationship]>,
    payload_bytes: u64,
    payload_fingerprint: Blake3Fingerprint,
}

impl ManifestArtifact {
    pub(super) const fn path(&self) -> &ExportArtifactPath {
        &self.path
    }

    pub(super) const fn kind(&self) -> &ExportArtifactKind {
        &self.kind
    }

    pub(super) const fn format_version(&self) -> ExportArtifactFormatVersion {
        self.format_version
    }

    pub(super) const fn media_type(&self) -> Option<&ExportMediaType> {
        self.media_type.as_ref()
    }

    pub(super) const fn relationships(&self) -> &[ExportArtifactRelationship] {
        &self.relationships
    }

    pub(super) const fn payload_bytes(&self) -> u64 {
        self.payload_bytes
    }

    pub(super) const fn payload_fingerprint(&self) -> Blake3Fingerprint {
        self.payload_fingerprint
    }
}

/// Standard unkeyed BLAKE3-256 value retained as raw digest bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct Blake3Fingerprint([u8; 32]);

impl Blake3Fingerprint {
    pub(super) fn of(bytes: &[u8]) -> Self {
        Self(*blake3::hash(bytes).as_bytes())
    }

    pub(super) fn from_lower_hex(value: &str) -> Option<Self> {
        if value.len() != 64 {
            return None;
        }
        let mut digest = [0_u8; 32];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            let high = lower_hex_value(pair[0])?;
            let low = lower_hex_value(pair[1])?;
            digest[index] = (high << 4) | low;
        }
        Some(Self(digest))
    }

    pub(super) const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

fn lower_hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// Failure category from the strict manifest contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ManifestCodecErrorKind {
    Invalid,
    Unsupported,
    Limit,
    InternalInvariant,
}

/// Closed manifest failure without raw parser or dependency evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ManifestCodecError {
    kind: ManifestCodecErrorKind,
}

impl ManifestCodecError {
    const fn invalid() -> Self {
        Self {
            kind: ManifestCodecErrorKind::Invalid,
        }
    }

    const fn unsupported() -> Self {
        Self {
            kind: ManifestCodecErrorKind::Unsupported,
        }
    }

    const fn limit() -> Self {
        Self {
            kind: ManifestCodecErrorKind::Limit,
        }
    }

    const fn internal_invariant() -> Self {
        Self {
            kind: ManifestCodecErrorKind::InternalInvariant,
        }
    }

    pub(super) const fn kind(self) -> ManifestCodecErrorKind {
        self.kind
    }
}

fn decode_manifest(
    value: &Value,
    expected_exporter: BuiltInExporterId,
) -> Result<OutputManifest, ManifestCodecError> {
    let root = exact_object(value, &["schemaVersion", "exporter", "artifacts"])?;
    let schema_version = decode_version(root[0])?;
    if schema_version
        != ExportArtifactFormatVersion::new(MANIFEST_SCHEMA_MAJOR, MANIFEST_SCHEMA_MINOR)
    {
        return Err(ManifestCodecError::unsupported());
    }

    let exporter = root[1].as_str().ok_or_else(ManifestCodecError::invalid)?;
    if exporter != expected_exporter.as_str() {
        return Err(ManifestCodecError::unsupported());
    }

    let submitted = root[2].as_array().ok_or_else(ManifestCodecError::invalid)?;
    if usize_to_u64(submitted.len()) > OutputLimitCounter::ArtifactRecordsPerSet.ceiling() {
        return Err(ManifestCodecError::limit());
    }
    if submitted.len() > ESM_ARTIFACT_LIMIT {
        return Err(ManifestCodecError::limit());
    }

    let mut records = Vec::with_capacity(submitted.len());
    let mut skeleton = Vec::with_capacity(submitted.len());
    for value in submitted {
        let (record, artifact) = decode_artifact(value)?;
        records.push(record);
        skeleton.push(artifact);
    }

    let checked = ExportArtifactSet::try_new(skeleton).map_err(|error| map_export_error(&error))?;
    if !same_canonical_skeleton(&records, checked.artifacts()) {
        return Err(ManifestCodecError::invalid());
    }
    validate_payload_limits(&records)?;

    Ok(OutputManifest {
        exporter: expected_exporter,
        artifacts: records.into_boxed_slice(),
    })
}

fn decode_artifact(
    value: &Value,
) -> Result<(ManifestArtifact, ExportArtifact), ManifestCodecError> {
    let fields = exact_object(
        value,
        &["path", "kind", "formatVersion", "metadata", "payload"],
    )?;
    let path = decode_path(fields[0])?;
    let kind =
        ExportArtifactKind::try_new(fields[1].as_str().ok_or_else(ManifestCodecError::invalid)?)
            .map_err(|error| map_export_error(&error))?;
    let format_version = decode_version(fields[2])?;
    let (media_type, relationships) = decode_metadata(fields[3])?;
    let (payload_bytes, payload_fingerprint) = decode_payload(fields[4])?;

    let metadata = ExportArtifactMetadata::try_new(media_type.clone(), relationships.clone())
        .map_err(|error| map_export_error(&error))?;
    let skeleton = ExportArtifact::new(
        path.clone(),
        kind.clone(),
        format_version,
        ExportArtifactPayload::try_new(Box::<[u8]>::default())
            .map_err(|error| map_export_error(&error))?,
        metadata,
    );
    Ok((
        ManifestArtifact {
            path,
            kind,
            format_version,
            media_type,
            relationships: relationships.into_boxed_slice(),
            payload_bytes,
            payload_fingerprint,
        },
        skeleton,
    ))
}

fn decode_metadata(
    value: &Value,
) -> Result<(Option<ExportMediaType>, Vec<ExportArtifactRelationship>), ManifestCodecError> {
    let fields = exact_object(value, &["mediaType", "relationships"])?;
    let media_type = match fields[0] {
        Value::Null => None,
        Value::String(value) => Some(
            ExportMediaType::try_new(value.as_str()).map_err(|error| map_export_error(&error))?,
        ),
        _ => return Err(ManifestCodecError::invalid()),
    };
    let values = fields[1]
        .as_array()
        .ok_or_else(ManifestCodecError::invalid)?;
    if usize_to_u64(values.len()) > OutputLimitCounter::RelationshipRecordsPerArtifact.ceiling() {
        return Err(ManifestCodecError::limit());
    }
    let mut relationships = Vec::with_capacity(values.len());
    for value in values {
        let fields = exact_object(value, &["kind", "target"])?;
        let kind = match fields[0].as_str().ok_or_else(ManifestCodecError::invalid)? {
            "eager_load" => ExportArtifactRelationshipKind::EagerLoad,
            "lazy_load" => ExportArtifactRelationshipKind::LazyLoad,
            _ => return Err(ManifestCodecError::invalid()),
        };
        relationships.push(ExportArtifactRelationship::new(
            kind,
            decode_path(fields[1])?,
        ));
    }
    Ok((media_type, relationships))
}

fn decode_payload(value: &Value) -> Result<(u64, Blake3Fingerprint), ManifestCodecError> {
    let fields = exact_object(value, &["bytes", "fingerprint"])?;
    let bytes = plain_u64(fields[0])?;
    let fingerprint = exact_object(fields[1], &["algorithm", "digest"])?;
    let algorithm = fingerprint[0]
        .as_str()
        .ok_or_else(ManifestCodecError::invalid)?;
    if algorithm != BLAKE3_ALGORITHM {
        return Err(ManifestCodecError::unsupported());
    }
    let digest = fingerprint[1]
        .as_str()
        .ok_or_else(ManifestCodecError::invalid)?;
    let digest =
        Blake3Fingerprint::from_lower_hex(digest).ok_or_else(ManifestCodecError::invalid)?;
    Ok((bytes, digest))
}

fn decode_version(value: &Value) -> Result<ExportArtifactFormatVersion, ManifestCodecError> {
    let fields = exact_object(value, &["major", "minor"])?;
    Ok(ExportArtifactFormatVersion::new(
        plain_u16(fields[0])?,
        plain_u16(fields[1])?,
    ))
}

fn decode_path(value: &Value) -> Result<ExportArtifactPath, ManifestCodecError> {
    let values = value.as_array().ok_or_else(ManifestCodecError::invalid)?;
    ExportArtifactPath::try_new(
        values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(Box::<str>::from)
                    .ok_or_else(ManifestCodecError::invalid)
            })
            .collect::<Result<Vec<_>, _>>()?,
    )
    .map_err(|error| map_export_error(&error))
}

fn same_canonical_skeleton(records: &[ManifestArtifact], checked: &[ExportArtifact]) -> bool {
    records.len() == checked.len()
        && records.iter().zip(checked).all(|(record, artifact)| {
            record.path == *artifact.logical_path()
                && record.kind == *artifact.kind()
                && record.format_version == artifact.format_version()
                && record.media_type.as_ref() == artifact.metadata().media_type()
                && record.relationships.as_ref() == artifact.metadata().relationships()
        })
}

fn validate_payload_limits(records: &[ManifestArtifact]) -> Result<(), ManifestCodecError> {
    if records
        .iter()
        .any(|record| record.payload_bytes > OutputLimitCounter::PayloadBytesPerArtifact.ceiling())
    {
        return Err(ManifestCodecError::limit());
    }
    let mut total = 0_u64;
    for record in records {
        total = total
            .checked_add(record.payload_bytes)
            .ok_or_else(ManifestCodecError::limit)?;
    }
    if total > OutputLimitCounter::PayloadBytesPerSet.ceiling() {
        return Err(ManifestCodecError::limit());
    }
    Ok(())
}

fn validate_exporter_route_limits(manifest: &OutputManifest) -> Result<(), ManifestCodecError> {
    match manifest.exporter() {
        BuiltInExporterId::Esm => validate_esm_route_limits(manifest.artifacts()),
    }
}

fn validate_esm_route_limits(records: &[ManifestArtifact]) -> Result<(), ManifestCodecError> {
    if records.len() > ESM_ARTIFACT_LIMIT {
        return Err(ManifestCodecError::limit());
    }
    let mut modules = 0_usize;
    let mut loaders = 0_usize;
    let mut accessors = 0_usize;
    for record in records {
        match record.kind().as_str() {
            ESM_MODULE_KIND => modules = modules.saturating_add(1),
            ESM_LOADER_KIND => {
                loaders = loaders.saturating_add(1);
                if record.relationships().len() > ESM_LOADER_RELATIONSHIP_LIMIT {
                    return Err(ManifestCodecError::limit());
                }
            }
            ESM_ACCESSOR_KIND => accessors = accessors.saturating_add(1),
            _ => {}
        }
    }
    if modules > ESM_MODULE_LIMIT || loaders > ESM_LOADER_LIMIT || accessors > ESM_ACCESSOR_LIMIT {
        return Err(ManifestCodecError::limit());
    }
    Ok(())
}

fn map_export_error(error: &ExportError) -> ManifestCodecError {
    match error.kind() {
        ExportErrorKind::OutputLimitExceeded => ManifestCodecError::limit(),
        ExportErrorKind::InternalInvariant => ManifestCodecError::internal_invariant(),
        ExportErrorKind::UnsupportedBatch
        | ExportErrorKind::GenerationFailed
        | ExportErrorKind::InvalidOutput => ManifestCodecError::invalid(),
    }
}

pub(super) fn exact_object<'a>(
    value: &'a Value,
    names: &[&str],
) -> Result<Vec<&'a Value>, ManifestCodecError> {
    let object = value.as_object().ok_or_else(ManifestCodecError::invalid)?;
    if object.len() != names.len() || object.keys().any(|key| !names.contains(&key.as_str())) {
        return Err(ManifestCodecError::invalid());
    }
    names
        .iter()
        .map(|name| object.get(*name).ok_or_else(ManifestCodecError::invalid))
        .collect()
}

pub(super) fn plain_u16(value: &Value) -> Result<u16, ManifestCodecError> {
    let value = plain_u64(value)?;
    u16::try_from(value).map_err(|_| ManifestCodecError::invalid())
}

pub(super) fn plain_u64(value: &Value) -> Result<u64, ManifestCodecError> {
    let number = value.as_number().ok_or_else(ManifestCodecError::invalid)?;
    let spelling = number.to_string();
    if spelling.is_empty()
        || !spelling.bytes().all(|byte| byte.is_ascii_digit())
        || (spelling.len() > 1 && spelling.starts_with('0'))
    {
        return Err(ManifestCodecError::invalid());
    }
    spelling
        .parse::<u64>()
        .map_err(|_| ManifestCodecError::invalid())
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

pub(super) fn parse_unique_json(source: &str) -> Result<Value, ManifestCodecError> {
    let mut deserializer = serde_json::Deserializer::from_str(source);
    UniqueJsonValueSeed
        .deserialize(&mut deserializer)
        .and_then(|value| {
            deserializer.end()?;
            Ok(value)
        })
        .map_err(|_| ManifestCodecError::invalid())
}

#[derive(Clone, Copy)]
struct UniqueJsonValueSeed;

struct UniqueJsonValueVisitor;

impl<'de> DeserializeSeed<'de> for UniqueJsonValueSeed {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(UniqueJsonValueVisitor)
    }
}

impl<'de> Visitor<'de> for UniqueJsonValueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value without duplicate object members")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| de::Error::custom("non-finite JSON number"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(Value::String(value))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(UniqueJsonValueSeed)? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A>(self, mut object: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = serde_json::Map::new();
        while let Some(key) = object.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(de::Error::custom("duplicate object member"));
            }
            let value = object.next_value_seed(UniqueJsonValueSeed)?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

fn encode_manifest(manifest: &OutputManifest) -> Result<Box<[u8]>, ManifestCodecError> {
    let mut writer = CanonicalJsonWriter::new();
    writer.bytes(b"{\n")?;
    writer.member_name(1, "schemaVersion")?;
    writer.bytes(b"{\n")?;
    writer.member_u64(2, "major", u64::from(MANIFEST_SCHEMA_MAJOR), true)?;
    writer.member_u64(2, "minor", u64::from(MANIFEST_SCHEMA_MINOR), false)?;
    writer.indent(1)?;
    writer.bytes(b"},\n")?;
    writer.member_name(1, "exporter")?;
    writer.string(manifest.exporter.as_str())?;
    writer.bytes(b",\n")?;
    writer.member_name(1, "artifacts")?;
    writer.artifacts(&manifest.artifacts)?;
    writer.bytes(b"\n}\n")?;
    Ok(writer.finish())
}

pub(super) struct CanonicalJsonWriter {
    bytes: Vec<u8>,
    limit: usize,
}

impl CanonicalJsonWriter {
    fn new() -> Self {
        Self::with_limit(OUTPUT_MANIFEST_WIRE_LIMIT)
    }

    pub(super) fn with_limit(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }

    pub(super) fn finish(self) -> Box<[u8]> {
        self.bytes.into_boxed_slice()
    }

    pub(super) fn bytes(&mut self, bytes: &[u8]) -> Result<(), ManifestCodecError> {
        let next = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or_else(ManifestCodecError::limit)?;
        if next > self.limit {
            return Err(ManifestCodecError::limit());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    pub(super) fn indent(&mut self, level: usize) -> Result<(), ManifestCodecError> {
        for _ in 0..level {
            self.bytes(b"  ")?;
        }
        Ok(())
    }

    pub(super) fn member_name(
        &mut self,
        level: usize,
        name: &str,
    ) -> Result<(), ManifestCodecError> {
        self.indent(level)?;
        self.string(name)?;
        self.bytes(b": ")
    }

    pub(super) fn member_u64(
        &mut self,
        level: usize,
        name: &str,
        value: u64,
        comma: bool,
    ) -> Result<(), ManifestCodecError> {
        self.member_name(level, name)?;
        self.bytes(value.to_string().as_bytes())?;
        self.bytes(if comma { b",\n" } else { b"\n" })
    }

    pub(super) fn string(&mut self, value: &str) -> Result<(), ManifestCodecError> {
        self.bytes(b"\"")?;
        for character in value.chars() {
            match character {
                '"' => self.bytes(b"\\\"")?,
                '\\' => self.bytes(b"\\\\")?,
                '\u{0008}' => self.bytes(b"\\b")?,
                '\t' => self.bytes(b"\\t")?,
                '\n' => self.bytes(b"\\n")?,
                '\u{000c}' => self.bytes(b"\\f")?,
                '\r' => self.bytes(b"\\r")?,
                '\u{0000}'..='\u{001f}' => {
                    let value = character as u8;
                    self.bytes(&[
                        b'\\',
                        b'u',
                        b'0',
                        b'0',
                        LOWER_HEX[(value >> 4) as usize],
                        LOWER_HEX[(value & 0x0f) as usize],
                    ])?;
                }
                _ => {
                    let mut encoded = [0_u8; 4];
                    self.bytes(character.encode_utf8(&mut encoded).as_bytes())?;
                }
            }
        }
        self.bytes(b"\"")
    }

    fn artifacts(&mut self, artifacts: &[ManifestArtifact]) -> Result<(), ManifestCodecError> {
        if artifacts.is_empty() {
            return self.bytes(b"[]");
        }
        self.bytes(b"[\n")?;
        for (index, artifact) in artifacts.iter().enumerate() {
            self.artifact(artifact)?;
            self.bytes(if index + 1 == artifacts.len() {
                b"\n"
            } else {
                b",\n"
            })?;
        }
        self.indent(1)?;
        self.bytes(b"]")
    }

    fn artifact(&mut self, artifact: &ManifestArtifact) -> Result<(), ManifestCodecError> {
        self.indent(2)?;
        self.bytes(b"{\n")?;
        self.member_name(3, "path")?;
        self.path(artifact.path(), 3)?;
        self.bytes(b",\n")?;
        self.member_name(3, "kind")?;
        self.string(artifact.kind().as_str())?;
        self.bytes(b",\n")?;
        self.member_name(3, "formatVersion")?;
        self.version(artifact.format_version(), 3)?;
        self.bytes(b",\n")?;
        self.member_name(3, "metadata")?;
        self.metadata(artifact)?;
        self.bytes(b",\n")?;
        self.member_name(3, "payload")?;
        self.payload(artifact)?;
        self.bytes(b"\n")?;
        self.indent(2)?;
        self.bytes(b"}")
    }

    fn version(
        &mut self,
        version: ExportArtifactFormatVersion,
        level: usize,
    ) -> Result<(), ManifestCodecError> {
        self.bytes(b"{\n")?;
        self.member_u64(level + 1, "major", u64::from(version.major()), true)?;
        self.member_u64(level + 1, "minor", u64::from(version.minor()), false)?;
        self.indent(level)?;
        self.bytes(b"}")
    }

    fn path(&mut self, path: &ExportArtifactPath, level: usize) -> Result<(), ManifestCodecError> {
        self.bytes(b"[\n")?;
        for (index, segment) in path.segments().iter().enumerate() {
            self.indent(level + 1)?;
            self.string(segment.as_str())?;
            self.bytes(if index + 1 == path.segments().len() {
                b"\n"
            } else {
                b",\n"
            })?;
        }
        self.indent(level)?;
        self.bytes(b"]")
    }

    fn metadata(&mut self, artifact: &ManifestArtifact) -> Result<(), ManifestCodecError> {
        self.bytes(b"{\n")?;
        self.member_name(4, "mediaType")?;
        if let Some(media_type) = artifact.media_type() {
            self.string(media_type.as_str())?;
        } else {
            self.bytes(b"null")?;
        }
        self.bytes(b",\n")?;
        self.member_name(4, "relationships")?;
        self.relationships(artifact.relationships())?;
        self.bytes(b"\n")?;
        self.indent(3)?;
        self.bytes(b"}")
    }

    fn relationships(
        &mut self,
        relationships: &[ExportArtifactRelationship],
    ) -> Result<(), ManifestCodecError> {
        if relationships.is_empty() {
            return self.bytes(b"[]");
        }
        self.bytes(b"[\n")?;
        for (index, relationship) in relationships.iter().enumerate() {
            self.indent(5)?;
            self.bytes(b"{\n")?;
            self.member_name(6, "kind")?;
            self.string(match relationship.kind() {
                ExportArtifactRelationshipKind::EagerLoad => "eager_load",
                ExportArtifactRelationshipKind::LazyLoad => "lazy_load",
            })?;
            self.bytes(b",\n")?;
            self.member_name(6, "target")?;
            self.path(relationship.target(), 6)?;
            self.bytes(b"\n")?;
            self.indent(5)?;
            self.bytes(if index + 1 == relationships.len() {
                b"}\n"
            } else {
                b"},\n"
            })?;
        }
        self.indent(4)?;
        self.bytes(b"]")
    }

    fn payload(&mut self, artifact: &ManifestArtifact) -> Result<(), ManifestCodecError> {
        self.bytes(b"{\n")?;
        self.member_u64(4, "bytes", artifact.payload_bytes(), true)?;
        self.member_name(4, "fingerprint")?;
        self.bytes(b"{\n")?;
        self.member_name(5, "algorithm")?;
        self.string(BLAKE3_ALGORITHM)?;
        self.bytes(b",\n")?;
        self.member_name(5, "digest")?;
        self.bytes(b"\"")?;
        for byte in artifact.payload_fingerprint().as_bytes() {
            self.bytes(&[
                LOWER_HEX[(byte >> 4) as usize],
                LOWER_HEX[(byte & 0x0f) as usize],
            ])?;
        }
        self.bytes(b"\"\n")?;
        self.indent(4)?;
        self.bytes(b"}\n")?;
        self.indent(3)?;
        self.bytes(b"}")
    }
}

#[cfg(test)]
mod tests {
    use intlify_export::{
        ExportArtifact, ExportArtifactFormatVersion, ExportArtifactKind, ExportArtifactMetadata,
        ExportArtifactPath, ExportArtifactPayload, ExportArtifactRelationship,
        ExportArtifactRelationshipKind, ExportArtifactSet, ExportMediaType,
    };

    use super::{
        validate_esm_route_limits, Blake3Fingerprint, CanonicalJsonWriter, CheckedOutputManifest,
        ManifestCodecErrorKind, ESM_ACCESSOR_KIND, ESM_ARTIFACT_LIMIT,
        ESM_LOADER_RELATIONSHIP_LIMIT, ESM_MODULE_LIMIT, OUTPUT_MANIFEST_WIRE_LIMIT,
    };
    use crate::messages::delivery::BuiltInExporterId;

    fn artifact_set(path: &str, payload: &[u8]) -> ExportArtifactSet {
        ExportArtifactSet::try_new(vec![ExportArtifact::new(
            ExportArtifactPath::try_new([path]).unwrap(),
            ExportArtifactKind::try_new("dev.intlify/esm-module").unwrap(),
            ExportArtifactFormatVersion::DRAFT_V0_1,
            ExportArtifactPayload::try_new(payload.to_vec()).unwrap(),
            ExportArtifactMetadata::try_new(
                Some(ExportMediaType::try_new("text/javascript").unwrap()),
                Vec::new(),
            )
            .unwrap(),
        )])
        .unwrap()
    }

    fn related_artifact_set() -> ExportArtifactSet {
        let module = |path: &str| {
            ExportArtifact::new(
                ExportArtifactPath::try_new([path]).unwrap(),
                ExportArtifactKind::try_new("dev.intlify/esm-module").unwrap(),
                ExportArtifactFormatVersion::DRAFT_V0_1,
                ExportArtifactPayload::try_new([]).unwrap(),
                ExportArtifactMetadata::try_new(
                    Some(ExportMediaType::try_new("text/javascript").unwrap()),
                    Vec::new(),
                )
                .unwrap(),
            )
        };
        let first = ExportArtifactPath::try_new(["a.mjs"]).unwrap();
        let second = ExportArtifactPath::try_new(["b.mjs"]).unwrap();
        ExportArtifactSet::try_new(vec![
            ExportArtifact::new(
                ExportArtifactPath::try_new(["loader.mjs"]).unwrap(),
                ExportArtifactKind::try_new("dev.intlify/loader-map").unwrap(),
                ExportArtifactFormatVersion::DRAFT_V0_1,
                ExportArtifactPayload::try_new([]).unwrap(),
                ExportArtifactMetadata::try_new(
                    Some(ExportMediaType::try_new("text/javascript").unwrap()),
                    vec![
                        ExportArtifactRelationship::new(
                            ExportArtifactRelationshipKind::LazyLoad,
                            second,
                        ),
                        ExportArtifactRelationship::new(
                            ExportArtifactRelationshipKind::LazyLoad,
                            first,
                        ),
                    ],
                )
                .unwrap(),
            ),
            module("b.mjs"),
            module("a.mjs"),
        ])
        .unwrap()
    }

    #[test]
    fn canonical_writer_has_fixed_shape_and_literal_unicode_scalars() {
        let checked = CheckedOutputManifest::from_artifact_set(
            BuiltInExporterId::Esm,
            &artifact_set("quote\"\u{2028}.mjs", b"abc"),
        )
        .unwrap();
        let digest = blake3::hash(b"abc").to_hex().to_string();
        let expected = format!(
            concat!(
                "{{\n",
                "  \"schemaVersion\": {{\n",
                "    \"major\": 0,\n",
                "    \"minor\": 1\n",
                "  }},\n",
                "  \"exporter\": \"esm\",\n",
                "  \"artifacts\": [\n",
                "    {{\n",
                "      \"path\": [\n",
                "        \"quote\\\"\u{2028}.mjs\"\n",
                "      ],\n",
                "      \"kind\": \"dev.intlify/esm-module\",\n",
                "      \"formatVersion\": {{\n",
                "        \"major\": 0,\n",
                "        \"minor\": 1\n",
                "      }},\n",
                "      \"metadata\": {{\n",
                "        \"mediaType\": \"text/javascript\",\n",
                "        \"relationships\": []\n",
                "      }},\n",
                "      \"payload\": {{\n",
                "        \"bytes\": 3,\n",
                "        \"fingerprint\": {{\n",
                "          \"algorithm\": \"blake3-256\",\n",
                "          \"digest\": \"{}\"\n",
                "        }}\n",
                "      }}\n",
                "    }}\n",
                "  ]\n",
                "}}\n"
            ),
            digest
        );
        assert_eq!(checked.canonical_bytes(), expected.as_bytes());
        assert_eq!(
            checked.canonical_fingerprint(),
            Blake3Fingerprint::of(expected.as_bytes())
        );
        assert_eq!(
            checked.manifest().artifacts()[0].payload_fingerprint(),
            Blake3Fingerprint::of(b"abc")
        );
    }

    #[test]
    fn valid_noncanonical_input_decodes_to_the_same_canonical_manifest() {
        let expected = CheckedOutputManifest::from_artifact_set(
            BuiltInExporterId::Esm,
            &artifact_set("loader.mjs", b"payload"),
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(expected.canonical_bytes()).unwrap();
        let noncanonical = serde_json::to_vec(&value).unwrap();
        assert_ne!(noncanonical, expected.canonical_bytes());

        let decoded = CheckedOutputManifest::decode(&noncanonical, BuiltInExporterId::Esm).unwrap();
        assert_eq!(decoded, expected);
    }

    #[test]
    fn strict_decoder_rejects_duplicate_unknown_and_mistyped_members() {
        let digest = blake3::hash(b"").to_hex();
        let base =
            "{\"schemaVersion\":{\"major\":0,\"minor\":1},\"exporter\":\"esm\",\"artifacts\":[]}"
                .to_owned();
        for source in [
            "{\"schemaVersion\":{\"major\":0,\"minor\":1},\"exporter\":\"esm\",\"exporter\":\"esm\",\"artifacts\":[]}"
                .to_owned(),
            format!("{} ", base.replace("\"artifacts\":[]", "\"artifacts\":[],\"extra\":null")),
            base.replace("\"artifacts\":[]", "\"artifacts\":null"),
            format!(
                "{{\"schemaVersion\":{{\"major\":0,\"minor\":1}},\"exporter\":\"esm\",\"artifacts\":[{{\"path\":[\"x\"],\"kind\":\"dev.intlify/esm-module\",\"formatVersion\":{{\"major\":0,\"minor\":1}},\"metadata\":{{\"mediaType\":null,\"relationships\":[]}},\"payload\":{{\"bytes\":0,\"fingerprint\":{{\"algorithm\":\"blake3-256\",\"digest\":\"{}\"}}}}}}]}}",
                digest.to_string().to_uppercase()
            ),
        ] {
            assert_eq!(
                CheckedOutputManifest::decode(source.as_bytes(), BuiltInExporterId::Esm)
                    .unwrap_err()
                    .kind(),
                ManifestCodecErrorKind::Invalid
            );
        }
    }

    #[test]
    fn decoder_reconstructs_canonical_artifact_and_relationship_order() {
        let canonical = CheckedOutputManifest::from_artifact_set(
            BuiltInExporterId::Esm,
            &related_artifact_set(),
        )
        .unwrap();

        let mut artifact_order: serde_json::Value =
            serde_json::from_slice(canonical.canonical_bytes()).unwrap();
        artifact_order["artifacts"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1);
        assert_eq!(
            CheckedOutputManifest::decode(
                &serde_json::to_vec(&artifact_order).unwrap(),
                BuiltInExporterId::Esm,
            )
            .unwrap_err()
            .kind(),
            ManifestCodecErrorKind::Invalid
        );

        let mut relationship_order: serde_json::Value =
            serde_json::from_slice(canonical.canonical_bytes()).unwrap();
        let loader = relationship_order["artifacts"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|artifact| artifact["path"][0] == "loader.mjs")
            .unwrap();
        loader["metadata"]["relationships"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1);
        assert_eq!(
            CheckedOutputManifest::decode(
                &serde_json::to_vec(&relationship_order).unwrap(),
                BuiltInExporterId::Esm,
            )
            .unwrap_err()
            .kind(),
            ManifestCodecErrorKind::Invalid
        );
    }

    #[test]
    fn unsupported_version_exporter_and_algorithm_are_not_adopted() {
        let expected = CheckedOutputManifest::from_artifact_set(
            BuiltInExporterId::Esm,
            &artifact_set("loader.mjs", b"payload"),
        )
        .unwrap();
        let source = std::str::from_utf8(expected.canonical_bytes()).unwrap();
        for unsupported in [
            source.replacen("\"minor\": 1", "\"minor\": 2", 1),
            source.replacen("\"exporter\": \"esm\"", "\"exporter\": \"other\"", 1),
            source.replacen(
                "\"algorithm\": \"blake3-256\"",
                "\"algorithm\": \"sha256\"",
                1,
            ),
        ] {
            assert_eq!(
                CheckedOutputManifest::decode(unsupported.as_bytes(), BuiltInExporterId::Esm)
                    .unwrap_err()
                    .kind(),
                ManifestCodecErrorKind::Unsupported
            );
        }
    }

    #[test]
    fn wire_limit_bom_utf8_and_plain_unsigned_integer_rules_are_strict() {
        let base =
            b"{\"schemaVersion\":{\"major\":0,\"minor\":1},\"exporter\":\"esm\",\"artifacts\":[]}";
        let mut exact = base.to_vec();
        exact.resize(OUTPUT_MANIFEST_WIRE_LIMIT, b' ');
        assert!(CheckedOutputManifest::decode(&exact, BuiltInExporterId::Esm).is_ok());
        exact.push(b' ');
        assert_eq!(
            CheckedOutputManifest::decode(&exact, BuiltInExporterId::Esm)
                .unwrap_err()
                .kind(),
            ManifestCodecErrorKind::Limit
        );
        assert_eq!(
            CheckedOutputManifest::decode(b"\xef\xbb\xbf{}", BuiltInExporterId::Esm)
                .unwrap_err()
                .kind(),
            ManifestCodecErrorKind::Invalid
        );
        assert_eq!(
            CheckedOutputManifest::decode(&[0xff], BuiltInExporterId::Esm)
                .unwrap_err()
                .kind(),
            ManifestCodecErrorKind::Invalid
        );
        let exponent =
            std::str::from_utf8(base)
                .unwrap()
                .replacen("\"major\":0", "\"major\":0e0", 1);
        assert_eq!(
            CheckedOutputManifest::decode(exponent.as_bytes(), BuiltInExporterId::Esm)
                .unwrap_err()
                .kind(),
            ManifestCodecErrorKind::Invalid
        );
    }

    #[test]
    fn claimed_payload_limits_are_checked_without_allocating_payload_bytes() {
        let expected = CheckedOutputManifest::from_artifact_set(
            BuiltInExporterId::Esm,
            &artifact_set("loader.mjs", b""),
        )
        .unwrap();
        let source = std::str::from_utf8(expected.canonical_bytes()).unwrap();
        let over = source.replacen("\"bytes\": 0", "\"bytes\": 268435457", 1);
        assert_eq!(
            CheckedOutputManifest::decode(over.as_bytes(), BuiltInExporterId::Esm)
                .unwrap_err()
                .kind(),
            ManifestCodecErrorKind::Limit
        );

        let mut aggregate: serde_json::Value =
            serde_json::from_slice(expected.canonical_bytes()).unwrap();
        let template = aggregate["artifacts"][0].clone();
        aggregate["artifacts"] = serde_json::Value::Array(
            (0..5)
                .map(|index| {
                    let mut artifact = template.clone();
                    artifact["path"] = serde_json::json!([format!("{index}.mjs")]);
                    artifact["payload"]["bytes"] = serde_json::json!(268_435_456_u64);
                    artifact
                })
                .collect(),
        );
        assert_eq!(
            CheckedOutputManifest::decode(
                &serde_json::to_vec(&aggregate).unwrap(),
                BuiltInExporterId::Esm,
            )
            .unwrap_err()
            .kind(),
            ManifestCodecErrorKind::Limit
        );
    }

    #[test]
    fn built_in_esm_route_limits_remain_independent_of_common_limits() {
        let checked = CheckedOutputManifest::from_artifact_set(
            BuiltInExporterId::Esm,
            &related_artifact_set(),
        )
        .unwrap();
        let module = checked
            .manifest()
            .artifacts()
            .iter()
            .find(|artifact| artifact.kind().as_str() == "dev.intlify/esm-module")
            .unwrap()
            .clone();
        let mut loader = checked
            .manifest()
            .artifacts()
            .iter()
            .find(|artifact| artifact.kind().as_str() == "dev.intlify/loader-map")
            .unwrap()
            .clone();

        assert_eq!(
            validate_esm_route_limits(&vec![module.clone(); ESM_MODULE_LIMIT + 1])
                .unwrap_err()
                .kind(),
            ManifestCodecErrorKind::Limit
        );
        assert_eq!(
            validate_esm_route_limits(&[loader.clone(), loader.clone()])
                .unwrap_err()
                .kind(),
            ManifestCodecErrorKind::Limit
        );
        let mut accessor = module.clone();
        accessor.kind = ExportArtifactKind::try_new(ESM_ACCESSOR_KIND).unwrap();
        assert_eq!(
            validate_esm_route_limits(&vec![accessor; ESM_ARTIFACT_LIMIT])
                .unwrap_err()
                .kind(),
            ManifestCodecErrorKind::Limit
        );
        assert_eq!(
            validate_esm_route_limits(&vec![module; ESM_ARTIFACT_LIMIT + 1])
                .unwrap_err()
                .kind(),
            ManifestCodecErrorKind::Limit
        );

        let relationship = loader.relationships()[0].clone();
        loader.relationships =
            vec![relationship; ESM_LOADER_RELATIONSHIP_LIMIT + 1].into_boxed_slice();
        assert_eq!(
            validate_esm_route_limits(&[loader]).unwrap_err().kind(),
            ManifestCodecErrorKind::Limit
        );

        let mut wire: serde_json::Value = serde_json::from_slice(
            CheckedOutputManifest::from_artifact_set(
                BuiltInExporterId::Esm,
                &artifact_set("template.mjs", b""),
            )
            .unwrap()
            .canonical_bytes(),
        )
        .unwrap();
        let mut first_loader = wire["artifacts"][0].clone();
        first_loader["path"] = serde_json::json!(["loader-a.mjs"]);
        first_loader["kind"] = serde_json::json!("dev.intlify/loader-map");
        let mut second_loader = first_loader.clone();
        second_loader["path"] = serde_json::json!(["loader-b.mjs"]);
        wire["artifacts"] = serde_json::json!([first_loader, second_loader]);
        assert_eq!(
            CheckedOutputManifest::decode(
                &serde_json::to_vec(&wire).unwrap(),
                BuiltInExporterId::Esm,
            )
            .unwrap_err()
            .kind(),
            ManifestCodecErrorKind::Limit
        );
    }

    #[test]
    fn canonical_string_writer_uses_every_required_escape_class() {
        let mut writer = CanonicalJsonWriter::new();
        writer
            .string("\"\\\u{0008}\t\n\u{000c}\r\u{0000}\u{001f}/\u{2028}\u{2029}")
            .unwrap();
        assert_eq!(
            writer.finish().as_ref(),
            b"\"\\\"\\\\\\b\\t\\n\\f\\r\\u0000\\u001f/\xe2\x80\xa8\xe2\x80\xa9\""
        );
    }

    #[test]
    fn canonical_writer_accepts_the_exact_wire_limit_and_rejects_the_next_byte() {
        let mut writer = CanonicalJsonWriter::new();
        writer
            .bytes(&vec![b'x'; OUTPUT_MANIFEST_WIRE_LIMIT])
            .unwrap();
        assert_eq!(writer.bytes.len(), OUTPUT_MANIFEST_WIRE_LIMIT);
        assert_eq!(
            writer.bytes(b"x").unwrap_err().kind(),
            ManifestCodecErrorKind::Limit
        );
        assert_eq!(writer.bytes.len(), OUTPUT_MANIFEST_WIRE_LIMIT);
    }
}
