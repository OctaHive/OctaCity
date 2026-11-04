//! Installed author policy is part of the immutable command selection.
use crate::*;
use octacity_server_factory::*;

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}

#[test]
fn protected_author_policy_cannot_change_under_a_previous_command_pin() {
  let (_, _, mut command) = crate::factory::commands::tests::factory_command_tests::command_fixture();
  let inventory = EvidenceRequirement::new(
    key("private.inventory"),
    EvidenceOutputKind::Artifact,
    command.outputs[0].schema().clone(),
    command.executor.clone(),
    command.plugin.clone(),
  );
  command.outputs.push(inventory.clone());
  let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    outputs: FactoryOutputPermissions::try_new(
      vec![
        key("hidden.contract"),
        key("private.inventory"),
        key("audit.report"),
        key("author.transcript"),
      ],
      4,
      9_000,
      1,
      1_000,
    )
    .unwrap(),
    ..FactoryPermissionDraft::default()
  })
  .unwrap();
  command.project_permissions = permissions.clone();
  command.local_permissions = LocalPermissionCeiling::fully_enforced(permissions);
  let limits: FactoryProtectedTestPolicy = serde_json::from_value(serde_json::json!({
    "classes":["project.contract"], "max_files":4, "max_bytes":9_000,
    "reserved_inputs":4, "retention_millis":1_000,
  }))
  .unwrap();
  let policy = FactoryProtectedTestAuthoringPolicy::new(
    limits,
    inventory,
    vec![FactoryProtectedTestFileOutput {
      output: key("hidden.contract"),
      class: key("project.contract"),
      path: FactoryRepositoryPath::new("contract.rs").unwrap(),
    }],
  )
  .unwrap();
  assert!(FactoryProtectedTestAuthoringProfile::new(command.clone(), policy.clone()).is_err());
  command.identity = ImmutableReference::new(
    command.identity.identity().clone(),
    command.identity.version().clone(),
    policy.digest().unwrap(),
  );
  let installed = FactoryProtectedTestAuthoringProfile::new(command.clone(), policy.clone()).unwrap();
  let first = installed.command().task_digest();
  let mut changed = serde_json::to_value(policy).unwrap();
  changed["files"][0]["path"] = serde_json::json!("new-contract.rs");
  let changed: FactoryProtectedTestAuthoringPolicy = serde_json::from_value(changed).unwrap();
  assert!(
    FactoryProtectedTestAuthoringProfile::new(command.clone(), changed.clone()).is_err(),
    "an old installed identity cannot silently select another protected file"
  );
  command.identity = ImmutableReference::new(
    command.identity.identity().clone(),
    command.identity.version().clone(),
    changed.digest().unwrap(),
  );
  let updated = FactoryProtectedTestAuthoringProfile::new(command.clone(), changed.clone()).unwrap();
  let mut encoded = serde_json::to_value(changed).unwrap();
  encoded["transcript"] = serde_json::json!("author.transcript");
  let with_transcript: FactoryProtectedTestAuthoringPolicy = serde_json::from_value(encoded.clone()).unwrap();
  assert!(FactoryProtectedTestAuthoringProfile::new(command.clone(), with_transcript.clone()).is_err());
  command.identity = ImmutableReference::new(
    command.identity.identity().clone(),
    command.identity.version().clone(),
    with_transcript.digest().unwrap(),
  );
  assert!(FactoryProtectedTestAuthoringProfile::new(command, with_transcript).is_ok());
  encoded["transcript"] = serde_json::json!("hidden.contract");
  assert!(serde_json::from_value::<FactoryProtectedTestAuthoringPolicy>(encoded).is_err());
  assert_ne!(first, updated.command().task_digest());
}

#[test]
fn protected_author_policy_rejects_outputs_that_fill_a_publication_page() {
  let (_, _, command) = crate::factory::commands::tests::factory_command_tests::command_fixture();
  let inventory = EvidenceRequirement::new(
    key("private.inventory"),
    EvidenceOutputKind::Artifact,
    command.outputs[0].schema().clone(),
    command.executor,
    command.plugin,
  );
  for (transcript, other_outputs) in [(None, 2), (Some("author.transcript"), 3)] {
    let page = usize::from(octacity_server_store::MAX_ARTIFACT_PAGE_SIZE);
    let files = (0..page - other_outputs)
      .map(|i| {
        serde_json::json!({
          "output":format!("hidden.{i}"), "class":"project.contract", "path":format!("tests/contract-{i}.rs")
        })
      })
      .collect::<Vec<_>>();
    let mut policy = serde_json::json!({
      "policy":{
        "classes":["project.contract"], "max_files":page, "max_bytes":9_000,
        "reserved_inputs":4, "retention_millis":1_000,
      },
      "inventory":inventory,
      "files":files,
      "transcript":transcript,
    });
    assert!(serde_json::from_value::<FactoryProtectedTestAuthoringPolicy>(policy.clone()).is_err());
    policy["files"].as_array_mut().unwrap().pop();
    assert!(serde_json::from_value::<FactoryProtectedTestAuthoringPolicy>(policy).is_ok());
  }
}
