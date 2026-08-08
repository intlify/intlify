// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Checked persistent-control identities and wire codecs for output registration.
//!
//! Lock and transaction-journal files are integration-owned protocol records,
//! not exporter artifacts. This module derives every sibling basename from the
//! checked output root, rejects noncanonical or ambiguous JSON, and retains no
//! ambient path, process identifier, timestamp, or weak random fallback.

use serde_json::Value;

use super::super::delivery::DeliveryOutputRoot;
use super::manifest::{
    exact_object, parse_unique_json, plain_u16, Blake3Fingerprint, CanonicalJsonWriter,
    ManifestCodecError, ManifestCodecErrorKind, BLAKE3_ALGORITHM, LOWER_HEX,
};

pub(super) const CONTROL_WIRE_LIMIT: usize = 65_536;

const CONTROL_SCHEMA_MAJOR: u16 = 0;
const CONTROL_SCHEMA_MINOR: u16 = 1;
const CONTROL_ID_DOMAIN: &[u8] = b"dev.intlify/output-root-control";
const CONTROL_FILE_PREFIX: &str = ".intlify-output-";

/// BLAKE3 identity of one exact portable output-root segment sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct OutputRootControlId([u8; 32]);

impl OutputRootControlId {
    pub(super) fn derive(output_root: &DeliveryOutputRoot) -> Result<Self, ControlCodecError> {
        let segment_count = u16::try_from(output_root.segments().len())
            .map_err(|_| ControlCodecError::internal_invariant())?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(CONTROL_ID_DOMAIN);
        hasher.update(&[0]);
        hasher.update(&CONTROL_SCHEMA_MAJOR.to_be_bytes());
        hasher.update(&CONTROL_SCHEMA_MINOR.to_be_bytes());
        hasher.update(&segment_count.to_be_bytes());
        for segment in output_root.segments() {
            let length = u16::try_from(segment.len())
                .map_err(|_| ControlCodecError::internal_invariant())?;
            hasher.update(&length.to_be_bytes());
            hasher.update(segment.as_bytes());
        }
        Ok(Self(*hasher.finalize().as_bytes()))
    }

    fn hex(self) -> String {
        lower_hex(&self.0)
    }

    pub(super) fn lock_basename(self) -> String {
        format!("{CONTROL_FILE_PREFIX}{}.lock", self.hex())
    }

    pub(super) fn journal_basename(self) -> String {
        format!("{CONTROL_FILE_PREFIX}{}.transaction.json", self.hex())
    }

    pub(super) fn staging_basename(self, transaction: TransactionId) -> String {
        format!(
            "{CONTROL_FILE_PREFIX}{}.staging-{}",
            self.hex(),
            transaction.hex()
        )
    }

    pub(super) fn backup_basename(self, transaction: TransactionId) -> String {
        format!(
            "{CONTROL_FILE_PREFIX}{}.backup-{}",
            self.hex(),
            transaction.hex()
        )
    }

    pub(super) fn journal_temporary_basename(self, transaction: TransactionId) -> String {
        format!(
            "{CONTROL_FILE_PREFIX}{}.transaction.tmp-{}",
            self.hex(),
            transaction.hex()
        )
    }
}

/// Cryptographically random identifier for one output-root transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct TransactionId([u8; 16]);

impl TransactionId {
    pub(super) fn generate() -> Result<Self, getrandom::Error> {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes)?;
        Ok(Self(bytes))
    }

    #[cfg(test)]
    pub(super) const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    fn from_lower_hex(value: &str) -> Option<Self> {
        Some(Self(parse_lower_hex::<16>(value)?))
    }

    fn hex(self) -> String {
        lower_hex(&self.0)
    }
}

/// Canonical persistent lock ownership record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LockRecord {
    output_root: DeliveryOutputRoot,
    canonical_bytes: Box<[u8]>,
}

impl LockRecord {
    pub(super) fn new(output_root: DeliveryOutputRoot) -> Result<Self, ControlCodecError> {
        let canonical_bytes = encode_lock_record(&output_root)?;
        Ok(Self {
            output_root,
            canonical_bytes,
        })
    }

    pub(super) fn decode(input: &[u8]) -> Result<Self, ControlCodecError> {
        let value = decode_control_json(input)?;
        let fields = exact_object(&value, &["schemaVersion", "out"]).map_err(map_manifest_error)?;
        decode_schema_version(fields[0])?;
        let output_root = decode_output_root(fields[1])?;
        let record = Self::new(output_root)?;
        if input != record.canonical_bytes() {
            return Err(ControlCodecError::invalid());
        }
        Ok(record)
    }

    pub(super) const fn output_root(&self) -> &DeliveryOutputRoot {
        &self.output_root
    }

    pub(super) const fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }
}

/// Prior output-root shape recorded before commit begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum JournalOldRoot {
    Absent,
    Empty,
    Managed {
        manifest_fingerprint: Blake3Fingerprint,
    },
}

/// Durable commit phase written to the transaction journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum JournalPhase {
    Staged,
    OldMoved,
    NewInstalled,
}

impl JournalPhase {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Staged => "staged",
            Self::OldMoved => "old_moved",
            Self::NewInstalled => "new_installed",
        }
    }

    fn decode(value: &Value) -> Result<Self, ControlCodecError> {
        match value.as_str() {
            Some("staged") => Ok(Self::Staged),
            Some("old_moved") => Ok(Self::OldMoved),
            Some("new_installed") => Ok(Self::NewInstalled),
            _ => Err(ControlCodecError::invalid()),
        }
    }
}

/// Checked transaction journal and all names derivable from its two IDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct JournalRecord {
    control_id: OutputRootControlId,
    transaction_id: TransactionId,
    output_root: DeliveryOutputRoot,
    staging: Box<str>,
    backup: Box<str>,
    old_root: JournalOldRoot,
    new_manifest_fingerprint: Blake3Fingerprint,
    phase: JournalPhase,
    canonical_bytes: Box<[u8]>,
}

impl JournalRecord {
    pub(super) fn new(
        control_id: OutputRootControlId,
        transaction_id: TransactionId,
        output_root: DeliveryOutputRoot,
        old_root: JournalOldRoot,
        new_manifest_fingerprint: Blake3Fingerprint,
        phase: JournalPhase,
    ) -> Result<Self, ControlCodecError> {
        if OutputRootControlId::derive(&output_root)? != control_id {
            return Err(ControlCodecError::internal_invariant());
        }
        let staging = control_id.staging_basename(transaction_id).into_boxed_str();
        let backup = control_id.backup_basename(transaction_id).into_boxed_str();
        let mut record = Self {
            control_id,
            transaction_id,
            output_root,
            staging,
            backup,
            old_root,
            new_manifest_fingerprint,
            phase,
            canonical_bytes: Box::new([]),
        };
        record.canonical_bytes = encode_journal_record(&record)?;
        Ok(record)
    }

    pub(super) fn decode(
        input: &[u8],
        expected_control_id: OutputRootControlId,
    ) -> Result<Self, ControlCodecError> {
        let value = decode_control_json(input)?;
        let fields = exact_object(
            &value,
            &[
                "schemaVersion",
                "transactionId",
                "out",
                "staging",
                "backup",
                "oldRoot",
                "newManifestFingerprint",
                "phase",
            ],
        )
        .map_err(map_manifest_error)?;
        decode_schema_version(fields[0])?;
        let transaction_id = fields[1]
            .as_str()
            .and_then(TransactionId::from_lower_hex)
            .ok_or_else(ControlCodecError::invalid)?;
        let output_root = decode_output_root(fields[2])?;
        if OutputRootControlId::derive(&output_root)? != expected_control_id {
            return Err(ControlCodecError::invalid());
        }
        let expected_staging = expected_control_id.staging_basename(transaction_id);
        let expected_backup = expected_control_id.backup_basename(transaction_id);
        if fields[3].as_str() != Some(expected_staging.as_str())
            || fields[4].as_str() != Some(expected_backup.as_str())
        {
            return Err(ControlCodecError::invalid());
        }
        let old_root = decode_old_root(fields[5])?;
        let new_manifest_fingerprint = decode_fingerprint(fields[6])?;
        let phase = JournalPhase::decode(fields[7])?;
        let record = Self::new(
            expected_control_id,
            transaction_id,
            output_root,
            old_root,
            new_manifest_fingerprint,
            phase,
        )?;
        if input != record.canonical_bytes() {
            return Err(ControlCodecError::invalid());
        }
        Ok(record)
    }

    pub(super) fn with_phase(&self, phase: JournalPhase) -> Result<Self, ControlCodecError> {
        Self::new(
            self.control_id,
            self.transaction_id,
            self.output_root.clone(),
            self.old_root,
            self.new_manifest_fingerprint,
            phase,
        )
    }

    pub(super) const fn transaction_id(&self) -> TransactionId {
        self.transaction_id
    }

    pub(super) const fn output_root(&self) -> &DeliveryOutputRoot {
        &self.output_root
    }

    pub(super) const fn staging(&self) -> &str {
        &self.staging
    }

    pub(super) const fn backup(&self) -> &str {
        &self.backup
    }

    pub(super) const fn old_root(&self) -> JournalOldRoot {
        self.old_root
    }

    pub(super) const fn new_manifest_fingerprint(&self) -> Blake3Fingerprint {
        self.new_manifest_fingerprint
    }

    pub(super) const fn phase(&self) -> JournalPhase {
        self.phase
    }

    pub(super) const fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }
}

/// Closed codec classification without parser or dependency text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ControlCodecErrorKind {
    Invalid,
    Unsupported,
    Limit,
    InternalInvariant,
}

/// Failure to admit or encode a persistent control record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ControlCodecError {
    kind: ControlCodecErrorKind,
}

impl ControlCodecError {
    const fn invalid() -> Self {
        Self {
            kind: ControlCodecErrorKind::Invalid,
        }
    }

    const fn unsupported() -> Self {
        Self {
            kind: ControlCodecErrorKind::Unsupported,
        }
    }

    const fn limit() -> Self {
        Self {
            kind: ControlCodecErrorKind::Limit,
        }
    }

    const fn internal_invariant() -> Self {
        Self {
            kind: ControlCodecErrorKind::InternalInvariant,
        }
    }

    pub(super) const fn kind(self) -> ControlCodecErrorKind {
        self.kind
    }
}

fn decode_control_json(input: &[u8]) -> Result<Value, ControlCodecError> {
    if input.len() > CONTROL_WIRE_LIMIT {
        return Err(ControlCodecError::limit());
    }
    if input.starts_with(&[0xef, 0xbb, 0xbf]) {
        return Err(ControlCodecError::invalid());
    }
    let source = std::str::from_utf8(input).map_err(|_| ControlCodecError::invalid())?;
    parse_unique_json(source).map_err(map_manifest_error)
}

fn decode_schema_version(value: &Value) -> Result<(), ControlCodecError> {
    let fields = exact_object(value, &["major", "minor"]).map_err(map_manifest_error)?;
    let major = plain_u16(fields[0]).map_err(map_manifest_error)?;
    let minor = plain_u16(fields[1]).map_err(map_manifest_error)?;
    if (major, minor) != (CONTROL_SCHEMA_MAJOR, CONTROL_SCHEMA_MINOR) {
        return Err(ControlCodecError::unsupported());
    }
    Ok(())
}

fn decode_output_root(value: &Value) -> Result<DeliveryOutputRoot, ControlCodecError> {
    let segments = value.as_array().ok_or_else(ControlCodecError::invalid)?;
    let mut spelling = String::new();
    for (index, segment) in segments.iter().enumerate() {
        let segment = segment.as_str().ok_or_else(ControlCodecError::invalid)?;
        if index != 0 {
            spelling.push('/');
        }
        spelling.push_str(segment);
    }
    DeliveryOutputRoot::try_new(&spelling).map_err(|_| ControlCodecError::invalid())
}

fn decode_old_root(value: &Value) -> Result<JournalOldRoot, ControlCodecError> {
    let object = value.as_object().ok_or_else(ControlCodecError::invalid)?;
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(ControlCodecError::invalid)?;
    match kind {
        "absent" if object.len() == 1 => Ok(JournalOldRoot::Absent),
        "empty" if object.len() == 1 => Ok(JournalOldRoot::Empty),
        "managed" => {
            let fields = exact_object(value, &["kind", "manifestFingerprint"])
                .map_err(map_manifest_error)?;
            if fields[0].as_str() != Some("managed") {
                return Err(ControlCodecError::invalid());
            }
            Ok(JournalOldRoot::Managed {
                manifest_fingerprint: decode_fingerprint(fields[1])?,
            })
        }
        _ => Err(ControlCodecError::invalid()),
    }
}

fn decode_fingerprint(value: &Value) -> Result<Blake3Fingerprint, ControlCodecError> {
    let fields = exact_object(value, &["algorithm", "digest"]).map_err(map_manifest_error)?;
    if fields[0].as_str() != Some(BLAKE3_ALGORITHM) {
        return Err(ControlCodecError::unsupported());
    }
    fields[1]
        .as_str()
        .and_then(Blake3Fingerprint::from_lower_hex)
        .ok_or_else(ControlCodecError::invalid)
}

fn encode_lock_record(output_root: &DeliveryOutputRoot) -> Result<Box<[u8]>, ControlCodecError> {
    let mut writer = CanonicalJsonWriter::with_limit(CONTROL_WIRE_LIMIT);
    writer.bytes(b"{\n").map_err(map_manifest_error)?;
    write_schema_version(&mut writer, true)?;
    writer.member_name(1, "out").map_err(map_manifest_error)?;
    write_output_root(&mut writer, output_root, 1)?;
    writer.bytes(b"\n}\n").map_err(map_manifest_error)?;
    Ok(writer.finish())
}

fn encode_journal_record(record: &JournalRecord) -> Result<Box<[u8]>, ControlCodecError> {
    let mut writer = CanonicalJsonWriter::with_limit(CONTROL_WIRE_LIMIT);
    writer.bytes(b"{\n").map_err(map_manifest_error)?;
    write_schema_version(&mut writer, true)?;
    write_string_member(
        &mut writer,
        1,
        "transactionId",
        &record.transaction_id.hex(),
        true,
    )?;
    writer.member_name(1, "out").map_err(map_manifest_error)?;
    write_output_root(&mut writer, &record.output_root, 1)?;
    writer.bytes(b",\n").map_err(map_manifest_error)?;
    write_string_member(&mut writer, 1, "staging", &record.staging, true)?;
    write_string_member(&mut writer, 1, "backup", &record.backup, true)?;
    writer
        .member_name(1, "oldRoot")
        .map_err(map_manifest_error)?;
    write_old_root(&mut writer, record.old_root, 1)?;
    writer.bytes(b",\n").map_err(map_manifest_error)?;
    writer
        .member_name(1, "newManifestFingerprint")
        .map_err(map_manifest_error)?;
    write_fingerprint(&mut writer, record.new_manifest_fingerprint, 1)?;
    writer.bytes(b",\n").map_err(map_manifest_error)?;
    write_string_member(&mut writer, 1, "phase", record.phase.as_str(), false)?;
    writer.bytes(b"}\n").map_err(map_manifest_error)?;
    Ok(writer.finish())
}

fn write_schema_version(
    writer: &mut CanonicalJsonWriter,
    comma: bool,
) -> Result<(), ControlCodecError> {
    writer
        .member_name(1, "schemaVersion")
        .map_err(map_manifest_error)?;
    writer.bytes(b"{\n").map_err(map_manifest_error)?;
    writer
        .member_u64(2, "major", u64::from(CONTROL_SCHEMA_MAJOR), true)
        .map_err(map_manifest_error)?;
    writer
        .member_u64(2, "minor", u64::from(CONTROL_SCHEMA_MINOR), false)
        .map_err(map_manifest_error)?;
    writer.indent(1).map_err(map_manifest_error)?;
    writer
        .bytes(if comma { b"},\n" } else { b"}\n" })
        .map_err(map_manifest_error)
}

fn write_output_root(
    writer: &mut CanonicalJsonWriter,
    output_root: &DeliveryOutputRoot,
    level: usize,
) -> Result<(), ControlCodecError> {
    writer.bytes(b"[\n").map_err(map_manifest_error)?;
    let count = output_root.segments().len();
    for (index, segment) in output_root.segments().enumerate() {
        writer.indent(level + 1).map_err(map_manifest_error)?;
        writer.string(segment).map_err(map_manifest_error)?;
        writer
            .bytes(if index + 1 == count { b"\n" } else { b",\n" })
            .map_err(map_manifest_error)?;
    }
    writer.indent(level).map_err(map_manifest_error)?;
    writer.bytes(b"]").map_err(map_manifest_error)
}

fn write_old_root(
    writer: &mut CanonicalJsonWriter,
    old_root: JournalOldRoot,
    level: usize,
) -> Result<(), ControlCodecError> {
    writer.bytes(b"{\n").map_err(map_manifest_error)?;
    match old_root {
        JournalOldRoot::Absent => {
            write_string_member(writer, level + 1, "kind", "absent", false)?;
        }
        JournalOldRoot::Empty => {
            write_string_member(writer, level + 1, "kind", "empty", false)?;
        }
        JournalOldRoot::Managed {
            manifest_fingerprint,
        } => {
            write_string_member(writer, level + 1, "kind", "managed", true)?;
            writer
                .member_name(level + 1, "manifestFingerprint")
                .map_err(map_manifest_error)?;
            write_fingerprint(writer, manifest_fingerprint, level + 1)?;
            writer.bytes(b"\n").map_err(map_manifest_error)?;
        }
    }
    writer.indent(level).map_err(map_manifest_error)?;
    writer.bytes(b"}").map_err(map_manifest_error)
}

fn write_fingerprint(
    writer: &mut CanonicalJsonWriter,
    fingerprint: Blake3Fingerprint,
    level: usize,
) -> Result<(), ControlCodecError> {
    writer.bytes(b"{\n").map_err(map_manifest_error)?;
    write_string_member(writer, level + 1, "algorithm", BLAKE3_ALGORITHM, true)?;
    write_string_member(
        writer,
        level + 1,
        "digest",
        &lower_hex(&fingerprint.as_bytes()),
        false,
    )?;
    writer.indent(level).map_err(map_manifest_error)?;
    writer.bytes(b"}").map_err(map_manifest_error)
}

fn write_string_member(
    writer: &mut CanonicalJsonWriter,
    level: usize,
    name: &str,
    value: &str,
    comma: bool,
) -> Result<(), ControlCodecError> {
    writer
        .member_name(level, name)
        .map_err(map_manifest_error)?;
    writer.string(value).map_err(map_manifest_error)?;
    writer
        .bytes(if comma { b",\n" } else { b"\n" })
        .map_err(map_manifest_error)
}

fn map_manifest_error(error: ManifestCodecError) -> ControlCodecError {
    match error.kind() {
        ManifestCodecErrorKind::Invalid => ControlCodecError::invalid(),
        ManifestCodecErrorKind::Unsupported => ControlCodecError::unsupported(),
        ManifestCodecErrorKind::Limit => ControlCodecError::limit(),
        ManifestCodecErrorKind::InternalInvariant => ControlCodecError::internal_invariant(),
    }
}

fn lower_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(LOWER_HEX[(byte >> 4) as usize]));
        output.push(char::from(LOWER_HEX[(byte & 0x0f) as usize]));
    }
    output
}

fn parse_lower_hex<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N.checked_mul(2)? {
        return None;
    }
    let mut bytes = [0_u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = (lower_hex_value(pair[0])? << 4) | lower_hex_value(pair[1])?;
    }
    Some(bytes)
}

fn lower_hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ControlCodecErrorKind, JournalOldRoot, JournalPhase, JournalRecord, LockRecord,
        OutputRootControlId, TransactionId, CONTROL_WIRE_LIMIT,
    };
    use crate::messages::delivery::DeliveryOutputRoot;
    use crate::messages::registration::manifest::Blake3Fingerprint;

    fn output_root() -> DeliveryOutputRoot {
        DeliveryOutputRoot::try_new("generated/messages").unwrap()
    }

    fn transaction_id() -> TransactionId {
        TransactionId::from_bytes([0xabu8; 16])
    }

    #[test]
    fn output_root_identity_and_all_derived_names_are_stable() {
        let control = OutputRootControlId::derive(&output_root()).unwrap();
        assert_eq!(
            control.lock_basename(),
            ".intlify-output-4d2903cdb530a10aff97706816d0decfef225709fc1b25a6176cfff1dd14f9c4.lock"
        );
        assert_eq!(
            control.staging_basename(transaction_id()),
            ".intlify-output-4d2903cdb530a10aff97706816d0decfef225709fc1b25a6176cfff1dd14f9c4.staging-abababababababababababababababab"
        );
        assert_eq!(
            control.backup_basename(transaction_id()),
            ".intlify-output-4d2903cdb530a10aff97706816d0decfef225709fc1b25a6176cfff1dd14f9c4.backup-abababababababababababababababab"
        );
        assert_eq!(
            control.journal_basename(),
            ".intlify-output-4d2903cdb530a10aff97706816d0decfef225709fc1b25a6176cfff1dd14f9c4.transaction.json"
        );
    }

    #[test]
    fn lock_record_round_trips_only_its_canonical_wire_form() {
        let record = LockRecord::new(output_root()).unwrap();
        assert_eq!(
            LockRecord::decode(record.canonical_bytes()).unwrap(),
            record
        );

        let mut noncanonical = record.canonical_bytes().to_vec();
        noncanonical.push(b' ');
        assert_eq!(
            LockRecord::decode(&noncanonical).unwrap_err().kind(),
            ControlCodecErrorKind::Invalid
        );
        let duplicate = std::str::from_utf8(record.canonical_bytes())
            .unwrap()
            .replacen("\n}", ",\n  \"out\": [\"generated\", \"messages\"]\n}", 1);
        assert_eq!(
            LockRecord::decode(duplicate.as_bytes()).unwrap_err().kind(),
            ControlCodecErrorKind::Invalid
        );
    }

    #[test]
    fn journal_round_trips_all_old_root_shapes_and_phases() {
        let output = output_root();
        let control = OutputRootControlId::derive(&output).unwrap();
        let new_fingerprint = Blake3Fingerprint::of(b"new");
        for old_root in [
            JournalOldRoot::Absent,
            JournalOldRoot::Empty,
            JournalOldRoot::Managed {
                manifest_fingerprint: Blake3Fingerprint::of(b"old"),
            },
        ] {
            for phase in [
                JournalPhase::Staged,
                JournalPhase::OldMoved,
                JournalPhase::NewInstalled,
            ] {
                let record = JournalRecord::new(
                    control,
                    transaction_id(),
                    output.clone(),
                    old_root,
                    new_fingerprint,
                    phase,
                )
                .unwrap();
                assert_eq!(
                    JournalRecord::decode(record.canonical_bytes(), control).unwrap(),
                    record
                );
            }
        }
    }

    #[test]
    fn journal_rejects_names_versions_fingerprints_and_wire_limits() {
        let output = output_root();
        let control = OutputRootControlId::derive(&output).unwrap();
        let record = JournalRecord::new(
            control,
            transaction_id(),
            output,
            JournalOldRoot::Absent,
            Blake3Fingerprint::of(b"new"),
            JournalPhase::Staged,
        )
        .unwrap();
        let source = std::str::from_utf8(record.canonical_bytes()).unwrap();
        for invalid in [
            source.replacen("\"minor\": 1", "\"minor\": 2", 1),
            source.replacen(".staging-", ".staging-x", 1),
            source.replacen("\"blake3-256\"", "\"sha256\"", 1),
            source.replacen("\"staged\"", "\"unknown\"", 1),
        ] {
            assert!(JournalRecord::decode(invalid.as_bytes(), control).is_err());
        }

        let over = vec![b' '; CONTROL_WIRE_LIMIT + 1];
        assert_eq!(
            JournalRecord::decode(&over, control).unwrap_err().kind(),
            ControlCodecErrorKind::Limit
        );
    }

    #[test]
    fn transaction_id_requires_exact_lowercase_128_bit_hex() {
        assert!(TransactionId::from_lower_hex("abababababababababababababababab").is_some());
        assert!(TransactionId::from_lower_hex("ABABABABABABABABABABABABABABABAB").is_none());
        assert!(TransactionId::from_lower_hex("ab").is_none());
    }
}
