//! Durable provider-neutral source configuration and fenced discovery contracts.
use crate::{PublishedRepository, StoreError};
use async_trait::async_trait;
use octacity_server_domain::{EntityKind, ProjectId};
use octacity_server_factory::{
  FactoryConfiguration, FactoryDigest, WorkSourceGroup, WorkSourceGroupId, WorkSourceGroupVersion, WorkSourceInput,
  WorkSourceInputId, WorkSourceInputVersion,
};

/// Shared hard ceiling for normalized snapshots and each encoded private source input.
pub const MAX_SOURCE_INPUT_BYTES: u32 = 1_048_576;
/// Shared UTF-8 title ceiling; installed mappings can choose a smaller value.
pub const MAX_SOURCE_TITLE_BYTES: u32 = 4_096;
/// Shared UTF-8 body/field ceiling; installed mappings can choose a smaller value.
pub const MAX_SOURCE_BODY_BYTES: u32 = 262_144;
/// Hard private snapshot/materialization lifetime (30 days); retries never renew it.
pub const MAX_SOURCE_RETENTION_MILLIS: u64 = 2_592_000_000;

/// One immutable input publication with a compare-and-swap precondition.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct WorkSourceInputChange {
  /// Current version, or absence for a new stable identity.
  pub expected_version: Option<WorkSourceInputVersion>,
  /// Next resolved immutable version.
  pub input: WorkSourceInput,
}
/// One immutable exclusive group publication.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct WorkSourceGroupChange {
  /// Current revision, or absence for a new group.
  pub expected_version: Option<WorkSourceGroupVersion>,
  /// Next revision with pinned input definitions.
  pub group: WorkSourceGroup,
}
/// Bounded atomic publication of inputs and an optional exclusive group.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PublishWorkSources {
  /// Existing owner of every published resource.
  pub project_id: ProjectId,
  /// At most 64 distinct input identities.
  pub inputs: Vec<WorkSourceInputChange>,
  /// Group whose members must be the exact current inputs after publication.
  pub group: Option<WorkSourceGroupChange>,
}
impl PublishWorkSources {
  /// Validates bounds, ownership and next-version relationships without I/O.
  pub fn validate(&self) -> Result<(), StoreError> {
    let mut seen = std::collections::BTreeSet::new();
    if self.inputs.len() > 64 || (self.inputs.is_empty() && self.group.is_none()) {
      return Err(source_conflict());
    }
    for change in &self.inputs {
      if change.input.project_id() != self.project_id
        || !seen.insert(change.input.id())
        || change.input.version().get() != next(change.expected_version.map(|v| v.get()))?
      {
        return Err(source_conflict());
      }
    }
    if let Some(change) = &self.group
      && (change.group.project_id() != self.project_id
        || change.group.version().get() != next(change.expected_version.map(|v| v.get()))?)
    {
      return Err(source_conflict());
    }
    Ok(())
  }
}
fn next(version: Option<u64>) -> Result<u64, StoreError> {
  version.unwrap_or(0).checked_add(1).ok_or_else(source_conflict)
}
/// Checks exact stored target and repository content rather than trusting a restored document.
pub fn validate_source_input_dependencies(
  input: &WorkSourceInput,
  target: &FactoryConfiguration,
  repository: &PublishedRepository,
) -> Result<(), StoreError> {
  let definition = input.definition();
  if definition.target != *target.reference()
    || input.root_flow() != target.flow().closure.root()
    || (definition.enabled && !target.is_enabled())
    || repository.project_id != input.project_id()
    || definition.repository.id != repository.id
    || definition.repository.version != repository.version
    || definition.repository.definition_digest
      != FactoryDigest::content_sha256(
        &serde_json::to_vec(&repository.definition).map_err(|_| StoreError::Unavailable)?,
      )
  {
    return Err(source_conflict());
  }
  Ok(())
}
/// Safe conflict used for stale, foreign or mismatched source state.
pub fn source_conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::FactoryConfiguration,
  }
}

/// Trusted persistence seam; callers resolve and authorize installed capabilities before publication.
#[async_trait]
pub trait FactorySourceStore: crate::FactoryAdmissionStore {
  /// Coordinates technical single-write reporting under durable source-item ordering.
  /// Backends without this capability fail closed without contacting a provider.
  async fn coordinate_source_projection(
    &self,
    _request: crate::SourceProjectionRequest,
  ) -> Result<crate::SourceProjectionState, StoreError> {
    Err(StoreError::Unavailable)
  }
  /// Resolves the original settled source input for this exact Project-owned Work.
  /// Manual/unassociated Work returns None; current input replacement is irrelevant.
  async fn source_preparation_for_work(
    &self,
    project: ProjectId,
    work: octacity_server_factory::WorkEnvelopeId,
  ) -> Result<Option<WorkSourcePreparation>, StoreError>;
  /// Lists bounded safe stage associations without reading private candidate snapshots.
  async fn source_issue_correlations(
    &self,
    project: ProjectId,
    issue: FactoryDigest,
    after: Option<WorkSourceInputId>,
    limit: u16,
  ) -> Result<Vec<WorkSourceIssueAssociation>, StoreError>;
  /// Atomically checks ownership, admits through the existing Work authority, and records correlation.
  async fn admit_prepared_source_work(
    &self,
    guard: AdmitPreparedWorkSource,
    request: crate::ManagementMutation<crate::AdmitFactoryWork>,
  ) -> Result<crate::FactoryAdmissionMutationOutcome, StoreError>;
  /// Lists a bounded private pending page for the current enabled members, in stable operation order.
  /// Original scan provenance does not prevent recovery after an input joins or leaves a group.
  /// Disabled/removed members remain retained but cannot block this target's progress.
  async fn pending_source_preparations(
    &self,
    project: ProjectId,
    target: WorkSourceScanTarget,
    after: Option<FactoryDigest>,
    limit: u16,
  ) -> Result<Vec<WorkSourcePreparation>, StoreError>;
  /// Freezes normalized artifact and source intent exactly once under a current owner.
  async fn publish_source_preparation_intent(
    &self,
    request: PublishSourcePreparationIntent,
  ) -> Result<WorkSourcePreparation, StoreError>;
  /// Reserves the first immutable candidate under a live current scan fence.
  async fn reserve_source_preparation(
    &self,
    request: ReserveWorkSourcePreparation,
  ) -> Result<WorkSourcePreparation, StoreError>;
  /// Reads original private preparation by exact Project and operation identity.
  async fn source_preparation(
    &self,
    project: ProjectId,
    id: FactoryDigest,
  ) -> Result<WorkSourcePreparation, StoreError>;
  /// Lists at most 64 current enabled independent inputs/groups, excluding grouped members.
  /// An obsolete group blocks independent fallback until its revision is repaired.
  async fn source_scan_targets(
    &self,
    project: ProjectId,
    after: Option<WorkSourceScanTarget>,
    limit: u16,
  ) -> Result<Vec<WorkSourceScanTarget>, StoreError>;
  /// Resolves the exact current enabled selection under this live owner, without a provider call.
  async fn source_scan_members(
    &self,
    claim: WorkSourceScanClaim,
    now: octacity_server_domain::Timestamp,
  ) -> Result<Vec<WorkSourceInput>, StoreError>;
  /// Retains one selection disposition and returns only a current, uniquely selected input.
  async fn select_source_input(
    &self,
    claim: WorkSourceScanClaim,
    now: octacity_server_domain::Timestamp,
    item: WorkSourceItem,
    matched: Vec<WorkSourceInputId>,
  ) -> Result<octacity_server_factory::WorkSourceSelection, StoreError>;
  /// Retains scheduling under the current fence, without advancing discovery progress.
  async fn finish_source_scan(
    &self,
    claim: WorkSourceScanClaim,
    now: octacity_server_domain::Timestamp,
    schedule: crate::WorkSourceSchedule,
  ) -> Result<(), StoreError>;
  /// Reads safe scheduling facts for exactly this Project and retained target definition.
  async fn source_scan_schedule(
    &self,
    project: ProjectId,
    target: WorkSourceScanTarget,
  ) -> Result<Option<crate::WorkSourceSchedule>, StoreError>;
  /// Claims only the current enabled selection; a live lease returns no claim.
  async fn claim_source_scan(&self, request: ClaimWorkSourceScan) -> Result<Option<WorkSourceScan>, StoreError>;
  /// Advances only a covered position under the current owner and version precondition.
  async fn checkpoint_source_scan(
    &self,
    claim: WorkSourceScanClaim,
    now: octacity_server_domain::Timestamp,
    expected: Option<WorkSourceProgress>,
    progress: WorkSourceProgress,
  ) -> Result<(), StoreError>;
  /// Clears at most `limit` expired private pages, including disabled inputs; resets covered progress.
  async fn expire_source_poll_pages(
    &self,
    now: octacity_server_domain::Timestamp,
    limit: u16,
  ) -> Result<usize, StoreError>;
  /// Publishes all versions or none, rejecting stale preconditions and foreign dependencies.
  async fn publish_work_sources(&self, request: PublishWorkSources) -> Result<(), StoreError>;
  /// Reads an exact immutable Project-owned input, including disabled historical versions.
  async fn source_input(
    &self,
    project: ProjectId,
    id: WorkSourceInputId,
    version: WorkSourceInputVersion,
  ) -> Result<WorkSourceInput, StoreError>;
  /// Reads an exact immutable Project-owned group revision.
  async fn source_group(
    &self,
    project: ProjectId,
    id: WorkSourceGroupId,
    version: WorkSourceGroupVersion,
  ) -> Result<WorkSourceGroup, StoreError>;
}

/// Exact definition pinned for one input or exclusive-group scan.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkSourceScanTarget {
  /// One independent input.
  Input {
    /// Stable input identity.
    id: WorkSourceInputId,
    /// Exact immutable input version.
    version: WorkSourceInputVersion,
  },
  /// One mutually exclusive selection pass.
  Group {
    /// Stable group identity.
    id: WorkSourceGroupId,
    /// Exact immutable group revision.
    version: WorkSourceGroupVersion,
  },
}
impl WorkSourceScanTarget {
  /// Stable scan identity; replacing a definition never resets the fence counter.
  pub const fn identity(self) -> (&'static str, uuid::Uuid) {
    match self {
      Self::Input { id, .. } => ("input", id.as_uuid()),
      Self::Group { id, .. } => ("group", id.as_uuid()),
    }
  }
}
/// Bounded resumable provider position. It is private worker state, not diagnostics.
#[derive(Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourceProgress {
  /// Optional private retained polling details; absent in historical checkpoint documents.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub polling: Option<crate::WorkSourcePollingState>,
  /// Inclusive beginning of the covered update window.
  pub window_start: octacity_server_domain::Timestamp,
  /// Frozen upper update boundary for this pass.
  pub window_end: octacity_server_domain::Timestamp,
  /// Opaque provider continuation, at most 4096 bytes; never a credential.
  pub continuation: Option<String>,
  /// Whether this position belongs to periodic full reconciliation.
  pub reconciliation: bool,
}
impl std::fmt::Debug for WorkSourceProgress {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("WorkSourceProgress")
      .field("window_start", &self.window_start)
      .field("window_end", &self.window_end)
      .field("continuation", &self.continuation.as_ref().map(|_| "<redacted>"))
      .field("reconciliation", &self.reconciliation)
      .finish()
  }
}
impl WorkSourceProgress {
  /// Rejects inverted windows and oversized or malformed opaque continuations.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.window_start > self.window_end
      || self
        .continuation
        .as_ref()
        .is_some_and(|v| v.is_empty() || v.len() > 4096 || v.chars().any(char::is_control))
    {
      return Err(source_conflict());
    }
    if let Some(polling) = &self.polling {
      polling.validate(self)?;
    }
    Ok(())
  }
}
/// Trusted clock and worker identity used to acquire a bounded scan lease.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimWorkSourceScan<T = WorkSourceScanTarget> {
  /// Owner of the pinned definitions.
  pub project_id: ProjectId,
  /// Exact current read target; Factory intake defaults to an enabled input or group.
  pub target: T,
  /// Stable worker identity.
  pub owner: octacity_server_factory::FactoryKey,
  /// Injected trusted observation time.
  pub now: octacity_server_domain::Timestamp,
  /// Exclusive deadline, at most five minutes after acquisition.
  pub until: octacity_server_domain::Timestamp,
}
/// Fencing proof; every fresh write compares the complete retained claim.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourceScanClaim<T = WorkSourceScanTarget> {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Exact pinned selection definition.
  pub target: T,
  /// Worker holding this lease.
  pub owner: octacity_server_factory::FactoryKey,
  /// Monotonically increasing token across all versions of the scan identity.
  pub fence: u64,
  /// Authoritative acquisition time.
  pub acquired_at: octacity_server_domain::Timestamp,
  /// Exclusive lease deadline.
  pub until: octacity_server_domain::Timestamp,
}
/// Retained scan lease and its last covered position.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourceScan<T = WorkSourceScanTarget> {
  /// Last bounded classification, retained independently of covered page progress.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub last_selection: Option<crate::WorkSourceSelectionRecord>,
  /// Optional durable retry/cadence state; historical rows omit this field.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub schedule: Option<crate::WorkSourceSchedule>,
  /// Complete ownership proof.
  pub claim: WorkSourceScanClaim<T>,
  /// Covered position, reset when the definition changes.
  pub progress: Option<WorkSourceProgress>,
}
/// Computes an acquisition while retaining progress and a monotonic fencing token.
pub fn next_source_scan<T: Clone + PartialEq>(
  request: ClaimWorkSourceScan<T>,
  current: Option<&WorkSourceScan<T>>,
) -> Result<Option<WorkSourceScan<T>>, StoreError> {
  let duration = request.until.unix_millis() - request.now.unix_millis();
  if !(1..=300_000).contains(&duration) {
    return Err(source_conflict());
  }
  if current.is_some_and(|v| v.claim.until > request.now) {
    return Ok(None);
  }
  if current
    .filter(|v| v.claim.target == request.target)
    .and_then(|v| v.schedule.as_ref())
    .is_some_and(|s| s.exhausted || request.now < s.retry_at)
  {
    return Ok(None);
  }
  let fence = current
    .map_or(Some(1), |v| v.claim.fence.checked_add(1))
    .ok_or_else(source_conflict)?;
  let progress = current
    .filter(|v| v.claim.target == request.target)
    .and_then(|v| v.progress.clone());
  let schedule = current
    .filter(|v| v.claim.target == request.target)
    .and_then(|v| v.schedule.clone());
  Ok(Some(WorkSourceScan {
    last_selection: current
      .filter(|v| v.claim.target == request.target)
      .and_then(|v| v.last_selection.clone()),
    schedule,
    claim: WorkSourceScanClaim {
      project_id: request.project_id,
      target: request.target,
      owner: request.owner,
      fence,
      acquired_at: request.now,
      until: request.until,
    },
    progress,
  }))
}
/// Validates complete ownership and trusted time before any fresh scan mutation.
pub fn require_source_scan<T: PartialEq>(
  current: &WorkSourceScan<T>,
  claim: &WorkSourceScanClaim<T>,
  now: octacity_server_domain::Timestamp,
) -> Result<(), StoreError> {
  if current.claim != *claim || now < claim.acquired_at || now >= claim.until {
    return Err(source_conflict());
  }
  Ok(())
}

/// Opaque immutable native identities resolved inside the installed provider boundary.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourceItem {
  /// Native ticket container; distinct from an operator resource reference.
  pub resource: octacity_server_factory::ExternalWorkIdentity,
  /// Native stable ticket identity, never an update timestamp or label.
  pub item: octacity_server_factory::ExternalWorkIdentity,
}
/// Durable reservation before artifact preparation or mutable VCS resolution.
#[derive(Clone, Eq, PartialEq)]
pub struct ReserveWorkSourcePreparation {
  /// Current scan owner and exact selection version.
  pub claim: WorkSourceScanClaim,
  /// One selected enabled member input.
  pub input_id: WorkSourceInputId,
  /// Verified provider identities.
  pub item: WorkSourceItem,
  /// Bounded frozen adapter snapshot; private data, never authority or diagnostics.
  pub snapshot: Vec<u8>,
  /// Trusted observation time.
  pub now: octacity_server_domain::Timestamp,
}
/// Original source association of an already authoritative admission.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourceCorrelation {
  /// Existing immutable Work; this record cannot create or deduplicate Work.
  pub work_id: octacity_server_factory::WorkEnvelopeId,
  /// Existing original Run identity.
  pub run_id: octacity_server_factory::FactoryRunId,
}
/// Original candidate retained across worker restart and observation edits.
#[derive(Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourcePreparation {
  /// Stable private preparation operation, independent of token and definition versions.
  pub id: FactoryDigest,
  /// Common native ticket association across distinct input stages.
  pub issue: FactoryDigest,
  /// Exact input/target/reporter definition originally selected.
  pub input: WorkSourceInput,
  /// Original native ticket identities.
  pub item: WorkSourceItem,
  /// Original scan definition, including exclusive group revision when applicable.
  pub target: WorkSourceScanTarget,
  /// Original trusted snapshot reservation time.
  #[serde(default = "source_snapshot_epoch")]
  pub reserved_at: octacity_server_domain::Timestamp,
  /// Hard private retention bound, including snapshots orphaned before mapping.
  #[serde(default = "source_snapshot_deadline")]
  pub snapshot_expires_at: octacity_server_domain::Timestamp,
  /// Frozen private observation, at most one MiB.
  pub snapshot: Vec<u8>,
  /// Complete normalized original command before mutable VCS resolution.
  pub intent: Option<crate::FactoryWorkAdmissionIntent>,
  /// Original authoritative admission association, once confirmed.
  pub admission: Option<WorkSourceCorrelation>,
}
impl std::fmt::Debug for WorkSourcePreparation {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("WorkSourcePreparation")
      .field("id", &self.id)
      .field("input_id", &self.input.id())
      .field("snapshot", &"<redacted>")
      .field("admission", &self.admission)
      .finish()
  }
}
impl ReserveWorkSourcePreparation {
  /// Bounds the immutable private snapshot before touching storage.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.snapshot.is_empty() || self.snapshot.len() > MAX_SOURCE_INPUT_BYTES as usize {
      return Err(source_conflict());
    }
    Ok(())
  }
  /// Freezes one authorized selected input. The store separately checks membership and ownership.
  pub fn freeze(self, input: WorkSourceInput) -> Result<WorkSourcePreparation, StoreError> {
    self.validate()?;
    if input.id() != self.input_id || input.project_id() != self.claim.project_id || !input.definition().enabled {
      return Err(source_conflict());
    }
    let issue = crate::source_native_issue(
      input.project_id(),
      input.definition().source.installation.identity(),
      &self.item,
    );
    let id = FactoryDigest::sha256(
      "octacity.factory.source-preparation.v1",
      &[&issue.as_bytes(), input.id().to_string().as_bytes()],
    );
    Ok(WorkSourcePreparation {
      id,
      issue,
      input,
      item: self.item,
      target: self.claim.target,
      reserved_at: self.now,
      snapshot_expires_at: octacity_server_domain::Timestamp::from_unix_millis(
        self
          .now
          .unix_millis()
          .checked_add(MAX_SOURCE_RETENTION_MILLIS as i64)
          .ok_or_else(source_conflict)?,
      )
      .map_err(|_| source_conflict())?,
      snapshot: self.snapshot,
      intent: None,
      admission: None,
    })
  }
}

/// Immutable normalized command produced by the trusted source mapper.
#[derive(Clone, Eq, PartialEq)]
pub struct PublishSourcePreparationIntent {
  /// Current scan ownership.
  pub claim: WorkSourceScanClaim,
  /// Original reservation identity.
  pub preparation_id: FactoryDigest,
  /// Fully normalized original admission command.
  pub intent: crate::FactoryWorkAdmissionIntent,
  /// Trusted observation time.
  pub now: octacity_server_domain::Timestamp,
}
impl WorkSourcePreparation {
  /// Technical ordering bucket shared across installations for the same opaque native ids.
  /// Admission/correlation retains its installation scope. Adapter field digests must
  /// additionally namespace the actual provider instance, isolating coincident ids.
  pub fn projection_item(&self) -> FactoryDigest {
    FactoryDigest::sha256(
      "octacity.factory.projection-item.v1",
      &[
        self.item.resource.as_str().as_bytes(),
        self.item.item.as_str().as_bytes(),
      ],
    )
  }

  /// Existing admission identity; definition versions and observation content are excluded.
  pub fn admission_identity(&self, security_scope: crate::ManagementSecurityScope) -> crate::FactoryAdmissionIdentity {
    source_admission_identity(&self.input, &self.item, security_scope)
  }
  /// Checks a normalized command against every original pinned source/target fact.
  pub fn with_intent(&self, request: PublishSourcePreparationIntent) -> Result<Self, StoreError> {
    let intent = &request.intent;
    let definition = self.input.definition();
    if self.input.project_id() != request.claim.project_id
      || self.id != request.preparation_id
      || intent.project_id != self.input.project_id()
      || intent.configuration_id != definition.target.id()
      || intent.configuration_version != definition.target.version()
      || intent.repository_id != definition.repository.id
      || intent.repository_version != definition.repository.version
      || intent.external_identity
        != self
          .admission_identity(crate::ManagementSecurityScope::trusted_network())
          .external_identity
      || self.intent.as_ref().is_some_and(|prior| prior != intent)
    {
      return Err(source_conflict());
    }
    let mut retained = self.clone();
    retained.intent = Some(request.intent);
    Ok(retained)
  }
}
/// Constructs the sole installed-source admission namespace and input-scoped item identity.
pub fn source_admission_identity(
  input: &WorkSourceInput,
  item: &WorkSourceItem,
  security_scope: crate::ManagementSecurityScope,
) -> crate::FactoryAdmissionIdentity {
  let installation = input.definition().source.installation.identity();
  let item = FactoryDigest::sha256(
    "octacity.factory.source-item.v1",
    &[
      installation.as_str().as_bytes(),
      item.resource.as_str().as_bytes(),
      item.item.as_str().as_bytes(),
      input.id().to_string().as_bytes(),
    ],
  );
  crate::FactoryAdmissionIdentity {
    source_scope: crate::FactoryWorkSourceScope {
      source: installed_source_namespace(input.project_id(), installation),
      security_scope,
    },
    external_identity: octacity_server_factory::ExternalWorkIdentity::new(format!("source-item.{item}"))
      .expect("fixed bounded item identity"),
  }
}

/// Atomic admission of an already normalized preparation under the live scan fence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmitPreparedWorkSource {
  /// Current owner; expiry and definition replacement reject even exact mutation retries.
  pub claim: WorkSourceScanClaim,
  /// Original immutable preparation identity.
  pub preparation_id: FactoryDigest,
  /// Trusted commit observation time.
  pub now: octacity_server_domain::Timestamp,
}
impl WorkSourcePreparation {
  /// Reconstructs the strict existing admission probe from the original normalized intent.
  pub fn admission_probe(
    &self,
    scope: crate::ManagementSecurityScope,
  ) -> Result<crate::FactoryAdmissionProbe, StoreError> {
    let intent = self.intent.as_ref().ok_or_else(source_conflict)?;
    crate::factory_source_admission_probe(self.admission_identity(scope), intent)
  }
  /// Verifies that a new Work repeats the retained command rather than fresh observation data.
  pub fn validate_admission(
    &self,
    guard: &AdmitPreparedWorkSource,
    request: &crate::AdmitFactoryWork,
  ) -> Result<(), StoreError> {
    let intent = self.intent.as_ref().ok_or_else(source_conflict)?;
    if self.id != guard.preparation_id
      || self.input.project_id() != guard.claim.project_id
      || self.admission_probe(request.probe.source_scope.security_scope.clone())? != request.probe
      || self.input.definition().target != *request.work.configuration()
      || intent.project_id != request.work.subject().project_id()
      || intent.repository_id != request.work.subject().repository_id()
      || intent.repository_version != request.repository_version
      || intent.artifacts != *request.work.artifacts()
      || intent.classification.priority() != request.work.priority()
      || intent.classification.risk() != request.work.risk()
      || intent.classification.metadata() != request.work.metadata()
      || intent.admitted_at != request.admitted_at
      || matches!(&intent.source, crate::RepositorySourceSelection::ExactRevision(revision) if revision != request.work.subject().base_revision())
    {
      return Err(source_conflict());
    }
    Ok(())
  }
  /// Associates a receipt returned by the existing admission authority.
  pub fn correlate(&mut self, receipt: &crate::PublishedFactoryAdmission) {
    self.admission = Some(WorkSourceCorrelation {
      work_id: receipt.work.id(),
      run_id: receipt.run.id(),
    });
  }
}

/// Safe original stage association; private ticket content is deliberately absent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkSourceIssueAssociation {
  /// Stable stage input.
  pub input_id: WorkSourceInputId,
  /// Original selected definition and optional reporter mapping.
  pub input_version: WorkSourceInputVersion,
  /// Existing authoritative Work and Run.
  pub admission: WorkSourceCorrelation,
}

impl WorkSourcePreparation {
  /// A new scan owner may resume original intent only for the same stable selected
  /// input and installed resource. The old claim itself never regains authority.
  /// The store must resolve `selected` under the current target's live fence first;
  /// the original scan target remains provenance, not recovery authority.
  pub fn require_selected_owner(
    &self,
    claim: &WorkSourceScanClaim,
    selected: &WorkSourceInput,
  ) -> Result<(), StoreError> {
    let original = &self.input.definition().source;
    let current = &selected.definition().source;
    if self.input.project_id() != claim.project_id
      || selected.project_id() != claim.project_id
      || self.input.id() != selected.id()
      || !selected.definition().enabled
      || original.installation.identity() != current.installation.identity()
      || original.resource.identity() != current.resource.identity()
    {
      return Err(source_conflict());
    }
    Ok(())
  }
}

/// Stable Project/installation namespace shared by source ingress and durable preparation.
pub fn installed_source_namespace(
  project: ProjectId,
  installation: &octacity_server_factory::FactoryKey,
) -> octacity_server_factory::FactoryKey {
  let namespace = FactoryDigest::sha256(
    "octacity.factory.installed-source.v1",
    &[project.to_string().as_bytes(), installation.as_str().as_bytes()],
  );
  octacity_server_factory::FactoryKey::new(format!("installed-source.{namespace}"))
    .expect("fixed bounded source identity")
}

fn source_snapshot_epoch() -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(0).expect("valid epoch")
}
fn source_snapshot_deadline() -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(MAX_SOURCE_RETENTION_MILLIS as i64)
    .expect("bounded legacy deadline")
}

/// Existing native observation hash; deliberately unchanged from the Source preparation journal.
pub fn source_native_issue(
  project: ProjectId,
  installation: &octacity_server_factory::FactoryKey,
  item: &WorkSourceItem,
) -> FactoryDigest {
  FactoryDigest::sha256(
    "octacity.factory.source-correlation.v1",
    &[
      project.to_string().as_bytes(),
      installation.as_str().as_bytes(),
      item.resource.as_str().as_bytes(),
      item.item.as_str().as_bytes(),
    ],
  )
}
/// Existing admission security-partition hash; carries no credential material.
pub fn source_security_partition(scope: &crate::ManagementSecurityScope) -> FactoryDigest {
  FactoryDigest::sha256("octacity.factory.security-scope.v1", &[scope.as_str().as_bytes()])
}
/// Common display correlation, excluding input identity; never used to reserve or replay work.
pub fn source_native_correlation(issue: FactoryDigest, partition: FactoryDigest) -> FactoryDigest {
  FactoryDigest::sha256(
    "octacity.factory.native-task.v1",
    &[&issue.as_bytes(), &partition.as_bytes()],
  )
}
