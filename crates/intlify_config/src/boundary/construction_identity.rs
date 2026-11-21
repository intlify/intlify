// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's `resolver-construction-identity-reference` boundary: the Resolver
//! Construction Identity an invocation names, admitted as the identity the
//! resolver was constructed with or refused with one of the three reasons of
//! its row (D15:1106, 3151).
//!
//! The reference is the identity's text, `rci0_` and the unpadded base64url
//! of its 32 bytes. The boundary's checks run in 015's order: a missing
//! reference, then a text that is not a canonical identity, which includes a
//! last character with non-zero unused bits, and last the comparison with the
//! constructed identity, so a malformed text is never compared (D15:987,
//! 3155). The boundary reads no capability bound and fixes no Safe Origin
//! Kind. The admitted reference is the identity every outcome of the
//! invocation carries; no phase changes it (D15:1150, 2147).

use crate::base64url;
use crate::construction::identity::ConstructionIdentity;

/// Why the boundary refuses a reference: one of the three reasons of its row,
/// in that row's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdentityRefusal {
    /// No reference was supplied.
    Missing,
    /// The text is not the canonical presentation of any identity.
    Malformed,
    /// A canonical identity other than the constructed one.
    Mismatch,
}

impl IdentityRefusal {
    /// The reason as 015's boundary table spells it.
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::Missing => "missing-construction-identity",
            Self::Malformed => "malformed-construction-identity",
            Self::Mismatch => "construction-identity-mismatch",
        }
    }
}

/// Admit `reference`, `None` when absent, against the identity `constructed`
/// of the resolver the invocation runs on: that identity, or the boundary's
/// first failing check.
pub(crate) fn admit(
    reference: Option<&str>,
    constructed: &ConstructionIdentity,
) -> Result<ConstructionIdentity, IdentityRefusal> {
    let text = reference.ok_or(IdentityRefusal::Missing)?;
    let bytes =
        base64url::parse_construction_identity(text).map_err(|_| IdentityRefusal::Malformed)?;
    if &bytes != constructed.bytes() {
        return Err(IdentityRefusal::Mismatch);
    }
    Ok(*constructed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base64url::{encode, CONSTRUCTION_IDENTITY_PREFIX};
    use crate::resolver::outcome::tests::identity;

    /// The text after the prefix of the constructed identity.
    fn digest_text() -> String {
        identity().text()[CONSTRUCTION_IDENTITY_PREFIX.len()..].to_owned()
    }

    /// The constructed identity's bytes with one bit flipped, as text.
    fn other_identity() -> String {
        let mut bytes = *identity().bytes();
        bytes[31] ^= 0x01;
        format!("{CONSTRUCTION_IDENTITY_PREFIX}{}", encode(&bytes))
    }

    /// The constructed identity's text with the lowest unused bit of its last
    /// character set. The last of the 43 characters carries four bits of the
    /// 32nd byte above two unused bits, so its value is a multiple of four and
    /// the next character of the alphabet, which is the next ASCII character
    /// for every such value, sets that bit.
    fn with_an_unused_bit() -> String {
        let mut text = identity().text().into_bytes();
        *text.last_mut().unwrap() += 1;
        String::from_utf8(text).unwrap()
    }

    #[test]
    fn the_constructed_identity_is_admitted() {
        let constructed = identity();
        assert_eq!(
            admit(Some(&constructed.text()), &constructed),
            Ok(constructed)
        );
    }

    #[test]
    fn an_absent_reference_is_missing() {
        assert_eq!(admit(None, &identity()), Err(IdentityRefusal::Missing));
    }

    #[test]
    fn a_text_that_presents_no_identity_is_malformed() {
        let digest = digest_text();
        let prefix = CONSTRUCTION_IDENTITY_PREFIX;
        for text in [
            String::new(),
            digest.clone(),
            format!("rci1_{digest}"),
            format!("RCI0_{digest}"),
            format!(" {prefix}{digest}"),
            format!("{prefix}{}", &digest[..42]),
            format!("{prefix}{digest}A"),
            format!("{prefix}{digest}="),
            format!("{prefix}{}+", &digest[..42]),
            with_an_unused_bit(),
        ] {
            assert_eq!(
                admit(Some(&text), &identity()),
                Err(IdentityRefusal::Malformed),
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_malformed_text_is_refused_before_it_is_compared() {
        // Read leniently, the text with an unused bit set would present the
        // constructed identity itself.
        let text = with_an_unused_bit();
        assert_ne!(text, identity().text());
        assert_eq!(
            admit(Some(&text), &identity()),
            Err(IdentityRefusal::Malformed)
        );
    }

    #[test]
    fn another_canonical_identity_is_a_mismatch() {
        assert_eq!(
            admit(Some(&other_identity()), &identity()),
            Err(IdentityRefusal::Mismatch)
        );
    }

    #[test]
    fn each_refusal_has_its_reason() {
        for (refusal, reason) in [
            (IdentityRefusal::Missing, "missing-construction-identity"),
            (
                IdentityRefusal::Malformed,
                "malformed-construction-identity",
            ),
            (IdentityRefusal::Mismatch, "construction-identity-mismatch"),
        ] {
            assert_eq!(refusal.reason(), reason);
        }
    }
}
