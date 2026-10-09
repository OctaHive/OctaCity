//! Trusted construction of immutable Factory evidence from ordinary Build outputs.

use octacity_server_artifacts::{ArtifactRecord, ArtifactState, ArtifactType};
use octacity_server_domain::Timestamp;
use octacity_server_factory::{
  CandidateSubject, ChangeSet, DeterministicGateOutcome, EvidenceItem, EvidenceItemInput, EvidenceManifest,
  EvidenceManifestId, EvidenceOutputKind, EvidenceProducer, EvidenceRequirement, FactoryArtifactReference,
  FactoryDigest, FactoryError, FactoryKey, FactoryStageTarget, ImmutableReference,
};
use octacity_server_orchestrator::BuildState;
use octacity_server_store::{FactoryBuildLink, FactoryBuildObservationRecord, FactoryBuildParent};
use thiserror::Error;

/// Schema-validated provenance and outcome for one retained validation output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryEvidenceAttestation {
  subject: CandidateSubject,
  kind: FactoryKey,
  schema: ImmutableReference,
  tool: ImmutableReference,
  plugin: ImmutableReference,
  outcome: DeterministicGateOutcome,
  fresh_until: Timestamp,
}

impl FactoryEvidenceAttestation {
  /// Captures metadata emitted by a trusted schema-specific output verifier.
  #[must_use]
  pub const fn new(
    subject: CandidateSubject,
    kind: FactoryKey,
    schema: ImmutableReference,
    tool: ImmutableReference,
    plugin: ImmutableReference,
    outcome: DeterministicGateOutcome,
    fresh_until: Timestamp,
  ) -> Self {
    Self {
      subject,
      kind,
      schema,
      tool,
      plugin,
      outcome,
      fresh_until,
    }
  }
}

/// Published Artifact bytes plus metadata produced by a trusted schema verifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedFactoryEvidence {
  record: ArtifactRecord,
  attestation: FactoryEvidenceAttestation,
}

impl VerifiedFactoryEvidence {
  /// Verifies publication, retention, output type, size, and digest before retaining metadata.
  pub fn new(
    record: ArtifactRecord,
    bytes: &[u8],
    attestation: FactoryEvidenceAttestation,
  ) -> Result<Self, FactoryEvidenceError> {
    let identity = record.identity();
    if record.state() != ArtifactState::Published || record.published_at().is_none() {
      return Err(FactoryEvidenceError::NotRetained);
    }
    if identity.size_bytes != u64::try_from(bytes.len()).unwrap_or(u64::MAX)
      || identity.digest.as_bytes() != FactoryDigest::content_sha256(bytes).as_bytes()
    {
      return Err(FactoryEvidenceError::Integrity);
    }
    if let Some(deadline) = identity.retention.delete_after()
      && attestation.fresh_until > deadline
    {
      return Err(FactoryEvidenceError::NotRetained);
    }
    match &identity.artifact_type {
      ArtifactType::Report(format) if format.as_str() == attestation.schema.identity().as_str() => {}
      ArtifactType::Artifact => {}
      ArtifactType::Report(_) => return Err(FactoryEvidenceError::InvalidEvidence),
    }
    Ok(Self { record, attestation })
  }
}

/// Stable failures from trusted Evidence Manifest construction.
#[derive(Debug, Error)]
pub enum FactoryEvidenceError {
  /// Candidate, producer, output type, schema, or Build observation is inconsistent.
  #[error("Factory validation evidence is invalid")]
  InvalidEvidence,
  /// Published bytes differ from their immutable Artifact identity.
  #[error("Factory validation evidence integrity verification failed")]
  Integrity,
  /// The Artifact is not currently published or retained through its freshness window.
  #[error("Factory validation evidence is not retained")]
  NotRetained,
  /// The Factory domain rejected an incomplete, stale, or inconsistent manifest.
  #[error("Factory Evidence Manifest is invalid")]
  Manifest(#[from] FactoryError),
}

/// Constructs one complete immutable manifest from exact successful Validation Build outputs.
pub fn construct_factory_evidence_manifest(
  id: EvidenceManifestId,
  change_set: &ChangeSet,
  link: &FactoryBuildLink,
  observation: &FactoryBuildObservationRecord,
  requirements: Vec<EvidenceRequirement>,
  outputs: Vec<VerifiedFactoryEvidence>,
  constructed_at: Timestamp,
) -> Result<EvidenceManifest, FactoryEvidenceError> {
  validate_build(change_set, link, observation, constructed_at)?;
  let mut items = Vec::with_capacity(outputs.len());
  for output in outputs {
    let identity = output.record.identity();
    let published_at = output.record.published_at().ok_or(FactoryEvidenceError::NotRetained)?;
    if !observation.outputs.contains(identity)
      || identity.build_id != link.build_id
      || identity.attempt_id != observation.attempt_id
      || !observation.job_ids.contains(&identity.job_id)
      || identity
        .retention
        .delete_after()
        .is_some_and(|deadline| deadline <= constructed_at)
    {
      return Err(FactoryEvidenceError::InvalidEvidence);
    }
    let output_kind = match identity.artifact_type {
      ArtifactType::Report(_) => EvidenceOutputKind::Report,
      ArtifactType::Artifact => EvidenceOutputKind::Artifact,
    };
    items.push(EvidenceItem::new(EvidenceItemInput {
      subject: output.attestation.subject,
      kind: output.attestation.kind,
      output_kind,
      artifact: FactoryArtifactReference::new(
        identity.artifact_id,
        FactoryDigest::from_bytes(identity.digest.as_bytes()),
        identity.size_bytes,
      )
      .map_err(FactoryEvidenceError::Manifest)?,
      schema: output.attestation.schema,
      producer: EvidenceProducer::new(
        identity.build_id,
        identity.attempt_id,
        identity.job_id,
        output.attestation.tool,
        output.attestation.plugin,
      ),
      outcome: output.attestation.outcome,
      published_at,
      fresh_until: output.attestation.fresh_until,
    }));
  }
  EvidenceManifest::new(
    id,
    change_set,
    change_set.subject().clone(),
    constructed_at,
    requirements,
    items,
  )
  .map_err(FactoryEvidenceError::Manifest)
}

fn validate_build(
  change_set: &ChangeSet,
  link: &FactoryBuildLink,
  observation: &FactoryBuildObservationRecord,
  constructed_at: Timestamp,
) -> Result<(), FactoryEvidenceError> {
  if link.target != FactoryStageTarget::Validation
    || link.parent != Some(FactoryBuildParent::ChangeSet(change_set.id()))
    || link.exact_revision != *change_set.subject().candidate_revision()
    || observation.run_id != link.run_id
    || observation.stage_attempt_id != link.stage_attempt_id
    || observation.target != FactoryStageTarget::Validation
    || observation.build_id != link.build_id
    || observation.attempt_id != link.attempt_id
    || observation.state != BuildState::Succeeded
    || observation.observed_at > constructed_at
  {
    return Err(FactoryEvidenceError::InvalidEvidence);
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use octacity_server_artifacts::{
    ArtifactContentDigest, ArtifactEvent, ArtifactMediaType, ArtifactReportFormat, ArtifactRetentionPolicy,
  };
  use octacity_server_domain::{
    ArtifactId, ArtifactName, AttemptId, AttemptVersion, BuildConfigurationId, BuildConfigurationVersion, BuildId,
    BuildVersion, ImmutableRevision, JobId, LeaseId, ProjectId, RepositoryId,
  };
  use octacity_server_factory::{
    BudgetLimit, BuildConfigurationRef, ExactSubject, FactoryClaim, FactoryClaimFence, FactoryClaimOwnership,
    FactoryConfigurationId, FactoryConfigurationRef, FactoryConfigurationVersion, FactoryMetadata, FactoryRun,
    FactoryRunId, RiskClass, StageAttempt, StageAttemptId, StageAttemptNumber, WorkArtifacts, WorkClassification,
    WorkEnvelope, WorkEnvelopeId, WorkPriority,
  };
  use octacity_server_store::{FactoryBuildLinkInput, FactoryBuildObservationInput};

  use super::*;

  struct Fixture {
    change_set: ChangeSet,
    link: FactoryBuildLink,
    observation: FactoryBuildObservationRecord,
    record: ArtifactRecord,
    bytes: Vec<u8>,
    requirement: EvidenceRequirement,
    attestation: FactoryEvidenceAttestation,
  }

  fn time(value: i64) -> Timestamp {
    Timestamp::from_unix_millis(value).expect("fixture timestamp")
  }

  fn digest(byte: u8) -> FactoryDigest {
    FactoryDigest::from_bytes([byte; 32])
  }

  fn key(value: &str) -> FactoryKey {
    FactoryKey::new(value).expect("fixture key")
  }

  fn reference(identity: &str, byte: u8) -> ImmutableReference {
    ImmutableReference::new(key(identity), key("v1"), digest(byte))
  }

  fn artifact(byte: u8) -> FactoryArtifactReference {
    FactoryArtifactReference::new(ArtifactId::generate(), digest(byte), 1).expect("fixture artifact")
  }

  fn stage(run: &FactoryRun, number: u64, target: FactoryStageTarget) -> StageAttempt {
    StageAttempt::new(
      StageAttemptId::generate(),
      run,
      StageAttemptNumber::new(number).expect("fixture stage number"),
      target,
      BudgetLimit::new(1, 1, 1, 1, 1).expect("fixture budget"),
      digest(u8::try_from(number).expect("bounded fixture number")),
      FactoryClaimOwnership::new(
        key("worker"),
        FactoryClaim::new(FactoryClaimFence::new(digest(9)), time(1), time(30)).expect("fixture claim"),
      ),
    )
  }

  fn fixture() -> Fixture {
    let exact = ExactSubject::new(
      ProjectId::generate(),
      RepositoryId::generate(),
      ImmutableRevision::new("base").expect("fixture base revision"),
    );
    let configuration = FactoryConfigurationRef::new(
      FactoryConfigurationId::generate(),
      FactoryConfigurationVersion::INITIAL,
      exact.project_id(),
      digest(1),
    );
    let work = WorkEnvelope::new(
      WorkEnvelopeId::generate(),
      configuration.clone(),
      octacity_server_factory::ExternalWorkIdentity::new("source/work").expect("fixture work identity"),
      exact,
      WorkArtifacts::new(artifact(30), artifact(31), vec![]).expect("fixture work artifacts"),
      WorkClassification::new(
        WorkPriority::new(1).expect("fixture priority"),
        RiskClass::Low,
        FactoryMetadata::default(),
      ),
    )
    .expect("fixture work");
    let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
    let implementation = stage(&run, 1, FactoryStageTarget::Implementation);
    let subject = CandidateSubject::new(
      run.subject().clone(),
      ImmutableRevision::new("candidate").expect("fixture candidate revision"),
      digest(2),
    );
    let change_set = ChangeSet::new(
      octacity_server_factory::ChangeSetId::generate(),
      &implementation,
      subject.clone(),
      ArtifactId::generate(),
      ArtifactId::generate(),
    )
    .expect("fixture ChangeSet");
    let validation = stage(&run, 2, FactoryStageTarget::Validation);
    let build_id = BuildId::generate();
    let attempt_id = AttemptId::generate();
    let job_id = JobId::generate();
    let link = FactoryBuildLink::new(
      &validation,
      FactoryBuildLinkInput {
        build_id,
        attempt_id,
        job_ids: vec![job_id],
        factory_configuration: configuration,
        target: FactoryStageTarget::Validation,
        build_configuration: BuildConfigurationRef::new(
          BuildConfigurationId::generate(),
          BuildConfigurationVersion::INITIAL,
          run.subject().project_id(),
          digest(3),
        ),
        task_envelope_digest: digest(4),
        exact_revision: subject.candidate_revision().clone(),
        parent: Some(FactoryBuildParent::ChangeSet(change_set.id())),
        effective_policy_digest: digest(5),
        input_digest: digest(6),
      },
    );
    let bytes = br#"{"tests":1,"failed":0}"#.to_vec();
    let content_digest = FactoryDigest::content_sha256(&bytes);
    let identity = octacity_server_artifacts::ArtifactIdentity {
      artifact_id: ArtifactId::generate(),
      build_id,
      attempt_id,
      job_id,
      lease_id: LeaseId::generate(),
      logical_name: ArtifactName::new("tests.json").expect("fixture artifact name"),
      artifact_type: ArtifactType::Report(ArtifactReportFormat::new("junit-v1").expect("fixture report format")),
      media_type: ArtifactMediaType::new("application/json").expect("fixture media type"),
      size_bytes: bytes.len() as u64,
      digest: ArtifactContentDigest::from_bytes(content_digest.as_bytes()),
      retention: ArtifactRetentionPolicy::Keep,
    };
    let pending = ArtifactRecord::pending(identity.clone(), time(9)).expect("fixture pending Artifact");
    let verifying = pending
      .transition(&identity, pending.version(), ArtifactEvent::BeginVerification, time(10))
      .expect("fixture verifying Artifact");
    let record = verifying
      .transition(&identity, verifying.version(), ArtifactEvent::Publish, time(11))
      .expect("fixture published Artifact");
    let observation = FactoryBuildObservationRecord::new(
      &link,
      FactoryBuildObservationInput {
        build_version: BuildVersion::INITIAL,
        attempt_id,
        attempt_version: AttemptVersion::INITIAL,
        job_ids: vec![job_id],
        outputs: vec![identity],
        state: BuildState::Succeeded,
        infrastructure_retry_eligible: false,
        observed_at: time(12),
      },
    )
    .expect("fixture observation");
    let schema = reference("junit-v1", 7);
    let tool = reference("test-runner", 8);
    let plugin = reference("octa-test", 9);
    let requirement = EvidenceRequirement::new(
      key("tests"),
      EvidenceOutputKind::Report,
      schema.clone(),
      tool.clone(),
      plugin.clone(),
    );
    let attestation = FactoryEvidenceAttestation::new(
      subject,
      key("tests"),
      schema,
      tool,
      plugin,
      DeterministicGateOutcome::Passed,
      time(20),
    );
    Fixture {
      change_set,
      link,
      observation,
      record,
      bytes,
      requirement,
      attestation,
    }
  }

  #[test]
  fn constructs_complete_manifest_from_exact_retained_build_output() {
    let fixture = fixture();
    let output = VerifiedFactoryEvidence::new(fixture.record, &fixture.bytes, fixture.attestation)
      .expect("fixture evidence verifies");

    let manifest = construct_factory_evidence_manifest(
      EvidenceManifestId::generate(),
      &fixture.change_set,
      &fixture.link,
      &fixture.observation,
      vec![fixture.requirement],
      vec![output],
      time(13),
    )
    .expect("fixture manifest is valid");

    assert_eq!(manifest.subject(), fixture.change_set.subject());
    assert_eq!(manifest.items().len(), 1);
    assert_eq!(manifest.items()[0].producer().build_id(), fixture.link.build_id);
    assert_eq!(manifest.items()[0].outcome(), DeterministicGateOutcome::Passed);
  }

  #[test]
  fn rejects_digest_drift_before_manifest_construction() {
    let fixture = fixture();
    assert!(matches!(
      VerifiedFactoryEvidence::new(fixture.record, b"tampered", fixture.attestation),
      Err(FactoryEvidenceError::Integrity)
    ));
  }

  #[test]
  fn rejects_stale_candidate_and_missing_required_report() {
    let mut fixture = fixture();
    fixture.attestation.subject = CandidateSubject::new(
      fixture.change_set.subject().exact().clone(),
      ImmutableRevision::new("new-candidate").expect("fixture candidate revision"),
      digest(40),
    );
    let output = VerifiedFactoryEvidence::new(fixture.record, &fixture.bytes, fixture.attestation)
      .expect("output integrity remains valid");
    assert!(matches!(
      construct_factory_evidence_manifest(
        EvidenceManifestId::generate(),
        &fixture.change_set,
        &fixture.link,
        &fixture.observation,
        vec![fixture.requirement.clone()],
        vec![output],
        time(13),
      ),
      Err(FactoryEvidenceError::Manifest(FactoryError::InconsistentSubject))
    ));
    assert!(matches!(
      construct_factory_evidence_manifest(
        EvidenceManifestId::generate(),
        &fixture.change_set,
        &fixture.link,
        &fixture.observation,
        vec![fixture.requirement],
        vec![],
        time(13),
      ),
      Err(FactoryEvidenceError::Manifest(FactoryError::InvalidReference {
        relationship: "evidence completeness"
      }))
    ));
  }

  #[test]
  fn rejects_stale_or_unretained_output() {
    let mut fixture = fixture();
    fixture.attestation.fresh_until = time(13);
    let output = VerifiedFactoryEvidence::new(fixture.record, &fixture.bytes, fixture.attestation)
      .expect("output integrity remains valid");
    assert!(matches!(
      construct_factory_evidence_manifest(
        EvidenceManifestId::generate(),
        &fixture.change_set,
        &fixture.link,
        &fixture.observation,
        vec![fixture.requirement],
        vec![output],
        time(13),
      ),
      Err(FactoryEvidenceError::Manifest(FactoryError::InvalidReference {
        relationship: "evidence freshness"
      }))
    ));
  }
}
