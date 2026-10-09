// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! What admitting an identity artifact can fail on, whatever its kind.
//!
//! Every kind this crate owns is read the same way: design 017's shared read
//! (bounded strict decoding, the exact kind, schema and specification, the
//! closed body, the integrity digest), then this reader's bounds, then the
//! body's structural rules. Only the structural rules differ from kind to
//! kind, so the failure is one type over them.

use intlify_authoring::ReadFailure;

use crate::limits::IdentityLimitKind;

/// Why an identity artifact was not admitted.
///
/// `F` is the structural failure of the requested kind, so admitting one kind
/// never reports another kind's rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityAdmissionFailure<F> {
    /// The bytes are not one sealed artifact of the requested kind.
    Read(ReadFailure),
    /// A named bound was exhausted before the body was checked.
    Limit(IdentityLimitKind),
    /// The body breaks a structural rule of its kind.
    Structure(F),
}

/// Refuse a count over its inclusive bound, naming the bound.
pub(crate) fn within<F>(
    count: usize,
    bound: u64,
    kind: IdentityLimitKind,
) -> Result<(), IdentityAdmissionFailure<F>> {
    if count as u64 > bound {
        return Err(IdentityAdmissionFailure::Limit(kind));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_count_at_its_bound_is_within_it_and_one_more_is_not() {
        let at: Result<(), IdentityAdmissionFailure<()>> = within(3, 3, IdentityLimitKind::Entries);
        assert_eq!(at, Ok(()));
        let over: Result<(), IdentityAdmissionFailure<()>> =
            within(4, 3, IdentityLimitKind::Entries);
        assert_eq!(
            over,
            Err(IdentityAdmissionFailure::Limit(IdentityLimitKind::Entries))
        );
        // A zero bound admits nothing but an empty collection.
        assert_eq!(within::<()>(0, 0, IdentityLimitKind::Decisions), Ok(()));
    }
}
