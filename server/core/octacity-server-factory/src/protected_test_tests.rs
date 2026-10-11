use crate::flow_test_support::{key, reference, work};
use crate::*;
use octacity_server_domain::{ArtifactId, AttemptId, BuildId, JobId, Timestamp};

fn at(n: i64) -> Timestamp {
  Timestamp::from_unix_millis(n).unwrap()
}

#[test]
fn protected_bundles_require_every_declared_class_and_restore_only_a_bounded_public_handoff() {
  let plan = ProtectedTestPlan::new(
    FactoryDigest::from_bytes([1; 32]),
    FactoryTaskSubject::Exact(work().subject().clone()),
    ProtectedTestCoverage::new(vec![key("integration"), key("project.contract")], 4, 4096, 4).unwrap(),
    at(1000),
  )
  .unwrap();
  let artifact =
    FactoryArtifactReference::new(ArtifactId::generate(), FactoryDigest::from_bytes([2; 32]), 100).unwrap();
  let producer = EvidenceProducer::new(
    BuildId::generate(),
    AttemptId::generate(),
    JobId::generate(),
    reference("test.capture"),
    reference("tests.plugin"),
  );
  let file = |class, path| ProtectedTestFile {
    class: key(class),
    path: FactoryRepositoryPath::new(path).unwrap(),
    artifact: FactoryArtifactReference::new(ArtifactId::generate(), FactoryDigest::from_bytes([3; 32]), 10).unwrap(),
  };
  assert!(
    ProtectedTestBundle::new(
      plan.clone(),
      artifact.clone(),
      vec![file("integration", "hidden/assertions.rs")],
      producer.clone(),
      at(30),
      at(1000)
    )
    .is_err(),
    "one declared class cannot stand in for the other required class"
  );
  let bundle = ProtectedTestBundle::new(
    plan,
    artifact,
    vec![
      file("integration", "hidden/assertions.rs"),
      file("project.contract", "hidden/expected.bin"),
    ],
    producer,
    at(30),
    at(1000),
  )
  .unwrap();
  let frozen = bundle.canonical_bytes().unwrap();
  let restored: ProtectedTestBundle = serde_json::from_slice(&frozen).unwrap();
  assert_eq!(restored.digest().unwrap(), bundle.digest().unwrap());
  assert_eq!(
    serde_json::to_value(restored.public_handoff()).unwrap(),
    serde_json::json!({
      "accepted_contract": "01".repeat(32), "classes":["integration", "project.contract"]
    })
  );
  let mut replaced: serde_json::Value = serde_json::from_slice(&frozen).unwrap();
  replaced["files"].as_array_mut().unwrap().pop();
  assert!(serde_json::from_value::<ProtectedTestBundle>(replaced).is_err());
}

#[test]
fn protected_files_cannot_alias_one_artifact_as_two_independent_test_inputs() {
  let plan = ProtectedTestPlan::new(
    FactoryDigest::from_bytes([1; 32]),
    FactoryTaskSubject::Exact(work().subject().clone()),
    ProtectedTestCoverage::new(vec![key("integration"), key("security")], 4, 4096, 4).unwrap(),
    at(1000),
  )
  .unwrap();
  let shared = FactoryArtifactReference::new(ArtifactId::generate(), FactoryDigest::from_bytes([3; 32]), 10).unwrap();
  let files = vec![
    ProtectedTestFile {
      class: key("integration"),
      path: FactoryRepositoryPath::new("integration.rs").unwrap(),
      artifact: shared.clone(),
    },
    ProtectedTestFile {
      class: key("security"),
      path: FactoryRepositoryPath::new("security.rs").unwrap(),
      artifact: shared,
    },
  ];
  assert!(
    ProtectedTestBundle::new(
      plan,
      FactoryArtifactReference::new(ArtifactId::generate(), FactoryDigest::from_bytes([2; 32]), 100).unwrap(),
      files,
      EvidenceProducer::new(
        BuildId::generate(),
        AttemptId::generate(),
        JobId::generate(),
        reference("capture"),
        reference("plugin")
      ),
      at(30),
      at(1000)
    )
    .is_err()
  );
}

#[test]
fn protected_byte_limit_covers_inventory_and_sources_together() {
  let plan = ProtectedTestPlan::new(
    FactoryDigest::from_bytes([1; 32]),
    FactoryTaskSubject::Exact(work().subject().clone()),
    ProtectedTestCoverage::new(vec![key("custom")], 2, 100, 4).unwrap(),
    at(1000),
  )
  .unwrap();
  let producer = EvidenceProducer::new(
    BuildId::generate(),
    AttemptId::generate(),
    JobId::generate(),
    reference("capture"),
    reference("plugin"),
  );
  let artifact =
    |size| FactoryArtifactReference::new(ArtifactId::generate(), FactoryDigest::from_bytes([2; 32]), size).unwrap();
  let files = |size| {
    vec![ProtectedTestFile {
      class: key("custom"),
      path: FactoryRepositoryPath::new("acceptance.rs").unwrap(),
      artifact: artifact(size),
    }]
  };
  assert!(
    ProtectedTestBundle::new(
      plan.clone(),
      artifact(80),
      files(80),
      producer.clone(),
      at(30),
      at(1000)
    )
    .is_err(),
    "the configured 100-byte limit must not accept 160 retained bytes"
  );
  assert!(ProtectedTestBundle::new(plan, artifact(40), files(60), producer, at(30), at(1000)).is_ok());
}
