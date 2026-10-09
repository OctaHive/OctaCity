//! Fenced acceptance of trusted ChangeSet outputs from an ordinary Build.
//!
//! Capture remains an Agent concern and publication remains the generic
//! Artifact pipeline. This adapter recognizes only reserved capture outputs,
//! revalidates their immutable bytes, and appends one exact candidate under
//! the current Factory Run and outbox fences.

use octacity_protocol::{
  CHANGE_SET_BUNDLE_MEDIA_TYPE, CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_MEDIA_TYPE, CHANGE_SET_MANIFEST_OUTPUT,
  CHANGE_SET_PATCH_MEDIA_TYPE, CHANGE_SET_PATCH_OUTPUT, CapturedChangeSetFileV1, CapturedChangeSetManifestV1,
};
use octacity_server_artifacts::{ArtifactIdentity, ArtifactType};
use octacity_server_domain::{ImmutableRevision, Timestamp};
use octacity_server_factory::{
  CandidateSubject, ChangeSet, ChangeSetId, FactoryDigest, FactoryKey, FactoryLifecycleProgress, FactoryRun,
  FactoryRunVersion, FactoryStageProgress, FactoryStageTarget,
};
use octacity_server_orchestrator::BuildState;
use octacity_server_store::{
  AuditActorKind, ClaimedFactoryOutbox, CommitFactoryRunTransition, FactoryAuditFact, FactoryBudgetRecord,
  FactoryLifecycleCheckpoint, FactoryOutboxSettlement, FactoryOutboxState, FactoryRunHistoryAppend, FactoryRunSnapshot,
  FactoryRunStore, MutationDisposition, SettleFactoryOutbox, StoreError,
};
use thiserror::Error;
use uuid::Uuid;

/// Exact published Artifact bytes verified against their immutable identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedFactoryArtifact {
  identity: ArtifactIdentity,
  bytes: Vec<u8>,
}

impl VerifiedFactoryArtifact {
  /// Verifies bounded bytes against already-published logical metadata.
  pub fn new(identity: ArtifactIdentity, bytes: Vec<u8>) -> Result<Self, FactoryChangeSetError> {
    if identity.size_bytes != u64::try_from(bytes.len()).unwrap_or(u64::MAX)
      || identity.digest.as_bytes() != FactoryDigest::content_sha256(&bytes).as_bytes()
    {
      return Err(FactoryChangeSetError::Integrity);
    }
    Ok(Self { identity, bytes })
  }
}

/// Result of accepting or exactly replaying one candidate capture.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryChangeSetAcceptance {
  /// Applied or replayed mutation disposition.
  pub disposition: MutationDisposition,
  /// Exact accepted ChangeSet.
  pub change_set: ChangeSet,
}

/// Stable failures from trusted candidate acceptance.
#[derive(Debug, Error)]
pub enum FactoryChangeSetError {
  /// Current Run, Stage Attempt, Build, names, or manifest are inconsistent.
  #[error("Factory ChangeSet capture evidence is invalid")]
  InvalidEvidence,
  /// Published bytes differ from their immutable Artifact identities.
  #[error("Factory ChangeSet Artifact integrity verification failed")]
  Integrity,
  /// The authoritative store rejected or could not complete the operation.
  #[error("Factory ChangeSet store operation failed")]
  Store(#[from] StoreError),
}

/// Accepts one claimed `candidate.capture` operation under current fences.
pub async fn accept_factory_change_set<S: FactoryRunStore>(
  store: &S,
  claimed: ClaimedFactoryOutbox,
  documents: Vec<VerifiedFactoryArtifact>,
  observed_at: Timestamp,
) -> Result<FactoryChangeSetAcceptance, FactoryChangeSetError> {
  let snapshot = store.factory_run_snapshot(claimed.record.run_id).await?;
  let already_settled = validate_claim(&snapshot, &claimed, observed_at)?;
  let stage_id = snapshot
    .current
    .stage_attempt_id
    .ok_or(FactoryChangeSetError::InvalidEvidence)?;
  let stage = snapshot
    .stage_attempts
    .iter()
    .find(|stage| stage.id() == stage_id)
    .ok_or(FactoryChangeSetError::InvalidEvidence)?;
  let observation = snapshot
    .build_observations
    .iter()
    .find(|observation| {
      observation.stage_attempt_id == stage_id
        && Some(observation.build_id) == snapshot.current.build_id
        && observation.state == BuildState::Succeeded
    })
    .ok_or(FactoryChangeSetError::InvalidEvidence)?;
  let link = snapshot
    .linked_builds
    .iter()
    .find(|link| link.stage_attempt_id == stage_id && link.build_id == observation.build_id)
    .ok_or(FactoryChangeSetError::InvalidEvidence)?;
  let output = CaptureOutputs::load(&observation.outputs, documents)?;
  let manifest = parse_manifest(&output.manifest)?;
  let capture_base_revision = expected_capture_base(&snapshot, stage, link)?;
  validate_manifest(&manifest, stage_id.to_string(), capture_base_revision, &output)?;

  let changeset_digest = FactoryDigest::sha256(
    "octacity.factory.changeset.v1",
    &[
      manifest.base_revision.as_bytes(),
      manifest.candidate_revision.as_bytes(),
      &output.bundle.identity.digest.as_bytes(),
      &output.manifest.identity.digest.as_bytes(),
    ],
  );
  let subject = CandidateSubject::new(
    stage.subject().clone(),
    ImmutableRevision::new(manifest.candidate_revision).map_err(|_| FactoryChangeSetError::InvalidEvidence)?,
    changeset_digest,
  );
  let id = ChangeSetId::from_uuid(Uuid::new_v5(
    &stage_id.as_uuid(),
    claimed.record.operation_id.as_bytes().as_slice(),
  ))
  .map_err(|_| FactoryChangeSetError::InvalidEvidence)?;
  let candidate = ChangeSet::new(
    id,
    stage,
    subject,
    output.bundle.identity.artifact_id,
    output.manifest.identity.artifact_id,
  )
  .map_err(|_| FactoryChangeSetError::InvalidEvidence)?;

  if let Some(existing) = snapshot
    .candidates
    .iter()
    .find(|record| record.stage_attempt_id() == stage_id)
  {
    if existing != &candidate || snapshot.current.candidate_id != Some(existing.id()) {
      return Err(FactoryChangeSetError::InvalidEvidence);
    }
    if !already_settled {
      settle(store, &claimed, observed_at).await?;
    }
    return Ok(FactoryChangeSetAcceptance {
      disposition: MutationDisposition::Replayed,
      change_set: existing.clone(),
    });
  }
  if already_settled {
    return Err(FactoryChangeSetError::InvalidEvidence);
  }
  commit_candidate(store, &snapshot, &claimed, candidate.clone(), observed_at).await?;
  settle(store, &claimed, observed_at).await?;
  Ok(FactoryChangeSetAcceptance {
    disposition: MutationDisposition::Applied,
    change_set: candidate,
  })
}

fn expected_capture_base<'a>(
  snapshot: &'a FactoryRunSnapshot,
  stage: &octacity_server_factory::StageAttempt,
  link: &'a octacity_server_store::FactoryBuildLink,
) -> Result<&'a str, FactoryChangeSetError> {
  match (stage.target(), link.parent) {
    (FactoryStageTarget::Implementation, None) => Ok(link.exact_revision.as_str()),
    (FactoryStageTarget::Rework, Some(octacity_server_store::FactoryBuildParent::Decision(id))) => snapshot
      .decisions
      .iter()
      .find(|decision| decision.id() == id && decision.subject().exact() == stage.subject())
      .map(|decision| decision.subject().candidate_revision().as_str())
      .ok_or(FactoryChangeSetError::InvalidEvidence),
    _ => Err(FactoryChangeSetError::InvalidEvidence),
  }
}

struct CaptureOutputs {
  bundle: VerifiedFactoryArtifact,
  manifest: VerifiedFactoryArtifact,
  patch: Option<VerifiedFactoryArtifact>,
}

impl CaptureOutputs {
  fn load(
    published: &[ArtifactIdentity],
    documents: Vec<VerifiedFactoryArtifact>,
  ) -> Result<Self, FactoryChangeSetError> {
    let published_components = published
      .iter()
      .filter(|identity| {
        matches!(
          identity.logical_name.as_str(),
          CHANGE_SET_BUNDLE_OUTPUT | CHANGE_SET_MANIFEST_OUTPUT | CHANGE_SET_PATCH_OUTPUT
        )
      })
      .count();
    if documents.len() != published_components {
      return Err(FactoryChangeSetError::InvalidEvidence);
    }
    for document in &documents {
      if !published.contains(&document.identity) {
        return Err(FactoryChangeSetError::InvalidEvidence);
      }
    }
    let mut bundle = None;
    let mut manifest = None;
    let mut patch = None;
    for document in documents {
      let slot = match document.identity.logical_name.as_str() {
        CHANGE_SET_BUNDLE_OUTPUT => &mut bundle,
        CHANGE_SET_MANIFEST_OUTPUT => &mut manifest,
        CHANGE_SET_PATCH_OUTPUT => &mut patch,
        _ => return Err(FactoryChangeSetError::InvalidEvidence),
      };
      if slot.replace(document).is_some() {
        return Err(FactoryChangeSetError::InvalidEvidence);
      }
    }
    let output = Self {
      bundle: bundle.ok_or(FactoryChangeSetError::InvalidEvidence)?,
      manifest: manifest.ok_or(FactoryChangeSetError::InvalidEvidence)?,
      patch,
    };
    if !artifact_type(&output.bundle.identity, CHANGE_SET_BUNDLE_MEDIA_TYPE)
      || !artifact_type(&output.manifest.identity, CHANGE_SET_MANIFEST_MEDIA_TYPE)
      || output.bundle.identity.job_id != output.manifest.identity.job_id
      || output.bundle.identity.lease_id != output.manifest.identity.lease_id
      || output.patch.as_ref().is_some_and(|patch| {
        !artifact_type(&patch.identity, CHANGE_SET_PATCH_MEDIA_TYPE)
          || patch.identity.job_id != output.bundle.identity.job_id
          || patch.identity.lease_id != output.bundle.identity.lease_id
      })
    {
      return Err(FactoryChangeSetError::InvalidEvidence);
    }
    Ok(output)
  }
}

fn artifact_type(identity: &ArtifactIdentity, media_type: &str) -> bool {
  matches!(identity.artifact_type, ArtifactType::Artifact) && identity.media_type.as_str() == media_type
}

fn parse_manifest(document: &VerifiedFactoryArtifact) -> Result<CapturedChangeSetManifestV1, FactoryChangeSetError> {
  CapturedChangeSetManifestV1::decode_canonical(&document.bytes).map_err(|_| FactoryChangeSetError::InvalidEvidence)
}

fn validate_manifest(
  manifest: &CapturedChangeSetManifestV1,
  stage_attempt_id: String,
  base_revision: &str,
  output: &CaptureOutputs,
) -> Result<(), FactoryChangeSetError> {
  if manifest.stage_attempt_id != stage_attempt_id
    || manifest.base_revision != base_revision
    || !file_matches(&manifest.bundle, &output.bundle)
  {
    return Err(FactoryChangeSetError::InvalidEvidence);
  }
  match (&manifest.patch, &output.patch) {
    (None, None) => Ok(()),
    (Some(expected), Some(actual)) if expected.name == CHANGE_SET_PATCH_OUTPUT && file_matches(expected, actual) => {
      Ok(())
    }
    _ => Err(FactoryChangeSetError::InvalidEvidence),
  }
}

fn file_matches(expected: &CapturedChangeSetFileV1, actual: &VerifiedFactoryArtifact) -> bool {
  expected.size_bytes == actual.identity.size_bytes
    && expected.sha256 == actual.identity.digest.to_string()
    && expected.name == actual.identity.logical_name.as_str()
}

fn validate_claim(
  snapshot: &FactoryRunSnapshot,
  claimed: &ClaimedFactoryOutbox,
  observed_at: Timestamp,
) -> Result<bool, FactoryChangeSetError> {
  if claimed.record.kind.as_str() != "candidate.capture" || claimed.record.state != FactoryOutboxState::Claimed {
    return Err(FactoryChangeSetError::InvalidEvidence);
  }
  let operation_records = snapshot
    .outbox
    .iter()
    .filter(|record| record.operation_id == claimed.record.operation_id)
    .collect::<Vec<_>>();
  if operation_records.is_empty()
    || operation_records.iter().any(|record| {
      record.run_id != claimed.record.run_id
        || record.kind != claimed.record.kind
        || record.input_digest != claimed.record.input_digest
    })
  {
    return Err(FactoryChangeSetError::InvalidEvidence);
  }
  let already_settled = operation_records
    .iter()
    .any(|record| record.state == FactoryOutboxState::Delivered);
  if already_settled {
    return Ok(true);
  }
  if !operation_records.contains(&&claimed.record) {
    return Err(FactoryChangeSetError::InvalidEvidence);
  }
  let owner = claimed
    .record
    .owner
    .as_ref()
    .ok_or(FactoryChangeSetError::InvalidEvidence)?;
  let outbox_claim = claimed.record.claim.ok_or(FactoryChangeSetError::InvalidEvidence)?;
  outbox_claim
    .verify_fence(outbox_claim.fence(), observed_at)
    .map_err(|_| FactoryChangeSetError::InvalidEvidence)?;
  let run_claim = snapshot
    .current_claim
    .as_ref()
    .filter(|claim| &claim.owner == owner)
    .ok_or(FactoryChangeSetError::InvalidEvidence)?;
  run_claim
    .claim
    .verify_fence(run_claim.claim.fence(), observed_at)
    .map_err(|_| FactoryChangeSetError::InvalidEvidence)?;
  Ok(false)
}

async fn commit_candidate<S: FactoryRunStore>(
  store: &S,
  snapshot: &FactoryRunSnapshot,
  claimed: &ClaimedFactoryOutbox,
  candidate: ChangeSet,
  committed_at: Timestamp,
) -> Result<(), FactoryChangeSetError> {
  let current_checkpoint = snapshot
    .lifecycle_checkpoints
    .iter()
    .find(|checkpoint| checkpoint.id == snapshot.current.lifecycle_checkpoint_id)
    .ok_or(FactoryChangeSetError::InvalidEvidence)?;
  let target = match &current_checkpoint.progress {
    FactoryLifecycleProgress::Stage {
      target,
      progress: FactoryStageProgress::BuildSucceeded,
    } => target.clone(),
    _ => return Err(FactoryChangeSetError::InvalidEvidence),
  };
  let current_budget = snapshot
    .budgets
    .iter()
    .find(|budget| budget.id == snapshot.current.budget_id)
    .ok_or(FactoryChangeSetError::InvalidEvidence)?;
  let claim = snapshot
    .current_claim
    .as_ref()
    .ok_or(FactoryChangeSetError::InvalidEvidence)?;
  let version = FactoryRunVersion::new(
    snapshot
      .run
      .version()
      .get()
      .checked_add(1)
      .ok_or(FactoryChangeSetError::InvalidEvidence)?,
  )
  .map_err(|_| FactoryChangeSetError::InvalidEvidence)?;
  let next_run = FactoryRun::restore(
    snapshot.run.id(),
    snapshot.run.configuration().clone(),
    &snapshot.work,
    snapshot.run.subject().clone(),
    snapshot.run.state(),
    version,
  )
  .map_err(|_| FactoryChangeSetError::InvalidEvidence)?;
  let budget = FactoryBudgetRecord::new(snapshot.run.id(), version, current_budget.usage, committed_at);
  let checkpoint = FactoryLifecycleCheckpoint::new(
    snapshot.run.id(),
    version,
    FactoryLifecycleProgress::Stage {
      target,
      progress: FactoryStageProgress::CandidateCaptured,
    },
    current_checkpoint.signal,
    current_checkpoint.cancellation_requested,
    committed_at,
  );
  let mut current = snapshot.current.clone();
  current.budget_id = budget.id;
  current.lifecycle_checkpoint_id = checkpoint.id;
  current.candidate_id = Some(candidate.id());
  store
    .commit_factory_run_transition(CommitFactoryRunTransition {
      run_id: snapshot.run.id(),
      expected_version: snapshot.run.version(),
      claim_id: claim.id,
      owner: claim.owner.clone(),
      fence: claim.claim.fence(),
      committed_at,
      next_run,
      budget,
      lifecycle_checkpoint: checkpoint,
      append: FactoryRunHistoryAppend {
        candidates: vec![candidate],
        ..FactoryRunHistoryAppend::default()
      },
      current,
      audit: FactoryAuditFact::new(
        snapshot.run.id(),
        AuditActorKind::Worker,
        Some(FactoryDigest::sha256(
          "octacity.factory.changeset-worker.v1",
          &[claim.owner.as_str().as_bytes()],
        )),
        key("factory.candidate.accepted")?,
        claimed.record.operation_id,
        key("accepted")?,
        committed_at,
      ),
      outbox: Vec::new(),
    })
    .await?;
  Ok(())
}

async fn settle<S: FactoryRunStore>(
  store: &S,
  claimed: &ClaimedFactoryOutbox,
  observed_at: Timestamp,
) -> Result<(), FactoryChangeSetError> {
  store
    .settle_factory_outbox(SettleFactoryOutbox {
      operation_id: claimed.record.operation_id,
      owner: claimed
        .record
        .owner
        .clone()
        .ok_or(FactoryChangeSetError::InvalidEvidence)?,
      fence: claimed
        .record
        .claim
        .ok_or(FactoryChangeSetError::InvalidEvidence)?
        .fence(),
      observed_at,
      settlement: FactoryOutboxSettlement::Delivered,
    })
    .await?;
  Ok(())
}

fn key(value: &str) -> Result<FactoryKey, FactoryChangeSetError> {
  FactoryKey::new(value).map_err(|_| FactoryChangeSetError::InvalidEvidence)
}
