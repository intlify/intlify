// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The host state, held in memory.
//!
//! One `MemoryHost` is one host domain with one owner binding: its state
//! (not enrolled, uninitialized, active or unavailable), the chain of
//! registries it published with their provenance, and the authority state,
//! meaning the grants and whether a development session is active. Every
//! read and write of that state takes one guard, and a write checks the
//! state and authorizes the request while it holds it, so nothing can change
//! between the final check and the point of publication.
//!
//! Nothing here is durable. A publication lives as long as the host value
//! does, and proves nothing about a disk.

use std::sync::{Arc, Mutex, MutexGuard};

use intlify_authoring::{
    AdmittedInventory, AuthoringArtifact, AuthoringArtifactReference, AuthoringBasis,
    OwnerIdentity, PrimitiveError, SourceSnapshot,
};
use intlify_authoring_identity::{
    admit_registry, AdmittedRegistry, AdmittedUpdate, IdentityLimits, IntentRegistrySnapshot,
    PreviousUpdate, RegistryArtifact,
};

use super::outcome::{
    Blocked, Conflict, Durability, HostFailure, Operation, Publication, PublicationRecord,
};
use super::random::{self, Randomness};
use super::session::Session;
use crate::authority::{
    Action, AuthorityLimits, Destination, Establishment, LocalAuthority, Principal, UpdateRequest,
};

/// Where one owner binding stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingState {
    /// No owner binding is enrolled. Only explicit enrollment creates one.
    NotEnrolled,
    /// An enrolled new owner with no registry chain yet. Explicit
    /// initialization may give it one.
    Uninitialized,
    /// One registry chain, whose current snapshot reads and updates use.
    Active,
    /// The control state cannot be validated. Nothing is written, and the
    /// state is never read as either of the ones before.
    Unavailable,
}

/// Inclusive bounds on the work one host does and the history it keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostLimits {
    /// The bounds reconciliation, compilation and admission work within.
    pub identity: IdentityLimits,
    /// The bounds of each authority a session establishes.
    pub authority: AuthorityLimits,
    /// Registry generations the host retains, the genesis included.
    pub generations: u64,
}

/// What a test-owned host is set up with.
#[derive(Debug, Clone)]
pub struct HostSetup<'a> {
    /// The host's identity for the owner's registry destination.
    pub binding: &'a str,
    /// The application owner.
    pub owner: OwnerIdentity,
    /// The owning scope.
    pub scope: &'a str,
    /// The analysis context every session's authority binds.
    pub context: AuthoringBasis,
    /// The bounds of the host's work.
    pub limits: HostLimits,
}

/// What a session asks the host for when it opens.
#[derive(Debug, Clone, Default)]
pub struct Acquisition {
    /// The source snapshots acquired for the session, one per unit.
    pub sources: Vec<SourceSnapshot>,
    /// Earlier snapshots of the chain to pin for reading.
    pub pins: Vec<AuthoringArtifactReference>,
}

/// One registry the host published, with what produced it.
#[derive(Debug)]
pub(super) struct Generation {
    pub(super) registry: Arc<AdmittedRegistry>,
    step: Option<(AdmittedUpdate, AdmittedInventory)>,
    pub(super) record: PublicationRecord,
}

impl Generation {
    /// The update that produced this registry and the inventory it was
    /// planned from, absent for a genesis.
    pub(super) fn previous(&self) -> Option<PreviousUpdate<'_>> {
        self.step
            .as_ref()
            .map(|(update, inventory)| PreviousUpdate { update, inventory })
    }
}

#[derive(Debug)]
struct AuthorityState {
    revision: u64,
    grants: Vec<(Principal, Vec<Action>)>,
    development: bool,
}

#[derive(Debug)]
struct HostState {
    binding: BindingState,
    generation: u64,
    authority: AuthorityState,
    chain: Vec<Arc<Generation>>,
}

/// A test-owned host domain with one owner binding, held in memory.
#[derive(Debug)]
pub struct MemoryHost<R> {
    destination: Destination,
    context: AuthoringBasis,
    limits: HostLimits,
    state: Mutex<HostState>,
    randomness: Mutex<R>,
}

impl<R: Randomness> MemoryHost<R> {
    /// Set up a host with no owner binding enrolled, no grants and no
    /// development session.
    pub fn new(setup: HostSetup<'_>, randomness: R) -> Result<Self, PrimitiveError> {
        Ok(Self {
            destination: Destination::new(setup.binding, setup.owner, setup.scope, None)?,
            context: setup.context,
            limits: setup.limits,
            state: Mutex::new(HostState {
                binding: BindingState::NotEnrolled,
                generation: 0,
                authority: AuthorityState {
                    revision: 0,
                    grants: Vec::new(),
                    development: false,
                },
                chain: Vec::new(),
            }),
            randomness: Mutex::new(randomness),
        })
    }

    /// Take the guard. A guard a panic left behind means the state cannot be
    /// trusted, which is what unavailable means.
    fn lock(&self) -> Result<MutexGuard<'_, HostState>, HostFailure> {
        self.state
            .lock()
            .map_err(|_| HostFailure::Blocked(Blocked::Unavailable))
    }

    /// Return where the owner binding stands.
    pub fn binding_state(&self) -> BindingState {
        self.lock()
            .map_or(BindingState::Unavailable, |state| state.binding)
    }

    /// Enroll the owner binding as a genuinely new owner. Enrolling an
    /// enrolled or unavailable binding is refused, never repeated.
    pub fn enroll(&self) -> Result<(), HostFailure> {
        let mut state = self.lock()?;
        match state.binding {
            BindingState::NotEnrolled => {
                state.binding = BindingState::Uninitialized;
                state.generation += 1;
                Ok(())
            }
            BindingState::Unavailable => Err(HostFailure::Blocked(Blocked::Unavailable)),
            BindingState::Uninitialized | BindingState::Active => {
                Err(HostFailure::Blocked(Blocked::AlreadyEnrolled))
            }
        }
    }

    /// Replace the grants. Every session opened before is now under a
    /// changed authority, and its writes are refused.
    pub fn set_grants(&self, grants: Vec<(Principal, Vec<Action>)>) -> Result<(), HostFailure> {
        let mut state = self.lock()?;
        state.authority.grants = grants;
        state.authority.revision += 1;
        state.generation += 1;
        Ok(())
    }

    /// Start a development session for the destination. Sessions opened
    /// from now on may publish eligible updates automatically; sessions
    /// opened before are under a changed authority.
    pub fn start_development_session(&self) -> Result<(), HostFailure> {
        self.set_development(true)
    }

    /// End the development session. Sessions opened before are under a
    /// changed authority, whatever they were going to publish.
    pub fn end_development_session(&self) -> Result<(), HostFailure> {
        self.set_development(false)
    }

    fn set_development(&self, active: bool) -> Result<(), HostFailure> {
        let mut state = self.lock()?;
        state.authority.development = active;
        state.authority.revision += 1;
        Ok(())
    }

    /// Lose the control state, as a host would on finding it missing or
    /// inconsistent. The binding becomes unavailable for good.
    pub fn lose_control_state(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.binding = BindingState::Unavailable;
        }
    }

    /// Return the provenance of every publication, oldest first.
    pub fn records(&self) -> Result<Vec<PublicationRecord>, HostFailure> {
        let state = self.lock()?;
        Ok(state
            .chain
            .iter()
            .map(|generation| generation.record.clone())
            .collect())
    }

    /// Open a session for one caller: establish its authority from the host
    /// state and the acquisition, and pin the current registry.
    pub fn open(
        &self,
        caller: &Principal,
        acquisition: Acquisition,
    ) -> Result<Session<'_, R>, HostFailure> {
        let state = self.lock()?;
        match state.binding {
            BindingState::NotEnrolled => return Err(HostFailure::Blocked(Blocked::NotEnrolled)),
            BindingState::Unavailable => return Err(HostFailure::Blocked(Blocked::Unavailable)),
            BindingState::Uninitialized | BindingState::Active => {}
        }
        let current = state.chain.last().cloned();
        let mut anchors: Vec<AuthoringArtifactReference> = current
            .iter()
            .map(|generation| generation.registry.reference())
            .collect();
        let mut pinned = Vec::new();
        for pin in acquisition.pins {
            let generation = state
                .chain
                .iter()
                .find(|generation| generation.registry.reference() == pin)
                .ok_or(HostFailure::Blocked(Blocked::UnknownSnapshot))?;
            if !anchors.contains(&pin) {
                anchors.push(pin);
                pinned.push(Arc::clone(generation));
            }
        }
        let destination = match &current {
            Some(generation) => self
                .destination
                .clone()
                .with_registry(generation.registry.snapshot().registry_identity().clone()),
            None => self.destination.clone(),
        };
        let authority = LocalAuthority::establish(
            Establishment {
                destination,
                context: self.context.clone(),
                grants: state.authority.grants.clone(),
                acquired: acquisition.sources,
                anchors,
                development: state.authority.development,
            },
            &self.limits.authority,
        )
        .map_err(HostFailure::Establishment)?;
        authority.invoke(caller).map_err(HostFailure::Denied)?;
        Ok(Session {
            host: self,
            authority,
            caller: caller.clone(),
            revision: state.authority.revision,
            current,
            pinned,
        })
    }

    /// Draw from the host's randomness.
    pub(super) fn draw<T>(
        &self,
        draw: impl FnOnce(&mut R) -> Result<T, random::RandomnessFailure>,
    ) -> Result<T, HostFailure> {
        let mut randomness = self
            .randomness
            .lock()
            .map_err(|_| HostFailure::Randomness(random::RandomnessFailure))?;
        draw(&mut randomness).map_err(HostFailure::Randomness)
    }

    pub(super) const fn limits(&self) -> &HostLimits {
        &self.limits
    }

    pub(super) const fn destination(&self) -> &Destination {
        &self.destination
    }

    /// Check that an unchanged result is still the state the session saw.
    pub(super) fn still_current(&self, session: &Session<'_, R>) -> Result<(), HostFailure> {
        let state = self.lock()?;
        check_current(&state, session)
    }

    /// Publish one prepared update, conditionally.
    pub(super) fn commit_update(
        &self,
        session: &Session<'_, R>,
        request: &UpdateRequest<'_>,
    ) -> Result<Publication, HostFailure> {
        let mut state = self.lock()?;
        check_current(&state, session)?;
        if state
            .chain
            .last()
            .map(|generation| generation.registry.reference())
            != Some(request.base.reference())
        {
            return Err(HostFailure::Conflict(Conflict::StaleBase));
        }
        let permit = session
            .invocation()
            .authorize_update(request, &self.limits.authority)
            .map_err(HostFailure::Denied)?;
        within_history(&state, &self.limits)?;
        let registry = sealed(permit.result().clone(), &self.limits.identity)?;
        state.generation += 1;
        let record = PublicationRecord {
            generation: state.generation,
            authority_revision: state.authority.revision,
            publisher: permit.publisher().clone(),
            operation: Operation::Update(permit.mode()),
            base: Some(permit.base().clone()),
            inventory: Some(permit.inventory().clone()),
            update: Some(permit.update().clone()),
            result: registry.reference(),
            confirmations: permit
                .confirmations()
                .iter()
                .map(|confirmation| {
                    (
                        confirmation.confirmer().clone(),
                        confirmation.decision().intent_id().clone(),
                    )
                })
                .collect(),
        };
        state.chain.push(Arc::new(Generation {
            registry: Arc::new(registry),
            step: Some((request.plan.update().clone(), request.inventory.clone())),
            record: record.clone(),
        }));
        Ok(Publication {
            record,
            durability: Durability::InMemory,
        })
    }

    /// Make one empty genesis current, conditionally.
    pub(super) fn commit_initialization(
        &self,
        session: &Session<'_, R>,
        genesis: AdmittedRegistry,
    ) -> Result<Publication, HostFailure> {
        let mut state = self.lock()?;
        match state.binding {
            BindingState::Unavailable => return Err(HostFailure::Blocked(Blocked::Unavailable)),
            BindingState::Active => {
                return Err(HostFailure::Conflict(Conflict::AlreadyInitialized));
            }
            BindingState::NotEnrolled | BindingState::Uninitialized => {}
        }
        if state.authority.revision != session.revision {
            return Err(HostFailure::Conflict(Conflict::AuthorityChanged));
        }
        let permit = session
            .invocation()
            .authorize_initialization(&genesis)
            .map_err(HostFailure::Denied)?;
        within_history(&state, &self.limits)?;
        state.binding = BindingState::Active;
        state.generation += 1;
        let record = PublicationRecord {
            generation: state.generation,
            authority_revision: state.authority.revision,
            publisher: permit.publisher().clone(),
            operation: Operation::Initialize,
            base: None,
            inventory: None,
            update: None,
            result: genesis.reference(),
            confirmations: Box::new([]),
        };
        state.chain.push(Arc::new(Generation {
            registry: Arc::new(genesis),
            step: None,
            record: record.clone(),
        }));
        Ok(Publication {
            record,
            durability: Durability::InMemory,
        })
    }
}

/// Check the binding and the authority state against what the session saw.
fn check_current<R>(state: &HostState, session: &Session<'_, R>) -> Result<(), HostFailure> {
    if state.binding == BindingState::Unavailable {
        return Err(HostFailure::Blocked(Blocked::Unavailable));
    }
    let current = state
        .chain
        .last()
        .map(|generation| generation.registry.reference());
    let seen = session
        .current
        .as_ref()
        .map(|generation| generation.registry.reference());
    if current != seen {
        return Err(HostFailure::Conflict(Conflict::StaleBase));
    }
    if state.authority.revision != session.revision {
        return Err(HostFailure::Conflict(Conflict::AuthorityChanged));
    }
    Ok(())
}

fn within_history(state: &HostState, limits: &HostLimits) -> Result<(), HostFailure> {
    if state.chain.len() as u64 >= limits.generations {
        return Err(HostFailure::HistoryFull);
    }
    Ok(())
}

/// Seal a snapshot and admit its bytes, as a later reader would.
pub(super) fn sealed(
    snapshot: IntentRegistrySnapshot,
    limits: &IdentityLimits,
) -> Result<AdmittedRegistry, HostFailure> {
    let artifact = RegistryArtifact::seal(snapshot).map_err(|_| HostFailure::Unsealable)?;
    let bytes = serde_json::to_vec(&artifact).map_err(|_| HostFailure::Unsealable)?;
    admit_registry(&bytes, limits).map_err(HostFailure::Unadmissible)
}

#[cfg(test)]
mod tests {
    use intlify_authoring::{OwnerKind, SourceSnapshot};

    use super::*;
    use crate::authority::fixtures::{basis, id_of, Chain};
    use crate::authority::{AuthorizationFailure, EstablishmentFailure, UpdateMode};
    use crate::host::fixtures::{
        acquisition, advance, alice, committed_randomness, enrolled, host_limits, host_with, ALL,
    };

    #[test]
    fn enrollment_happens_once_and_never_from_an_unavailable_binding() {
        let host = host_with(committed_randomness(), host_limits());
        assert_eq!(host.binding_state(), BindingState::NotEnrolled);
        assert_eq!(
            host.open(&alice(), Acquisition::default()).err(),
            Some(HostFailure::Blocked(Blocked::NotEnrolled))
        );
        host.enroll().unwrap();
        assert_eq!(host.binding_state(), BindingState::Uninitialized);
        assert_eq!(
            host.enroll(),
            Err(HostFailure::Blocked(Blocked::AlreadyEnrolled))
        );
        let chain = Chain::load();
        host.set_grants(vec![(alice(), ALL.to_vec())]).unwrap();
        advance(&host, &chain, 0);
        assert_eq!(
            host.enroll(),
            Err(HostFailure::Blocked(Blocked::AlreadyEnrolled))
        );
        // Lost control state is neither of the states before.
        host.lose_control_state();
        assert_eq!(host.binding_state(), BindingState::Unavailable);
        assert_eq!(
            host.enroll(),
            Err(HostFailure::Blocked(Blocked::Unavailable))
        );
        assert_eq!(
            host.open(&alice(), Acquisition::default()).err(),
            Some(HostFailure::Blocked(Blocked::Unavailable))
        );
    }

    #[test]
    fn a_session_is_established_from_the_host_state_it_opened_in() {
        let chain = Chain::load();
        let host = enrolled(committed_randomness(), &[Action::ReadRegistry]);
        let empty = host.open(&alice(), acquisition(&chain, 1)).unwrap();
        // No chain yet: the destination names none, and nothing is anchored.
        assert_eq!(empty.authority().destination().registry(), None);
        assert!(empty.authority().anchors().is_empty());
        assert_eq!(empty.authority().context(), &basis());
        assert!(!empty.authority().development_session());
        assert_eq!(empty.caller(), &alice());

        host.set_grants(vec![(alice(), ALL.to_vec())]).unwrap();
        advance(&host, &chain, 2);
        host.start_development_session().unwrap();
        let mut sources = acquisition(&chain, 2).sources;
        sources.reverse();
        let session = host
            .open(
                &alice(),
                Acquisition {
                    sources,
                    pins: vec![chain.registry(1).reference(), chain.registry(2).reference()],
                },
            )
            .unwrap();
        let authority = session.authority();
        assert_eq!(
            authority.destination().registry(),
            Some(chain.registry(2).snapshot().registry_identity())
        );
        assert_eq!(
            authority.destination().owner(),
            chain.registry(2).snapshot().owner()
        );
        // The current registry, then the pins it does not repeat.
        assert_eq!(
            authority.anchors(),
            [chain.registry(2).reference(), chain.registry(1).reference()]
        );
        assert_eq!(session.pinned.len(), 1);
        assert_eq!(
            authority.acquired(),
            acquisition(&chain, 2).sources.as_slice()
        );
        assert!(authority.development_session());
        assert_eq!(
            authority
                .actions(&alice())
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            ALL
        );
    }

    #[test]
    fn opening_refuses_what_the_host_cannot_establish() {
        let chain = Chain::load();
        let host = enrolled(committed_randomness(), ALL);
        // A caller the host never established.
        assert_eq!(
            host.open(&Principal::new("mallory").unwrap(), Acquisition::default())
                .err(),
            Some(HostFailure::Denied(AuthorizationFailure::UnknownCaller))
        );
        // A source of another owner.
        let source = acquisition(&chain, 1).sources[0].clone();
        let foreign = SourceSnapshot::new(
            OwnerIdentity::new(OwnerKind::Application, "elsewhere").unwrap(),
            source.unit().as_str(),
            source.revision().as_str(),
            source.grammar().clone(),
            source.byte_length(),
            source.utf8_digest().as_str(),
        )
        .unwrap();
        assert_eq!(
            host.open(
                &alice(),
                Acquisition {
                    sources: vec![foreign],
                    pins: vec![],
                }
            )
            .err(),
            Some(HostFailure::Establishment(
                EstablishmentFailure::ForeignSource
            ))
        );
        // A pin of no snapshot this host holds.
        assert_eq!(
            host.open(
                &alice(),
                Acquisition {
                    sources: vec![],
                    pins: vec![chain.registry(1).reference()],
                }
            )
            .err(),
            Some(HostFailure::Blocked(Blocked::UnknownSnapshot))
        );
        // Grant entries the evaluator refuses, as it would any.
        host.set_grants(vec![(alice(), vec![]), (alice(), vec![])])
            .unwrap();
        assert_eq!(
            host.open(&alice(), Acquisition::default()).err(),
            Some(HostFailure::Establishment(
                EstablishmentFailure::DuplicatePrincipal
            ))
        );
    }

    #[test]
    fn each_publication_keeps_its_provenance() {
        let chain = Chain::load();
        let host = enrolled(committed_randomness(), ALL);
        advance(&host, &chain, 3);
        let records = host.records().unwrap();
        assert_eq!(records.len(), 4);
        // Enrollment and the grants were generations 1 and 2.
        let generations: Vec<u64> = records.iter().map(PublicationRecord::generation).collect();
        assert_eq!(generations, [3, 4, 5, 6]);
        assert!(records
            .iter()
            .all(|record| record.authority_revision() == 1 && record.publisher() == &alice()));
        assert_eq!(records[0].operation(), Operation::Initialize);
        for (n, record) in records.iter().enumerate().skip(1) {
            assert_eq!(record.operation(), Operation::Update(UpdateMode::Manual));
            assert_eq!(record.base(), Some(&chain.registry(n - 1).reference()));
            assert_eq!(record.inventory(), Some(&chain.inventory(n).reference()));
            assert_eq!(record.update(), Some(&chain.update(n).reference()));
            assert_eq!(record.result(), &chain.registry(n).reference());
        }
        assert!(records[1].confirmations().is_empty());
        assert_eq!(records[3].confirmations(), [(alice(), id_of(3, 1))]);
    }

    #[test]
    fn a_development_session_changes_the_authority_and_no_generation() {
        let chain = Chain::load();
        let host = enrolled(committed_randomness(), ALL);
        advance(&host, &chain, 0);
        host.start_development_session().unwrap();
        host.end_development_session().unwrap();
        advance_one(&host, &chain);
        let records = host.records().unwrap();
        assert_eq!(records[1].generation(), records[0].generation() + 1);
        assert_eq!(records[1].authority_revision(), 3);
    }

    /// Publish update 1 in a session of its own.
    fn advance_one(host: &MemoryHost<crate::host::random::scripted::Scripted>, chain: &Chain) {
        use crate::authority::fixtures::sources;
        use crate::host::fixtures::{membership, retained, Step};
        use crate::host::outcome::Preparation;

        let session = host.open(&alice(), acquisition(chain, 1)).unwrap();
        let owned = sources();
        let retained = retained(&owned);
        let membership = membership();
        let step = Step::of(chain, 1);
        let Ok(Preparation::Prepared(prepared)) =
            session.prepare(chain.inventory(1), &step.inputs(&retained, &membership))
        else {
            panic!("update 1 is planned");
        };
        session.publish(&prepared, &[], UpdateMode::Manual).unwrap();
    }

    #[test]
    fn a_guard_a_panic_left_behind_reads_as_unavailable() {
        let host = enrolled(committed_randomness(), ALL);
        std::thread::scope(|scope| {
            let _ = scope
                .spawn(|| {
                    let _state = host.state.lock().unwrap();
                    panic!("a write that never finished");
                })
                .join();
        });
        assert_eq!(host.binding_state(), BindingState::Unavailable);
        assert_eq!(
            host.open(&alice(), Acquisition::default()).err(),
            Some(HostFailure::Blocked(Blocked::Unavailable))
        );
        assert_eq!(
            host.records(),
            Err(HostFailure::Blocked(Blocked::Unavailable))
        );
    }
}
