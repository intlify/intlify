// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Private checked delivery-target identities, options, and selection.
//!
//! This module owns the bounded, host-independent values that sit between raw
//! delivery configuration and exporter construction. It canonicalizes target
//! order and rejects overlapping portable output roots before command-level
//! target selection. It does not decode JSON, publish schema, expose a command,
//! construct exporters, or access the filesystem.

use std::collections::BTreeMap;

use intlify_contract::Locale;
use intlify_export::ExportArtifactPath;
use intlify_linker::LinkPolicy;

const DELIVERY_TARGETS_LIMIT: usize = 64;
const DELIVERY_TARGET_NAME_BYTES_LIMIT: usize = 255;
const OUTPUT_ROOT_SEGMENTS_LIMIT: usize = 64;
const OUTPUT_ROOT_SEGMENT_BYTES_LIMIT: usize = 255;
const OUTPUT_ROOT_BYTES_LIMIT: u64 = 4_096;
const EAGER_LOCALES_LIMIT: usize = 1_024;
const LOCALE_BYTES_LIMIT: usize = 255;

/// Borrowed, shape-checked fields for one submitted delivery target.
///
/// The future raw configuration adapter owns missing-field, JSON-type, unknown
/// member, and pointer validation before constructing this value.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DeliveryTargetInput<'a> {
    name: &'a str,
    exporter: &'a str,
    out: &'a str,
    eager_locales: &'a [String],
    placement: Option<&'a str>,
}

impl<'a> DeliveryTargetInput<'a> {
    /// Retain the exact borrowed target fields for checked resolution.
    pub(crate) const fn new(
        name: &'a str,
        exporter: &'a str,
        out: &'a str,
        eager_locales: &'a [String],
        placement: Option<&'a str>,
    ) -> Self {
        Self {
            name,
            exporter,
            out,
            eager_locales,
            placement,
        }
    }

    pub(crate) const fn name(self) -> &'a str {
        self.name
    }

    pub(crate) const fn exporter(self) -> &'a str {
        self.exporter
    }

    pub(crate) const fn out(self) -> &'a str {
        self.out
    }

    pub(crate) const fn eager_locales(self) -> &'a [String] {
        self.eager_locales
    }

    pub(crate) const fn placement(self) -> Option<&'a str> {
        self.placement
    }
}

/// Exact checked machine-facing target identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct DeliveryTargetName(Box<str>);

impl DeliveryTargetName {
    /// Validate the exact `[a-z0-9][a-z0-9._-]{0,254}` grammar.
    pub(crate) fn try_new(value: &str) -> Result<Self, DeliveryTargetNameError> {
        let observed = usize_to_u64(value.len());
        if value.len() > DELIVERY_TARGET_NAME_BYTES_LIMIT {
            return Err(DeliveryTargetNameError::ByteLimitExceeded {
                limit: DELIVERY_TARGET_NAME_BYTES_LIMIT as u64,
                observed,
            });
        }

        let mut bytes = value.bytes();
        let Some(first) = bytes.next() else {
            return Err(DeliveryTargetNameError::InvalidGrammar);
        };
        if !is_lowercase_ascii_alphanumeric(first)
            || !bytes.all(|byte| {
                is_lowercase_ascii_alphanumeric(byte) || matches!(byte, b'.' | b'_' | b'-')
            })
        {
            return Err(DeliveryTargetNameError::InvalidGrammar);
        }

        Ok(Self(value.into()))
    }

    /// Return the exact checked target-name spelling.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Failure while checking a target name or explicit `--target` operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeliveryTargetNameError {
    /// The value violates the closed ASCII target-name grammar.
    InvalidGrammar,
    /// The exact UTF-8 byte length exceeds the fixed ceiling.
    ByteLimitExceeded { limit: u64, observed: u64 },
}

/// Initial closed built-in exporter identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum BuiltInExporterId {
    /// Data-only ESM modules and TypeScript accessors.
    Esm,
}

impl BuiltInExporterId {
    /// Admit only an exact built-in exporter identifier.
    pub(crate) fn try_new(value: &str) -> Result<Self, UnsupportedExporterId> {
        match value {
            "esm" => Ok(Self::Esm),
            _ => Err(UnsupportedExporterId),
        }
    }

    /// Return the exact registry identifier.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Esm => "esm",
        }
    }
}

/// The supplied exporter identifier is not in the closed built-in registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnsupportedExporterId;

/// Initial target-wide placement policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) enum DeliveryPlacement {
    /// Materialize every reachable message in every selected delivery unit.
    #[default]
    Duplicate,
}

impl DeliveryPlacement {
    /// Resolve omission and exact `duplicate` to the same checked value.
    pub(crate) fn try_new(value: Option<&str>) -> Result<Self, UnsupportedDeliveryPlacement> {
        match value {
            None | Some("duplicate") => Ok(Self::Duplicate),
            Some(_) => Err(UnsupportedDeliveryPlacement),
        }
    }

    /// Return the exact normalized configuration token.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Duplicate => "duplicate",
        }
    }
}

/// The supplied placement token is unsupported by the initial delivery path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnsupportedDeliveryPlacement;

/// Exact portable project-root-relative output root.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct DeliveryOutputRoot {
    spelling: Box<str>,
    path: ExportArtifactPath,
}

impl DeliveryOutputRoot {
    /// Validate slash-separated portable segments without host path rules.
    pub(crate) fn try_new(value: &str) -> Result<Self, DeliveryOutputRootError> {
        if value.is_empty() {
            return Err(DeliveryOutputRootError::InvalidGrammar);
        }

        let segment_count = value.split('/').count();
        if segment_count > OUTPUT_ROOT_SEGMENTS_LIMIT {
            return Err(DeliveryOutputRootError::LimitExceeded {
                counter: DeliveryOutputRootLimit::Segments,
                limit: OUTPUT_ROOT_SEGMENTS_LIMIT as u64,
                observed: usize_to_u64(segment_count),
            });
        }

        if value.starts_with('/')
            || value.ends_with('/')
            || value.contains("//")
            || value.contains('\\')
            || has_windows_drive_prefix(value)
        {
            return Err(DeliveryOutputRootError::InvalidGrammar);
        }

        let mut decoded_bytes = 0_u64;
        let mut segments = Vec::with_capacity(segment_count);
        for segment in value.split('/') {
            let segment_bytes = usize_to_u64(segment.len());
            if segment.len() > OUTPUT_ROOT_SEGMENT_BYTES_LIMIT {
                return Err(DeliveryOutputRootError::LimitExceeded {
                    counter: DeliveryOutputRootLimit::SegmentBytes,
                    limit: OUTPUT_ROOT_SEGMENT_BYTES_LIMIT as u64,
                    observed: segment_bytes,
                });
            }
            decoded_bytes = decoded_bytes.checked_add(segment_bytes).ok_or(
                DeliveryOutputRootError::LimitExceeded {
                    counter: DeliveryOutputRootLimit::DecodedBytes,
                    limit: OUTPUT_ROOT_BYTES_LIMIT,
                    observed: u64::MAX,
                },
            )?;
            if decoded_bytes > OUTPUT_ROOT_BYTES_LIMIT {
                return Err(DeliveryOutputRootError::LimitExceeded {
                    counter: DeliveryOutputRootLimit::DecodedBytes,
                    limit: OUTPUT_ROOT_BYTES_LIMIT,
                    observed: decoded_bytes,
                });
            }
            if !valid_output_segment(segment) {
                return Err(DeliveryOutputRootError::InvalidGrammar);
            }
            segments.push(segment);
        }

        // The common logical-path constructor is the shared scalar authority.
        // The checks above retain delivery-specific limit evidence and make a
        // failure here an invalid target instead of leaking exporter errors.
        let path = ExportArtifactPath::try_new(segments)
            .map_err(|_| DeliveryOutputRootError::InvalidGrammar)?;
        Ok(Self {
            spelling: value.into(),
            path,
        })
    }

    /// Return the exact slash-separated configuration spelling.
    pub(crate) fn as_str(&self) -> &str {
        &self.spelling
    }

    /// Return the exact checked portable segments.
    pub(crate) fn segments(&self) -> impl ExactSizeIterator<Item = &str> {
        self.path
            .segments()
            .iter()
            .map(intlify_export::ExportArtifactPathSegment::as_str)
    }

    fn conflicts_with(&self, other: &Self) -> bool {
        let left = self.path.segments();
        let right = other.path.segments();
        segment_prefix(left, right) || segment_prefix(right, left)
    }
}

/// Fixed output-root counter used by delivery validation evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeliveryOutputRootLimit {
    /// Segments in one output root.
    Segments,
    /// Decoded UTF-8 bytes in one segment.
    SegmentBytes,
    /// Sum of decoded segment bytes in one output root.
    DecodedBytes,
}

/// Failure while checking a portable output root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeliveryOutputRootError {
    /// The value is not a non-empty portable relative segment sequence.
    InvalidGrammar,
    /// One fixed path counter exceeded its inclusive ceiling.
    LimitExceeded {
        counter: DeliveryOutputRootLimit,
        limit: u64,
        observed: u64,
    },
}

/// Canonical production-locale subset loaded eagerly by one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CheckedEagerLocales(Box<[Locale]>);

impl CheckedEagerLocales {
    /// Validate occurrence count, exact locale identity, membership, and uniqueness.
    pub(crate) fn try_new(
        spellings: &[String],
        policy: &LinkPolicy,
    ) -> Result<Self, EagerLocalesError> {
        if spellings.len() > EAGER_LOCALES_LIMIT {
            return Err(EagerLocalesError::CountLimitExceeded {
                limit: EAGER_LOCALES_LIMIT as u64,
                observed: usize_to_u64(spellings.len()),
            });
        }

        let mut locales = Vec::with_capacity(spellings.len());
        let mut first_occurrences = BTreeMap::new();
        for (occurrence, spelling) in spellings.iter().enumerate() {
            if spelling.len() > LOCALE_BYTES_LIMIT {
                return Err(EagerLocalesError::LocaleByteLimitExceeded {
                    occurrence,
                    limit: LOCALE_BYTES_LIMIT as u64,
                    observed: usize_to_u64(spelling.len()),
                });
            }
            let locale = Locale::try_new(spelling.as_str())
                .map_err(|_| EagerLocalesError::InvalidLocale { occurrence })?;
            if policy.production_locales().binary_search(&locale).is_err() {
                return Err(EagerLocalesError::LocaleNotProduction { occurrence, locale });
            }
            if let Some(first_occurrence) = first_occurrences.insert(locale.clone(), occurrence) {
                return Err(EagerLocalesError::DuplicateLocale {
                    occurrence,
                    first_occurrence,
                    locale,
                });
            }
            locales.push(locale);
        }
        locales.sort();
        Ok(Self(locales.into_boxed_slice()))
    }

    /// Return eager locales in exact canonical locale order.
    pub(crate) const fn locales(&self) -> &[Locale] {
        &self.0
    }
}

/// Failure while checking one target's eager-locale set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EagerLocalesError {
    /// The submitted occurrence count exceeds the fixed ceiling.
    CountLimitExceeded { limit: u64, observed: u64 },
    /// One occurrence exceeds the shared locale byte ceiling.
    LocaleByteLimitExceeded {
        occurrence: usize,
        limit: u64,
        observed: u64,
    },
    /// One occurrence is not a valid exact locale identity.
    InvalidLocale { occurrence: usize },
    /// One valid locale is absent from the production set.
    LocaleNotProduction { occurrence: usize, locale: Locale },
    /// One equal locale repeats an earlier occurrence.
    DuplicateLocale {
        occurrence: usize,
        first_occurrence: usize,
        locale: Locale,
    },
}

/// One immutable checked delivery target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedDeliveryTarget {
    name: DeliveryTargetName,
    exporter: BuiltInExporterId,
    out: DeliveryOutputRoot,
    eager_locales: CheckedEagerLocales,
    placement: DeliveryPlacement,
}

impl ResolvedDeliveryTarget {
    /// Return the exact checked target identity.
    pub(crate) const fn name(&self) -> &DeliveryTargetName {
        &self.name
    }

    /// Return the selected built-in exporter identity.
    pub(crate) const fn exporter(&self) -> BuiltInExporterId {
        self.exporter
    }

    /// Return the portable project-root-relative output root.
    pub(crate) const fn out(&self) -> &DeliveryOutputRoot {
        &self.out
    }

    /// Return the canonical eager-locale subset.
    pub(crate) const fn eager_locales(&self) -> &CheckedEagerLocales {
        &self.eager_locales
    }

    /// Return the target-wide placement policy.
    pub(crate) const fn placement(&self) -> DeliveryPlacement {
        self.placement
    }
}

#[derive(Debug)]
struct IndexedDeliveryTarget {
    submitted_index: usize,
    target: ResolvedDeliveryTarget,
}

/// Canonical non-empty checked delivery-target collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedDeliveryTargets(Box<[ResolvedDeliveryTarget]>);

impl ResolvedDeliveryTargets {
    /// Validate and canonicalize one complete submitted target collection.
    pub(crate) fn try_new(
        inputs: &[DeliveryTargetInput<'_>],
        policy: &LinkPolicy,
    ) -> Result<Self, DeliveryTargetsError> {
        if inputs.is_empty() || inputs.len() > DELIVERY_TARGETS_LIMIT {
            return Err(DeliveryTargetsError::TargetCount {
                minimum: 1,
                maximum: DELIVERY_TARGETS_LIMIT as u64,
                observed: usize_to_u64(inputs.len()),
            });
        }

        // Each pass is complete before the next field family starts. This
        // keeps failure selection independent of target declaration order for
        // semantic duplicate and output-conflict checks.
        let names = inputs
            .iter()
            .enumerate()
            .map(|(target_index, input)| {
                DeliveryTargetName::try_new(input.name).map_err(|error| {
                    DeliveryTargetsError::InvalidTargetName {
                        target_index,
                        error,
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut first_names = BTreeMap::new();
        for (target_index, name) in names.iter().enumerate() {
            if let Some(first_target_index) = first_names.insert(name.clone(), target_index) {
                return Err(DeliveryTargetsError::DuplicateTargetName {
                    name: name.clone(),
                    first_target_index,
                    second_target_index: target_index,
                });
            }
        }

        let exporters = inputs
            .iter()
            .enumerate()
            .map(|(target_index, input)| {
                BuiltInExporterId::try_new(input.exporter)
                    .map_err(|_| DeliveryTargetsError::UnsupportedExporter { target_index })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let output_roots = inputs
            .iter()
            .enumerate()
            .map(|(target_index, input)| {
                DeliveryOutputRoot::try_new(input.out).map_err(|error| {
                    DeliveryTargetsError::InvalidOutputRoot {
                        target_index,
                        error,
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let eager_locales = inputs
            .iter()
            .enumerate()
            .map(|(target_index, input)| {
                CheckedEagerLocales::try_new(input.eager_locales, policy).map_err(|error| {
                    DeliveryTargetsError::InvalidEagerLocales {
                        target_index,
                        error,
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let placements = inputs
            .iter()
            .enumerate()
            .map(|(target_index, input)| {
                DeliveryPlacement::try_new(input.placement)
                    .map_err(|_| DeliveryTargetsError::UnsupportedPlacement { target_index })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut targets = names
            .into_iter()
            .zip(exporters)
            .zip(output_roots)
            .zip(eager_locales)
            .zip(placements)
            .enumerate()
            .map(
                |(submitted_index, ((((name, exporter), out), eager_locales), placement))| {
                    IndexedDeliveryTarget {
                        submitted_index,
                        target: ResolvedDeliveryTarget {
                            name,
                            exporter,
                            out,
                            eager_locales,
                            placement,
                        },
                    }
                },
            )
            .collect::<Vec<_>>();
        targets.sort_by(|left, right| left.target.name.cmp(&right.target.name));

        for first_index in 0..targets.len() {
            for second_index in (first_index + 1)..targets.len() {
                let first = &targets[first_index];
                let second = &targets[second_index];
                if first.target.out.conflicts_with(&second.target.out) {
                    return Err(DeliveryTargetsError::OutputRootConflict {
                        first_name: first.target.name.clone(),
                        second_name: second.target.name.clone(),
                        first_target_index: first.submitted_index,
                        second_target_index: second.submitted_index,
                    });
                }
            }
        }

        Ok(Self(
            targets
                .into_iter()
                .map(|indexed| indexed.target)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        ))
    }

    /// Return every target in canonical checked-name order.
    pub(crate) const fn targets(&self) -> &[ResolvedDeliveryTarget] {
        &self.0
    }

    /// Resolve omission to all targets or one exact validated operand.
    pub(crate) fn select(
        &self,
        operand: Option<&str>,
    ) -> Result<SelectedDeliveryTargets<'_>, DeliveryTargetSelectionError> {
        let Some(operand) = operand else {
            return Ok(SelectedDeliveryTargets(&self.0));
        };
        let name = DeliveryTargetName::try_new(operand)
            .map_err(DeliveryTargetSelectionError::InvalidOperand)?;
        let index = self
            .0
            .binary_search_by(|target| target.name.cmp(&name))
            .map_err(|_| DeliveryTargetSelectionError::UnknownTarget(name))?;
        Ok(SelectedDeliveryTargets(&self.0[index..=index]))
    }
}

/// Complete checked-delivery construction failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeliveryTargetsError {
    /// The submitted target count is outside the inclusive configured range.
    TargetCount {
        minimum: u64,
        maximum: u64,
        observed: u64,
    },
    /// One target name violates its checked scalar contract.
    InvalidTargetName {
        target_index: usize,
        error: DeliveryTargetNameError,
    },
    /// One checked name repeats an earlier submitted target.
    DuplicateTargetName {
        name: DeliveryTargetName,
        first_target_index: usize,
        second_target_index: usize,
    },
    /// One target names an exporter outside the closed built-in registry.
    UnsupportedExporter { target_index: usize },
    /// One target output root is malformed or over limit.
    InvalidOutputRoot {
        target_index: usize,
        error: DeliveryOutputRootError,
    },
    /// One target eager-locale set is invalid.
    InvalidEagerLocales {
        target_index: usize,
        error: EagerLocalesError,
    },
    /// One target requests placement other than exact `duplicate`.
    UnsupportedPlacement { target_index: usize },
    /// Two canonical-name-ordered targets own equal or nested output roots.
    OutputRootConflict {
        first_name: DeliveryTargetName,
        second_name: DeliveryTargetName,
        first_target_index: usize,
        second_target_index: usize,
    },
}

/// Borrowed selected-target view in canonical target-name order.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SelectedDeliveryTargets<'a>(&'a [ResolvedDeliveryTarget]);

impl<'a> SelectedDeliveryTargets<'a> {
    /// Return the selected canonical target slice.
    pub(crate) const fn targets(self) -> &'a [ResolvedDeliveryTarget] {
        self.0
    }
}

/// Failure while resolving an optional exact `--target` operand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeliveryTargetSelectionError {
    /// The operand fails the same scalar contract as configured names.
    InvalidOperand(DeliveryTargetNameError),
    /// The valid exact operand is absent from the configured target set.
    UnknownTarget(DeliveryTargetName),
}

fn is_lowercase_ascii_alphanumeric(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit()
}

fn has_windows_drive_prefix(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn valid_output_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && !segment
            .chars()
            .any(|character| character.is_control() || is_bidirectional_control(character))
}

fn is_bidirectional_control(character: char) -> bool {
    matches!(
        character,
        '\u{061C}'
            | '\u{200E}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
    )
}

fn segment_prefix(
    prefix: &[intlify_export::ExportArtifactPathSegment],
    path: &[intlify_export::ExportArtifactPathSegment],
) -> bool {
    prefix.len() <= path.len() && prefix.iter().zip(path).all(|(left, right)| left == right)
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use intlify_contract::{LinkLimits, Locale};
    use intlify_linker::{DynamicReferenceMode, LinkPolicy, PlacementPolicy};

    use super::{
        BuiltInExporterId, CheckedEagerLocales, DeliveryOutputRoot, DeliveryOutputRootError,
        DeliveryOutputRootLimit, DeliveryPlacement, DeliveryTargetInput, DeliveryTargetName,
        DeliveryTargetNameError, DeliveryTargetSelectionError, DeliveryTargetsError,
        EagerLocalesError, ResolvedDeliveryTargets,
    };

    fn locale(value: &str) -> Locale {
        Locale::try_new(value).unwrap()
    }

    fn policy(locales: &[&str]) -> LinkPolicy {
        LinkPolicy::try_new(
            locales.iter().map(|value| locale(value)).collect(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &LinkLimits::default(),
        )
        .unwrap()
    }

    fn input<'a>(
        name: &'a str,
        out: &'a str,
        eager_locales: &'a [String],
    ) -> DeliveryTargetInput<'a> {
        DeliveryTargetInput::new(name, "esm", out, eager_locales, None)
    }

    #[test]
    fn target_names_preserve_exact_ascii_identity_and_bounds() {
        let exact = format!("a{}", "-".repeat(254));
        assert_eq!(DeliveryTargetName::try_new(&exact).unwrap().as_str(), exact);
        assert!(matches!(
            DeliveryTargetName::try_new(&format!("a{}", "-".repeat(255))),
            Err(DeliveryTargetNameError::ByteLimitExceeded {
                limit: 255,
                observed: 256
            })
        ));

        for invalid in [
            "",
            "-web",
            "Web",
            "wé\u{200B}\u{200B}b",
            "web target",
            "web/",
        ] {
            assert_eq!(
                DeliveryTargetName::try_new(invalid),
                Err(DeliveryTargetNameError::InvalidGrammar),
                "{invalid:?}"
            );
        }
        assert_ne!(
            DeliveryTargetName::try_new("web").unwrap(),
            DeliveryTargetName::try_new("web-prod").unwrap()
        );
    }

    #[test]
    fn output_roots_reuse_portable_path_grammar_and_exact_limits() {
        let segment_boundary = "a".repeat(255);
        assert!(DeliveryOutputRoot::try_new(&segment_boundary).is_ok());
        assert!(matches!(
            DeliveryOutputRoot::try_new(&"a".repeat(256)),
            Err(DeliveryOutputRootError::LimitExceeded {
                counter: DeliveryOutputRootLimit::SegmentBytes,
                limit: 255,
                observed: 256
            })
        ));

        let segment_count_boundary = vec!["a"; 64].join("/");
        assert!(DeliveryOutputRoot::try_new(&segment_count_boundary).is_ok());
        let first_segment_over = vec!["a"; 65].join("/");
        assert!(matches!(
            DeliveryOutputRoot::try_new(&first_segment_over),
            Err(DeliveryOutputRootError::LimitExceeded {
                counter: DeliveryOutputRootLimit::Segments,
                limit: 64,
                observed: 65
            })
        ));

        let mut exact_total_segments = vec!["a".repeat(255); 16];
        exact_total_segments.push("a".repeat(16));
        let exact_total = exact_total_segments.join("/");
        let root = DeliveryOutputRoot::try_new(&exact_total).unwrap();
        assert_eq!(root.as_str(), exact_total);
        assert_eq!(root.segments().map(str::len).sum::<usize>(), 4_096);

        *exact_total_segments.last_mut().unwrap() = "a".repeat(17);
        assert!(matches!(
            DeliveryOutputRoot::try_new(&exact_total_segments.join("/")),
            Err(DeliveryOutputRootError::LimitExceeded {
                counter: DeliveryOutputRootLimit::DecodedBytes,
                limit: 4_096,
                observed: 4_097
            })
        ));

        for invalid in [
            "",
            "/dist",
            "dist/",
            "dist//messages",
            ".",
            "dist/..",
            "C:/dist",
            r"\\server\share",
            r"dist\messages",
            "dist/\0messages",
            "dist/\u{0001}messages",
            "dist/\u{202e}messages",
        ] {
            assert_eq!(
                DeliveryOutputRoot::try_new(invalid),
                Err(DeliveryOutputRootError::InvalidGrammar),
                "{invalid:?}"
            );
        }
    }

    #[test]
    fn eager_locales_are_bounded_checked_members_and_canonicalized() {
        let policy = policy(&["ja", "en", "fr"]);
        let spellings = vec!["ja".to_owned(), "en".to_owned()];
        let checked = CheckedEagerLocales::try_new(&spellings, &policy).unwrap();
        assert_eq!(
            checked
                .locales()
                .iter()
                .map(Locale::as_str)
                .collect::<Vec<_>>(),
            ["en", "ja"]
        );
        assert!(CheckedEagerLocales::try_new(&[], &policy)
            .unwrap()
            .locales()
            .is_empty());

        let too_many = vec!["en".to_owned(); 1_025];
        assert!(matches!(
            CheckedEagerLocales::try_new(&too_many, &policy),
            Err(EagerLocalesError::CountLimitExceeded {
                limit: 1_024,
                observed: 1_025
            })
        ));
        assert!(matches!(
            CheckedEagerLocales::try_new(&[String::new()], &policy),
            Err(EagerLocalesError::InvalidLocale { occurrence: 0 })
        ));
        assert!(matches!(
            CheckedEagerLocales::try_new(&["de".to_owned()], &policy),
            Err(EagerLocalesError::LocaleNotProduction { occurrence: 0, .. })
        ));
        assert!(matches!(
            CheckedEagerLocales::try_new(&["en".to_owned(), "en".to_owned()], &policy),
            Err(EagerLocalesError::DuplicateLocale {
                occurrence: 1,
                first_occurrence: 0,
                ..
            })
        ));
    }

    #[test]
    fn eager_locale_maximum_is_accepted_without_deduction() {
        let locale_spellings = (0..1_024)
            .map(|index| format!("locale-{index:04}"))
            .collect::<Vec<_>>();
        let locale_refs = locale_spellings
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let policy = policy(&locale_refs);
        let mut eager = locale_spellings;
        eager.reverse();
        let checked = CheckedEagerLocales::try_new(&eager, &policy).unwrap();
        assert_eq!(checked.locales().len(), 1_024);
        assert_eq!(checked.locales()[0].as_str(), "locale-0000");
        assert_eq!(checked.locales()[1_023].as_str(), "locale-1023");
    }

    #[test]
    fn placement_and_exporter_admit_only_the_initial_exact_tokens() {
        assert_eq!(
            DeliveryPlacement::try_new(None).unwrap(),
            DeliveryPlacement::Duplicate
        );
        assert_eq!(
            DeliveryPlacement::try_new(Some("duplicate")).unwrap(),
            DeliveryPlacement::Duplicate
        );
        assert!(DeliveryPlacement::try_new(Some("hoist")).is_err());
        assert_eq!(DeliveryPlacement::Duplicate.as_str(), "duplicate");

        assert_eq!(
            BuiltInExporterId::try_new("esm").unwrap(),
            BuiltInExporterId::Esm
        );
        assert_eq!(BuiltInExporterId::Esm.as_str(), "esm");
        for invalid in ["", "ESM", " esm", "blob"] {
            assert!(BuiltInExporterId::try_new(invalid).is_err());
        }
    }

    #[test]
    fn target_sets_ignore_declaration_order_and_reject_name_duplicates_first() {
        let policy = policy(&["en"]);
        let eager = Vec::new();
        let first = [
            input("web", "dist/web", &eager),
            input("native", "dist/native", &eager),
        ];
        let second = [
            input("native", "dist/native", &eager),
            input("web", "dist/web", &eager),
        ];
        let first = ResolvedDeliveryTargets::try_new(&first, &policy).unwrap();
        let second = ResolvedDeliveryTargets::try_new(&second, &policy).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            first
                .targets()
                .iter()
                .map(|target| target.name().as_str())
                .collect::<Vec<_>>(),
            ["native", "web"]
        );
        assert_eq!(first.targets()[0].exporter().as_str(), "esm");
        assert_eq!(first.targets()[0].out().as_str(), "dist/native");
        assert!(first.targets()[0].eager_locales().locales().is_empty());
        assert_eq!(first.targets()[0].placement(), DeliveryPlacement::Duplicate);

        let duplicate = [
            DeliveryTargetInput::new("web", "other", "dist/web", &eager, None),
            DeliveryTargetInput::new("web", "esm", "dist/other", &eager, None),
        ];
        assert!(matches!(
            ResolvedDeliveryTargets::try_new(&duplicate, &policy),
            Err(DeliveryTargetsError::DuplicateTargetName {
                first_target_index: 0,
                second_target_index: 1,
                ..
            })
        ));
    }

    #[test]
    fn target_count_preflight_runs_before_target_contents() {
        let policy = policy(&["en"]);
        assert!(matches!(
            ResolvedDeliveryTargets::try_new(&[], &policy),
            Err(DeliveryTargetsError::TargetCount {
                minimum: 1,
                maximum: 64,
                observed: 0
            })
        ));

        let names = (0..65).map(|index| format!("t{index}")).collect::<Vec<_>>();
        let eager = Vec::new();
        let inputs = names
            .iter()
            .map(|name| input(name, "dist", &eager))
            .collect::<Vec<_>>();
        assert!(matches!(
            ResolvedDeliveryTargets::try_new(&inputs, &policy),
            Err(DeliveryTargetsError::TargetCount {
                minimum: 1,
                maximum: 64,
                observed: 65
            })
        ));
    }

    #[test]
    fn output_conflicts_use_canonical_target_pairs_and_original_indexes() {
        let policy = policy(&["en"]);
        let eager = Vec::new();
        let inputs = [
            input("z", "other", &eager),
            input("b", "dist", &eager),
            input("a", "dist/messages", &eager),
        ];
        let error = ResolvedDeliveryTargets::try_new(&inputs, &policy).unwrap_err();
        assert!(matches!(
            error,
            DeliveryTargetsError::OutputRootConflict {
                ref first_name,
                ref second_name,
                first_target_index: 2,
                second_target_index: 1,
            } if first_name.as_str() == "a" && second_name.as_str() == "b"
        ));

        let equal = [input("a", "dist/a", &eager), input("b", "dist/a", &eager)];
        assert!(matches!(
            ResolvedDeliveryTargets::try_new(&equal, &policy),
            Err(DeliveryTargetsError::OutputRootConflict { .. })
        ));

        let lexical_not_structural = [input("a", "dist/a", &eager), input("b", "dist/ab", &eager)];
        assert!(ResolvedDeliveryTargets::try_new(&lexical_not_structural, &policy).is_ok());
    }

    #[test]
    fn selection_validates_one_exact_operand_after_complete_configuration() {
        let policy = policy(&["en"]);
        let eager = Vec::new();
        let targets = ResolvedDeliveryTargets::try_new(
            &[
                input("web", "dist/web", &eager),
                input("native", "dist/native", &eager),
            ],
            &policy,
        )
        .unwrap();

        assert_eq!(
            targets
                .select(None)
                .unwrap()
                .targets()
                .iter()
                .map(|target| target.name().as_str())
                .collect::<Vec<_>>(),
            ["native", "web"]
        );
        assert_eq!(
            targets.select(Some("web")).unwrap().targets()[0]
                .name()
                .as_str(),
            "web"
        );
        assert!(matches!(
            targets.select(Some("Web")),
            Err(DeliveryTargetSelectionError::InvalidOperand(_))
        ));
        assert!(matches!(
            targets.select(Some("mobile")),
            Err(DeliveryTargetSelectionError::UnknownTarget(ref name))
                if name.as_str() == "mobile"
        ));
    }

    #[test]
    fn checked_delivery_values_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<DeliveryTargetName>();
        assert_send_sync::<DeliveryOutputRoot>();
        assert_send_sync::<CheckedEagerLocales>();
        assert_send_sync::<ResolvedDeliveryTargets>();
    }
}
