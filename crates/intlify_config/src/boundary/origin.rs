// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's `safe-origin-kind` boundary: the Safe Origin Kind candidate a binding
//! supplies beside the value of one typed input slot, admitted as the kind
//! that slot fixes or refused with one of the four reasons of its row
//! (D15:718, 3144, 3157; 015-166).
//!
//! Each typed input slot fixes one Safe Origin Kind, so the slot is named by
//! that kind. Only that kind's exact ID is admitted. A candidate that differs
//! from a registered ID only in ASCII case or surrounding ASCII whitespace is
//! non-canonical, whichever ID it varies; another registered ID is
//! incompatible with the slot; anything else is unknown. The candidate is
//! admitted on its own: the value beside it neither skips it nor is skipped
//! by it, and it reads no capability bound.

use crate::resolver::origin::SafeOriginKind;

/// Why the boundary refuses a candidate: one of the four reasons of its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OriginRefusal {
    /// No candidate was supplied.
    Missing,
    /// The candidate is no registered ID, in any spelling.
    Unknown,
    /// The candidate is a registered ID in another ASCII case or with
    /// surrounding ASCII whitespace.
    NonCanonical,
    /// The candidate is a registered ID other than the one the slot fixes.
    EvidenceKindIncompatible,
}

impl OriginRefusal {
    /// The reason as 015's boundary table spells it.
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::Missing => "missing-safe-origin-kind",
            Self::Unknown => "unknown-safe-origin-kind",
            Self::NonCanonical => "non-canonical-safe-origin-kind",
            Self::EvidenceKindIncompatible => "evidence-kind-incompatible",
        }
    }
}

/// Admit the candidate supplied beside the value of the typed input slot that
/// fixes `slot`. An absent candidate is `None`.
pub(crate) fn admit(
    slot: SafeOriginKind,
    candidate: Option<&str>,
) -> Result<SafeOriginKind, OriginRefusal> {
    let candidate = candidate.ok_or(OriginRefusal::Missing)?;
    if candidate == slot.id() {
        return Ok(slot);
    }
    if registered(candidate) {
        return Err(OriginRefusal::EvidenceKindIncompatible);
    }
    // ASCII whitespace is TAB, LF, FF, CR, and SPACE, as `is_ascii_whitespace`
    // has it, and only at either end.
    let spelling = candidate
        .trim_matches(|character: char| character.is_ascii_whitespace())
        .to_ascii_lowercase();
    if registered(&spelling) {
        Err(OriginRefusal::NonCanonical)
    } else {
        Err(OriginRefusal::Unknown)
    }
}

/// Whether `candidate` is exactly one of the ten IDs.
fn registered(candidate: &str) -> bool {
    SafeOriginKind::ALL
        .iter()
        .any(|kind| kind.id() == candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each slot admits exactly the ID it fixes, and no other slot admits it.
    #[test]
    fn a_slot_admits_only_the_id_it_fixes() {
        for slot in SafeOriginKind::ALL {
            assert_eq!(admit(slot, Some(slot.id())), Ok(slot));
            for other in SafeOriginKind::ALL
                .into_iter()
                .filter(|other| *other != slot)
            {
                assert_eq!(
                    admit(slot, Some(other.id())),
                    Err(OriginRefusal::EvidenceKindIncompatible),
                    "{} in {}",
                    other.id(),
                    slot.id()
                );
            }
        }
    }

    #[test]
    fn an_absent_candidate_is_missing() {
        for slot in SafeOriginKind::ALL {
            assert_eq!(admit(slot, None), Err(OriginRefusal::Missing));
        }
    }

    /// Case and whitespace at either end make a registered ID non-canonical,
    /// whether it is the slot's ID or another, since that reason precedes
    /// `evidence-kind-incompatible`.
    #[test]
    fn a_case_or_whitespace_variant_is_non_canonical() {
        let slot = SafeOriginKind::ConfigurationFile;
        for candidate in [
            "Configuration-File",
            "CONFIGURATION-FILE",
            " configuration-file",
            "configuration-file\t",
            "\nconfiguration-file\r",
            "\u{c}configuration-file",
            " \t Configuration-File \r\n",
            "PROFILE-SELECTOR-CLI",
            " implementation-capability",
        ] {
            assert_eq!(
                admit(slot, Some(candidate)),
                Err(OriginRefusal::NonCanonical),
                "{candidate:?}"
            );
        }
    }

    /// Whitespace inside, whitespace outside 015's five characters, a
    /// non-ASCII case variant, and any other spelling are unknown.
    #[test]
    fn any_other_candidate_is_unknown() {
        let slot = SafeOriginKind::ConfigurationFile;
        for candidate in [
            "",
            " ",
            "config-file",
            "configuration -file",
            "configuration_file",
            "\u{b}configuration-file",
            "\u{a0}configuration-file",
            "configuration-fil\u{e9}",
            "conf\u{130}guration-file",
            "configuration-file-",
        ] {
            assert_eq!(
                admit(slot, Some(candidate)),
                Err(OriginRefusal::Unknown),
                "{candidate:?}"
            );
        }
    }

    /// Each refusal spells one of the four reasons of 015's boundary row.
    #[test]
    fn each_refusal_has_its_reason() {
        for (refusal, reason) in [
            (OriginRefusal::Missing, "missing-safe-origin-kind"),
            (OriginRefusal::Unknown, "unknown-safe-origin-kind"),
            (
                OriginRefusal::NonCanonical,
                "non-canonical-safe-origin-kind",
            ),
            (
                OriginRefusal::EvidenceKindIncompatible,
                "evidence-kind-incompatible",
            ),
        ] {
            assert_eq!(refusal.reason(), reason);
        }
    }
}
