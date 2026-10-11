use octacity_server_domain::{ArtifactId, ImmutableRevision, ProjectId, RepositoryId};

use crate::{CandidateSubject, ChangeSet, ChangeSetId, ExactSubject, FactoryDigest, FactoryError};

fn digest(byte: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([byte; 32])
}

#[test]
fn a_candidate_binds_to_a_frozen_arbitrary_node_without_a_stage_role() {
  let (work, _, _, input, node) = crate::flow_test_support::execution_fixture(crate::FlowNodeKind::BuildCommand);
  let subject = CandidateSubject::new(
    work.subject().clone(),
    ImmutableRevision::new("candidate").unwrap(),
    digest(3),
  );
  let candidate = ChangeSet::new(
    ChangeSetId::generate(),
    &input,
    &node,
    subject.clone(),
    ArtifactId::generate(),
    ArtifactId::generate(),
  )
  .unwrap();
  assert_eq!(candidate.node_attempt_id(), node.id());
  assert_eq!(candidate.subject(), &subject);
  let mut legacy = serde_json::to_value(&candidate).unwrap();
  legacy["stage_attempt_id"] = serde_json::json!("00000000-0000-0000-0000-000000000123");
  assert!(
    serde_json::from_value::<ChangeSet>(legacy).is_err(),
    "the removed stage identity is not an accepted candidate field"
  );
  let (_, _, _, unrelated, _) = crate::flow_test_support::execution_fixture(crate::FlowNodeKind::BuildCommand);
  assert!(
    ChangeSet::new(
      ChangeSetId::generate(),
      &unrelated,
      &node,
      subject,
      ArtifactId::generate(),
      ArtifactId::generate()
    )
    .is_err(),
    "a candidate cannot replace its exact frozen producing input"
  );
}

#[test]
fn changeset_rejects_a_candidate_for_another_exact_subject() {
  let (_, _, _, input, node) = crate::flow_test_support::execution_fixture(crate::FlowNodeKind::BuildCommand);
  let other = ExactSubject::new(
    ProjectId::generate(),
    RepositoryId::generate(),
    ImmutableRevision::new("other").expect("fixture revision"),
  );
  assert_eq!(
    ChangeSet::new(
      ChangeSetId::generate(),
      &input,
      &node,
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
