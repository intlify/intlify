// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! One caller's session with the host.
//!
//! Opening a session establishes one authority from the host state and the
//! session's acquisition, and pins the registry current at that moment. The
//! session's operations run under that authority: reading, analyzing,
//! preparing and confirming outside the host's guard, and publishing and
//! initializing under it, where the state and the authority are checked
//! again at the point of publication.
//!
//! A session is narrow by design. Its confirmations count only under its
//! authority, and once anything it relies on changes, its writes are refused
//! and a new session has to be opened, confirmations included.

use std::sync::Arc;

use intlify_authoring::{AdmittedInventory, AuthoringArtifactReference, MessageIntentId, Token};
use intlify_authoring_identity::{
    compile, reconcile, CandidateFailure, Compilation, CompileEvidence, ContinuityInputs,
    ExplicitDecision, IdentityLimitKind, IdentityWorkspace, IntentRegistrySnapshot, LineageLink,
    ReconcileFailure, ReconcileInputs, Reconciliation, RetainedSources, SourceEdit,
};

use super::memory::{sealed, Generation, MemoryHost};
use super::outcome::{
    Blocked, Currency, HostFailure, Preparation, PreparedUpdate, Publication, RegistryRead,
};
use super::random::{self, Randomness};
use crate::authority::{
    Action, AuthorizationFailure, Confirmation, Invocation, LocalAuthority, Principal, UpdateMode,
    UpdateRequest,
};

/// What a caller brings to prepare an update, besides the inventory.
#[derive(Debug, Clone, Copy)]
pub struct PrepareInputs<'a> {
    /// The bytes of every snapshot an edit names.
    pub sources: &'a RetainedSources<'a>,
    /// How each changed unit got from the base's snapshot to its current one.
    pub edits: &'a [SourceEdit],
    /// Every unit of the owning scope.
    pub membership: &'a [Token],
    /// Explicit decisions, each bound to the base and the inventory.
    pub explicit: &'a [ExplicitDecision],
    /// Lineage links the plan records as they are.
    pub links: &'a [LineageLink],
}

/// One caller's session: an established authority and the registry current
/// when it opened.
#[derive(Debug)]
pub struct Session<'h, R> {
    pub(super) host: &'h MemoryHost<R>,
    pub(super) authority: LocalAuthority,
    pub(super) caller: Principal,
    pub(super) revision: u64,
    pub(super) current: Option<Arc<Generation>>,
    pub(super) pinned: Vec<Arc<Generation>>,
}

impl<R: Randomness> Session<'_, R> {
    /// Borrow the authority the session runs under.
    #[must_use]
    pub const fn authority(&self) -> &LocalAuthority {
        &self.authority
    }

    /// Borrow the caller.
    #[must_use]
    pub const fn caller(&self) -> &Principal {
        &self.caller
    }

    /// The caller under the session's authority. Opening the session
    /// established both, so the caller is always one of its principals.
    pub(super) fn invocation(&self) -> Invocation<'_> {
        self.authority
            .invoke(&self.caller)
            .expect("a session's caller is established")
    }

    fn current(&self) -> Result<&Generation, HostFailure> {
        self.current
            .as_deref()
            .ok_or(HostFailure::Blocked(Blocked::Uninitialized))
    }

    /// Read the registry current when the session opened, or a pinned
    /// earlier one by its reference.
    pub fn read(
        &self,
        pinned: Option<&AuthoringArtifactReference>,
    ) -> Result<RegistryRead, HostFailure> {
        let generation = match pinned {
            None => self.current()?,
            Some(reference) => self
                .current
                .iter()
                .chain(&self.pinned)
                .map(Arc::as_ref)
                .find(|generation| generation.registry.reference() == *reference)
                .ok_or(HostFailure::Blocked(Blocked::UnknownSnapshot))?,
        };
        self.invocation()
            .authorize_read(&generation.registry)
            .map_err(HostFailure::Denied)?;
        let current = self
            .current
            .as_deref()
            .is_some_and(|current| std::ptr::eq(current, generation));
        Ok(RegistryRead {
            registry: Arc::clone(&generation.registry),
            currency: if current {
                Currency::Current
            } else {
                Currency::Historical
            },
        })
    }

    /// Compile an inventory against the current registry. Nothing is
    /// allocated, decided or published.
    pub fn analyze(
        &self,
        inventory: &AdmittedInventory,
        sources: &RetainedSources<'_>,
        edits: &[SourceEdit],
    ) -> Result<Compilation, HostFailure> {
        let current = self.current()?;
        self.invocation()
            .authorize_analysis(inventory, Some(&current.registry))
            .map_err(HostFailure::Denied)?;
        compile(
            &current.registry,
            inventory,
            &CompileEvidence {
                sources,
                edits,
                previous: current.previous(),
            },
            &self.host.limits().identity,
            &mut IdentityWorkspace::new(),
        )
        .map_err(HostFailure::Compile)
    }

    /// Prepare one update of the current registry from an inventory.
    ///
    /// Fresh Intent IDs are drawn only once reconciliation shows how many new
    /// declarations need one, and exactly that many. A plan that needs none
    /// draws none.
    pub fn prepare(
        &self,
        inventory: &AdmittedInventory,
        inputs: &PrepareInputs<'_>,
    ) -> Result<Preparation, HostFailure> {
        let current = self.current()?;
        self.invocation()
            .authorize_analysis(inventory, Some(&current.registry))
            .map_err(HostFailure::Denied)?;
        let limits = &self.host.limits().identity;
        let mut workspace = IdentityWorkspace::new();
        let mut reconciled = |candidates: &[MessageIntentId]| {
            reconcile(
                &current.registry,
                inventory,
                &ReconcileInputs {
                    evidence: ContinuityInputs {
                        sources: inputs.sources,
                        edits: inputs.edits,
                        membership: inputs.membership,
                        previous: current.previous(),
                    },
                    explicit: inputs.explicit,
                    links: inputs.links,
                    candidates,
                },
                limits,
                &mut workspace,
            )
        };
        let reconciliation = match reconciled(&[]) {
            Err(ReconcileFailure::Candidates(CandidateFailure::Exhausted { needed })) => {
                // Nothing is drawn beyond what the plan may use.
                if needed as u64 > limits.candidates {
                    return Err(HostFailure::Reconcile(ReconcileFailure::Limit(
                        IdentityLimitKind::Candidates,
                    )));
                }
                let owner = current.registry.snapshot().owner();
                let candidates = self
                    .host
                    .draw(|randomness| random::intent_ids(owner, needed, randomness))?;
                reconciled(&candidates)
            }
            other => other,
        }
        .map_err(HostFailure::Reconcile)?;
        match reconciliation {
            Reconciliation::Planned(plan) => Ok(Preparation::Prepared(Box::new(PreparedUpdate {
                base: Arc::clone(&current.registry),
                inventory: inventory.clone(),
                plan: *plan,
            }))),
            Reconciliation::Unresolved(report) => Ok(Preparation::Unresolved(report)),
            Reconciliation::Unchanged => {
                // Unchanged says the base stays current, which only holds
                // while it still is.
                self.host.still_current(self)?;
                Ok(Preparation::Unchanged)
            }
        }
    }

    /// Confirm one explicit decision for the current registry and an
    /// inventory, under the session's authority.
    pub fn confirm(
        &self,
        inventory: &AdmittedInventory,
        decision: &ExplicitDecision,
    ) -> Result<Confirmation, HostFailure> {
        let current = self.current()?;
        self.invocation()
            .confirm(&current.registry, inventory, decision)
            .map_err(HostFailure::Denied)
    }

    /// Publish a prepared update: under the host's guard, check that its
    /// base is still current and the authority unchanged, authorize it, and
    /// make its result current.
    pub fn publish(
        &self,
        prepared: &PreparedUpdate,
        confirmations: &[Confirmation],
        mode: UpdateMode,
    ) -> Result<Publication, HostFailure> {
        self.host.commit_update(
            self,
            &UpdateRequest {
                base: &prepared.base,
                inventory: &prepared.inventory,
                plan: &prepared.plan,
                confirmations,
                mode,
            },
        )
    }

    /// Initialize the binding with one empty genesis under a freshly drawn
    /// registry identity.
    ///
    /// The identity is drawn only for a caller holding `initialize-registry`
    /// in a session that saw no chain; the binding's state and the authority
    /// are checked again when the genesis is made current.
    pub fn initialize(&self) -> Result<Publication, HostFailure> {
        let invocation = self.invocation();
        invocation
            .require(Action::InitializeRegistry)
            .map_err(HostFailure::Denied)?;
        if self.current.is_some() {
            return Err(HostFailure::Denied(
                AuthorizationFailure::AlreadyInitialized,
            ));
        }
        let identity = self
            .host
            .draw(|randomness| random::registry_identity(randomness))?;
        let destination = self.host.destination();
        // The destination's scope is already a checked token.
        let genesis = IntentRegistrySnapshot::genesis(
            destination.owner().clone(),
            destination.scope().as_str(),
            identity,
        )
        .expect("the destination's scope is a valid token");
        let genesis = sealed(genesis, &self.host.limits().identity)?;
        self.host.commit_initialization(self, genesis)
    }
}

#[cfg(test)]
mod tests {
    use intlify_authoring_identity::{IdentityAdmissionFailure, IdentityLimits};

    use super::*;
    use crate::authority::fixtures::{id_of, restore, sources, the_edit, Chain, REGISTRY};
    use crate::host::fixtures::{
        acquisition, advance, alice, bytes, bytes_of, committed_randomness, enrolled, host_limits,
        host_with, inputs, membership, retained, Step, ALL,
    };
    use crate::host::memory::{Acquisition, BindingState, MemoryHost};
    use crate::host::outcome::{Conflict, Durability, Operation};
    use crate::host::random::scripted::Scripted;

    fn prepared(preparation: Result<Preparation, HostFailure>) -> PreparedUpdate {
        match preparation {
            Ok(Preparation::Prepared(prepared)) => *prepared,
            other => panic!("not prepared: {other:?}"),
        }
    }

    /// Prepare committed update `n` in a session of its own.
    fn prepare_step<'h>(
        host: &'h MemoryHost<Scripted>,
        chain: &Chain,
        n: usize,
    ) -> (Session<'h, Scripted>, Result<Preparation, HostFailure>) {
        let session = host.open(&alice(), acquisition(chain, n)).unwrap();
        let owned = sources();
        let retained = retained(&owned);
        let membership = membership();
        let step = Step::of(chain, n);
        let preparation = session.prepare(chain.inventory(n), &step.inputs(&retained, &membership));
        (session, preparation)
    }

    #[test]
    fn initialization_draws_one_identity_and_makes_an_empty_genesis_current() {
        let chain = Chain::load();
        let host = enrolled(committed_randomness(), ALL);
        let first = host.open(&alice(), Acquisition::default()).unwrap();
        let second = host.open(&alice(), Acquisition::default()).unwrap();
        let published = first.initialize().unwrap();
        // The committed genesis, drawn under the committed identity.
        assert_eq!(published.record().result(), &chain.registry(0).reference());
        assert_eq!(published.record().operation(), Operation::Initialize);
        assert_eq!(published.record().publisher(), &alice());
        assert_eq!(published.record().base(), None);
        assert_eq!(published.record().inventory(), None);
        assert_eq!(published.record().update(), None);
        assert!(published.record().confirmations().is_empty());
        assert_eq!(published.durability(), Durability::InMemory);
        assert_eq!(host.binding_state(), BindingState::Active);
        // A session that saw no chain loses the race at the commit.
        assert_eq!(
            second.initialize(),
            Err(HostFailure::Conflict(Conflict::AlreadyInitialized))
        );
        // A session that sees the chain is refused before drawing.
        let third = host.open(&alice(), Acquisition::default()).unwrap();
        assert_eq!(
            third.initialize(),
            Err(HostFailure::Denied(
                AuthorizationFailure::AlreadyInitialized
            ))
        );
        assert_eq!(host.records().unwrap().len(), 1);
    }

    #[test]
    fn initialization_draws_nothing_for_a_caller_without_its_grant() {
        let chain = Chain::load();
        let others: Vec<Action> = ALL
            .iter()
            .copied()
            .filter(|action| *action != Action::InitializeRegistry)
            .collect();
        let host = enrolled(committed_randomness(), &others);
        let session = host.open(&alice(), Acquisition::default()).unwrap();
        assert_eq!(
            session.initialize(),
            Err(HostFailure::Denied(AuthorizationFailure::Denied(
                Action::InitializeRegistry
            )))
        );
        // The first value is still the next one: nothing was drawn.
        host.set_grants(vec![(alice(), ALL.to_vec())]).unwrap();
        let session = host.open(&alice(), Acquisition::default()).unwrap();
        assert_eq!(
            session.initialize().unwrap().record().result(),
            &chain.registry(0).reference()
        );
    }

    #[test]
    fn a_failed_draw_makes_nothing_current() {
        let host = enrolled(Scripted::new([]), ALL);
        let session = host.open(&alice(), Acquisition::default()).unwrap();
        assert_eq!(
            session.initialize(),
            Err(HostFailure::Randomness(super::random::RandomnessFailure))
        );
        assert_eq!(host.binding_state(), BindingState::Uninitialized);
        assert!(host.records().unwrap().is_empty());
    }

    #[test]
    fn preparing_draws_exactly_the_ids_new_declarations_need() {
        let chain = Chain::load();
        let host = enrolled(committed_randomness(), ALL);
        advance(&host, &chain, 1);
        // An inventory with nothing new draws nothing and plans nothing.
        let (_, unchanged) = prepare_step(&host, &chain, 1);
        assert_eq!(unchanged, Ok(Preparation::Unchanged));
        // So the next value is the one update 2 was allocated with.
        let (_, planned) = prepare_step(&host, &chain, 2);
        assert_eq!(
            prepared(planned).plan().update().reference(),
            chain.update(2).reference()
        );
        // Every preparation draws its own: none is left for another.
        let (_, again) = prepare_step(&host, &chain, 2);
        assert_eq!(
            again,
            Err(HostFailure::Randomness(super::random::RandomnessFailure))
        );
    }

    #[test]
    fn the_candidate_bound_is_checked_before_anything_is_drawn() {
        let chain = Chain::load();
        let mut limits = host_limits();
        limits.identity = IdentityLimits {
            candidates: 2,
            ..limits.identity
        };
        // Two values after the identity: drawing three would fail.
        let host = host_with(
            Scripted::new([
                bytes(REGISTRY),
                bytes_of(&id_of(1, 0)),
                bytes_of(&id_of(1, 2)),
            ]),
            limits,
        );
        host.enroll().unwrap();
        host.set_grants(vec![(alice(), ALL.to_vec())]).unwrap();
        advance(&host, &chain, 0);
        let (_, refused) = prepare_step(&host, &chain, 1);
        assert_eq!(
            refused,
            Err(HostFailure::Reconcile(ReconcileFailure::Limit(
                IdentityLimitKind::Candidates
            )))
        );
    }

    #[test]
    fn an_unchanged_result_holds_only_while_its_base_is_current() {
        let chain = Chain::load();
        let host = enrolled(committed_randomness(), ALL);
        advance(&host, &chain, 1);
        let session = host.open(&alice(), acquisition(&chain, 1)).unwrap();
        let owned = sources();
        let retained = retained(&owned);
        let membership = membership();
        let unchanged = || {
            session.prepare(
                chain.inventory(1),
                &inputs(&retained, &membership, &[], &[]),
            )
        };
        assert_eq!(unchanged(), Ok(Preparation::Unchanged));
        // The authority changes: the session's view no longer holds.
        host.start_development_session().unwrap();
        assert_eq!(
            unchanged(),
            Err(HostFailure::Conflict(Conflict::AuthorityChanged))
        );
        // Another session publishes: the base is no longer current.
        let other = host.open(&alice(), acquisition(&chain, 2)).unwrap();
        let step = Step::of(&chain, 2);
        let update =
            prepared(other.prepare(chain.inventory(2), &step.inputs(&retained, &membership)));
        other.publish(&update, &[], UpdateMode::Manual).unwrap();
        assert_eq!(unchanged(), Err(HostFailure::Conflict(Conflict::StaleBase)));
    }

    #[test]
    fn an_unresolved_preparation_is_reported_as_it_is() {
        let chain = Chain::load();
        let host = enrolled(committed_randomness(), ALL);
        advance(&host, &chain, 1);
        let session = host.open(&alice(), acquisition(&chain, 2)).unwrap();
        let owned = sources();
        let retained = retained(&owned);
        let membership = membership();
        // Without the edit, nothing shows where the old declarations went.
        assert!(matches!(
            session.prepare(
                chain.inventory(2),
                &inputs(&retained, &membership, &[], &[])
            ),
            Ok(Preparation::Unresolved(_))
        ));
        assert_eq!(host.records().unwrap().len(), 2);
    }

    #[test]
    fn a_read_says_whether_its_registry_is_current() {
        let chain = Chain::load();
        let host = enrolled(committed_randomness(), ALL);
        let empty = host.open(&alice(), Acquisition::default()).unwrap();
        assert_eq!(
            empty.read(None),
            Err(HostFailure::Blocked(Blocked::Uninitialized))
        );
        advance(&host, &chain, 2);
        let session = host
            .open(
                &alice(),
                Acquisition {
                    pins: vec![chain.registry(1).reference()],
                    ..Acquisition::default()
                },
            )
            .unwrap();
        let current = session.read(None).unwrap();
        assert_eq!(current.registry(), chain.registry(2));
        assert_eq!(current.currency(), Currency::Current);
        let current = session.read(Some(&chain.registry(2).reference())).unwrap();
        assert_eq!(current.currency(), Currency::Current);
        let earlier = session.read(Some(&chain.registry(1).reference())).unwrap();
        assert_eq!(earlier.registry(), chain.registry(1));
        assert_eq!(earlier.currency(), Currency::Historical);
        // Only what the session pinned.
        assert_eq!(
            session.read(Some(&chain.registry(0).reference())),
            Err(HostFailure::Blocked(Blocked::UnknownSnapshot))
        );
        // Reading needs its grant.
        host.set_grants(vec![(alice(), vec![Action::AnalyzeSource])])
            .unwrap();
        let reader = host.open(&alice(), Acquisition::default()).unwrap();
        assert_eq!(
            reader.read(None),
            Err(HostFailure::Denied(AuthorizationFailure::Denied(
                Action::ReadRegistry
            )))
        );
    }

    #[test]
    fn analysis_compiles_against_the_current_registry_and_changes_nothing() {
        let chain = Chain::load();
        let host = enrolled(committed_randomness(), ALL);
        let empty = host.open(&alice(), acquisition(&chain, 1)).unwrap();
        let owned = sources();
        let retained = retained(&owned);
        assert_eq!(
            empty.analyze(chain.inventory(1), &retained, &[]),
            Err(HostFailure::Blocked(Blocked::Uninitialized))
        );
        advance(&host, &chain, 2);
        let session = host.open(&alice(), acquisition(&chain, 2)).unwrap();
        let Ok(Compilation::Compiled(scope)) = session.analyze(chain.inventory(2), &retained, &[])
        else {
            panic!("compiled");
        };
        assert_eq!(scope.intents().len(), 3);
        // A read-only analysis needs its read grants, and gains no writes.
        host.set_grants(vec![(alice(), vec![Action::ReadRegistry])])
            .unwrap();
        let reader = host.open(&alice(), acquisition(&chain, 2)).unwrap();
        assert_eq!(
            reader.analyze(chain.inventory(2), &retained, &[]),
            Err(HostFailure::Denied(AuthorizationFailure::Denied(
                Action::AnalyzeSource
            )))
        );
        assert_eq!(host.records().unwrap().len(), 3);
    }

    #[test]
    fn a_publication_is_refused_once_its_base_or_authority_moved() {
        let chain = Chain::load();
        // Two values for update 2, one for each preparation.
        let host = enrolled(
            Scripted::new([
                bytes(REGISTRY),
                bytes_of(&id_of(1, 0)),
                bytes_of(&id_of(1, 2)),
                bytes_of(&id_of(1, 1)),
                bytes_of(&id_of(2, 2)),
                [0x77; 16],
            ]),
            ALL,
        );
        advance(&host, &chain, 1);
        let (first, one) = prepare_step(&host, &chain, 2);
        // The second draws its own ID, so it carries no copy link to the
        // first one's.
        let second = host.open(&alice(), acquisition(&chain, 2)).unwrap();
        let owned = sources();
        let retained = retained(&owned);
        let membership = membership();
        let edits = [the_edit(2)];
        let other = second.prepare(
            chain.inventory(2),
            &inputs(&retained, &membership, &edits, &[]),
        );
        let (one, other) = (prepared(one), prepared(other));
        first.publish(&one, &[], UpdateMode::Manual).unwrap();
        // The same base is no longer current: at most one publishes.
        assert_eq!(
            second.publish(&other, &[], UpdateMode::Manual),
            Err(HostFailure::Conflict(Conflict::StaleBase))
        );
        // Prepared again on the new base, then the grants change.
        let (third, next) = prepare_step(&host, &chain, 3);
        let next = prepared(next);
        let confirmation = third.confirm(chain.inventory(3), &restore(&chain)).unwrap();
        host.set_grants(vec![(alice(), ALL.to_vec())]).unwrap();
        assert_eq!(
            third.publish(&next, &[confirmation], UpdateMode::Manual),
            Err(HostFailure::Conflict(Conflict::AuthorityChanged))
        );
        // And once the control state is lost, nothing is written at all.
        let (fourth, again) = prepare_step(&host, &chain, 3);
        let again = prepared(again);
        let confirmation = fourth
            .confirm(chain.inventory(3), &restore(&chain))
            .unwrap();
        host.lose_control_state();
        assert_eq!(
            fourth.publish(&again, &[confirmation], UpdateMode::Manual),
            Err(HostFailure::Blocked(Blocked::Unavailable))
        );
        assert_eq!(host.records().unwrap().len(), 3);
    }

    #[test]
    fn a_publication_is_authorized_at_the_commit() {
        let chain = Chain::load();
        let host = enrolled(committed_randomness(), ALL);
        advance(&host, &chain, 2);
        let (session, update) = prepare_step(&host, &chain, 3);
        let update = prepared(update);
        // The restore has no confirmation.
        assert_eq!(
            session.publish(&update, &[], UpdateMode::Manual),
            Err(HostFailure::Denied(
                AuthorizationFailure::ConfirmationMissing(id_of(3, 1))
            ))
        );
        // A confirmation from another session is another authority's.
        let elsewhere = host.open(&alice(), acquisition(&chain, 3)).unwrap();
        let foreign = elsewhere
            .confirm(chain.inventory(3), &restore(&chain))
            .unwrap();
        assert_eq!(
            session.publish(&update, &[foreign], UpdateMode::Manual),
            Err(HostFailure::Denied(AuthorizationFailure::ConfirmationStale))
        );
        let confirmation = session
            .confirm(chain.inventory(3), &restore(&chain))
            .unwrap();
        let published = session
            .publish(&update, &[confirmation], UpdateMode::Manual)
            .unwrap();
        assert_eq!(published.record().result(), &chain.registry(3).reference());
        assert_eq!(published.record().confirmations(), [(alice(), id_of(3, 1))]);
    }

    #[test]
    fn a_development_session_publishes_only_proven_plans() {
        let chain = Chain::load();
        // Update 1 is prepared twice, and each preparation draws its own IDs.
        let host = enrolled(
            Scripted::new([
                bytes(REGISTRY),
                [0x11; 16],
                [0x12; 16],
                [0x13; 16],
                [0x21; 16],
                [0x22; 16],
                [0x23; 16],
            ]),
            ALL,
        );
        advance(&host, &chain, 0);
        // Without a session, nothing is automatic.
        let (session, update) = prepare_step(&host, &chain, 1);
        let update = prepared(update);
        assert_eq!(
            session.publish(&update, &[], UpdateMode::Development),
            Err(HostFailure::Denied(AuthorizationFailure::SessionInactive))
        );
        host.start_development_session().unwrap();
        let (session, update) = prepare_step(&host, &chain, 1);
        let update = prepared(update);
        let published = session
            .publish(&update, &[], UpdateMode::Development)
            .unwrap();
        assert_eq!(
            published.record().operation(),
            Operation::Update(UpdateMode::Development)
        );
    }

    #[test]
    fn the_host_bounds_refuse_a_write_rather_than_drop_anything() {
        let chain = Chain::load();
        // Room for the genesis only.
        let mut limits = host_limits();
        limits.generations = 1;
        let host = host_with(committed_randomness(), limits);
        host.enroll().unwrap();
        host.set_grants(vec![(alice(), ALL.to_vec())]).unwrap();
        advance(&host, &chain, 0);
        let (session, update) = prepare_step(&host, &chain, 1);
        assert_eq!(
            session.publish(&prepared(update), &[], UpdateMode::Manual),
            Err(HostFailure::HistoryFull)
        );
        // A result the host's own reader would not admit never becomes
        // current: update 1 makes three entries.
        let mut limits = host_limits();
        limits.identity = IdentityLimits {
            entries: 2,
            ..limits.identity
        };
        let host = host_with(committed_randomness(), limits);
        host.enroll().unwrap();
        host.set_grants(vec![(alice(), ALL.to_vec())]).unwrap();
        advance(&host, &chain, 0);
        let (session, update) = prepare_step(&host, &chain, 1);
        assert_eq!(
            session.publish(&prepared(update), &[], UpdateMode::Manual),
            Err(HostFailure::Unadmissible(IdentityAdmissionFailure::Limit(
                IdentityLimitKind::Entries
            )))
        );
        assert_eq!(host.records().unwrap().len(), 1);
    }

    #[test]
    fn a_plan_prepared_on_an_earlier_base_is_stale_in_any_session() {
        let chain = Chain::load();
        let mut values = vec![
            bytes(REGISTRY),
            bytes_of(&id_of(1, 0)),
            bytes_of(&id_of(1, 2)),
            bytes_of(&id_of(1, 1)),
            bytes_of(&id_of(2, 2)),
        ];
        values.push([0x5a; 16]);
        let host = enrolled(Scripted::new(values), ALL);
        advance(&host, &chain, 1);
        let (session, update) = prepare_step(&host, &chain, 2);
        let update = prepared(update);
        // A second plan on registry 1, drawing its own ID and no link.
        let earlier = host.open(&alice(), acquisition(&chain, 2)).unwrap();
        let owned = sources();
        let retained = retained(&owned);
        let membership = membership();
        let edits = [the_edit(2)];
        let old = prepared(earlier.prepare(
            chain.inventory(2),
            &inputs(&retained, &membership, &edits, &[]),
        ));
        session.publish(&update, &[], UpdateMode::Manual).unwrap();
        // A session opened after sees registry 2 as current, and the plan's
        // base is still not it.
        let fresh = host.open(&alice(), acquisition(&chain, 2)).unwrap();
        assert_eq!(
            fresh.publish(&old, &[], UpdateMode::Manual),
            Err(HostFailure::Conflict(Conflict::StaleBase))
        );
    }

    #[test]
    fn preparing_needs_the_analysis_grants() {
        let chain = Chain::load();
        let host = enrolled(committed_randomness(), ALL);
        advance(&host, &chain, 0);
        host.set_grants(vec![(
            alice(),
            vec![Action::ReadRegistry, Action::UpdateRegistry],
        )])
        .unwrap();
        let (_, refused) = prepare_step(&host, &chain, 1);
        assert_eq!(
            refused,
            Err(HostFailure::Denied(AuthorizationFailure::Denied(
                Action::AnalyzeSource
            )))
        );
    }

    #[test]
    fn the_candidate_bound_admits_its_exact_value() {
        let chain = Chain::load();
        let mut limits = host_limits();
        limits.identity = IdentityLimits {
            candidates: 3,
            ..limits.identity
        };
        let host = host_with(committed_randomness(), limits);
        host.enroll().unwrap();
        host.set_grants(vec![(alice(), ALL.to_vec())]).unwrap();
        advance(&host, &chain, 0);
        let (_, planned) = prepare_step(&host, &chain, 1);
        assert_eq!(
            prepared(planned).plan().update().reference(),
            chain.update(1).reference()
        );
    }
}
