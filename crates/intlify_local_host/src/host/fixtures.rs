// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Unit-test fixtures for the host: a host over the identity crate's
//! committed chain, and randomness that gives exactly the values that chain
//! was allocated with.

use intlify_authoring::{MessageIntentId, SourceSnapshot, Token, UnitResult};
use intlify_authoring_identity::{
    ExplicitDecision, LineageKind, LineageLink, RetainedSources, SourceEdit,
};

use super::memory::{Acquisition, HostLimits, HostSetup, MemoryHost};
use super::outcome::Preparation;
use super::random::scripted::Scripted;
use super::session::PrepareInputs;
use crate::authority::fixtures::{
    basis, id_of, identity_limits, limits, owner, restore, sources, the_edit, Chain, REGISTRY,
};
use crate::authority::{Action, Principal, UpdateMode};

/// Every action.
pub(crate) const ALL: &[Action] = &Action::ALL;

pub(crate) fn alice() -> Principal {
    Principal::new("alice").expect("a principal")
}

/// The 16 bytes a 32-digit hexadecimal value spells.
pub(crate) fn bytes(hex: &str) -> [u8; 16] {
    let mut bytes = [0; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).expect("hexadecimal");
    }
    bytes
}

pub(crate) fn bytes_of(id: &MessageIntentId) -> [u8; 16] {
    bytes(id.value().as_str())
}

/// The values the committed chain was allocated with, in drawing order: the
/// registry identity, update 1's three IDs in canonical declaration order
/// (pay, cancel, home) and update 2's one.
pub(crate) fn committed_randomness() -> Scripted {
    Scripted::new([
        bytes(REGISTRY),
        bytes_of(&id_of(1, 0)),
        bytes_of(&id_of(1, 2)),
        bytes_of(&id_of(1, 1)),
        bytes_of(&id_of(2, 2)),
    ])
}

pub(crate) fn host_limits() -> HostLimits {
    HostLimits {
        identity: identity_limits(),
        authority: limits(),
        generations: 16,
    }
}

/// A host for the committed chain's owner, not yet enrolled.
pub(crate) fn host_with(randomness: Scripted, limits: HostLimits) -> MemoryHost<Scripted> {
    MemoryHost::new(
        HostSetup {
            binding: "storefront-registry",
            owner: owner(),
            scope: "storefront-web",
            context: basis(),
            limits,
        },
        randomness,
    )
    .expect("a host")
}

/// A host enrolled for the committed chain's owner, with alice holding
/// `actions`.
pub(crate) fn enrolled(randomness: Scripted, actions: &[Action]) -> MemoryHost<Scripted> {
    let host = host_with(randomness, host_limits());
    host.enroll().expect("a new owner");
    host.set_grants(vec![(alice(), actions.to_vec())])
        .expect("grants");
    host
}

/// The acquisition of committed inventory `n`'s units.
pub(crate) fn acquisition(chain: &Chain, n: usize) -> Acquisition {
    Acquisition {
        sources: chain
            .inventory(n)
            .inventory()
            .units()
            .iter()
            .map(UnitResult::source)
            .cloned()
            .collect(),
        pins: vec![],
    }
}

/// Both units of the chain's owning scope.
pub(crate) fn membership() -> [Token; 2] {
    [
        Token::new("checkout").expect("a unit"),
        Token::new("nav").expect("a unit"),
    ]
}

/// Retained bytes for every committed source.
pub(crate) fn retained(owned: &[(SourceSnapshot, String)]) -> RetainedSources<'_> {
    RetainedSources::new(
        owned
            .iter()
            .map(|(snapshot, text)| (snapshot.clone(), text.as_bytes())),
    )
    .expect("retained sources")
}

/// Inputs with nothing but the given edits and explicit decisions.
pub(crate) fn inputs<'a>(
    sources: &'a RetainedSources<'a>,
    membership: &'a [Token],
    edits: &'a [SourceEdit],
    explicit: &'a [ExplicitDecision],
) -> PrepareInputs<'a> {
    PrepareInputs {
        sources,
        edits,
        membership,
        explicit,
        links: &[],
    }
}

/// What committed update `n` was planned with besides the chain's own
/// evidence: its edit, update 3's explicit restore, and update 2's copy link,
/// which is the host's and names the ID that update allocates.
pub(crate) struct Step {
    pub(crate) edits: Vec<SourceEdit>,
    pub(crate) explicit: Vec<ExplicitDecision>,
    pub(crate) links: Vec<LineageLink>,
}

impl Step {
    pub(crate) fn of(chain: &Chain, n: usize) -> Self {
        Self {
            edits: if n > 1 { vec![the_edit(n)] } else { vec![] },
            explicit: if n == 3 { vec![restore(chain)] } else { vec![] },
            links: if n == 2 {
                vec![LineageLink::new(
                    LineageKind::Copy,
                    vec![id_of(2, 0)],
                    vec![id_of(2, 2)],
                )]
            } else {
                vec![]
            },
        }
    }

    pub(crate) fn inputs<'a>(
        &'a self,
        sources: &'a RetainedSources<'a>,
        membership: &'a [Token],
    ) -> PrepareInputs<'a> {
        PrepareInputs {
            sources,
            edits: &self.edits,
            membership,
            explicit: &self.explicit,
            links: &self.links,
        }
    }
}

/// Make committed registry `n` current through the host itself: initialize,
/// then publish updates 1 to `n` as the chain made them, confirming update
/// 3's restore.
pub(crate) fn advance(host: &MemoryHost<Scripted>, chain: &Chain, n: usize) {
    host.open(&alice(), Acquisition::default())
        .expect("a session")
        .initialize()
        .expect("a genesis");
    let owned = sources();
    let retained = retained(&owned);
    let membership = membership();
    for n in 1..=n {
        let session = host
            .open(&alice(), acquisition(chain, n))
            .expect("a session");
        let step = Step::of(chain, n);
        let confirmations: Vec<_> = step
            .explicit
            .iter()
            .map(|decision| {
                session
                    .confirm(chain.inventory(n), decision)
                    .expect("a confirmation")
            })
            .collect();
        let Preparation::Prepared(prepared) = session
            .prepare(chain.inventory(n), &step.inputs(&retained, &membership))
            .expect("a preparation")
        else {
            panic!("update {n} is planned");
        };
        session
            .publish(&prepared, &confirmations, UpdateMode::Manual)
            .expect("a publication");
    }
}
