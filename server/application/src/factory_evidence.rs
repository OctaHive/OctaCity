//! Trusted construction of immutable Factory evidence from ordinary Build outputs.

use crate::FactoryNodeBuildCompletion;
use octacity_server_artifacts::{ArtifactRecord, ArtifactState, ArtifactType};
use octacity_server_domain::Timestamp;
use octacity_server_factory::{
  CandidateSubject, ChangeSet, DeterministicGateOutcome, EvidenceItem, EvidenceItemInput, EvidenceManifest,
  EvidenceManifestId, EvidenceOutputKind, EvidenceProducer, EvidenceRequirement, FactoryArtifactReference,
  FactoryDigest, FactoryError, FactoryKey, ImmutableReference,
};
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

/// Constructs evidence from a verified generic node result and its exact ordinary execution.
/// The frozen context must select this candidate. Logical node names and phase
/// roles carry no authority; every output must match the retained Build provenance.
pub fn construct_factory_evidence_manifest(
  id: EvidenceManifestId,
  change_set: &ChangeSet,
  completed: &FactoryNodeBuildCompletion,
  requirements: Vec<EvidenceRequirement>,
  outputs: Vec<VerifiedFactoryEvidence>,
  constructed_at: Timestamp,
) -> Result<EvidenceManifest, FactoryEvidenceError> {
  validate_build(change_set, completed, constructed_at)?;
  let provenance = completed
    .record
    .build_provenance()
    .ok_or(FactoryEvidenceError::InvalidEvidence)?;
  let mut items = Vec::with_capacity(outputs.len());
  for output in outputs {
    let identity = output.record.identity();
    let published_at = output.record.published_at().ok_or(FactoryEvidenceError::NotRetained)?;
    let verified = provenance
      .outputs()
      .iter()
      .find(|row| row.artifact.artifact_id() == identity.artifact_id)
      .ok_or(FactoryEvidenceError::InvalidEvidence)?;
    if identity.build_id != completed.execution.build_id()
      || identity.attempt_id != completed.execution.attempt_id()
      || !completed.execution.jobs().contains(&identity.job_id)
      || verified.artifact.content_digest().as_bytes() != identity.digest.as_bytes()
      || verified.artifact.encoded_size() != identity.size_bytes
      || verified.producer.build_id() != identity.build_id
      || verified.producer.attempt_id() != identity.attempt_id
      || verified.producer.job_id() != identity.job_id
      || verified.name != output.attestation.kind
      || verified.name.as_str() != identity.logical_name.as_str()
      || verified.schema != output.attestation.schema
      || verified.producer.tool() != &output.attestation.tool
      || verified.producer.plugin() != &output.attestation.plugin
      || published_at > verified.verified_at
      || output.attestation.fresh_until > verified.fresh_until
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
  completed: &FactoryNodeBuildCompletion,
  constructed_at: Timestamp,
) -> Result<(), FactoryEvidenceError> {
  let record = &completed.record;
  if record.input().subject() != change_set.subject().exact()
    || record
      .input()
      .context()
      .and_then(|context| context.subject().candidate())
      != Some(change_set.subject())
    || record.node_attempt_id() != completed.execution.node_attempt_id()
    || completed.completion.node_attempt_id() != record.node_attempt_id()
    || record.digest().map_err(FactoryEvidenceError::Manifest)? != completed.completion.output_digest()
    || record.observed_at() > constructed_at
    || completed.completion.observed_at() > constructed_at
    || record.build_provenance().is_none_or(|proof| {
      proof.outputs().iter().any(|output| {
        output.producer.build_id() != completed.execution.build_id()
          || output.producer.attempt_id() != completed.execution.attempt_id()
          || !completed.execution.jobs().contains(&output.producer.job_id())
          || output.verified_at > constructed_at
      })
    })
  {
    return Err(FactoryEvidenceError::InvalidEvidence);
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::factory_node_test_support::{Builds, Outputs, fixtures::*};
  use crate::{
    FactoryNodeBuildAdapter, FactoryNodeBuildCompletion, FactoryNodeBuildExecutionFacts, FactoryNodeBuildOutputs,
    FactoryNodeBuildStep,
  };
  use octacity_server_domain::{ArtifactId, ImmutableRevision};
  use octacity_server_factory::{BudgetUsage, FactoryClaimOwnership, FlowBuildIntent, FlowNodeKind};
  use octacity_server_orchestrator::BuildState;
  use std::sync::Arc;

  struct Fixture {
    change_set: ChangeSet,
    completed: FactoryNodeBuildCompletion,
    record: ArtifactRecord,
    bytes: Vec<u8>,
    requirement: EvidenceRequirement,
    attestation: FactoryEvidenceAttestation,
  }
  fn time(value: i64) -> Timestamp {
    Timestamp::from_unix_millis(value).unwrap()
  }
  fn digest(byte: u8) -> FactoryDigest {
    FactoryDigest::from_bytes([byte; 32])
  }

  fn fixture() -> Fixture {
    crate::factory_admission_tests::run_ready(async {
      let (_, admitted, schema, input, node) = execution_fixture(FlowNodeKind::BuildCommand);
      let subject = CandidateSubject::new(
        input.subject().clone(),
        ImmutableRevision::new("candidate").unwrap(),
        digest(2),
      );
      let change_set = ChangeSet::new(
        octacity_server_factory::ChangeSetId::generate(),
        &input,
        &node,
        subject.clone(),
        ArtifactId::generate(),
        ArtifactId::generate(),
      )
      .unwrap();
      let (input, node) =
        crate::factory_node_test_support::input_with_candidate_context(&admitted, input, &node, subject.clone());
      let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
      let builds = Arc::new(Builds::new());
      *builds.state.lock().unwrap() = BuildState::Succeeded;
      let bytes = br#"{"count":9}"#.to_vec();
      let mut document = crate::factory_node_test_support::document(
        &builds,
        &intent.profile().result_output,
        schema.reference(),
        bytes.clone(),
        intent.profile().tool.clone(),
      );
      document.plugin = intent.profile().plugin.clone();
      let record = document.record.clone();
      let outputs = FactoryNodeBuildOutputs {
        execution: FactoryNodeBuildExecutionFacts {
          profile: intent.profile().clone(),
          input_digest: intent.input().digest().unwrap(),
          permissions: intent.permissions().clone(),
          budget: intent.budget(),
          deadline: intent.node().deadline(),
        },
        documents: vec![document],
        usage: BudgetUsage {
          attempts: 1,
          output_bytes: bytes.len() as u64,
          ..Default::default()
        },
        verified_at: time(30),
        fresh_until: time(1000),
      };
      let requirement = EvidenceRequirement::new(
        intent.profile().result_output.clone(),
        EvidenceOutputKind::Report,
        schema.reference().clone(),
        intent.profile().tool.clone(),
        intent.profile().plugin.clone(),
      );
      let attestation = FactoryEvidenceAttestation::new(
        subject,
        requirement.kind().clone(),
        schema.reference().clone(),
        intent.profile().tool.clone(),
        intent.profile().plugin.clone(),
        DeterministicGateOutcome::Passed,
        time(40),
      );
      let adapter = FactoryNodeBuildAdapter::new(builds, Arc::new(Outputs(Some(outputs))));
      let FactoryNodeBuildStep::Completed(completed) = adapter
        .observe_or_dispatch(
          &intent,
          &admitted,
          &schema,
          FactoryClaimOwnership::new(intent.node().owner().clone(), intent.node().claim()),
          time(31),
        )
        .await
        .unwrap()
      else {
        panic!("verified Build must complete");
      };
      Fixture {
        change_set,
        completed: *completed,
        record,
        bytes,
        requirement,
        attestation,
      }
    })
  }

  #[test]
  fn constructs_complete_manifest_from_exact_retained_build_output() {
    let fixture = fixture();
    let output = VerifiedFactoryEvidence::new(fixture.record, &fixture.bytes, fixture.attestation)
      .expect("fixture evidence verifies");

    let manifest = construct_factory_evidence_manifest(
      EvidenceManifestId::generate(),
      &fixture.change_set,
      &fixture.completed,
      vec![fixture.requirement],
      vec![output],
      time(33),
    )
    .expect("fixture manifest is valid");

    assert_eq!(manifest.subject(), fixture.change_set.subject());
    assert_eq!(manifest.items().len(), 1);
    assert_eq!(
      manifest.items()[0].producer().build_id(),
      fixture.completed.execution.build_id()
    );
    assert_eq!(manifest.items()[0].outcome(), DeterministicGateOutcome::Passed);
  }

  #[test]
  fn retained_evidence_binds_exact_candidate_and_arbitrary_immutable_validation_inputs() {
    let fixture = fixture();
    let output = VerifiedFactoryEvidence::new(fixture.record, &fixture.bytes, fixture.attestation).unwrap();
    let manifest = construct_factory_evidence_manifest(
      EvidenceManifestId::generate(),
      &fixture.change_set,
      &fixture.completed,
      vec![fixture.requirement],
      vec![output],
      time(33),
    )
    .unwrap();
    let candidate_bundle =
      FactoryArtifactReference::new(fixture.change_set.bundle_artifact(), digest(51), 100).unwrap();
    let protected = FactoryArtifactReference::new(ArtifactId::generate(), digest(52), 100).unwrap();
    let inputs = vec![candidate_bundle.clone(), protected.clone()];
    let bound = manifest.with_inputs(inputs.clone()).unwrap();
    bound.verify_for(&fixture.change_set, &inputs).unwrap();
    let restored: EvidenceManifest = serde_json::from_slice(&serde_json::to_vec(&bound).unwrap()).unwrap();
    assert_eq!(restored.digest().unwrap(), bound.digest().unwrap());
    restored.verify_for(&fixture.change_set, &inputs).unwrap();
    let replaced = FactoryArtifactReference::new(protected.artifact_id(), digest(53), 100).unwrap();
    assert!(
      restored
        .verify_for(&fixture.change_set, &[candidate_bundle.clone(), replaced])
        .is_err()
    );
    assert!(
      restored
        .clone()
        .with_inputs(vec![protected.clone(), protected])
        .is_err()
    );
    let mut wire = serde_json::to_value(&fixture.change_set).unwrap();
    wire["subject"]["candidate_revision"] = serde_json::json!("other-candidate");
    let candidate: ChangeSet = serde_json::from_value(wire).unwrap();
    assert!(restored.verify_for(&candidate, &inputs).is_err());
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
        &fixture.completed,
        vec![fixture.requirement.clone()],
        vec![output],
        time(33),
      ),
      Err(FactoryEvidenceError::Manifest(FactoryError::InconsistentSubject))
    ));
    assert!(matches!(
      construct_factory_evidence_manifest(
        EvidenceManifestId::generate(),
        &fixture.change_set,
        &fixture.completed,
        vec![fixture.requirement],
        vec![],
        time(33),
      ),
      Err(FactoryEvidenceError::Manifest(FactoryError::InvalidReference {
        relationship: "evidence completeness"
      }))
    ));
  }

  #[test]
  fn rejects_stale_or_unretained_output() {
    let mut fixture = fixture();
    fixture.attestation.fresh_until = time(33);
    let output = VerifiedFactoryEvidence::new(fixture.record, &fixture.bytes, fixture.attestation)
      .expect("output integrity remains valid");
    assert!(matches!(
      construct_factory_evidence_manifest(
        EvidenceManifestId::generate(),
        &fixture.change_set,
        &fixture.completed,
        vec![fixture.requirement],
        vec![output],
        time(33),
      ),
      Err(FactoryEvidenceError::Manifest(FactoryError::InvalidReference {
        relationship: "evidence freshness"
      }))
    ));
  }
}
