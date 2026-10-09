use octacity_server_domain::{ArtifactId, ImmutableRevision, ProjectId, RepositoryId};

use crate::{
  BudgetLimit, CandidateSubject, ChangeSet, ChangeSetId, ExactSubject, FactoryArtifactReference, FactoryClaim,
  FactoryClaimFence, FactoryClaimOwnership, FactoryConfigurationId, FactoryConfigurationRef,
  FactoryConfigurationVersion, FactoryDigest, FactoryError, FactoryKey, FactoryMetadata, FactoryRun, FactoryRunId,
  FactoryStageTarget, RiskClass, StageAttempt, StageAttemptId, StageAttemptNumber, WorkArtifacts, WorkClassification,
  WorkEnvelope, WorkEnvelopeId, WorkPriority,
};

fn digest(byte: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([byte; 32])
}

fn artifact(byte: u8) -> FactoryArtifactReference {
  FactoryArtifactReference::new(ArtifactId::generate(), digest(byte), 1).expect("fixture artifact")
}

fn stage() -> StageAttempt {
  let subject = ExactSubject::new(
    ProjectId::generate(),
    RepositoryId::generate(),
    ImmutableRevision::new("base").expect("fixture revision"),
  );
  let configuration = FactoryConfigurationRef::new(
    FactoryConfigurationId::generate(),
    FactoryConfigurationVersion::INITIAL,
    subject.project_id(),
    digest(1),
  );
  let work = WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    configuration,
    crate::ExternalWorkIdentity::new("source/1").expect("fixture identity"),
    subject,
    WorkArtifacts::new(artifact(90), artifact(91), vec![]).expect("fixture artifacts"),
    WorkClassification::new(
      WorkPriority::new(1).expect("fixture priority"),
      RiskClass::Low,
      FactoryMetadata::default(),
    ),
  )
  .expect("fixture work");
  StageAttempt::new(
    StageAttemptId::generate(),
    &FactoryRun::admitted(FactoryRunId::generate(), &work),
    StageAttemptNumber::INITIAL,
    FactoryStageTarget::Implementation,
    BudgetLimit::new(1, 1, 1, 1, 1).expect("fixture budget"),
    digest(2),
    FactoryClaimOwnership::new(
      FactoryKey::new("worker").expect("fixture owner"),
      FactoryClaim::new(
        FactoryClaimFence::new(digest(9)),
        octacity_server_domain::Timestamp::from_unix_millis(1).expect("fixture claim start"),
        octacity_server_domain::Timestamp::from_unix_millis(2).expect("fixture claim deadline"),
      )
      .expect("fixture claim"),
    ),
  )
}

#[test]
fn changeset_rejects_a_candidate_for_another_exact_subject() {
  let stage = stage();
  let other = ExactSubject::new(
    ProjectId::generate(),
    RepositoryId::generate(),
    ImmutableRevision::new("other").expect("fixture revision"),
  );
  assert_eq!(
    ChangeSet::new(
      ChangeSetId::generate(),
      &stage,
      CandidateSubject::new(
        other,
        ImmutableRevision::new("candidate").expect("fixture revision"),
        digest(3),
      ),
      ArtifactId::generate(),
      ArtifactId::generate(),
    ),
    Err(FactoryError::InconsistentSubject)
  );
}
