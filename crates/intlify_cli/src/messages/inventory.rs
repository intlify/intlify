// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Configuration-authoritative catalog inventory and definition production.
//!
//! This module owns the project-link definition side: recursive catalog
//! enumeration, physical-source grouping, checked snapshot acquisition,
//! resource extraction, project-global domain admission, and host-to-contract
//! projection. It deliberately does not reuse operand-driven formatter input
//! selection, apply ignore files, scan reference sources, construct a
//! `LinkRequest`, or invoke the linker.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::{self, File, Metadata};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;

use file_id::FileId;
use intlify_contract::{
    ArtifactContractError, ArtifactLimitEvidence, ArtifactNamespace, ArtifactVersion, CatalogKey,
    CatalogKeyDomain as ContractCatalogKeyDomain, CatalogScopeId,
    CatalogScopeName as ContractCatalogScopeName, EntryReference, EntryStructuralPath,
    FingerprintAlgorithm, FingerprintDigest, InputFingerprint, LinkLimitCounter,
    LinkLimitObservation, LinkLimits, Locale, MessageDefinition, MessageDefinitionArtifact,
    MessagePayload, PortablePathSegment, PortableRelativePath, ProducerId, ProducerIdentity,
    ProducerRevision, SourceDocumentIdentity, ValueConstructionError,
};
use intlify_resource::{
    preflight_host_bytes, CatalogBindingConflictField, CatalogDefinitionRef,
    CatalogKeyDomain as ResourceCatalogKeyDomain, HostFormat, HostFormatRegistry,
    LinkerCatalogAssignmentError, LinkerCatalogResolution, ProjectRelativeResourcePath,
    ResolvedHostFormat, ResolvedLinkerCatalogAssignment, ResolvedResources, ResourcePhase,
    MAX_HOST_BYTES,
};
use serde_json::{json, Map, Value};

use super::config::ResolvedMessagesConfig;
use crate::error::OperationalError;
use crate::resource::resource_error;

/// Stable producer implementation-family identifier for catalog definitions.
pub(crate) const RESOURCE_DEFINITION_PRODUCER_ID: &str = "dev.intlify/resource-definition";

/// Build-derived revision for the catalog definition producer.
pub(crate) const RESOURCE_DEFINITION_PRODUCER_REVISION: &str =
    env!("INTLIFY_RESOURCE_DEFINITION_REVISION");

/// Complete definition-side inventory for one project-link generation.
#[derive(Debug)]
pub(crate) struct DefinitionInventory {
    target_scopes: Box<[CatalogScopeId]>,
    artifacts: Vec<MessageDefinitionArtifact>,
    failures: Vec<DefinitionSourceFailure>,
}

impl DefinitionInventory {
    /// Return every configuration-admitted project scope in canonical order.
    #[must_use]
    pub(crate) fn target_scopes(&self) -> &[CatalogScopeId] {
        &self.target_scopes
    }

    /// Return successful source-complete definition artifacts in source order.
    #[must_use]
    pub(crate) fn artifacts(&self) -> &[MessageDefinitionArtifact] {
        &self.artifacts
    }

    /// Return every source-attributable failure retained by the inventory.
    #[must_use]
    pub(crate) fn failures(&self) -> &[DefinitionSourceFailure] {
        &self.failures
    }
}

/// One failed authoritative definition participant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DefinitionSourceFailure {
    affected_scopes: Box<[CatalogScopeId]>,
    error: OperationalError,
}

impl DefinitionSourceFailure {
    /// Return scopes whose definition side cannot be proven closed.
    #[must_use]
    pub(crate) fn affected_scopes(&self) -> &[CatalogScopeId] {
        &self.affected_scopes
    }

    /// Return the integration-owned operational evidence.
    #[must_use]
    pub(crate) const fn error(&self) -> &OperationalError {
        &self.error
    }
}

/// Fail-complete setup or configuration contradiction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DefinitionInventoryError {
    error: OperationalError,
}

impl DefinitionInventoryError {
    /// Return the operational configuration or invariant failure.
    #[must_use]
    pub(crate) const fn operational_error(&self) -> &OperationalError {
        &self.error
    }
}

#[derive(Debug)]
struct Discovery {
    candidates: Vec<LogicalCandidate>,
    failures: Vec<PendingFailure>,
}

struct DiscoveryDirectory {
    directory: PathBuf,
    entries: std::vec::IntoIter<io::Result<fs::DirEntry>>,
}

#[derive(Debug)]
struct LogicalCandidate {
    relative: ProjectRelativeResourcePath,
    absolute: PathBuf,
}

#[derive(Debug)]
struct LogicalTarget {
    relative: ProjectRelativeResourcePath,
    absolute: PathBuf,
    assignment: ResolvedLinkerCatalogAssignment,
}

impl LogicalTarget {
    fn path(&self) -> &str {
        self.relative.as_str()
    }

    fn scope_id(&self) -> CatalogScopeId {
        contract_scope_id(self.assignment.scope().as_str())
            .expect("resource and contract scope grammars are coordinated")
    }
}

#[derive(Debug)]
struct PhysicalGroup {
    identity: FileId,
    targets: Vec<LogicalTarget>,
}

impl PhysicalGroup {
    fn primary(&self) -> &LogicalTarget {
        &self.targets[0]
    }

    fn primary_path(&self) -> &str {
        self.primary().path()
    }

    fn scope_id(&self) -> CatalogScopeId {
        self.primary().scope_id()
    }
}

#[derive(Debug)]
struct PreparedGroup {
    group: PhysicalGroup,
    resolved_format: ResolvedHostFormat,
    source_identity: SourceDocumentIdentity,
    aliases: Vec<PortableRelativePath>,
    scope: CatalogScopeId,
    locale: Locale,
}

impl PreparedGroup {
    fn primary_path(&self) -> &str {
        self.group.primary_path()
    }
}

#[derive(Debug)]
struct SuccessfulExtraction {
    prepared: PreparedGroup,
    snapshot: Arc<str>,
    catalog: intlify_resource::ExtractedCatalog,
}

#[derive(Debug)]
struct PendingFailure {
    order_path: Option<String>,
    failure: DefinitionSourceFailure,
}

#[derive(Debug, Clone)]
struct DomainObservation {
    scope: CatalogScopeId,
    domain: ResourceCatalogKeyDomain,
    definition: CatalogDefinitionRef,
    target_path: String,
    entry_order: usize,
}

#[derive(Debug)]
enum ProjectionError {
    Artifact(ArtifactContractError),
    Invariant,
}

/// Enumerate and produce the complete definition side for one resolved project.
///
/// Configuration contradictions return `Err` and expose no partial inventory.
/// Filesystem, extraction, and projection failures remain in the successful
/// inventory result so callers can derive definition-side completeness.
pub(crate) fn produce_definition_inventory(
    project_root: &Path,
    config_path: Option<&Path>,
    resources: &ResolvedResources,
    messages: &ResolvedMessagesConfig,
    registry: &HostFormatRegistry,
    limits: &LinkLimits,
) -> Result<DefinitionInventory, DefinitionInventoryError> {
    let target_scopes = resources
        .linker_scopes()
        .iter()
        .map(|scope| {
            contract_scope_id(scope.as_str()).map_err(|_| internal_inventory_error(config_path))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if target_scopes.is_empty() {
        return Ok(DefinitionInventory {
            target_scopes: Box::new([]),
            artifacts: Vec::new(),
            failures: Vec::new(),
        });
    }

    let Discovery {
        candidates,
        failures: mut pending_failures,
    } = discover_project_candidates(project_root, &target_scopes);
    let mut targets = Vec::new();

    // Candidate order is canonical before any assignment-dependent failure is
    // selected, independently of native directory enumeration.
    for candidate in candidates {
        let path = candidate.relative.as_str();
        let assignment = resources
            .resolve_linker_path(&candidate.relative)
            .map_err(|error| assignment_config_error(config_path, path, &error))?;
        let LinkerCatalogResolution::Matched(assignment) = assignment else {
            continue;
        };
        assignment
            .validate_production_locale(|locale| messages.contains_production_locale(locale))
            .map_err(|error| locale_not_production_error(config_path, path, &error))?;
        targets.push(LogicalTarget {
            relative: candidate.relative,
            absolute: candidate.absolute,
            assignment,
        });
    }

    let (groups, metadata_failures) = group_physical_targets(targets, config_path)?;
    pending_failures.extend(metadata_failures);

    let mut prepared_groups = Vec::new();
    for group in groups {
        match prepare_group(group, registry, limits) {
            Ok(prepared) => prepared_groups.push(prepared),
            Err(failure) => pending_failures.push(*failure),
        }
    }

    let mut successful = Vec::new();
    for prepared in prepared_groups {
        match extract_group(prepared, registry) {
            Ok(extraction) => {
                if extraction.catalog.entries().iter().any(|entry| {
                    entry.catalog_key_domain() == ResourceCatalogKeyDomain::StandaloneMf2
                }) {
                    pending_failures.push(projection_invariant_failure(
                        extraction.prepared.primary_path(),
                        extraction.prepared.scope.clone(),
                    ));
                } else {
                    successful.push(extraction);
                }
            }
            Err(failure) => pending_failures.push(*failure),
        }
    }

    let observed_domains = admit_catalog_domains(&successful, config_path)?;
    admit_declared_domains(messages, &observed_domains, config_path)?;

    let mut artifacts = Vec::new();
    for extraction in successful {
        let order_path = extraction.prepared.primary_path().to_owned();
        let scope = extraction.prepared.scope.clone();
        match project_definition_artifact(extraction, limits) {
            Ok(artifact) => artifacts.push(artifact),
            Err(error) => pending_failures.push(PendingFailure {
                order_path: Some(order_path.clone()),
                failure: DefinitionSourceFailure {
                    affected_scopes: vec![scope].into_boxed_slice(),
                    error: projection_error(&order_path, error),
                },
            }),
        }
    }

    pending_failures.sort_by(|left, right| {
        compare_optional_paths(left.order_path.as_deref(), right.order_path.as_deref())
            .then_with(|| left.failure.error.code.cmp(right.failure.error.code))
    });

    Ok(DefinitionInventory {
        target_scopes: target_scopes.into_boxed_slice(),
        artifacts,
        failures: pending_failures
            .into_iter()
            .map(|pending| pending.failure)
            .collect(),
    })
}

fn discover_project_candidates(project_root: &Path, scopes: &[CatalogScopeId]) -> Discovery {
    let mut discovery = Discovery {
        candidates: Vec::new(),
        failures: Vec::new(),
    };
    discover_directory(project_root, project_root, scopes, &mut discovery);
    discovery.candidates.sort_by(|left, right| {
        compare_portable_path_str(left.relative.as_str(), right.relative.as_str())
    });
    discovery
        .candidates
        .dedup_by(|left, right| left.relative == right.relative);
    discovery
}

fn discover_directory(
    project_root: &Path,
    root_directory: &Path,
    scopes: &[CatalogScopeId],
    discovery: &mut Discovery,
) {
    let Some(root) = open_discovery_directory(project_root, root_directory, scopes, discovery)
    else {
        return;
    };
    let mut work = vec![root];

    while !work.is_empty() {
        let entry = work
            .last_mut()
            .expect("the discovery worklist is non-empty")
            .entries
            .next();
        let Some(entry) = entry else {
            work.pop();
            continue;
        };
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                let directory = &work
                    .last()
                    .expect("the current discovery directory remains present")
                    .directory;
                let label = project_relative_label(project_root, directory);
                discovery.failures.push(PendingFailure {
                    order_path: label.clone(),
                    failure: all_scope_failure(
                        scopes,
                        directory_read_error(label.as_deref(), &error),
                    ),
                });
                continue;
            }
        };
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                record_discovery_path_failure(project_root, &path, scopes, &error, discovery);
                continue;
            }
        };

        if file_type.is_dir() {
            // The child frame is processed before the current frame resumes,
            // retaining deterministic depth-first order without recursion.
            if let Some(child) = open_discovery_directory(project_root, &path, scopes, discovery) {
                work.push(child);
            }
            continue;
        }

        if file_type.is_symlink() {
            match fs::metadata(&path) {
                Ok(metadata) if metadata.is_dir() => {
                    // Directory symlinks never enter project traversal.
                    continue;
                }
                Ok(metadata) if metadata.is_file() => {}
                Ok(_) => continue,
                Err(_) => {
                    // Keep a broken selected file symlink as a candidate so
                    // assignment can decide whether it is authoritative.
                }
            }
        } else if !file_type.is_file() {
            continue;
        }

        let Some(relative) = project_relative_resource_path(project_root, &path) else {
            discovery.failures.push(PendingFailure {
                order_path: project_relative_label(
                    project_root,
                    path.parent().unwrap_or(project_root),
                ),
                failure: all_scope_failure(
                    scopes,
                    unrepresentable_discovery_error(project_root, &path),
                ),
            });
            continue;
        };
        discovery.candidates.push(LogicalCandidate {
            relative,
            absolute: path,
        });
    }
}

fn open_discovery_directory(
    project_root: &Path,
    directory: &Path,
    scopes: &[CatalogScopeId],
    discovery: &mut Discovery,
) -> Option<DiscoveryDirectory> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            let label = project_relative_label(project_root, directory);
            discovery.failures.push(PendingFailure {
                order_path: label.clone(),
                failure: all_scope_failure(scopes, directory_read_error(label.as_deref(), &error)),
            });
            return None;
        }
    };
    let mut entries = entries.collect::<Vec<_>>();
    entries.sort_by(compare_directory_results);
    Some(DiscoveryDirectory {
        directory: directory.to_owned(),
        entries: entries.into_iter(),
    })
}

fn record_discovery_path_failure(
    project_root: &Path,
    path: &Path,
    scopes: &[CatalogScopeId],
    error: &io::Error,
    discovery: &mut Discovery,
) {
    let label = project_relative_label(project_root, path);
    discovery.failures.push(PendingFailure {
        order_path: label.clone(),
        failure: all_scope_failure(scopes, metadata_error(label.as_deref(), error)),
    });
}

fn group_physical_targets(
    targets: Vec<LogicalTarget>,
    config_path: Option<&Path>,
) -> Result<(Vec<PhysicalGroup>, Vec<PendingFailure>), DefinitionInventoryError> {
    let mut grouped = BTreeMap::<FileId, Vec<LogicalTarget>>::new();
    let mut failures = Vec::new();

    for target in targets {
        match inspect_physical_identity(&target.absolute) {
            Ok(identity) => grouped.entry(identity).or_default().push(target),
            Err(error) => {
                let path = target.path().to_owned();
                failures.push(PendingFailure {
                    order_path: Some(path.clone()),
                    failure: DefinitionSourceFailure {
                        affected_scopes: vec![target.scope_id()].into_boxed_slice(),
                        error: metadata_error(Some(&path), &error),
                    },
                });
            }
        }
    }

    let mut groups = grouped
        .into_iter()
        .map(|(identity, mut targets)| {
            targets.sort_by(|left, right| compare_portable_path_str(left.path(), right.path()));
            PhysicalGroup { identity, targets }
        })
        .collect::<Vec<_>>();
    groups.sort_by(|left, right| {
        compare_portable_path_str(left.primary_path(), right.primary_path())
    });

    for group in &groups {
        admit_group_bindings(group, config_path)?;
    }
    Ok((groups, failures))
}

fn admit_group_bindings(
    group: &PhysicalGroup,
    config_path: Option<&Path>,
) -> Result<(), DefinitionInventoryError> {
    let expected = group.primary();
    for target in &group.targets[1..] {
        if target.assignment.catalog().classification()
            != expected.assignment.catalog().classification()
        {
            return Err(group_format_conflict_error(config_path, expected, target));
        }
        let field = if target.assignment.scope() != expected.assignment.scope() {
            Some(CatalogBindingConflictField::Scope)
        } else if target.assignment.locale() != expected.assignment.locale() {
            Some(CatalogBindingConflictField::Locale)
        } else {
            None
        };
        if let Some(field) = field {
            return Err(group_binding_conflict_error(
                config_path,
                expected,
                target,
                field,
            ));
        }
    }
    Ok(())
}

fn prepare_group(
    group: PhysicalGroup,
    registry: &HostFormatRegistry,
    limits: &LinkLimits,
) -> Result<PreparedGroup, Box<PendingFailure>> {
    let primary = group.primary();
    let scope = contract_scope_id(primary.assignment.scope().as_str())
        .expect("resource and contract scope grammars are coordinated");
    let locale = Locale::try_new(primary.assignment.locale().as_str())
        .expect("resource and contract locale grammars are coordinated");
    let resolved_format = registry
        .resolve_format(primary.assignment.catalog())
        .map_err(|error| {
            Box::new(PendingFailure {
                order_path: Some(primary.path().to_owned()),
                failure: DefinitionSourceFailure {
                    affected_scopes: vec![scope.clone()].into_boxed_slice(),
                    error: resource_error(primary.path(), None, &error),
                },
            })
        })?;

    let source_path = portable_path(primary.path())
        .map_err(|_| Box::new(portable_identity_failure(primary.path(), scope.clone())))?;
    let source_identity = SourceDocumentIdentity::new(ArtifactNamespace::Project, source_path);
    let aliases = group.targets[1..]
        .iter()
        .map(|target| portable_path(target.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Box::new(portable_identity_failure(primary.path(), scope.clone())))?;

    preflight_first_over_limit(
        LinkLimitCounter::CatalogScopeNameBytes,
        usize_u64(scope.name().as_str().len()),
        limits,
    )
    .and_then(|()| {
        preflight_first_over_limit(
            LinkLimitCounter::LocaleBytes,
            usize_u64(locale.as_str().len()),
            limits,
        )
    })
    .map_err(|error| {
        Box::new(PendingFailure {
            order_path: Some(primary.path().to_owned()),
            failure: DefinitionSourceFailure {
                affected_scopes: vec![scope.clone()].into_boxed_slice(),
                error: artifact_production_error(primary.path(), &error),
            },
        })
    })?;

    // Validate the complete source envelope before opening or parsing the host
    // document. The zero digest is only structural preflight evidence; the
    // actual source-derived digest replaces it during final projection.
    MessageDefinitionArtifact::try_new(
        ArtifactVersion::DRAFT_V0_1,
        definition_producer_identity(),
        source_identity.clone(),
        aliases.clone(),
        InputFingerprint::new(
            FingerprintAlgorithm::Blake3_256,
            FingerprintDigest::new([0; 32]),
        ),
        Vec::new(),
        limits,
    )
    .map_err(|error| {
        Box::new(PendingFailure {
            order_path: Some(primary.path().to_owned()),
            failure: DefinitionSourceFailure {
                affected_scopes: vec![scope.clone()].into_boxed_slice(),
                error: artifact_production_error(primary.path(), &error),
            },
        })
    })?;

    Ok(PreparedGroup {
        group,
        resolved_format,
        source_identity,
        aliases,
        scope,
        locale,
    })
}

fn extract_group(
    prepared: PreparedGroup,
    registry: &HostFormatRegistry,
) -> Result<SuccessfulExtraction, Box<PendingFailure>> {
    let primary_path = prepared.primary_path().to_owned();
    let scope = prepared.scope.clone();
    let snapshot = acquire_snapshot(&prepared).map_err(|error| {
        Box::new(PendingFailure {
            order_path: Some(primary_path.clone()),
            failure: DefinitionSourceFailure {
                affected_scopes: vec![scope].into_boxed_slice(),
                error,
            },
        })
    })?;
    let catalog = registry
        .extract(prepared.resolved_format.clone(), Arc::clone(&snapshot))
        .map_err(|error| {
            Box::new(PendingFailure {
                order_path: Some(primary_path.clone()),
                failure: DefinitionSourceFailure {
                    affected_scopes: vec![prepared.scope.clone()].into_boxed_slice(),
                    error: resource_error(&primary_path, Some(&snapshot), &error),
                },
            })
        })?;
    Ok(SuccessfulExtraction {
        prepared,
        snapshot,
        catalog,
    })
}

fn acquire_snapshot(prepared: &PreparedGroup) -> Result<Arc<str>, OperationalError> {
    if !group_identity_is_current(&prepared.group) {
        return Err(source_changed_error(prepared.primary_path()));
    }

    let primary = prepared.group.primary();
    let mut file =
        File::open(&primary.absolute).map_err(|error| input_read_error(primary.path(), &error))?;
    let before = file
        .metadata()
        .map_err(|error| input_read_error(primary.path(), &error))?;
    if !before.is_file() {
        return Err(source_changed_error(primary.path()));
    }
    let before = FileState::from_metadata(&before);

    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_HOST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| input_read_error(primary.path(), &error))?;
    if let Err(error) = preflight_host_bytes(bytes.len(), ResourcePhase::Extract) {
        return Err(resource_error(primary.path(), None, &error));
    }

    let after = file
        .metadata()
        .map_err(|_| source_changed_error(primary.path()))?;
    if before != FileState::from_metadata(&after) || !group_identity_is_current(&prepared.group) {
        return Err(source_changed_error(primary.path()));
    }

    let source = String::from_utf8(bytes)
        .map_err(|error| invalid_utf8_error(primary.path(), error.utf8_error().valid_up_to()))?;
    Ok(Arc::from(source))
}

fn group_identity_is_current(group: &PhysicalGroup) -> bool {
    group.targets.iter().all(|target| {
        inspect_physical_identity(&target.absolute).is_ok_and(|identity| identity == group.identity)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileState {
    len: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    modified_nanos: i128,
    #[cfg(unix)]
    changed_nanos: i128,
}

impl FileState {
    fn from_metadata(metadata: &Metadata) -> Self {
        Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            modified_nanos: unix_modified_nanos(metadata),
            #[cfg(unix)]
            changed_nanos: unix_changed_nanos(metadata),
        }
    }
}

#[cfg(unix)]
fn unix_modified_nanos(metadata: &Metadata) -> i128 {
    use std::os::unix::fs::MetadataExt;

    i128::from(metadata.mtime()) * 1_000_000_000 + i128::from(metadata.mtime_nsec())
}

#[cfg(unix)]
fn unix_changed_nanos(metadata: &Metadata) -> i128 {
    use std::os::unix::fs::MetadataExt;

    i128::from(metadata.ctime()) * 1_000_000_000 + i128::from(metadata.ctime_nsec())
}

fn admit_catalog_domains(
    extractions: &[SuccessfulExtraction],
    config_path: Option<&Path>,
) -> Result<BTreeMap<CatalogScopeId, ContractCatalogKeyDomain>, DefinitionInventoryError> {
    let mut observations = Vec::new();
    for extraction in extractions {
        let domains = distinct_catalog_domains(&extraction.catalog);
        for target in &extraction.prepared.group.targets {
            for definition in target.assignment.participating_definitions() {
                for &(entry_order, domain) in &domains {
                    observations.push(DomainObservation {
                        scope: extraction.prepared.scope.clone(),
                        domain,
                        definition: *definition,
                        target_path: target.path().to_owned(),
                        entry_order,
                    });
                }
            }
        }
    }
    admit_domain_observations(observations, config_path)
}

fn distinct_catalog_domains(
    catalog: &intlify_resource::ExtractedCatalog,
) -> Vec<(usize, ResourceCatalogKeyDomain)> {
    let mut first_entry_by_domain = BTreeMap::new();
    for (entry_order, entry) in catalog.entries().iter().enumerate() {
        first_entry_by_domain
            .entry(entry.catalog_key_domain())
            .or_insert(entry_order);
    }
    let mut domains = first_entry_by_domain
        .into_iter()
        .map(|(domain, entry_order)| (entry_order, domain))
        .collect::<Vec<_>>();
    domains.sort_by_key(|(entry_order, _)| *entry_order);
    domains
}

fn admit_domain_observations(
    mut observations: Vec<DomainObservation>,
    config_path: Option<&Path>,
) -> Result<BTreeMap<CatalogScopeId, ContractCatalogKeyDomain>, DefinitionInventoryError> {
    // Physical-group order cannot define this gate: an alias from an early
    // group may sort after another group's primary. Re-establish the
    // project-wide logical-path, definition, and raw-entry order explicitly.
    observations.sort_by(|left, right| {
        compare_portable_path_str(&left.target_path, &right.target_path)
            .then_with(|| {
                left.definition
                    .definition_index()
                    .cmp(&right.definition.definition_index())
            })
            .then_with(|| left.entry_order.cmp(&right.entry_order))
    });

    let mut first_by_scope = BTreeMap::<CatalogScopeId, DomainObservation>::new();
    for observation in observations {
        if let Some(first) = first_by_scope.get(&observation.scope) {
            if first.domain != observation.domain {
                return Err(catalog_domain_conflict_error(
                    config_path,
                    first,
                    &observation,
                ));
            }
        } else {
            first_by_scope.insert(observation.scope.clone(), observation);
        }
    }
    Ok(first_by_scope
        .into_iter()
        .map(|(scope, observation)| {
            (
                scope,
                contract_domain(observation.domain)
                    .expect("standalone resource domains were removed before admission"),
            )
        })
        .collect())
}

fn admit_declared_domains(
    messages: &ResolvedMessagesConfig,
    observed: &BTreeMap<CatalogScopeId, ContractCatalogKeyDomain>,
    config_path: Option<&Path>,
) -> Result<(), DefinitionInventoryError> {
    for declaration in messages.domain_declarations() {
        let Some(observed_domain) = observed.get(declaration.scope()) else {
            continue;
        };
        if declaration.domain() != *observed_domain {
            return Err(scope_domain_mismatch_error(
                config_path,
                declaration.pointer(),
                declaration.scope(),
                declaration.domain(),
                *observed_domain,
            ));
        }
    }
    Ok(())
}

fn project_definition_artifact(
    extraction: SuccessfulExtraction,
    limits: &LinkLimits,
) -> Result<MessageDefinitionArtifact, ProjectionError> {
    let domain = extraction.catalog.entries().first().map_or_else(
        || host_format_domain(extraction.prepared.resolved_format.format()),
        |entry| {
            contract_domain(entry.catalog_key_domain())
                .expect("standalone resource domains were removed before projection")
        },
    );

    let definitions = extraction
        .catalog
        .entries()
        .iter()
        .map(|entry| {
            let entry_domain = contract_domain(entry.catalog_key_domain())
                .expect("standalone resource domains were removed before projection");
            let key = CatalogKey::try_new(entry_domain, entry.catalog_key().as_str())
                .map_err(|_| ProjectionError::Invariant)?;
            let structural_path =
                EntryStructuralPath::try_new(entry.key().structural_path().as_str())
                    .map_err(|_| ProjectionError::Invariant)?;
            let message = MessagePayload::try_new(entry.message_text())
                .map_err(|_| ProjectionError::Invariant)?;
            MessageDefinition::try_new(
                extraction.prepared.scope.clone(),
                entry_domain,
                key,
                extraction.prepared.locale.clone(),
                message,
                EntryReference::new(structural_path, entry.key().occurrence()),
            )
            .map_err(|_| ProjectionError::Invariant)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let fingerprint = definition_input_fingerprint(
        ArtifactVersion::DRAFT_V0_1,
        &extraction.prepared.source_identity,
        &extraction.prepared.aliases,
        extraction.prepared.resolved_format.format(),
        &extraction.prepared.scope,
        &extraction.prepared.locale,
        domain,
        extraction.snapshot.as_bytes(),
    );

    MessageDefinitionArtifact::try_new(
        ArtifactVersion::DRAFT_V0_1,
        definition_producer_identity(),
        extraction.prepared.source_identity,
        extraction.prepared.aliases,
        fingerprint,
        definitions,
        limits,
    )
    .map_err(ProjectionError::Artifact)
}

fn definition_producer_identity() -> ProducerIdentity {
    static IDENTITY: OnceLock<ProducerIdentity> = OnceLock::new();
    IDENTITY
        .get_or_init(|| {
            ProducerIdentity::new(
                ProducerId::try_new(RESOURCE_DEFINITION_PRODUCER_ID)
                    .expect("the built-in definition producer ID is contract-valid"),
                ProducerRevision::try_new(RESOURCE_DEFINITION_PRODUCER_REVISION)
                    .expect("the build-derived definition producer revision is contract-valid"),
            )
        })
        .clone()
}

#[allow(clippy::too_many_arguments)]
fn definition_input_fingerprint(
    version: ArtifactVersion,
    source: &SourceDocumentIdentity,
    aliases: &[PortableRelativePath],
    host_format: HostFormat,
    scope: &CatalogScopeId,
    locale: &Locale,
    domain: ContractCatalogKeyDomain,
    source_bytes: &[u8],
) -> InputFingerprint {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"intlify\0definition-fingerprint\0blake3-256\0");
    hasher.update(&1_u16.to_be_bytes());

    write_hash_field(
        &mut hasher,
        0x01,
        &[version.major().to_be_bytes(), version.minor().to_be_bytes()].concat(),
    );
    write_source_identity_field(&mut hasher, 0x02, source);
    write_path_sequence_field(&mut hasher, 0x03, aliases);
    write_hash_field(&mut hasher, 0x04, host_format.as_str().as_bytes());
    write_scope_field(&mut hasher, 0x05, scope);
    write_hash_field(&mut hasher, 0x06, locale.as_str().as_bytes());
    write_hash_field(&mut hasher, 0x07, domain.as_str().as_bytes());
    write_hash_field(&mut hasher, 0x08, &[]);
    write_hash_field(&mut hasher, 0x09, source_bytes);

    InputFingerprint::new(
        FingerprintAlgorithm::Blake3_256,
        FingerprintDigest::new(*hasher.finalize().as_bytes()),
    )
}

fn write_hash_field(hasher: &mut blake3::Hasher, tag: u8, payload: &[u8]) {
    hasher.update(&[tag]);
    hasher.update(&usize_u64(payload.len()).to_be_bytes());
    hasher.update(payload);
}

fn write_source_identity_field(
    hasher: &mut blake3::Hasher,
    tag: u8,
    source: &SourceDocumentIdentity,
) {
    let path_len = path_payload_len(source.path());
    let payload_len = field_len(1) + field_len(path_len);
    hasher.update(&[tag]);
    hasher.update(&payload_len.to_be_bytes());
    write_hash_field(hasher, 0x01, &[namespace_discriminant(source.namespace())]);
    write_path_field(hasher, 0x02, source.path());
}

fn write_scope_field(hasher: &mut blake3::Hasher, tag: u8, scope: &CatalogScopeId) {
    let payload_len = field_len(1) + field_len(usize_u64(scope.name().as_str().len()));
    hasher.update(&[tag]);
    hasher.update(&payload_len.to_be_bytes());
    write_hash_field(hasher, 0x01, &[namespace_discriminant(scope.namespace())]);
    write_hash_field(hasher, 0x02, scope.name().as_str().as_bytes());
}

fn write_path_sequence_field(hasher: &mut blake3::Hasher, tag: u8, paths: &[PortableRelativePath]) {
    let payload_len = 8_u64
        + paths
            .iter()
            .map(|path| 8_u64 + path_payload_len(path))
            .sum::<u64>();
    hasher.update(&[tag]);
    hasher.update(&payload_len.to_be_bytes());
    hasher.update(&usize_u64(paths.len()).to_be_bytes());
    for path in paths {
        hasher.update(&path_payload_len(path).to_be_bytes());
        write_path_payload(hasher, path);
    }
}

fn write_path_field(hasher: &mut blake3::Hasher, tag: u8, path: &PortableRelativePath) {
    hasher.update(&[tag]);
    hasher.update(&path_payload_len(path).to_be_bytes());
    write_path_payload(hasher, path);
}

fn write_path_payload(hasher: &mut blake3::Hasher, path: &PortableRelativePath) {
    hasher.update(&usize_u64(path.segments().len()).to_be_bytes());
    for segment in path.segments() {
        hasher.update(&usize_u64(segment.as_str().len()).to_be_bytes());
        hasher.update(segment.as_str().as_bytes());
    }
}

fn path_payload_len(path: &PortableRelativePath) -> u64 {
    8_u64
        + path
            .segments()
            .iter()
            .map(|segment| 8_u64 + usize_u64(segment.as_str().len()))
            .sum::<u64>()
}

const fn field_len(payload_len: u64) -> u64 {
    1 + 8 + payload_len
}

fn usize_u64(value: usize) -> u64 {
    u64::try_from(value).expect("supported Rust slice lengths fit u64")
}

const fn namespace_discriminant(namespace: ArtifactNamespace) -> u8 {
    match namespace {
        ArtifactNamespace::Project => 0,
    }
}

fn host_format_domain(format: HostFormat) -> ContractCatalogKeyDomain {
    match format {
        HostFormat::Json => ContractCatalogKeyDomain::JsonPointer,
    }
}

fn contract_domain(domain: ResourceCatalogKeyDomain) -> Option<ContractCatalogKeyDomain> {
    match domain {
        ResourceCatalogKeyDomain::StandaloneMf2 => None,
        ResourceCatalogKeyDomain::JsonPointer => Some(ContractCatalogKeyDomain::JsonPointer),
        ResourceCatalogKeyDomain::YamlTypedPath => Some(ContractCatalogKeyDomain::YamlTypedPath),
        ResourceCatalogKeyDomain::Xliff12 => Some(ContractCatalogKeyDomain::Xliff12),
        ResourceCatalogKeyDomain::Xliff2 => Some(ContractCatalogKeyDomain::Xliff2),
    }
}

fn contract_scope_id(value: &str) -> Result<CatalogScopeId, ValueConstructionError> {
    Ok(CatalogScopeId::new(
        ArtifactNamespace::Project,
        ContractCatalogScopeName::try_new(value)?,
    ))
}

fn portable_path(path: &str) -> Result<PortableRelativePath, ValueConstructionError> {
    PortableRelativePath::try_new(
        path.split('/')
            .map(PortablePathSegment::try_new)
            .collect::<Result<Vec<_>, _>>()?,
    )
}

fn preflight_first_over_limit(
    counter: LinkLimitCounter,
    observed: u64,
    limits: &LinkLimits,
) -> Result<(), ArtifactContractError> {
    let limit = limits.effective_limit(counter);
    if observed <= limit {
        return Ok(());
    }
    let evidence =
        ArtifactLimitEvidence::try_new(counter, limit, LinkLimitObservation::Exact(limit + 1))
            .expect("scope and locale counters use first-over artifact evidence");
    Err(ArtifactContractError::Limit(evidence))
}

fn project_definition_ref(target: &LogicalTarget) -> CatalogDefinitionRef {
    *target
        .assignment
        .participating_definitions()
        .first()
        .expect("a linker target has a participating definition")
}

fn project_format_ref(target: &LogicalTarget) -> CatalogDefinitionRef {
    target
        .assignment
        .catalog()
        .assigning_definitions()
        .first()
        .copied()
        .unwrap_or_else(|| project_definition_ref(target))
}

fn inspect_physical_identity(path: &Path) -> io::Result<FileId> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::IsADirectory,
            "selected project inventory participant is not a file",
        ));
    }
    file_id::get_file_id(path)
}

fn project_relative_resource_path(
    project_root: &Path,
    path: &Path,
) -> Option<ProjectRelativeResourcePath> {
    let relative = path.strip_prefix(project_root).ok()?;
    let label = exact_slash_path(relative)?;
    ProjectRelativeResourcePath::try_from(label.as_str()).ok()
}

fn project_relative_label(project_root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(project_root).ok()?;
    if relative.as_os_str().is_empty() {
        return Some(".".to_owned());
    }
    exact_slash_path(relative)
}

fn exact_slash_path(path: &Path) -> Option<String> {
    let mut normalized = String::new();
    for component in path.components() {
        use std::path::Component;
        match component {
            Component::Prefix(prefix) => {
                normalized.push_str(&prefix.as_os_str().to_str()?.replace('\\', "/"));
            }
            Component::RootDir => {
                if !normalized.ends_with('/') {
                    normalized.push('/');
                }
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.is_empty() && !normalized.ends_with('/') {
                    normalized.push('/');
                }
                normalized.push_str("..");
            }
            Component::Normal(part) => {
                if !normalized.is_empty() && !normalized.ends_with('/') {
                    normalized.push('/');
                }
                normalized.push_str(part.to_str()?);
            }
        }
    }
    Some(normalized)
}

fn compare_portable_path_str(left: &str, right: &str) -> Ordering {
    let mut left = left.split('/');
    let mut right = right.split('/');
    loop {
        match (left.next(), right.next()) {
            (Some(left), Some(right)) => {
                let order = left.as_bytes().cmp(right.as_bytes());
                if !order.is_eq() {
                    return order;
                }
            }
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (None, None) => return Ordering::Equal,
        }
    }
}

fn compare_optional_paths(left: Option<&str>, right: Option<&str>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => compare_portable_path_str(left, right),
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn compare_directory_results(
    left: &Result<fs::DirEntry, io::Error>,
    right: &Result<fs::DirEntry, io::Error>,
) -> Ordering {
    match (left, right) {
        (Ok(left), Ok(right)) => compare_native_os_str(&left.file_name(), &right.file_name()),
        (Ok(_), Err(_)) => Ordering::Less,
        (Err(_), Ok(_)) => Ordering::Greater,
        (Err(_), Err(_)) => Ordering::Equal,
    }
}

#[cfg(unix)]
fn compare_native_os_str(left: &OsStr, right: &OsStr) -> Ordering {
    use std::os::unix::ffi::OsStrExt;

    left.as_bytes().cmp(right.as_bytes())
}

#[cfg(windows)]
fn compare_native_os_str(left: &OsStr, right: &OsStr) -> Ordering {
    use std::os::windows::ffi::OsStrExt;

    left.encode_wide().cmp(right.encode_wide())
}

#[cfg(not(any(unix, windows)))]
fn compare_native_os_str(left: &OsStr, right: &OsStr) -> Ordering {
    left.as_encoded_bytes().cmp(right.as_encoded_bytes())
}

fn all_scope_failure(
    scopes: &[CatalogScopeId],
    error: OperationalError,
) -> DefinitionSourceFailure {
    DefinitionSourceFailure {
        affected_scopes: scopes.to_vec().into_boxed_slice(),
        error,
    }
}

fn assignment_config_error(
    config_path: Option<&Path>,
    target_path: &str,
    error: &LinkerCatalogAssignmentError,
) -> DefinitionInventoryError {
    let details = match error {
        LinkerCatalogAssignmentError::Format(conflict) => json!({
            "reason": "catalog_format_conflict",
            "pointer": format!(
                "/resources/catalogs/{}",
                conflict.assignment().definition_index()
            ),
            "conflictingPointer": format!(
                "/resources/catalogs/{}",
                conflict.conflicting_assignment().definition_index()
            ),
            "targetPath": target_path
        }),
        LinkerCatalogAssignmentError::Locale(locale) => json!({
            "reason": "locale_capture_mismatch",
            "pointer": format!(
                "/resources/catalogs/{}/locale/pattern",
                locale.definition().definition_index()
            ),
            "targetPath": target_path
        }),
        LinkerCatalogAssignmentError::Binding(conflict) => {
            let field = binding_field_name(conflict.field());
            json!({
                "reason": "catalog_binding_conflict",
                "field": field,
                "pointer": format!(
                    "/resources/catalogs/{}/{}",
                    conflict.assignment().definition_index(),
                    field
                ),
                "conflictingPointer": format!(
                    "/resources/catalogs/{}/{}",
                    conflict.conflicting_assignment().definition_index(),
                    field
                ),
                "targetPath": target_path,
                "conflictingTargetPath": target_path
            })
        }
    };
    configuration_error(
        config_path,
        "Resource catalog definition assignment is inconsistent.",
        details,
    )
}

fn locale_not_production_error(
    config_path: Option<&Path>,
    target_path: &str,
    error: &intlify_resource::CatalogLocaleNotProduction,
) -> DefinitionInventoryError {
    configuration_error(
        config_path,
        "Resource catalog locale is outside the message production set.",
        json!({
            "reason": "catalog_locale_not_production",
            "pointer": format!(
                "/resources/catalogs/{}/locale/pattern",
                error.definition_index()
            ),
            "scope": error.scope().as_str(),
            "locale": error.locale().as_str(),
            "targetPath": target_path
        }),
    )
}

fn group_format_conflict_error(
    config_path: Option<&Path>,
    expected: &LogicalTarget,
    conflicting: &LogicalTarget,
) -> DefinitionInventoryError {
    let expected_definition = project_format_ref(expected);
    let conflicting_definition = project_format_ref(conflicting);
    configuration_error(
        config_path,
        "Physical catalog aliases resolve to different host formats.",
        json!({
            "reason": "catalog_format_conflict",
            "pointer": format!(
                "/resources/catalogs/{}",
                conflicting_definition.definition_index()
            ),
            "conflictingPointer": format!(
                "/resources/catalogs/{}",
                expected_definition.definition_index()
            ),
            "targetPath": conflicting.path(),
            "conflictingTargetPath": expected.path()
        }),
    )
}

fn group_binding_conflict_error(
    config_path: Option<&Path>,
    expected: &LogicalTarget,
    conflicting: &LogicalTarget,
    field: CatalogBindingConflictField,
) -> DefinitionInventoryError {
    let field = binding_field_name(field);
    let expected_definition = project_definition_ref(expected);
    let conflicting_definition = project_definition_ref(conflicting);
    configuration_error(
        config_path,
        "Physical catalog aliases resolve to different linker bindings.",
        json!({
            "reason": "catalog_binding_conflict",
            "field": field,
            "pointer": format!(
                "/resources/catalogs/{}/{}",
                conflicting_definition.definition_index(),
                field
            ),
            "conflictingPointer": format!(
                "/resources/catalogs/{}/{}",
                expected_definition.definition_index(),
                field
            ),
            "targetPath": conflicting.path(),
            "conflictingTargetPath": expected.path()
        }),
    )
}

const fn binding_field_name(field: CatalogBindingConflictField) -> &'static str {
    match field {
        CatalogBindingConflictField::Scope => "scope",
        CatalogBindingConflictField::Locale => "locale",
    }
}

fn catalog_domain_conflict_error(
    config_path: Option<&Path>,
    expected: &DomainObservation,
    conflicting: &DomainObservation,
) -> DefinitionInventoryError {
    configuration_error(
        config_path,
        "One catalog scope contains multiple catalog-key domains.",
        json!({
            "reason": "catalog_scope_key_domain_conflict",
            "pointer": format!(
                "/resources/catalogs/{}/scope",
                conflicting.definition.definition_index()
            ),
            "conflictingPointer": format!(
                "/resources/catalogs/{}/scope",
                expected.definition.definition_index()
            ),
            "scope": conflicting.scope.name().as_str(),
            "domain": resource_domain_name(conflicting.domain),
            "conflictingDomain": resource_domain_name(expected.domain),
            "targetPath": conflicting.target_path,
            "conflictingTargetPath": expected.target_path
        }),
    )
}

fn scope_domain_mismatch_error(
    config_path: Option<&Path>,
    pointer: &str,
    scope: &CatalogScopeId,
    domain: ContractCatalogKeyDomain,
    observed: ContractCatalogKeyDomain,
) -> DefinitionInventoryError {
    configuration_error(
        config_path,
        "A configured message domain differs from the catalog scope domain.",
        json!({
            "reason": "scope_key_domain_mismatch",
            "pointer": pointer,
            "scope": scope.name().as_str(),
            "domain": domain.as_str(),
            "observedDomain": observed.as_str()
        }),
    )
}

fn configuration_error(
    config_path: Option<&Path>,
    message: &str,
    details: Value,
) -> DefinitionInventoryError {
    DefinitionInventoryError {
        error: OperationalError {
            kind: "config",
            code: "config_validation_failed",
            message: message.to_owned(),
            path: config_path.and_then(|path| path.to_str().map(str::to_owned)),
            details: Some(details),
        },
    }
}

fn internal_inventory_error(config_path: Option<&Path>) -> DefinitionInventoryError {
    DefinitionInventoryError {
        error: OperationalError {
            kind: "internal",
            code: "internal_error",
            message: "Message definition inventory state is inconsistent.".to_owned(),
            path: config_path.and_then(|path| path.to_str().map(str::to_owned)),
            details: Some(json!({
                "reason": "message_artifact_invariant_failed"
            })),
        },
    }
}

fn portable_identity_failure(path: &str, scope: CatalogScopeId) -> PendingFailure {
    PendingFailure {
        order_path: Some(path.to_owned()),
        failure: DefinitionSourceFailure {
            affected_scopes: vec![scope].into_boxed_slice(),
            error: OperationalError {
                kind: "input",
                code: "input_path_unrepresentable",
                message: format!(
                    "A catalog path cannot be represented as a portable identity: {path}"
                ),
                path: Some(path.to_owned()),
                details: Some(json!({
                    "reason": "portable_path_invalid",
                    "source": "project_inventory"
                })),
            },
        },
    }
}

fn projection_invariant_failure(path: &str, scope: CatalogScopeId) -> PendingFailure {
    PendingFailure {
        order_path: Some(path.to_owned()),
        failure: DefinitionSourceFailure {
            affected_scopes: vec![scope].into_boxed_slice(),
            error: OperationalError {
                kind: "internal",
                code: "internal_error",
                message: "A resource catalog entry cannot enter definition projection.".to_owned(),
                path: Some(path.to_owned()),
                details: Some(json!({
                    "reason": "message_artifact_invariant_failed"
                })),
            },
        },
    }
}

fn projection_error(path: &str, error: ProjectionError) -> OperationalError {
    match error {
        ProjectionError::Artifact(error) => artifact_production_error(path, &error),
        ProjectionError::Invariant => OperationalError {
            kind: "internal",
            code: "internal_error",
            message: "Message definition projection violated an invariant.".to_owned(),
            path: Some(path.to_owned()),
            details: Some(json!({
                "reason": "message_artifact_invariant_failed"
            })),
        },
    }
}

fn artifact_production_error(path: &str, error: &ArtifactContractError) -> OperationalError {
    match error {
        ArtifactContractError::Limit(evidence) => {
            let observed = match evidence.observation() {
                LinkLimitObservation::Exact(value) => json!(value),
                LinkLimitObservation::ArithmeticOverflow => json!("arithmetic_overflow"),
            };
            OperationalError {
                kind: "input",
                code: "message_artifact_failed",
                message: format!("Message definition artifact production failed: {path}"),
                path: Some(path.to_owned()),
                details: Some(json!({
                    "kind": "limit",
                    "evidence": {
                        "counter": evidence.counter().as_str(),
                        "limit": evidence.effective_limit(),
                        "observed": observed
                    }
                })),
            }
        }
        ArtifactContractError::InvalidArtifact(_)
        | ArtifactContractError::UnsupportedVersion(_) => OperationalError {
            kind: "internal",
            code: "internal_error",
            message: "Message definition artifact construction violated an invariant.".to_owned(),
            path: Some(path.to_owned()),
            details: Some(json!({
                "reason": "message_artifact_invariant_failed"
            })),
        },
    }
}

fn directory_read_error(path: Option<&str>, error: &io::Error) -> OperationalError {
    let mut details = io_details(error);
    details.insert("reason".to_owned(), json!("directory_read_failed"));
    OperationalError {
        kind: "io",
        code: "input_read_failed",
        message: "Project inventory directory could not be read.".to_owned(),
        path: path.map(str::to_owned),
        details: Some(Value::Object(details)),
    }
}

fn metadata_error(path: Option<&str>, error: &io::Error) -> OperationalError {
    let mut details = io_details(error);
    details.insert("reason".to_owned(), json!("metadata_failed"));
    OperationalError {
        kind: "io",
        code: "input_read_failed",
        message: "Project inventory file metadata could not be inspected.".to_owned(),
        path: path.map(str::to_owned),
        details: Some(Value::Object(details)),
    }
}

fn input_read_error(path: &str, error: &io::Error) -> OperationalError {
    OperationalError {
        kind: "io",
        code: "input_read_failed",
        message: format!("Project inventory file could not be read: {path}"),
        path: Some(path.to_owned()),
        details: Some(Value::Object(io_details(error))),
    }
}

fn invalid_utf8_error(path: &str, valid_up_to: usize) -> OperationalError {
    OperationalError {
        kind: "io",
        code: "input_read_failed",
        message: format!("Project inventory file is not valid UTF-8: {path}"),
        path: Some(path.to_owned()),
        details: Some(json!({
            "reason": "invalid_utf8",
            "validUpTo": valid_up_to
        })),
    }
}

fn source_changed_error(path: &str) -> OperationalError {
    OperationalError {
        kind: "io",
        code: "input_read_failed",
        message: format!("Project inventory source changed during acquisition: {path}"),
        path: Some(path.to_owned()),
        details: Some(json!({
            "reason": "source_changed"
        })),
    }
}

fn unrepresentable_discovery_error(project_root: &Path, path: &Path) -> OperationalError {
    OperationalError {
        kind: "input",
        code: "input_path_unrepresentable",
        message: "A discovered project inventory path is not valid Unicode.".to_owned(),
        path: None,
        details: Some(json!({
            "reason": "non_unicode_path",
            "source": "discovery",
            "parentPath": project_relative_label(
                project_root,
                path.parent().unwrap_or(project_root)
            )
        })),
    }
}

fn io_details(error: &io::Error) -> Map<String, Value> {
    let mut details = Map::new();
    details.insert("ioKind".to_owned(), json!(normalized_io_kind(error)));
    if let Some(raw_os_error) = error.raw_os_error() {
        details.insert("rawOsError".to_owned(), json!(raw_os_error));
    }
    details
}

fn normalized_io_kind(error: &io::Error) -> &'static str {
    match error.kind() {
        io::ErrorKind::NotFound => "not_found",
        io::ErrorKind::PermissionDenied => "permission_denied",
        io::ErrorKind::IsADirectory => "not_file",
        io::ErrorKind::NotADirectory => "not_directory",
        _ => "unknown",
    }
}

const fn resource_domain_name(domain: ResourceCatalogKeyDomain) -> &'static str {
    match domain {
        ResourceCatalogKeyDomain::StandaloneMf2 => "standalone-mf2",
        ResourceCatalogKeyDomain::JsonPointer => "json-pointer",
        ResourceCatalogKeyDomain::YamlTypedPath => "yaml-typed-path",
        ResourceCatalogKeyDomain::Xliff12 => "xliff-1.2",
        ResourceCatalogKeyDomain::Xliff2 => "xliff-2",
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;
    use std::fs;
    use std::ops::Deref;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use intlify_contract::{LinkLimitCounter, LinkLimits};
    use intlify_resource::{
        CatalogAssignmentOrigin, CatalogKeyDomain as ResourceCatalogKeyDomain, HostFormatRegistry,
        LinkerCatalogResolution, ProjectRelativeResourcePath, ResourcesConfig,
    };
    use serde_json::{json, Value};

    use super::{
        acquire_snapshot, admit_domain_observations, compare_portable_path_str, contract_scope_id,
        discover_project_candidates, distinct_catalog_domains, group_physical_targets,
        prepare_group, produce_definition_inventory, DefinitionInventory, DomainObservation,
        LogicalTarget, RESOURCE_DEFINITION_PRODUCER_ID, RESOURCE_DEFINITION_PRODUCER_REVISION,
    };
    use crate::messages::validate_messages_config;

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new(name: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after the epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "intlify-message-inventory-{name}-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("temporary project root should be created");
            Self(path)
        }
    }

    impl Deref for TempRoot {
        type Target = Path;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write(root: &Path, relative: &str, source: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().expect("fixture parent"))
            .expect("fixture parent should be created");
        fs::write(path, source).expect("fixture should be written");
    }

    fn resolved(
        resources_value: &Value,
        messages_value: &Value,
    ) -> (
        intlify_resource::ResolvedResources,
        crate::messages::ResolvedMessagesConfig,
    ) {
        let resources = ResourcesConfig::validate(Some(resources_value))
            .expect("resource configuration should be valid")
            .resolve();
        let (_, messages) = validate_messages_config(Some(messages_value), &resources)
            .expect("messages configuration should be valid")
            .expect("messages configuration is present");
        (resources, messages)
    }

    fn run(root: &Path, resources_value: &Value, messages_value: &Value) -> DefinitionInventory {
        let (resources, messages) = resolved(resources_value, messages_value);
        produce_definition_inventory(
            root,
            Some(&root.join("intlify.config.json")),
            &resources,
            &messages,
            &HostFormatRegistry::new(),
            &LinkLimits::default(),
        )
        .expect("definition inventory should complete")
    }

    fn path_binding_resources(include: &str) -> Value {
        json!({
            "catalogs": [{
                "scope": "app",
                "include": [include],
                "locale": {
                    "from": "path",
                    "pattern": "locales/{locale}.json"
                }
            }]
        })
    }

    fn locales(values: &[&str]) -> Value {
        json!({ "locales": values })
    }

    fn artifact_path(artifact: &intlify_contract::MessageDefinitionArtifact) -> String {
        artifact
            .source()
            .path()
            .segments()
            .iter()
            .map(intlify_contract::PortablePathSegment::as_str)
            .collect::<Vec<_>>()
            .join("/")
    }

    fn digest_hex(artifact: &intlify_contract::MessageDefinitionArtifact) -> String {
        let mut digest = String::with_capacity(64);
        for byte in artifact.input_fingerprint().digest().as_bytes() {
            write!(&mut digest, "{byte:02x}").expect("writing to a String cannot fail");
        }
        digest
    }

    #[test]
    fn produces_one_definition_artifact_per_catalog_physical_source() {
        let root = TempRoot::new("basic");
        write(&root, "locales/ja.json", r#"{"hello":"こんにちは"}"#);
        write(&root, "locales/en.json", r#"{"hello":"Hello"}"#);

        let inventory = run(
            &root,
            &path_binding_resources("locales/*.json"),
            &locales(&["en", "ja"]),
        );

        assert_eq!(inventory.target_scopes().len(), 1);
        assert!(inventory.failures().is_empty());
        assert_eq!(
            inventory
                .artifacts()
                .iter()
                .map(artifact_path)
                .collect::<Vec<_>>(),
            ["locales/en.json", "locales/ja.json"]
        );
        for artifact in inventory.artifacts() {
            let definition = &artifact.definitions()[0];
            assert_eq!(
                artifact.producer().id().as_str(),
                RESOURCE_DEFINITION_PRODUCER_ID
            );
            assert_eq!(
                artifact.producer().revision().as_str(),
                RESOURCE_DEFINITION_PRODUCER_REVISION
            );
            assert_eq!(artifact.definitions().len(), 1);
            assert_eq!(definition.scope().name().as_str(), "app");
            assert_eq!(
                definition.domain(),
                intlify_contract::CatalogKeyDomain::JsonPointer
            );
            assert_eq!(definition.key().as_str(), "/hello");
            assert_eq!(
                definition.locale().as_str(),
                if artifact_path(artifact) == "locales/en.json" {
                    "en"
                } else {
                    "ja"
                }
            );
            assert_eq!(
                definition.message().as_str(),
                if artifact_path(artifact) == "locales/en.json" {
                    "Hello"
                } else {
                    "こんにちは"
                }
            );
            assert_eq!(definition.source().structural_path().as_str(), "/hello");
            assert_eq!(definition.source().occurrence(), 0);
            assert!(artifact.logical_aliases().is_empty());
        }
    }

    #[test]
    fn valid_zero_match_catalog_is_a_closed_empty_contribution() {
        let root = TempRoot::new("zero-match");
        let inventory = run(
            &root,
            &path_binding_resources("locales/*.json"),
            &locales(&["en"]),
        );

        assert_eq!(inventory.target_scopes().len(), 1);
        assert!(inventory.artifacts().is_empty());
        assert!(inventory.failures().is_empty());
    }

    #[test]
    fn valid_zero_entry_catalog_produces_one_complete_empty_artifact() {
        let root = TempRoot::new("zero-entry");
        write(&root, "locales/en.json", "{}");
        let inventory = run(
            &root,
            &path_binding_resources("locales/*.json"),
            &locales(&["en"]),
        );

        assert_eq!(inventory.artifacts().len(), 1);
        assert!(inventory.artifacts()[0].definitions().is_empty());
        assert!(inventory.failures().is_empty());
    }

    #[test]
    fn duplicate_structural_paths_preserve_raw_entry_occurrences() {
        let root = TempRoot::new("duplicate-entry");
        write(
            &root,
            "locales/en.json",
            r#"{"hello":"first","hello":"second"}"#,
        );
        let inventory = run(
            &root,
            &path_binding_resources("locales/*.json"),
            &locales(&["en"]),
        );

        let definitions = inventory.artifacts()[0].definitions();
        assert_eq!(definitions.len(), 2);
        assert_eq!(definitions[0].message().as_str(), "first");
        assert_eq!(definitions[1].message().as_str(), "second");
        assert_eq!(definitions[0].source().structural_path().as_str(), "/hello");
        assert_eq!(definitions[1].source().structural_path().as_str(), "/hello");
        assert_eq!(definitions[0].source().occurrence(), 0);
        assert_eq!(definitions[1].source().occurrence(), 1);
    }

    #[test]
    fn catalog_domain_observations_retain_only_the_first_entry_per_domain() {
        let registry = HostFormatRegistry::new();
        let resolved = registry.resolve_direct_extension(".json").unwrap();
        let catalog = registry
            .extract(
                resolved,
                Arc::from(r#"{"first":"one","second":"two","third":"three"}"#),
            )
            .unwrap();

        assert_eq!(
            distinct_catalog_domains(&catalog),
            vec![(0, ResourceCatalogKeyDomain::JsonPointer)]
        );
    }

    #[test]
    fn authoritative_inventory_does_not_apply_hidden_or_dependency_defaults() {
        let root = TempRoot::new("authoritative");
        write(&root, ".hidden/messages.json", r#"{"hidden":"yes"}"#);
        write(
            &root,
            "node_modules/example/messages.json",
            r#"{"dependency":"yes"}"#,
        );
        let inventory = run(
            &root,
            &json!({
                "catalogs": [{
                    "scope": "app",
                    "include": [".hidden/*.json", "node_modules/**/*.json"],
                    "locale": { "from": "fixed", "value": "en" }
                }]
            }),
            &locales(&["en"]),
        );

        assert_eq!(
            inventory
                .artifacts()
                .iter()
                .map(artifact_path)
                .collect::<Vec<_>>(),
            [
                ".hidden/messages.json",
                "node_modules/example/messages.json"
            ]
        );
        assert!(inventory.failures().is_empty());
    }

    #[test]
    fn hard_link_aliases_produce_one_artifact_and_one_definition_set() {
        let root = TempRoot::new("hard-link");
        write(&root, "locales/messages.json", r#"{"hello":"Hello"}"#);
        fs::create_dir_all(root.join("aliases")).unwrap();
        fs::hard_link(
            root.join("locales/messages.json"),
            root.join("aliases/messages.json"),
        )
        .expect("hard-link alias should be created");

        let inventory = run(
            &root,
            &json!({
                "catalogs": [{
                    "scope": "app",
                    "include": ["locales/*.json", "aliases/*.json"],
                    "locale": { "from": "fixed", "value": "en" }
                }]
            }),
            &locales(&["en"]),
        );

        assert_eq!(inventory.artifacts().len(), 1);
        let artifact = &inventory.artifacts()[0];
        assert_eq!(artifact_path(artifact), "aliases/messages.json");
        assert_eq!(artifact.logical_aliases().len(), 1);
        assert_eq!(
            artifact.logical_aliases()[0]
                .segments()
                .iter()
                .map(intlify_contract::PortablePathSegment::as_str)
                .collect::<Vec<_>>(),
            ["locales", "messages.json"]
        );
        assert_eq!(artifact.definitions().len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn directory_symlinks_are_not_followed() {
        use std::os::unix::fs::symlink;

        let root = TempRoot::new("directory-symlink");
        write(&root, "actual/messages.json", r#"{"hello":"Hello"}"#);
        symlink(root.join("actual"), root.join("linked")).expect("directory symlink");

        let inventory = run(
            &root,
            &json!({
                "catalogs": [{
                    "scope": "app",
                    "include": ["linked/**/*.json"],
                    "locale": { "from": "fixed", "value": "en" }
                }]
            }),
            &locales(&["en"]),
        );

        assert!(inventory.artifacts().is_empty());
        assert!(inventory.failures().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn selected_file_symlinks_enter_physical_grouping() {
        use std::os::unix::fs::symlink;

        let root = TempRoot::new("file-symlink");
        write(&root, "actual/messages.json", r#"{"hello":"Hello"}"#);
        fs::create_dir_all(root.join("links")).unwrap();
        symlink(
            root.join("actual/messages.json"),
            root.join("links/messages.json"),
        )
        .expect("file symlink");

        let inventory = run(
            &root,
            &json!({
                "catalogs": [{
                    "scope": "app",
                    "include": ["links/*.json"],
                    "locale": { "from": "fixed", "value": "en" }
                }]
            }),
            &locales(&["en"]),
        );

        assert_eq!(inventory.artifacts().len(), 1);
        assert_eq!(
            artifact_path(&inventory.artifacts()[0]),
            "links/messages.json"
        );
        assert!(inventory.failures().is_empty());
    }

    #[test]
    fn extraction_failure_remains_visible_while_other_groups_project() {
        let root = TempRoot::new("extraction-failure");
        write(&root, "catalogs/good.json", r#"{"hello":"Hello"}"#);
        write(&root, "catalogs/bad.json", "{ broken");
        let inventory = run(
            &root,
            &json!({
                "catalogs": [{
                    "scope": "app",
                    "include": ["catalogs/*.json"],
                    "locale": { "from": "fixed", "value": "en" }
                }]
            }),
            &locales(&["en"]),
        );

        assert_eq!(inventory.artifacts().len(), 1);
        assert_eq!(
            artifact_path(&inventory.artifacts()[0]),
            "catalogs/good.json"
        );
        assert_eq!(inventory.failures().len(), 1);
        assert_eq!(inventory.failures()[0].affected_scopes().len(), 1);
        assert_eq!(
            inventory.failures()[0].error().code,
            "resource_parse_failed"
        );
    }

    #[test]
    fn concrete_path_locale_must_belong_to_the_production_set() {
        let root = TempRoot::new("locale-membership");
        write(&root, "locales/fr.json", r#"{"hello":"Bonjour"}"#);
        let resources_value = path_binding_resources("locales/*.json");
        let messages_value = locales(&["en"]);
        let (resources, messages) = resolved(&resources_value, &messages_value);

        let error = produce_definition_inventory(
            &root,
            Some(&root.join("intlify.config.json")),
            &resources,
            &messages,
            &HostFormatRegistry::new(),
            &LinkLimits::default(),
        )
        .unwrap_err();
        let details = error.operational_error().details.as_ref().unwrap();
        assert_eq!(details["reason"], "catalog_locale_not_production");
        assert_eq!(details["targetPath"], "locales/fr.json");
    }

    #[test]
    fn physical_alias_binding_conflict_is_configuration_failure() {
        let root = TempRoot::new("binding-conflict");
        write(&root, "one/messages.json", r#"{"hello":"Hello"}"#);
        fs::create_dir_all(root.join("two")).unwrap();
        fs::hard_link(
            root.join("one/messages.json"),
            root.join("two/messages.json"),
        )
        .unwrap();
        let (resources, messages) = resolved(
            &json!({
                "catalogs": [
                    {
                        "scope": "app",
                        "include": ["one/*.json"],
                        "locale": { "from": "fixed", "value": "en" }
                    },
                    {
                        "scope": "other",
                        "include": ["two/*.json"],
                        "locale": { "from": "fixed", "value": "en" }
                    }
                ]
            }),
            &locales(&["en"]),
        );

        let error = produce_definition_inventory(
            &root,
            Some(&root.join("intlify.config.json")),
            &resources,
            &messages,
            &HostFormatRegistry::new(),
            &LinkLimits::default(),
        )
        .unwrap_err();
        let details = error.operational_error().details.as_ref().unwrap();
        assert_eq!(details["reason"], "catalog_binding_conflict");
        assert_eq!(details["field"], "scope");
        assert_eq!(details["targetPath"], "two/messages.json");
        assert_eq!(details["conflictingTargetPath"], "one/messages.json");
    }

    #[test]
    fn physical_alias_format_conflict_uses_format_assignment_evidence() {
        let root = TempRoot::new("format-conflict");
        write(&root, "a/messages.json", r#"{"hello":"Hello"}"#);
        fs::create_dir_all(root.join("b")).unwrap();
        fs::hard_link(root.join("a/messages.json"), root.join("b/messages.yaml")).unwrap();
        let (resources, messages) = resolved(
            &json!({
                "catalogs": [
                    {
                        "include": ["a/*.json"],
                        "format": "json"
                    },
                    {
                        "scope": "app",
                        "include": ["a/*.json"],
                        "locale": { "from": "fixed", "value": "en" }
                    },
                    {
                        "scope": "app",
                        "include": ["b/*.yaml"],
                        "locale": { "from": "fixed", "value": "en" }
                    }
                ]
            }),
            &locales(&["en"]),
        );

        let error = produce_definition_inventory(
            &root,
            Some(&root.join("intlify.config.json")),
            &resources,
            &messages,
            &HostFormatRegistry::new(),
            &LinkLimits::default(),
        )
        .unwrap_err();
        let details = error.operational_error().details.as_ref().unwrap();
        assert_eq!(details["reason"], "catalog_format_conflict");
        assert_eq!(details["pointer"], "/resources/catalogs/2");
        assert_eq!(details["conflictingPointer"], "/resources/catalogs/0");
        assert_eq!(details["targetPath"], "b/messages.yaml");
        assert_eq!(details["conflictingTargetPath"], "a/messages.json");
    }

    #[cfg(unix)]
    #[test]
    fn physical_identity_change_before_snapshot_is_source_changed() {
        use std::os::unix::fs::symlink;

        let root = TempRoot::new("source-changed");
        write(&root, "actual/first.json", r#"{"first":"value"}"#);
        write(&root, "actual/second.json", r#"{"second":"value"}"#);
        fs::create_dir_all(root.join("links")).unwrap();
        let logical_path = root.join("links/messages.json");
        symlink(root.join("actual/first.json"), &logical_path).unwrap();

        let resources = ResourcesConfig::validate(Some(&json!({
            "catalogs": [{
                "scope": "app",
                "include": ["links/*.json"],
                "locale": { "from": "fixed", "value": "en" }
            }]
        })))
        .unwrap()
        .resolve();
        let scope = contract_scope_id("app").unwrap();
        let mut discovery = discover_project_candidates(&root, std::slice::from_ref(&scope));
        assert!(discovery.failures.is_empty());
        let candidate = discovery.candidates.pop().unwrap();
        let LinkerCatalogResolution::Matched(assignment) =
            resources.resolve_linker_path(&candidate.relative).unwrap()
        else {
            panic!("logical symlink should remain a selected catalog")
        };
        let (mut groups, failures) = group_physical_targets(
            vec![LogicalTarget {
                relative: candidate.relative,
                absolute: candidate.absolute,
                assignment,
            }],
            None,
        )
        .unwrap();
        assert!(failures.is_empty());
        let prepared = prepare_group(
            groups.pop().unwrap(),
            &HostFormatRegistry::new(),
            &LinkLimits::default(),
        )
        .unwrap();

        fs::remove_file(&logical_path).unwrap();
        symlink(root.join("actual/second.json"), &logical_path).unwrap();
        let error = acquire_snapshot(&prepared).unwrap_err();
        assert_eq!(error.code, "input_read_failed");
        assert_eq!(error.details.as_ref().unwrap()["reason"], "source_changed");
        assert_eq!(error.path.as_deref(), Some("links/messages.json"));
    }

    #[test]
    fn recognizer_domain_gate_uses_canonical_callee_order() {
        let root = TempRoot::new("recognizer-domain");
        write(&root, "locales/en.json", r#"{"hello":"Hello"}"#);
        let (resources, messages) = resolved(
            &path_binding_resources("locales/*.json"),
            &json!({
                "locales": ["en"],
                "producers": {
                    "js": {
                        "include": ["src/*.ts"],
                        "recognizers": {
                            "z": {
                                "kind": "lookup",
                                "scope": "app",
                                "domain": "xliff-2",
                                "keySyntax": "canonical"
                            },
                            "a": {
                                "kind": "lookup",
                                "scope": "app",
                                "domain": "yaml-typed-path",
                                "keySyntax": "canonical"
                            }
                        }
                    }
                }
            }),
        );

        let error = produce_definition_inventory(
            &root,
            Some(&root.join("intlify.config.json")),
            &resources,
            &messages,
            &HostFormatRegistry::new(),
            &LinkLimits::default(),
        )
        .unwrap_err();
        let details = error.operational_error().details.as_ref().unwrap();
        assert_eq!(details["reason"], "scope_key_domain_mismatch");
        assert_eq!(
            details["pointer"],
            "/messages/producers/js/recognizers/a/domain"
        );
        assert_eq!(details["observedDomain"], "json-pointer");
    }

    #[test]
    fn configured_root_domain_gate_preserves_submitted_array_order() {
        let root = TempRoot::new("root-domain");
        write(&root, "locales/en.json", r#"{"hello":"Hello"}"#);
        let (resources, messages) = resolved(
            &path_binding_resources("locales/*.json"),
            &json!({
                "locales": ["en"],
                "roots": [
                    {
                        "scope": "app",
                        "domain": "xliff-2",
                        "selector": { "kind": "all-in-scope" }
                    },
                    {
                        "scope": "app",
                        "domain": "yaml-typed-path",
                        "selector": { "kind": "all-in-scope" }
                    }
                ]
            }),
        );

        let error = produce_definition_inventory(
            &root,
            Some(&root.join("intlify.config.json")),
            &resources,
            &messages,
            &HostFormatRegistry::new(),
            &LinkLimits::default(),
        )
        .unwrap_err();
        assert_eq!(
            error.operational_error().details.as_ref().unwrap()["pointer"],
            "/messages/roots/0/domain"
        );
    }

    #[test]
    fn domain_configuration_failure_wins_over_extraction_failure() {
        let root = TempRoot::new("domain-over-source");
        write(&root, "catalogs/a-bad.json", "{ broken");
        write(&root, "catalogs/b-good.json", r#"{"hello":"Hello"}"#);
        let (resources, messages) = resolved(
            &json!({
                "catalogs": [{
                    "scope": "app",
                    "include": ["catalogs/*.json"],
                    "locale": { "from": "fixed", "value": "en" }
                }]
            }),
            &json!({
                "locales": ["en"],
                "roots": [{
                    "scope": "app",
                    "domain": "yaml-typed-path",
                    "selector": { "kind": "all-in-scope" }
                }]
            }),
        );

        let error = produce_definition_inventory(
            &root,
            Some(&root.join("intlify.config.json")),
            &resources,
            &messages,
            &HostFormatRegistry::new(),
            &LinkLimits::default(),
        )
        .unwrap_err();
        let operational = error.operational_error();
        assert_eq!(operational.kind, "config");
        assert_eq!(
            operational.details.as_ref().unwrap()["reason"],
            "scope_key_domain_mismatch"
        );
    }

    #[test]
    fn lower_locale_limit_fails_before_definition_artifact_projection() {
        let root = TempRoot::new("lower-limit");
        write(&root, "catalogs/messages.json", r#"{"hello":"Hello"}"#);
        let (resources, messages) = resolved(
            &json!({
                "catalogs": [{
                    "scope": "app",
                    "include": ["catalogs/*.json"],
                    "locale": { "from": "fixed", "value": "en" }
                }]
            }),
            &locales(&["en"]),
        );
        let limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::LocaleBytes, 1)
            .unwrap();

        let inventory = produce_definition_inventory(
            &root,
            Some(&root.join("intlify.config.json")),
            &resources,
            &messages,
            &HostFormatRegistry::new(),
            &limits,
        )
        .unwrap();
        assert!(inventory.artifacts().is_empty());
        assert_eq!(inventory.failures().len(), 1);
        assert_eq!(
            inventory.failures()[0].error().details.as_ref().unwrap()["kind"],
            "limit"
        );
        assert_eq!(
            inventory.failures()[0].error().details.as_ref().unwrap()["evidence"]["counter"],
            "locale_bytes"
        );
    }

    #[test]
    fn fingerprint_is_deterministic_and_covers_exact_host_bytes() {
        let root = TempRoot::new("fingerprint");
        let resources = json!({
            "catalogs": [{
                "scope": "app",
                "include": ["catalogs/*.json"],
                "locale": { "from": "fixed", "value": "en" }
            }]
        });
        write(&root, "catalogs/messages.json", r#"{"hello":"Hello"}"#);
        let first = run(&root, &resources, &locales(&["en"]));
        let second = run(&root, &resources, &locales(&["en"]));
        let first_digest = digest_hex(&first.artifacts()[0]);
        assert_eq!(first_digest, digest_hex(&second.artifacts()[0]));
        // Cross-version framing vector: update only with an intentional
        // fingerprint framing change and corresponding version bump.
        assert_eq!(
            first_digest,
            "a0fdf954bcdd85c7596d509b852cbdb14db45552e0178f2d9a079ef3267ca9b4"
        );

        write(&root, "catalogs/messages.json", "{ \"hello\": \"Hello\" }");
        let changed = run(&root, &resources, &locales(&["en"]));
        assert_ne!(first_digest, digest_hex(&changed.artifacts()[0]));
    }

    #[test]
    fn fingerprint_covers_resolved_scope_locale_and_alias_set() {
        let root = TempRoot::new("fingerprint-tuple");
        write(&root, "a/messages.json", r#"{"hello":"Hello"}"#);
        let catalog = |scope: &str, locale: &str, includes: &[&str]| {
            json!({
                "catalogs": [{
                    "scope": scope,
                    "include": includes,
                    "locale": { "from": "fixed", "value": locale }
                }]
            })
        };

        let base = run(
            &root,
            &catalog("app", "en", &["a/*.json"]),
            &locales(&["en", "ja"]),
        );
        let base_digest = digest_hex(&base.artifacts()[0]);
        let different_scope = run(
            &root,
            &catalog("other", "en", &["a/*.json"]),
            &locales(&["en", "ja"]),
        );
        let different_locale = run(
            &root,
            &catalog("app", "ja", &["a/*.json"]),
            &locales(&["en", "ja"]),
        );

        fs::create_dir_all(root.join("z")).unwrap();
        fs::hard_link(root.join("a/messages.json"), root.join("z/messages.json")).unwrap();
        let with_alias = run(
            &root,
            &catalog("app", "en", &["a/*.json", "z/*.json"]),
            &locales(&["en", "ja"]),
        );

        assert_ne!(base_digest, digest_hex(&different_scope.artifacts()[0]));
        assert_ne!(base_digest, digest_hex(&different_locale.artifacts()[0]));
        assert_ne!(base_digest, digest_hex(&with_alias.artifacts()[0]));
    }

    #[test]
    fn catalog_domain_conflict_selects_first_later_observation() {
        let resources = ResourcesConfig::validate(Some(&json!({
            "catalogs": [
                {
                    "scope": "app",
                    "include": ["a.json"],
                    "locale": { "from": "fixed", "value": "en" }
                },
                {
                    "scope": "app",
                    "include": ["b.json"],
                    "locale": { "from": "fixed", "value": "en" }
                },
                {
                    "scope": "app",
                    "include": ["z.json"],
                    "locale": { "from": "fixed", "value": "en" }
                }
            ]
        })))
        .unwrap()
        .resolve();
        let definition = |path: &str| {
            let path = ProjectRelativeResourcePath::try_from(path).unwrap();
            let LinkerCatalogResolution::Matched(assignment) =
                resources.resolve_linker_path(&path).unwrap()
            else {
                panic!("path should match")
            };
            assignment.participating_definitions()[0]
        };
        let scope = contract_scope_id("app").unwrap();
        let error = admit_domain_observations(
            vec![
                DomainObservation {
                    scope: scope.clone(),
                    domain: intlify_resource::CatalogKeyDomain::YamlTypedPath,
                    definition: definition("b.json"),
                    target_path: "b.json".to_owned(),
                    entry_order: 0,
                },
                DomainObservation {
                    scope: scope.clone(),
                    domain: intlify_resource::CatalogKeyDomain::Xliff2,
                    definition: definition("z.json"),
                    target_path: "z.json".to_owned(),
                    entry_order: 0,
                },
                DomainObservation {
                    scope,
                    domain: intlify_resource::CatalogKeyDomain::JsonPointer,
                    definition: definition("a.json"),
                    target_path: "a.json".to_owned(),
                    entry_order: 0,
                },
            ],
            None,
        )
        .unwrap_err();
        let details = error.operational_error().details.as_ref().unwrap();
        assert_eq!(details["reason"], "catalog_scope_key_domain_conflict");
        assert_eq!(details["targetPath"], "b.json");
        assert_eq!(details["conflictingTargetPath"], "a.json");
        assert_eq!(
            definition("a.json").origin(),
            CatalogAssignmentOrigin::Project
        );
    }

    #[test]
    fn portable_path_order_compares_segments_before_following_segments() {
        assert_eq!(
            compare_portable_path_str("a/z.json", "a-/x.json"),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_portable_path_str("a", "a/child"),
            std::cmp::Ordering::Less
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_discovery_keeps_definition_world_partial() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let root = TempRoot::new("non-unicode");
        let filename = OsString::from_vec(vec![b'm', b'e', 0xff, b'.', b'j', b's', b'o', b'n']);
        if fs::write(root.join(filename), b"{}").is_err() {
            // Some Unix filesystems, including common macOS volumes, reject
            // non-UTF-8 directory entries before the inventory can observe one.
            return;
        }
        let inventory = run(
            &root,
            &json!({
                "catalogs": [{
                    "scope": "app",
                    "include": ["*.json"],
                    "locale": { "from": "fixed", "value": "en" }
                }]
            }),
            &locales(&["en"]),
        );

        assert!(inventory.artifacts().is_empty());
        assert_eq!(inventory.failures().len(), 1);
        assert_eq!(
            inventory.failures()[0].error().code,
            "input_path_unrepresentable"
        );
        assert_eq!(inventory.failures()[0].affected_scopes().len(), 1);
    }

    #[test]
    fn inventory_values_are_thread_transferable() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<DefinitionInventory>();
        assert_send::<super::DefinitionSourceFailure>();
        assert_sync::<crate::messages::ResolvedMessagesConfig>();
    }
}
