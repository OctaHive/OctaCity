//! Installed author policy is part of the immutable command selection.
use crate::*;
use octacity_server_factory::*;

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}

#[test]
fn protected_author_policy_cannot_change_under_a_previous_command_pin() {
  let (_, _, mut command) = crate::factory_command_tests::command_fixture();
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
      vec![key("hidden.contract"), key("private.inventory"), key("audit.report")],
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
  let updated = FactoryProtectedTestAuthoringProfile::new(command, changed).unwrap();
  assert_ne!(first, updated.command().task_digest());
}
