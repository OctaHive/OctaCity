use crate::flow_test_support::*;
use crate::*;

#[test]
fn immutable_candidate_deliverables_require_declared_public_changes_and_reject_disclosure() {
  let policy: CandidateDeliverablePolicy = serde_json::from_value(serde_json::json!({
    "permitted_paths":["docs","src","tests/public","tools"],"forbidden_paths":["tests/private"],
    "required":[{"kind":"documentation","paths":["docs"],"min_files":1},
      {"kind":"public_tests","paths":["tests/public"],"min_files":1},
      {"kind":"scripts","paths":["tools"],"min_files":1}]
  }))
  .unwrap();
  let subject = FactoryTaskSubject::Exact(work().subject().repository().unwrap().clone());
  let contract = CandidateDeliverableContract::new(
    FactoryDigest::from_bytes([1; 32]),
    subject,
    FactoryDigest::from_bytes([2; 32]),
    policy.clone(),
  )
  .unwrap();
  let observations = |paths: &[&str]| {
    paths
      .iter()
      .enumerate()
      .map(|(i, p)| CandidateFileObservation {
        path: FactoryRepositoryPath::new(*p).unwrap(),
        content_digest: Some(FactoryDigest::from_bytes([i as u8 + 3; 32])),
      })
      .collect::<Vec<_>>()
  };
  let complete = observations(&["docs/api.md", "src/api.rs", "tests/public/api.rs", "tools/deploy.sh"]);
  contract.verify_files(&complete, &[]).unwrap();
  let rework = observations(&["src/api.rs"]);
  contract
    .verify_candidate(&rework, &complete, &[])
    .expect("rework preserves earlier public deliverables");
  let mut deleted = complete.clone();
  deleted.remove(2);
  assert!(
    contract.verify_candidate(&rework, &deleted, &[]).is_err(),
    "removing a previously added test invalidates the cumulative result"
  );
  assert!(
    contract
      .verify_candidate(&observations(&["tests/private/hidden.rs"]), &complete, &[])
      .is_err()
  );
  assert!(
    contract
      .verify_candidate(&rework, &complete, &[complete[2].content_digest.unwrap()])
      .is_err(),
    "inherited content must still be checked against protected files"
  );
  for index in [0, 2, 3] {
    let mut missing = complete.clone();
    missing.remove(index);
    assert!(
      contract.verify_files(&missing, &[]).is_err(),
      "each applicable deliverable is mandatory"
    );
  }
  for forbidden in ["tests/private/hidden.rs", ".github/workflows/unapproved.yml"] {
    let mut files = complete.clone();
    files.extend(observations(&[forbidden]));
    assert!(contract.verify_files(&files, &[]).is_err());
  }
  assert!(
    contract
      .verify_files(&complete, &[complete[2].content_digest.unwrap()])
      .is_err(),
    "copying a protected test into a public path is disclosure"
  );
  let mut deleted = complete.clone();
  deleted[2].content_digest = None;
  assert!(
    contract.verify_files(&deleted, &[]).is_err(),
    "deleting a test cannot satisfy required public tests"
  );
  let bytes = serde_json::to_vec(&contract).unwrap();
  let restored: CandidateDeliverableContract = serde_json::from_slice(&bytes).unwrap();
  assert_eq!(contract.digest().unwrap(), restored.digest().unwrap());
  assert_ne!(
    contract.digest().unwrap(),
    CandidateDeliverableContract::new(
      FactoryDigest::from_bytes([9; 32]),
      contract.subject().clone(),
      FactoryDigest::from_bytes([2; 32]),
      policy
    )
    .unwrap()
    .digest()
    .unwrap()
  );
}

#[test]
fn capture_policy_is_frozen_in_the_common_build_profile() {
  let mut wire = serde_json::to_value(binding(work().subject().project_id())).unwrap();
  wire["capture_deliverables"] = serde_json::json!({"accepted_contract":{"value":{"mapping":"constant","value":"a".repeat(64)}},
    "policy":{"permitted_paths":["src","tests/public"],"forbidden_paths":["tests/private"],
      "required":[{"kind":"tests","paths":["tests/public"],"min_files":1}]}});
  let binding: FlowBuildBinding = serde_json::from_value(wire.clone()).unwrap();
  assert_eq!(
    serde_json::to_value(binding).unwrap()["capture_deliverables"],
    wire["capture_deliverables"]
  );
}

#[test]
fn capture_dispatch_requires_a_valid_acceptance_projection_before_any_build() {
  let work = work();
  let mut wire = serde_json::to_value(binding(work.subject().project_id())).unwrap();
  wire["capture_deliverables"] = serde_json::json!({"accepted_contract":{"value":{"mapping":"pointer","path":"/missing_acceptance"}},
    "policy":{"permitted_paths":["src","tests/public"],"forbidden_paths":["tests/private"],
      "required":[{"kind":"tests","paths":["tests/public"],"min_files":1}]}});
  let binding: FlowBuildBinding = serde_json::from_value(wire).unwrap();
  let (_, admitted, _, input, node) =
    execution_fixture_for_work_with_binding(FlowNodeKind::Reasoning, work, binding, None, None);
  assert!(
    FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 0).is_err(),
    "an absent accepted contract cannot be discovered only after editing the repository"
  );
}

#[test]
fn an_allowlisted_deliverable_policy_does_not_require_artificial_exclusions() {
  let policy: CandidateDeliverablePolicy = serde_json::from_value(serde_json::json!({
    "permitted_paths":["source","unit"],"forbidden_paths":[],
    "required":[{"kind":"visible_checks","paths":["unit"],"min_files":1}]
  }))
  .expect("an explicit allowlist can already exclude all other paths");
  let contract = CandidateDeliverableContract::new(
    FactoryDigest::from_bytes([1; 32]),
    FactoryTaskSubject::Exact(work().subject().repository().unwrap().clone()),
    FactoryDigest::from_bytes([2; 32]),
    policy,
  )
  .unwrap();
  let file = CandidateFileObservation {
    path: FactoryRepositoryPath::new("unit/api.rs").unwrap(),
    content_digest: Some(FactoryDigest::from_bytes([3; 32])),
  };
  contract.verify_files(std::slice::from_ref(&file), &[]).unwrap();
  assert!(
    contract
      .verify_files(
        &[CandidateFileObservation {
          path: FactoryRepositoryPath::new("outside/api.rs").unwrap(),
          ..file
        }],
        &[]
      )
      .is_err()
  );
}
