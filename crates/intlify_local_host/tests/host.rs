// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 029's registry operations, and design 018's rows that need host
//! state, against the in-memory host.
//!
//! The host carries the identity crate's committed chain from a genesis to
//! its third update and gives back the committed artifacts, given the values
//! the chain was allocated with. The other cases are the conflicts and
//! refusals 029 and 018 require at the point of publication: two updates
//! from one base, two initializations, a change of authority or session, a
//! binding whose control state is lost, a pinned earlier registry, and an
//! empty plan. Every plan is reconciled and every request authorized by the
//! real code; the in-memory host proves only its own model, never disk
//! durability.

mod support;

use std::thread;

use intlify_authoring::{AdmittedInventory, OwnerKind, Token, UnitResult};
use intlify_authoring_identity::{
    EntryState, ExplicitDecision, IdentityDecision, LineageKind, LineageLink, RetainedSources,
    SourceEdit,
};
use intlify_local_host::host::{
    Acquisition, BindingState, Blocked, Conflict, Currency, Durability, HostFailure, HostLimits,
    HostSetup, MemoryHost, Operation, OsRandomness, Preparation, PrepareInputs, PreparedUpdate,
    Randomness, RandomnessFailure, Session,
};
use intlify_local_host::{Action, AuthorizationFailure, UpdateMode};
use support::{
    artifact, basis, id_of, identity_limits, limits, owner, partial_view, principal, restore,
    sources, the_edit, Chain, REGISTRY,
};

const ALL: &[Action] = &Action::ALL;

/// Values given in advance, then a failure.
struct Scripted {
    values: Vec<[u8; 16]>,
    drawn: usize,
}

impl Scripted {
    fn new(values: impl IntoIterator<Item = [u8; 16]>) -> Self {
        Self {
            values: values.into_iter().collect(),
            drawn: 0,
        }
    }
}

impl Randomness for Scripted {
    fn fill(&mut self, bytes: &mut [u8; 16]) -> Result<(), RandomnessFailure> {
        let value = self.values.get(self.drawn).ok_or(RandomnessFailure)?;
        *bytes = *value;
        self.drawn += 1;
        Ok(())
    }
}

fn bytes(hex: &str) -> [u8; 16] {
    let mut bytes = [0; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap();
    }
    bytes
}

/// The values the committed chain was allocated with, in drawing order.
fn committed_values() -> Vec<[u8; 16]> {
    vec![
        bytes(REGISTRY),
        bytes(id_of(1, 0).value().as_str()),
        bytes(id_of(1, 2).value().as_str()),
        bytes(id_of(1, 1).value().as_str()),
        bytes(id_of(2, 2).value().as_str()),
    ]
}

fn host_limits() -> HostLimits {
    HostLimits {
        identity: identity_limits(),
        authority: limits(),
        generations: 16,
    }
}

/// An enrolled host for the chain's owner with alice holding every action.
fn host<R: Randomness>(randomness: R) -> MemoryHost<R> {
    let host = MemoryHost::new(
        HostSetup {
            binding: "storefront-registry",
            owner: owner(),
            scope: "storefront-web",
            context: basis(),
            limits: host_limits(),
        },
        randomness,
    )
    .unwrap();
    host.enroll().unwrap();
    host.set_grants(vec![(principal("alice"), ALL.to_vec())])
        .unwrap();
    host
}

fn acquired(inventory: &AdmittedInventory) -> Acquisition {
    Acquisition {
        sources: inventory
            .inventory()
            .units()
            .iter()
            .map(UnitResult::source)
            .cloned()
            .collect(),
        pins: vec![],
    }
}

fn open<'h, R: Randomness>(
    host: &'h MemoryHost<R>,
    inventory: Option<&AdmittedInventory>,
) -> Session<'h, R> {
    host.open(
        &principal("alice"),
        inventory.map(acquired).unwrap_or_default(),
    )
    .unwrap()
}

/// What committed update `n` was planned with besides the chain's own
/// evidence. Update 2's copy link is the host's: it names the ID update 2
/// allocates, which the scripted values make known in advance.
struct Step {
    edits: Vec<SourceEdit>,
    explicit: Vec<ExplicitDecision>,
    links: Vec<LineageLink>,
}

impl Step {
    fn of(chain: &Chain, n: usize) -> Self {
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

    fn without_links(chain: &Chain, n: usize) -> Self {
        Self {
            links: vec![],
            ..Self::of(chain, n)
        }
    }

    fn inputs<'a>(
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

fn membership() -> [Token; 2] {
    [Token::new("checkout").unwrap(), Token::new("nav").unwrap()]
}

fn prepare<R: Randomness>(
    session: &Session<'_, R>,
    inventory: &AdmittedInventory,
    step: &Step,
) -> Result<Preparation, HostFailure> {
    let owned = sources();
    let retained = RetainedSources::new(
        owned
            .iter()
            .map(|(snapshot, text)| (snapshot.clone(), text.as_bytes())),
    )
    .unwrap();
    let membership = membership();
    session.prepare(inventory, &step.inputs(&retained, &membership))
}

fn prepared(preparation: Result<Preparation, HostFailure>) -> PreparedUpdate {
    match preparation {
        Ok(Preparation::Prepared(prepared)) => *prepared,
        other => panic!("not prepared: {other:?}"),
    }
}

/// Initialize, then publish committed updates 1 to `n` as the chain made
/// them.
fn advance<R: Randomness>(host: &MemoryHost<R>, chain: &Chain, n: usize) {
    open(host, None).initialize().unwrap();
    for n in 1..=n {
        let session = open(host, Some(chain.inventory(n)));
        let step = Step::of(chain, n);
        let confirmations: Vec<_> = step
            .explicit
            .iter()
            .map(|decision| session.confirm(chain.inventory(n), decision).unwrap())
            .collect();
        let update = prepared(prepare(&session, chain.inventory(n), &step));
        session
            .publish(&update, &confirmations, UpdateMode::Manual)
            .unwrap();
    }
}

#[test]
fn the_host_carries_the_committed_chain_from_its_genesis() {
    let chain = Chain::load();
    let host = host(Scripted::new(committed_values()));
    assert_eq!(host.binding_state(), BindingState::Uninitialized);
    let genesis = open(&host, None).initialize().unwrap();
    assert_eq!(genesis.durability(), Durability::InMemory);
    assert_eq!(genesis.record().operation(), Operation::Initialize);
    for n in 1..=3 {
        let session = open(&host, Some(chain.inventory(n)));
        let step = Step::of(&chain, n);
        // The restore is confirmed by the host before it is planned.
        let confirmations: Vec<_> = step
            .explicit
            .iter()
            .map(|decision| session.confirm(chain.inventory(n), decision).unwrap())
            .collect();
        let update = prepared(prepare(&session, chain.inventory(n), &step));
        assert_eq!(
            update.plan().update().reference(),
            chain.update(n).reference()
        );
        assert_eq!(update.plan().requires_confirmation(), n == 3);
        let published = session
            .publish(&update, &confirmations, UpdateMode::Manual)
            .unwrap();
        assert_eq!(published.record().result(), &chain.registry(n).reference());
    }
    assert_eq!(host.binding_state(), BindingState::Active);

    // Every registry the host made current is the committed artifact.
    let reader = host
        .open(
            &principal("alice"),
            Acquisition {
                sources: vec![],
                pins: (0..=3).map(|n| chain.registry(n).reference()).collect(),
            },
        )
        .unwrap();
    for n in 0..=3 {
        let read = reader.read(Some(&chain.registry(n).reference())).unwrap();
        assert_eq!(
            serde_json::to_value(read.registry().artifact()).unwrap(),
            artifact(&format!("registry-{n}"))
        );
        let currency = if n == 3 {
            Currency::Current
        } else {
            Currency::Historical
        };
        assert_eq!(read.currency(), currency);
    }

    // The provenance agrees with the state: each record names the base it
    // was published on and the registry it made current, in order.
    let records = host.records().unwrap();
    assert_eq!(records.len(), 4);
    for pair in records.windows(2) {
        assert_eq!(pair[1].base(), Some(pair[0].result()));
        assert!(pair[1].generation() > pair[0].generation());
    }
    assert_eq!(
        records[3].confirmations(),
        [(principal("alice"), id_of(3, 1))]
    );
}

#[test]
fn of_two_updates_prepared_from_one_base_at_most_one_is_published() {
    let chain = Chain::load();
    let mut values = committed_values();
    values.push([0x7e; 16]);
    let host = host(Scripted::new(values));
    advance(&host, &chain, 1);
    let first = open(&host, Some(chain.inventory(2)));
    let second = open(&host, Some(chain.inventory(2)));
    let one = prepared(prepare(&first, chain.inventory(2), &Step::of(&chain, 2)));
    // The second draws an ID of its own, so it carries no copy link.
    let other = prepared(prepare(
        &second,
        chain.inventory(2),
        &Step::without_links(&chain, 2),
    ));
    let outcomes = thread::scope(|scope| {
        let one = scope.spawn(|| first.publish(&one, &[], UpdateMode::Manual));
        let other = scope.spawn(|| second.publish(&other, &[], UpdateMode::Manual));
        [one.join().unwrap(), other.join().unwrap()]
    });
    let published = outcomes.iter().filter(|outcome| outcome.is_ok()).count();
    assert_eq!(published, 1);
    assert!(outcomes
        .iter()
        .any(|outcome| outcome == &Err(HostFailure::Conflict(Conflict::StaleBase))));
    assert_eq!(host.records().unwrap().len(), 3);
}

#[test]
fn of_two_initializations_exactly_one_succeeds() {
    let host = host(Scripted::new([[0x01; 16], [0x02; 16]]));
    let first = open(&host, None);
    let second = open(&host, None);
    let outcomes = thread::scope(|scope| {
        let one = scope.spawn(|| first.initialize());
        let other = scope.spawn(|| second.initialize());
        [one.join().unwrap(), other.join().unwrap()]
    });
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert!(outcomes
        .iter()
        .any(|outcome| outcome == &Err(HostFailure::Conflict(Conflict::AlreadyInitialized))));
    assert_eq!(host.records().unwrap().len(), 1);
}

#[test]
fn a_change_of_authority_or_session_before_the_commit_refuses_the_write() {
    let chain = Chain::load();
    // Update 1 is prepared three times, each drawing its own IDs.
    let mut values = vec![bytes(REGISTRY)];
    values.extend((1..=9).map(|n| [n; 16]));
    let host = host(Scripted::new(values));
    advance(&host, &chain, 0);

    // The grants change, even back to the same grants.
    let session = open(&host, Some(chain.inventory(1)));
    let update = prepared(prepare(&session, chain.inventory(1), &Step::of(&chain, 1)));
    host.set_grants(vec![(principal("alice"), ALL.to_vec())])
        .unwrap();
    assert_eq!(
        session.publish(&update, &[], UpdateMode::Manual),
        Err(HostFailure::Conflict(Conflict::AuthorityChanged))
    );

    // A development session ends before the automatic update commits.
    host.start_development_session().unwrap();
    let session = open(&host, Some(chain.inventory(1)));
    let update = prepared(prepare(&session, chain.inventory(1), &Step::of(&chain, 1)));
    host.end_development_session().unwrap();
    assert_eq!(
        session.publish(&update, &[], UpdateMode::Development),
        Err(HostFailure::Conflict(Conflict::AuthorityChanged))
    );

    // Nothing became current; a fresh session publishes.
    assert_eq!(host.records().unwrap().len(), 1);
    let session = open(&host, Some(chain.inventory(1)));
    let update = prepared(prepare(&session, chain.inventory(1), &Step::of(&chain, 1)));
    assert!(session.publish(&update, &[], UpdateMode::Manual).is_ok());
}

#[test]
fn development_updates_need_an_active_session_and_only_proven_decisions() {
    let chain = Chain::load();
    let host = host(Scripted::new(committed_values()));
    advance(&host, &chain, 2);
    host.start_development_session().unwrap();
    // Update 3 holds an explicit restore: manual work, confirmed or not.
    let session = open(&host, Some(chain.inventory(3)));
    let confirmation = session
        .confirm(chain.inventory(3), &restore(&chain))
        .unwrap();
    let update = prepared(prepare(&session, chain.inventory(3), &Step::of(&chain, 3)));
    assert_eq!(
        session.publish(
            &update,
            std::slice::from_ref(&confirmation),
            UpdateMode::Development
        ),
        Err(HostFailure::Denied(AuthorizationFailure::NotAutomatic))
    );
    let published = session
        .publish(&update, &[confirmation], UpdateMode::Manual)
        .unwrap();
    assert_eq!(
        published.record().operation(),
        Operation::Update(UpdateMode::Manual)
    );
}

#[test]
fn a_binding_whose_control_state_is_lost_is_never_new_again() {
    let chain = Chain::load();
    let host = host(Scripted::new(committed_values()));
    advance(&host, &chain, 1);
    let session = open(&host, Some(chain.inventory(2)));
    let update = prepared(prepare(&session, chain.inventory(2), &Step::of(&chain, 2)));
    host.lose_control_state();
    assert_eq!(host.binding_state(), BindingState::Unavailable);
    // No write lands, no session opens, and no enrollment starts over.
    assert_eq!(
        session.publish(&update, &[], UpdateMode::Manual),
        Err(HostFailure::Blocked(Blocked::Unavailable))
    );
    assert_eq!(
        host.open(&principal("alice"), Acquisition::default()).err(),
        Some(HostFailure::Blocked(Blocked::Unavailable))
    );
    assert_eq!(
        host.enroll(),
        Err(HostFailure::Blocked(Blocked::Unavailable))
    );
}

#[test]
fn a_pinned_earlier_registry_is_read_and_never_treated_as_current() {
    let chain = Chain::load();
    let mut values = committed_values();
    values.push([0x5a; 16]);
    let host = host(Scripted::new(values));
    advance(&host, &chain, 1);
    // Registry 2 is published while a plan made against registry 1 is
    // kept, drawing the value after update 2's.
    let session = open(&host, Some(chain.inventory(2)));
    let update = prepared(prepare(&session, chain.inventory(2), &Step::of(&chain, 2)));
    let stale = open(&host, Some(chain.inventory(2)));
    let old = prepared(prepare(
        &stale,
        chain.inventory(2),
        &Step::without_links(&chain, 2),
    ));
    session.publish(&update, &[], UpdateMode::Manual).unwrap();

    let reader = host
        .open(
            &principal("alice"),
            Acquisition {
                sources: vec![],
                pins: vec![chain.registry(1).reference()],
            },
        )
        .unwrap();
    let earlier = reader.read(Some(&chain.registry(1).reference())).unwrap();
    assert_eq!(earlier.currency(), Currency::Historical);
    assert_eq!(earlier.registry(), chain.registry(1));
    // Publishing on it, through any session, finds it is not current.
    assert_eq!(
        stale.publish(&old, &[], UpdateMode::Manual),
        Err(HostFailure::Conflict(Conflict::StaleBase))
    );
    let fresh = open(&host, Some(chain.inventory(2)));
    assert_eq!(
        fresh.publish(&old, &[], UpdateMode::Manual),
        Err(HostFailure::Conflict(Conflict::StaleBase))
    );
}

#[test]
fn reads_analyses_and_empty_plans_publish_nothing() {
    let chain = Chain::load();
    let host = host(Scripted::new(committed_values()));
    advance(&host, &chain, 1);
    let session = open(&host, Some(chain.inventory(1)));
    let owned = sources();
    let retained = RetainedSources::new(
        owned
            .iter()
            .map(|(snapshot, text)| (snapshot.clone(), text.as_bytes())),
    )
    .unwrap();
    session.read(None).unwrap();
    session.analyze(chain.inventory(1), &retained, &[]).unwrap();
    assert_eq!(
        prepare(&session, chain.inventory(1), &Step::of(&chain, 1)),
        Ok(Preparation::Unchanged)
    );
    assert_eq!(host.records().unwrap().len(), 2);
    // The next draw is still the one update 2 was allocated with.
    let session = open(&host, Some(chain.inventory(2)));
    let update = prepared(prepare(&session, chain.inventory(2), &Step::of(&chain, 2)));
    assert_eq!(
        update.plan().update().reference(),
        chain.update(2).reference()
    );
}

#[test]
fn a_partial_view_keeps_what_it_does_not_see() {
    let chain = Chain::load();
    let host = host(Scripted::new(committed_values()));
    advance(&host, &chain, 1);
    // Checkout alone: cancel is gone from it, and home is not in it.
    let view = partial_view(2, &["checkout"]);
    let session = open(&host, Some(&view));
    let update = prepared(prepare(&session, &view, &Step::without_links(&chain, 2)));
    assert!(update
        .plan()
        .update()
        .update()
        .decisions()
        .iter()
        .all(|decision| !matches!(decision, IdentityDecision::Retire(_))));
    session.publish(&update, &[], UpdateMode::Manual).unwrap();
    let reader = open(&host, None);
    let current = reader.read(None).unwrap();
    for (n, k) in [(2, 1), (1, 1)] {
        let entry = current.registry().snapshot().entry(&id_of(n, k)).unwrap();
        assert_eq!(entry.state(), EntryState::Active);
        assert_eq!(
            Some(entry),
            chain.registry(1).snapshot().entry(&id_of(n, k))
        );
    }
}

#[test]
fn a_failed_draw_allocates_nothing_and_nothing_stands_in() {
    let chain = Chain::load();
    // The identity, then nothing for update 1's three new declarations.
    let host = host(Scripted::new([bytes(REGISTRY)]));
    advance(&host, &chain, 0);
    let session = open(&host, Some(chain.inventory(1)));
    assert_eq!(
        prepare(&session, chain.inventory(1), &Step::of(&chain, 1)),
        Err(HostFailure::Randomness(RandomnessFailure))
    );
    assert_eq!(host.records().unwrap().len(), 1);
}

#[test]
fn operating_system_randomness_gives_each_chain_its_own_identity() {
    let chain = Chain::load();
    let one = host(OsRandomness);
    let other = host(OsRandomness);
    open(&one, None).initialize().unwrap();
    open(&other, None).initialize().unwrap();
    let identity = |host: &MemoryHost<OsRandomness>| {
        open(host, None)
            .read(None)
            .unwrap()
            .registry()
            .snapshot()
            .registry_identity()
            .clone()
    };
    assert_ne!(identity(&one), identity(&other));
    assert_ne!(
        &identity(&one),
        chain.registry(0).snapshot().registry_identity()
    );
    // And fresh IDs for update 1, none of them the committed ones.
    let session = open(&one, Some(chain.inventory(1)));
    let update = prepared(prepare(&session, chain.inventory(1), &Step::of(&chain, 1)));
    let allocated: Vec<_> = update
        .plan()
        .update()
        .update()
        .decisions()
        .iter()
        .map(|decision| decision.intent_id().clone())
        .collect();
    assert_eq!(allocated.len(), 3);
    assert!(allocated
        .iter()
        .all(|id| id.owner().kind() == OwnerKind::Application
            && ![id_of(1, 0), id_of(1, 1), id_of(1, 2)].contains(id)));
}

#[test]
fn a_caller_the_host_did_not_establish_opens_nothing() {
    let host = host(Scripted::new(committed_values()));
    assert_eq!(
        host.open(&principal("mallory"), Acquisition::default())
            .err(),
        Some(HostFailure::Denied(AuthorizationFailure::UnknownCaller))
    );
}
