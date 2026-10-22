use octacity_server_domain::ProjectId;
use octacity_server_domain::{BuildConfigurationId, BuildConfigurationVersion};
use octacity_server_factory::{
  BudgetLimit, BuildConfigurationRef, FactoryCredentialPurpose, FactoryDigest, FactoryKey, FactoryPermissionSet,
  FactoryRole, FactoryRoleDefinition, FactoryRoleId, FactoryRoleProfile, FactoryRoleProfileDefinition,
  FactoryRoleProfileId, FactoryRoleProfileVersion, FactoryRoleVersion, FactoryTaskToolchain, FactoryText,
  ImmutableReference, MAX_FACTORY_ROLE_DEFINITION_BYTES, MAX_FACTORY_ROLE_INSTRUCTION_BYTES,
  MAX_FACTORY_ROLE_PROFILE_ENTRIES,
};

fn exact(name: &str) -> ImmutableReference {
  ImmutableReference::new(
    FactoryKey::new(name).unwrap(),
    FactoryKey::new("v1").unwrap(),
    FactoryDigest::from_bytes([7; 32]),
  )
}

fn profile(project: ProjectId, effort: &str) -> FactoryRoleProfileDefinition {
  FactoryRoleProfileDefinition {
    name: FactoryText::new("UI specialist").unwrap(),
    instructions: "# Local rules\nUse selected skills.\n".into(),
    toolchain: FactoryTaskToolchain {
      plugin: exact("model.plugin"),
      executable: exact("model.harness"),
      model: exact("open.model"),
    },
    model_parameters: [(FactoryKey::new("reasoning_effort").unwrap(), serde_json::json!(effort))].into(),
    skills: [(FactoryKey::new("testing").unwrap(), exact("skills.testing"))].into(),
    context_requirements: [FactoryKey::new("requirements").unwrap()].into(),
    build_configuration: BuildConfigurationRef::new(
      BuildConfigurationId::generate(),
      BuildConfigurationVersion::INITIAL,
      project,
      FactoryDigest::from_bytes([8; 32]),
    ),
    credential_purpose: FactoryCredentialPurpose::Model,
    permissions: FactoryPermissionSet::deny_all(),
    budget: BudgetLimit::new(1, 1_000, 200, 100, 1_024).unwrap(),
  }
}

fn role(project: ProjectId, instructions: &str) -> FactoryRole {
  FactoryRole::publish(
    FactoryRoleId::generate(),
    project,
    FactoryRoleDefinition {
      name: FactoryText::new("Custom reviewer").unwrap(),
      instructions: instructions.into(),
    },
  )
  .unwrap()
}

#[test]
fn projects_publish_and_replace_arbitrary_roles_without_mutating_retained_versions() {
  let project_a = ProjectId::generate();
  let project_b = ProjectId::generate();
  let original = role(project_a, "# Review\nRead-only.\n");
  let other = role(project_b, "Check documentation.");
  let retained = original.reference().clone();
  let replacement = original
    .replace(
      FactoryRoleVersion::new(2).unwrap(),
      FactoryRoleDefinition {
        name: FactoryText::new("Frontend specialist").unwrap(),
        instructions: "Inspect UI changes.".into(),
      },
    )
    .unwrap();
  assert_eq!(original.reference(), &retained);
  assert_eq!(original.definition().instructions, "# Review\nRead-only.\n");
  assert_eq!(replacement.reference().id, retained.id);
  assert_eq!(replacement.reference().project_id, project_a);
  assert_eq!(replacement.reference().version.get(), 2);
  assert_ne!(replacement.reference().digest, retained.digest);
  assert_eq!(other.reference().project_id, project_b);
  assert_eq!(other.reference().version.get(), 1);
  assert_eq!(other.definition().instructions, "Check documentation.");
  assert!(
    original
      .replace(FactoryRoleVersion::new(3).unwrap(), original.definition().clone())
      .is_err()
  );
}

#[test]
fn independent_profiles_pin_exact_owned_roles_and_replace_without_changing_siblings() {
  let a = ProjectId::generate();
  let b = ProjectId::generate();
  let original_role = role(a, "Review implementation.");
  let foreign_role = role(b, "Review documentation.");
  let default = FactoryRoleProfile::publish(
    FactoryRoleProfileId::generate(),
    a,
    &original_role,
    profile(a, "medium"),
  )
  .unwrap();
  let mut pro_definition = default.definition().clone();
  pro_definition
    .model_parameters
    .insert(FactoryKey::new("reasoning_effort").unwrap(), serde_json::json!("high"));
  let pro = FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), a, &original_role, pro_definition).unwrap();
  let other =
    FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), b, &foreign_role, profile(b, "low")).unwrap();
  assert_ne!(default.reference().id, pro.reference().id);
  assert_ne!(default.reference().digest, pro.reference().digest);
  assert_eq!(default.definition().toolchain.model, pro.definition().toolchain.model);
  assert_eq!(default.role(), original_role.reference());
  let new_role = original_role
    .replace(
      FactoryRoleVersion::new(2).unwrap(),
      FactoryRoleDefinition {
        name: FactoryText::new("Renamed AI agent").unwrap(),
        instructions: "Updated shared instructions.".into(),
      },
    )
    .unwrap();
  let replaced = pro
    .replace(
      FactoryRoleProfileVersion::new(2).unwrap(),
      &new_role,
      pro.definition().clone(),
    )
    .unwrap();
  assert_eq!(replaced.reference().id, pro.reference().id);
  assert_eq!(replaced.role(), new_role.reference());
  assert_ne!(replaced.reference().digest, pro.reference().digest);
  assert_eq!(pro.role(), original_role.reference());
  assert_eq!(default.role(), original_role.reference());
  assert_eq!(other.role(), foreign_role.reference());
  assert_eq!(other.reference().version.get(), 1);
  assert!(FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), a, &foreign_role, profile(a, "high")).is_err());
  assert!(
    pro
      .replace(
        FactoryRoleProfileVersion::new(2).unwrap(),
        &foreign_role,
        pro.definition().clone()
      )
      .is_err()
  );
  assert!(
    pro
      .replace(
        FactoryRoleProfileVersion::new(3).unwrap(),
        &new_role,
        pro.definition().clone()
      )
      .is_err()
  );
}

#[test]
fn canonical_content_digests_cover_role_and_profile_content_and_survive_restoration() {
  let project = ProjectId::generate();
  let role = role(project, "# Review\nRead-only.\n");
  // Independent SHA-256 vector: two u64 big-endian length-framed UTF-8 fields,
  // domain and compact {name,instructions} JSON; identities and versions are separate references.
  assert_eq!(
    role.reference().digest.to_string(),
    "78d0868c80ac1fd63d3bf1fef303441827cf80a0263782702a24340338388662"
  );
  let mut first = profile(project, "medium");
  first
    .model_parameters
    .insert(FactoryKey::new("temperature").unwrap(), serde_json::json!(0.5));
  first
    .model_parameters
    .insert(FactoryKey::new("stream").unwrap(), serde_json::json!(true));
  let mut reordered = first.clone();
  reordered.model_parameters = [
    (FactoryKey::new("stream").unwrap(), serde_json::json!(true)),
    (FactoryKey::new("temperature").unwrap(), serde_json::json!(0.5)),
    (
      FactoryKey::new("reasoning_effort").unwrap(),
      serde_json::json!("medium"),
    ),
  ]
  .into();
  let a = FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, first).unwrap();
  let b = FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, reordered).unwrap();
  assert_eq!(a.reference().digest, b.reference().digest);
  let serialized = serde_json::to_value(&a).unwrap();
  assert_eq!(
    serde_json::from_value::<FactoryRoleProfile>(serialized.clone()).unwrap(),
    a
  );
  let serialized_role = serde_json::to_value(&role).unwrap();
  assert_eq!(
    serde_json::from_value::<FactoryRole>(serialized_role.clone()).unwrap(),
    role
  );
  let mut changed = serialized.clone();
  changed["definition"]["model_parameters"]["reasoning_effort"] = serde_json::json!("high");
  assert!(serde_json::from_value::<FactoryRoleProfile>(changed).is_err());
  let mut changed = serialized;
  changed["definition"]["toolchain"]["credentials"] = serde_json::json!("not a configuration field");
  assert!(serde_json::from_value::<FactoryRoleProfile>(changed).is_err());
  let mut changed = serialized_role;
  changed["definition"]["instructions"] = serde_json::json!("Different shared instructions");
  assert!(serde_json::from_value::<FactoryRole>(changed).is_err());
}

#[test]
fn publication_and_restore_enforce_utf8_instruction_collection_and_encoded_document_bounds() {
  let project = ProjectId::generate();
  let valid = role(project, &"я".repeat(MAX_FACTORY_ROLE_INSTRUCTION_BYTES / 2));
  let mut excessive = valid.definition().clone();
  excessive.instructions.push('я');
  assert!(FactoryRole::publish(FactoryRoleId::generate(), project, excessive).is_err());
  let mut bad = valid.definition().clone();
  bad.instructions = "instructions\0hidden".into();
  assert!(FactoryRole::publish(FactoryRoleId::generate(), project, bad).is_err());
  let base = profile(project, "medium");
  let mut maximal = base.clone();
  maximal.context_requirements = (0..MAX_FACTORY_ROLE_PROFILE_ENTRIES)
    .map(|n| FactoryKey::new(format!("context.{n}")).unwrap())
    .collect();
  FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &valid, maximal.clone()).unwrap();
  maximal.context_requirements.insert(FactoryKey::new("excess").unwrap());
  assert!(FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &valid, maximal).is_err());
  for invalid in [
    serde_json::json!({"nested": "data"}),
    serde_json::json!([1]),
    serde_json::Value::Null,
  ] {
    let mut bad = base.clone();
    bad
      .model_parameters
      .insert(FactoryKey::new("invalid").unwrap(), invalid);
    assert!(FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &valid, bad).is_err());
  }
  let published = FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &valid, base).unwrap();
  let mut oversized = published.definition().clone();
  oversized.model_parameters.insert(
    FactoryKey::new("large").unwrap(),
    serde_json::json!("\\".repeat(MAX_FACTORY_ROLE_DEFINITION_BYTES / 2)),
  );
  assert!(FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &valid, oversized).is_err());
  let mut restored = serde_json::to_value(&published).unwrap();
  restored["definition"]["instructions"] = serde_json::json!("я".repeat(MAX_FACTORY_ROLE_INSTRUCTION_BYTES / 2 + 1));
  assert!(serde_json::from_value::<FactoryRoleProfile>(restored).is_err());
}

#[test]
fn profiles_cannot_reparent_or_bind_a_foreign_build_configuration() {
  let project = ProjectId::generate();
  let original = role(project, "Shared instructions.");
  let another = role(project, "Another agent.");
  let profile = FactoryRoleProfile::publish(
    FactoryRoleProfileId::generate(),
    project,
    &original,
    profile(project, "medium"),
  )
  .unwrap();
  assert!(
    profile
      .replace(
        FactoryRoleProfileVersion::new(2).unwrap(),
        &another,
        profile.definition().clone()
      )
      .is_err()
  );
  let mut foreign = profile.definition().clone();
  foreign.build_configuration = BuildConfigurationRef::new(
    BuildConfigurationId::generate(),
    BuildConfigurationVersion::INITIAL,
    ProjectId::generate(),
    FactoryDigest::from_bytes([8; 32]),
  );
  assert!(FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &original, foreign.clone()).is_err());
  let mut restored = serde_json::to_value(profile).unwrap();
  restored["definition"] = serde_json::to_value(foreign).unwrap();
  assert!(serde_json::from_value::<FactoryRoleProfile>(restored).is_err());
}
