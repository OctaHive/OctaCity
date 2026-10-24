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
    model_identifier: None,
    thinking_parameter: None,
    template: None,
    execution: Default::default(),
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
      avatar: None,
      skills: Default::default(),
      template: None,
      icon: None,
      description: String::new(),
      concurrency_limit: None,
      name: FactoryText::new("Custom reviewer").unwrap(),
      instructions: instructions.into(),
    },
  )
  .unwrap()
}

#[test]
fn thinking_metadata_references_one_existing_model_parameter_without_copying_its_value() {
  let project = ProjectId::generate();
  let role = role(project, "Review changes.");
  let mut wire = serde_json::to_value(profile(project, "medium")).unwrap();
  wire["thinking_parameter"] = serde_json::json!("analysis_budget");
  wire["model_parameters"] = serde_json::json!({"analysis_budget": 1024, "private_option": "private-value"});
  let definition: FactoryRoleProfileDefinition = serde_json::from_value(wire).unwrap();
  let original = FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, definition).unwrap();
  let retained = serde_json::to_value(&original).unwrap();
  assert_eq!(retained["definition"]["thinking_parameter"], "analysis_budget");
  assert_eq!(retained["definition"]["model_parameters"]["analysis_budget"], 1024);
  assert_eq!(
    serde_json::from_value::<FactoryRoleProfile>(retained.clone()).unwrap(),
    original
  );
  let mut changed = retained;
  changed["definition"]["thinking_parameter"] = serde_json::json!("private_option");
  assert!(serde_json::from_value::<FactoryRoleProfile>(changed).is_err());
  let mut missing = serde_json::to_value(original.definition()).unwrap();
  missing["thinking_parameter"] = serde_json::json!("undeclared");
  let missing = serde_json::from_value(missing).unwrap();
  assert!(FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, missing).is_err());
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
        avatar: None,
        skills: Default::default(),
        template: None,
        icon: None,
        description: String::new(),
        concurrency_limit: None,
        name: FactoryText::new("Frontend specialist").unwrap(),
        instructions: "Inspect UI changes.".into(),
      },
    )
    .unwrap();
  assert_eq!(original.reference(), &retained);
  assert_eq!(original.definition().instructions.entries(), ["# Review\nRead-only.\n"]);
  assert_eq!(replacement.reference().id, retained.id);
  assert_eq!(replacement.reference().project_id, project_a);
  assert_eq!(replacement.reference().version.get(), 2);
  assert_ne!(replacement.reference().digest, retained.digest);
  assert_eq!(other.reference().project_id, project_b);
  assert_eq!(other.reference().version.get(), 1);
  assert_eq!(other.definition().instructions.entries(), ["Check documentation."]);
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
        avatar: None,
        skills: Default::default(),
        template: None,
        icon: None,
        description: String::new(),
        concurrency_limit: None,
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
  excessive.instructions = format!("{}я", valid.definition().instructions.entries()[0]).into();
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

#[test]
fn ai_agent_metadata_and_concurrency_are_immutable_role_content() {
  let project = ProjectId::generate();
  let definition = serde_json::json!({
    "name": "UI reviewer", "instructions": "Review changes.",
    "icon": "bot", "description": "Checks UI\nand accessibility.", "concurrency_limit": 2
  });
  let original = FactoryRole::publish(
    FactoryRoleId::generate(),
    project,
    serde_json::from_value(definition.clone()).unwrap(),
  )
  .unwrap();
  assert_eq!(serde_json::to_value(original.definition()).unwrap(), definition);
  let mut changed = definition.clone();
  changed["description"] = serde_json::json!("Updated responsibility");
  let replacement = original
    .replace(
      FactoryRoleVersion::new(2).unwrap(),
      serde_json::from_value(changed.clone()).unwrap(),
    )
    .unwrap();
  assert_eq!(serde_json::to_value(original.definition()).unwrap(), definition);
  assert_eq!(serde_json::to_value(replacement.definition()).unwrap(), changed);
  assert_ne!(replacement.reference().digest, original.reference().digest);
  let mut invalid = definition;
  invalid["concurrency_limit"] = serde_json::json!(0);
  assert!(serde_json::from_value::<FactoryRoleDefinition>(invalid).is_err());
}

#[test]
fn legacy_role_profile_snapshot_keeps_its_content_identity() {
  let snapshot: serde_json::Value = serde_json::from_str(include_str!("fixtures/role_profile.v1.json")).unwrap();
  let role: FactoryRole = serde_json::from_value(snapshot["role"].clone()).unwrap();
  let profile: FactoryRoleProfile = serde_json::from_value(snapshot["profile"].clone()).unwrap();
  assert_eq!(serde_json::to_value(&role).unwrap(), snapshot["role"]);
  assert_eq!(serde_json::to_value(&profile).unwrap(), snapshot["profile"]);
  assert_eq!(profile.role(), role.reference());
}

#[test]
fn ordered_role_and_profile_instructions_preserve_layers_and_digest_order() {
  let snapshot: serde_json::Value = serde_json::from_str(include_str!("fixtures/role_profile.v1.json")).unwrap();
  let legacy: FactoryRole = serde_json::from_value(snapshot["role"].clone()).unwrap();
  let project = legacy.reference().project_id;
  let mut definition = snapshot["role"]["definition"].clone();
  definition["instructions"] = serde_json::json!(["# Shared rules\nFollow Project rules.", "Review accessibility."]);
  let original = FactoryRole::publish(
    FactoryRoleId::generate(),
    project,
    serde_json::from_value(definition.clone()).unwrap(),
  )
  .unwrap();
  assert_eq!(
    original.definition().instructions.entries(),
    ["# Shared rules\nFollow Project rules.", "Review accessibility."]
  );
  definition["instructions"].as_array_mut().unwrap().reverse();
  let changed = original
    .replace(
      FactoryRoleVersion::new(2).unwrap(),
      serde_json::from_value(definition).unwrap(),
    )
    .unwrap();
  assert_ne!(original.reference().digest, changed.reference().digest);
  let mut definition = snapshot["profile"]["definition"].clone();
  definition["instructions"] = serde_json::json!(["Prefer public tests.", "Preserve formatting.\n"]);
  let profile = FactoryRoleProfile::publish(
    FactoryRoleProfileId::generate(),
    project,
    &original,
    serde_json::from_value(definition).unwrap(),
  )
  .unwrap();
  assert_eq!(
    profile.definition().instructions.entries(),
    ["Prefer public tests.", "Preserve formatting.\n"]
  );
  assert_eq!(legacy.definition().instructions.entries(), ["# Review\nRead-only.\n"]);
  assert_eq!(
    serde_json::from_value::<FactoryRoleProfile>(serde_json::to_value(&profile).unwrap()).unwrap(),
    profile
  );
}

#[test]
fn instruction_sets_enforce_total_bytes_and_entry_bounds_before_publication() {
  let project = ProjectId::generate();
  let legacy = role(project, "Legacy instruction");
  let base = serde_json::to_value(legacy.definition()).unwrap();
  for entries in [
    vec![String::new(); 65],
    vec!["a".repeat(MAX_FACTORY_ROLE_INSTRUCTION_BYTES), "b".into()],
  ] {
    let mut definition = base.clone();
    definition["instructions"] = serde_json::json!(entries);
    let definition = serde_json::from_value(definition).unwrap();
    assert!(FactoryRole::publish(FactoryRoleId::generate(), project, definition).is_err());
  }
  let mut valid = base.clone();
  valid["instructions"] = serde_json::json!([
    "я".repeat(MAX_FACTORY_ROLE_INSTRUCTION_BYTES / 4),
    "я".repeat(MAX_FACTORY_ROLE_INSTRUCTION_BYTES / 4)
  ]);
  FactoryRole::publish(
    FactoryRoleId::generate(),
    project,
    serde_json::from_value(valid).unwrap(),
  )
  .unwrap();
  let mut excessive_description = base;
  excessive_description["description"] =
    serde_json::json!("x".repeat(octacity_server_factory::MAX_FACTORY_TEXT_BYTES + 1));
  assert!(
    FactoryRole::publish(
      FactoryRoleId::generate(),
      project,
      serde_json::from_value(excessive_description).unwrap()
    )
    .is_err()
  );
}

fn configured_profile_definition(project: ProjectId) -> serde_json::Value {
  let mut definition = serde_json::to_value(profile(project, "medium")).unwrap();
  let connection = octacity_server_domain::IntegrationId::generate();
  definition["execution"] = serde_json::json!({
    "environment": {
      "LANGUAGE": {"kind":"public", "value":"ru\n"},
      "MODEL_AUTH": {"kind":"secret", "reference":{"provider":"operator-secrets", "reference":"projects/ui/model"}}
    },
    "cli_arguments": ["--output", "structured", ""],
    "mcp_servers": {"tools":{"configuration":exact("mcp.tools"),"integration":"tracker"}},
    "integrations": {"tracker":{
      "project_id":project, "connection_id":connection,"connection_version":1,"access_version":1,
      "resource":exact("tracker.resource"),"capability":exact("tracker.read")
    }}
  });
  definition
}

#[test]
fn profile_execution_settings_pin_existing_runtime_and_logical_integrations() {
  let project = ProjectId::generate();
  let role = role(project, "Shared instructions.");
  let definition = configured_profile_definition(project);
  let original = FactoryRoleProfile::publish(
    FactoryRoleProfileId::generate(),
    project,
    &role,
    serde_json::from_value(definition.clone()).unwrap(),
  )
  .unwrap();
  assert_eq!(serde_json::to_value(original.definition()).unwrap(), definition);
  assert_eq!(
    serde_json::from_value::<FactoryRoleProfile>(serde_json::to_value(&original).unwrap()).unwrap(),
    original
  );
  for (pointer, value) in [
    ("/model_parameters/reasoning_effort", serde_json::json!("thorough")),
    (
      "/toolchain/executable",
      serde_json::to_value(exact("another.runtime")).unwrap(),
    ),
    (
      "/toolchain/model",
      serde_json::to_value(exact("another.model")).unwrap(),
    ),
    ("/execution/environment/LANGUAGE/value", serde_json::json!("en")),
    (
      "/execution/environment/MODEL_AUTH/reference/reference",
      serde_json::json!("projects/ui/new-model"),
    ),
    ("/execution/cli_arguments/1", serde_json::json!("text")),
    (
      "/execution/mcp_servers/tools/configuration",
      serde_json::to_value(exact("other.tools")).unwrap(),
    ),
    ("/execution/integrations/tracker/access_version", serde_json::json!(2)),
  ] {
    let mut replacement = definition.clone();
    *replacement.pointer_mut(pointer).unwrap() = value;
    let replacement = original
      .replace(
        FactoryRoleProfileVersion::new(2).unwrap(),
        &role,
        serde_json::from_value(replacement).unwrap(),
      )
      .unwrap();
    assert_ne!(replacement.reference().digest, original.reference().digest, "{pointer}");
  }
  assert_eq!(serde_json::to_value(original.definition()).unwrap(), definition);
}

#[test]
fn profile_execution_publication_rejects_invalid_shape_bounds_and_foreign_bindings() {
  let project = ProjectId::generate();
  let role = role(project, "Shared rules.");
  let base = configured_profile_definition(project);
  let mut cases = vec![];
  for (pointer, value) in [
    (
      "/execution/environment",
      serde_json::json!({"BAD=NAME":{"kind":"public","value":"value"}}),
    ),
    (
      "/execution/environment/LANGUAGE/value",
      serde_json::json!("sensitive-env-value\0"),
    ),
    (
      "/execution/environment/MODEL_AUTH/reference/reference",
      serde_json::json!(" untrimmed "),
    ),
    ("/execution/cli_arguments", serde_json::json!(["arg\0"])),
    ("/execution/cli_arguments", serde_json::json!(vec!["x"; 65])),
    (
      "/execution/cli_arguments",
      serde_json::json!(["x".repeat(octacity_server_factory::MAX_COMMAND_ARGUMENT_BYTES + 1)]),
    ),
    (
      "/execution/mcp_servers/tools/integration",
      serde_json::json!("not-selected"),
    ),
    (
      "/execution/integrations/tracker/project_id",
      serde_json::to_value(ProjectId::generate()).unwrap(),
    ),
  ] {
    let mut definition = base.clone();
    *definition.pointer_mut(pointer).unwrap() = value;
    cases.push((pointer.to_owned(), definition));
  }
  for (collection, item) in [
    ("environment", "LANGUAGE"),
    ("mcp_servers", "tools"),
    ("integrations", "tracker"),
  ] {
    let mut definition = base.clone();
    definition["execution"][collection] = serde_json::Value::Object(
      (0..65)
        .map(|i| (format!("entry{i}"), base["execution"][collection][item].clone()))
        .collect(),
    );
    cases.push((collection.to_owned(), definition));
  }
  for (label, definition) in cases {
    let definition: FactoryRoleProfileDefinition = serde_json::from_value(definition).unwrap();
    let error =
      FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, definition).expect_err(&label);
    assert!(!error.to_string().contains("sensitive-env-value"));
  }
  for (pointer, value) in [
    (
      "/execution/environment/MODEL_AUTH",
      serde_json::json!({"kind":"secret","value":"literal-credential"}),
    ),
    (
      "/execution/mcp_servers/tools",
      serde_json::json!({"command":"unqualified-command","url":"https://unreviewed.invalid"}),
    ),
  ] {
    let mut definition = base.clone();
    *definition.pointer_mut(pointer).unwrap() = value;
    assert!(serde_json::from_value::<FactoryRoleProfileDefinition>(definition).is_err());
  }
}

#[test]
fn starter_role_templates_are_release_versioned_and_leave_execution_choices_unresolved() {
  let templates = octacity_server_factory::factory_role_templates().unwrap();
  assert_eq!(templates.len(), 3);
  let expected = [
    ("coder", vec![("default", "coder"), ("pro", "coder-pro")]),
    ("reviewer", vec![("default", "reviewer")]),
    ("architect", vec![("default", "architect")]),
  ];
  for (template, (identity, profiles)) in templates.iter().zip(expected) {
    let reference = template.reference().unwrap();
    assert_eq!(reference.identity().as_str(), identity);
    assert_eq!(reference.version().as_str(), "v1");
    assert_eq!(template.role.name.as_str(), identity);
    assert_eq!(
      template
        .profiles
        .iter()
        .map(|(key, seed)| (key.as_str(), seed.name.as_str()))
        .collect::<Vec<_>>(),
      profiles
    );
    let wire = serde_json::to_value(template).unwrap();
    // Starter content must not fabricate an executable binding or privilege tier.
    for seed in wire["profiles"].as_object().unwrap().values() {
      assert_eq!(
        seed.as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>(),
        ["instructions", "name"]
      );
    }
  }
}

#[test]
fn role_template_copies_keep_reviewed_choices_and_exact_provenance_without_live_inheritance() {
  use octacity_server_factory::{FactoryRoleTemplateBinding, factory_role_templates};
  let mut template = factory_role_templates().unwrap().remove(0);
  let origin = template.reference().unwrap();
  let project = ProjectId::generate();
  let bindings = template
    .profiles
    .iter()
    .map(|(key, seed)| {
      let mut definition = profile(project, if key.as_str() == "pro" { "high" } else { "medium" });
      definition.name = seed.name.clone();
      definition.instructions = seed.instructions.clone();
      (
        key.clone(),
        FactoryRoleTemplateBinding {
          id: FactoryRoleProfileId::generate(),
          definition,
        },
      )
    })
    .collect::<std::collections::BTreeMap<_, _>>();
  let role_id = FactoryRoleId::generate();
  let first = template
    .instantiate(project, role_id, template.role.clone(), bindings.clone())
    .unwrap();
  assert_eq!(first.role.reference().project_id, project);
  assert_eq!(first.role.definition().template.as_ref(), Some(&origin));
  for (key, copy) in &first.profiles {
    let mut expected = bindings[key].definition.clone();
    expected.template = Some(origin.clone());
    assert_eq!(
      copy.definition(),
      &expected,
      "copying must preserve every reviewed setting"
    );
    assert_eq!(copy.role(), first.role.reference());
  }
  let default = &first.profiles[&FactoryKey::new("default").unwrap()];
  let pro = &first.profiles[&FactoryKey::new("pro").unwrap()];
  assert_eq!(default.definition().toolchain, pro.definition().toolchain);
  assert_eq!(default.definition().permissions, pro.definition().permissions);
  assert_eq!(default.definition().budget, pro.definition().budget);
  assert_ne!(default.reference().digest, pro.reference().digest);
  let mut renamed = template.role.clone();
  renamed.name = FactoryText::new("Frontend specialist").unwrap();
  renamed.instructions = "Project-specific shared rules".into();
  let mut choices = bindings.clone();
  for choice in choices.values_mut() {
    choice.id = FactoryRoleProfileId::generate();
  }
  let mut additional = profile(project, "medium");
  additional.name = FactoryText::new("Security specialist").unwrap();
  choices.insert(
    FactoryKey::new("security").unwrap(),
    FactoryRoleTemplateBinding {
      id: FactoryRoleProfileId::generate(),
      definition: additional,
    },
  );
  let changed = &mut choices.get_mut(&FactoryKey::new("pro").unwrap()).unwrap();
  changed.id = FactoryRoleProfileId::generate();
  changed.definition.name = FactoryText::new("Accessibility specialist").unwrap();
  changed.definition.instructions = "Keep the reviewed UI contract".into();
  let second = template
    .instantiate(project, FactoryRoleId::generate(), renamed, choices)
    .unwrap();
  assert_ne!(first.role.reference().id, second.role.reference().id);
  assert_eq!(second.profiles.len(), 3);
  assert_eq!(
    second.profiles[&FactoryKey::new("security").unwrap()]
      .definition()
      .name
      .as_str(),
    "Security specialist"
  );
  assert_eq!(second.role.definition().name.as_str(), "Frontend specialist");
  assert_eq!(first.role.definition().name.as_str(), "coder");
  assert_eq!(
    second.profiles[&FactoryKey::new("pro").unwrap()]
      .definition()
      .name
      .as_str(),
    "Accessibility specialist"
  );
  template.version = FactoryKey::new("v2").unwrap();
  template.role.instructions = "New release responsibility".into();
  template
    .profiles
    .get_mut(&FactoryKey::new("pro").unwrap())
    .unwrap()
    .instructions = "New release instructions".into();
  assert_ne!(template.reference().unwrap(), origin);
  assert_eq!(first.role.definition().template.as_ref(), Some(&origin));
  assert_eq!(pro.definition().template.as_ref(), Some(&origin));
  assert_eq!(
    pro.definition().instructions,
    bindings[&FactoryKey::new("pro").unwrap()].definition.instructions
  );
  let restored: FactoryRoleProfile = serde_json::from_value(serde_json::to_value(pro).unwrap()).unwrap();
  assert_eq!(&restored, pro);
}

#[test]
fn role_template_copy_requires_complete_bounded_owned_profile_choices() {
  use octacity_server_factory::{FactoryRoleTemplateBinding, factory_role_templates};
  let template = factory_role_templates().unwrap().remove(0);
  let project = ProjectId::generate();
  let instantiate =
    |bindings| template.instantiate(project, FactoryRoleId::generate(), template.role.clone(), bindings);
  assert!(
    instantiate(Default::default()).is_err(),
    "unresolved templates cannot become complete definitions"
  );
  let mut bindings = template
    .profiles
    .keys()
    .map(|key| {
      (
        key.clone(),
        FactoryRoleTemplateBinding {
          id: FactoryRoleProfileId::generate(),
          definition: profile(project, "medium"),
        },
      )
    })
    .collect::<std::collections::BTreeMap<_, _>>();
  let pro = bindings.remove(&FactoryKey::new("pro").unwrap()).unwrap();
  assert!(
    instantiate(bindings.clone()).is_err(),
    "every starter profile requires explicit choices"
  );
  bindings.insert(FactoryKey::new("pro").unwrap(), pro.clone());
  bindings.get_mut(&FactoryKey::new("pro").unwrap()).unwrap().id = bindings[&FactoryKey::new("default").unwrap()].id;
  assert!(
    instantiate(bindings.clone()).is_err(),
    "independent profiles require independent identities"
  );
  bindings.insert(FactoryKey::new("pro").unwrap(), pro.clone());
  let foreign = profile(ProjectId::generate(), "medium");
  bindings.get_mut(&FactoryKey::new("pro").unwrap()).unwrap().definition = foreign;
  assert!(
    instantiate(bindings.clone()).is_err(),
    "template provenance grants no cross-Project binding"
  );
  bindings.insert(FactoryKey::new("pro").unwrap(), pro);
  for index in 0..63 {
    bindings.insert(
      FactoryKey::new(format!("additional-{index}")).unwrap(),
      FactoryRoleTemplateBinding {
        id: FactoryRoleProfileId::generate(),
        definition: profile(project, "medium"),
      },
    );
  }
  assert!(
    instantiate(bindings).is_err(),
    "a copy uses the shared profile cardinality bound"
  );
}

fn role_parameter_schema() -> octacity_server_factory::FlowDataSchema {
  use octacity_server_factory::{FlowDataSchema, FlowFieldSchema, FlowValueSchema};
  FlowDataSchema::new(
    FactoryKey::new("runtime.parameters").unwrap(),
    FactoryKey::new("v1").unwrap(),
    FlowValueSchema::Object {
      fields: [(
        FactoryKey::new("reasoning_effort").unwrap(),
        FlowFieldSchema::required(FlowValueSchema::Enum {
          values: vec![serde_json::json!("medium"), serde_json::json!("high")],
        }),
      )]
      .into(),
    },
  )
  .unwrap()
}
#[test]
fn installed_role_profile_capabilities_use_existing_catalog_and_parameter_schemas() {
  use octacity_server_factory::*;
  let project = ProjectId::generate();
  let definition = profile(project, "medium");
  let capability = FactoryRoleProfileCapability {
    model_catalog: None,
    ceiling: definition.clone(),
    parameters: role_parameter_schema(),
    public_environment: Default::default(),
    local_permissions: LocalPermissionCeiling::fully_enforced(definition.permissions.clone()),
  };
  let alias = FactoryKey::new("runtime.profile").unwrap();
  let choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
    references: vec![FactoryReferenceChoice {
      kind: FactoryChoiceKind::NodeCapability,
      alias: alias.clone(),
      reference: definition.toolchain.executable.clone(),
    }],
    build_configurations: vec![],
  })
  .unwrap()
  .with_role_profile_capability(alias.clone(), capability)
  .unwrap();
  assert_eq!(choices.role_profile_capabilities().len(), 1);
  let role = role(project, "Project instructions");
  let candidate = FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, definition).unwrap();
  choices.role_profile_capabilities()[&alias]
    .validate(&candidate)
    .unwrap();
  let mut unsupported = candidate.definition().clone();
  unsupported.model_parameters.insert(
    FactoryKey::new("reasoning_effort").unwrap(),
    serde_json::json!("unsupported"),
  );
  let unsupported = FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, unsupported).unwrap();
  assert_eq!(
    choices.role_profile_capabilities()[&alias].validate(&unsupported),
    Err(FactoryRoleProfileIncompatibility::ModelParameters)
  );
}

#[test]
fn installed_thinking_metadata_preserves_unannotated_profiles() {
  use octacity_server_factory::*;
  let project = ProjectId::generate();
  let role = role(project, "Project rules");
  let definition = profile(project, "medium");
  let legacy =
    FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, definition.clone()).unwrap();
  let mut annotated = definition;
  annotated.thinking_parameter = Some(FactoryKey::new("reasoning_effort").unwrap());
  let capability = FactoryRoleProfileCapability {
    model_catalog: None,
    ceiling: annotated.clone(),
    parameters: role_parameter_schema(),
    public_environment: Default::default(),
    local_permissions: LocalPermissionCeiling::fully_enforced(annotated.permissions.clone()),
  };
  assert_eq!(capability.validate(&legacy), Ok(()));
  let explicit = FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, annotated).unwrap();
  assert_eq!(capability.validate(&explicit), Ok(()));
}

#[test]
fn installed_profile_validation_rejects_unsupported_execution_and_authority_without_silent_narrowing() {
  use octacity_server_factory::*;
  let project = ProjectId::generate();
  let definition = profile(project, "medium");
  let empty_environment = Default::default();
  let capability = FactoryRoleProfileCapability {
    model_catalog: None,
    ceiling: definition.clone(),
    parameters: role_parameter_schema(),
    public_environment: empty_environment,
    local_permissions: LocalPermissionCeiling::fully_enforced(definition.permissions.clone()),
  };
  let role = role(project, "Project rules");
  let mut cases = vec![];
  let mut changed = definition.clone();
  changed.thinking_parameter = Some(FactoryKey::new("reasoning_effort").unwrap());
  cases.push((changed, FactoryRoleProfileIncompatibility::ModelParameters));
  let mut changed = definition.clone();
  changed.toolchain.model = exact("other.model");
  cases.push((changed, FactoryRoleProfileIncompatibility::Toolchain));
  let mut changed = definition.clone();
  changed.execution.cli_arguments = vec!["--disable-policy".into()];
  cases.push((changed, FactoryRoleProfileIncompatibility::Arguments));
  let mut changed = definition.clone();
  changed.execution.environment.insert(
    "HOME".into(),
    FactoryRoleEnvironmentValue::Public {
      value: "/private".into(),
    },
  );
  cases.push((changed, FactoryRoleProfileIncompatibility::Environment));
  let mut changed = definition.clone();
  changed
    .skills
    .insert(FactoryKey::new("testing").unwrap(), exact("substituted.skill"));
  cases.push((changed, FactoryRoleProfileIncompatibility::Dependencies));
  let mut changed = definition.clone();
  changed.execution.mcp_servers.insert(
    FactoryKey::new("tools").unwrap(),
    FactoryRoleMcpBinding {
      configuration: exact("uninstalled.mcp"),
      integration: None,
    },
  );
  cases.push((changed, FactoryRoleProfileIncompatibility::Dependencies));
  let mut changed = definition.clone();
  changed
    .context_requirements
    .insert(FactoryKey::new("private-context").unwrap());
  cases.push((changed, FactoryRoleProfileIncompatibility::Dependencies));
  let mut changed = definition.clone();
  changed.credential_purpose = FactoryCredentialPurpose::Delivery;
  cases.push((changed, FactoryRoleProfileIncompatibility::CredentialPurpose));
  let mut changed = definition.clone();
  changed.budget = BudgetLimit::new(2, 1000, 200, 100, 1024).unwrap();
  cases.push((changed, FactoryRoleProfileIncompatibility::Budget));
  for (changed, expected) in cases {
    let candidate = FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, changed).unwrap();
    assert_eq!(capability.validate(&candidate), Err(expected));
  }
  let mut unsupported_backend = capability.clone();
  unsupported_backend.local_permissions =
    LocalPermissionCeiling::try_new(definition.permissions.clone(), vec![]).unwrap();
  let candidate = FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, definition).unwrap();
  assert_eq!(
    unsupported_backend.validate(&candidate),
    Err(FactoryRoleProfileIncompatibility::Permissions)
  );
}

#[test]
fn installed_profile_declarations_share_definition_bounds() {
  use octacity_server_factory::*;
  let project = ProjectId::generate();
  let definition = profile(project, "medium");
  let alias = FactoryKey::new("runtime").unwrap();
  let capability = FactoryRoleProfileCapability {
    model_catalog: None,
    ceiling: definition.clone(),
    parameters: role_parameter_schema(),
    public_environment: Default::default(),
    local_permissions: LocalPermissionCeiling::fully_enforced(definition.permissions.clone()),
  };
  let choices = || {
    FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
      references: vec![FactoryReferenceChoice {
        kind: FactoryChoiceKind::NodeCapability,
        alias: alias.clone(),
        reference: definition.toolchain.executable.clone(),
      }],
      build_configurations: vec![],
    })
    .unwrap()
  };
  let mut invalid = capability.clone();
  invalid.ceiling.instructions = "bad\0instructions".into();
  assert!(
    choices().with_role_profile_capability(alias.clone(), invalid).is_err(),
    "installed declarations use the same content bounds"
  );
  let mut invalid = capability;
  invalid.public_environment = (0..65)
    .map(|i| (format!("VALUE_{i}"), role_parameter_schema()))
    .collect();
  assert!(choices().with_role_profile_capability(alias, invalid).is_err());
}

#[test]
fn agent_skills_are_shared_by_default_and_pro_without_copying_them_into_profiles() {
  use octacity_server_factory::*;
  let mut input = execution_package_input();
  let shared = input.profile.definition().skills.clone();
  assert!(!shared.is_empty());
  let mut definition = serde_json::to_value(input.role.definition()).unwrap();
  definition["skills"] = serde_json::to_value(&shared).unwrap();
  let definition = serde_json::from_value(definition).expect("role accepts shared skills");
  let role = FactoryRole::publish(input.role.reference().id, input.role.reference().project_id, definition).unwrap();
  let mut settings = input.profile.definition().clone();
  settings.skills.clear();
  input.role = role;
  for name in ["default", "pro"] {
    settings.name = FactoryText::new(name).unwrap();
    input.profile = FactoryRoleProfile::publish(
      FactoryRoleProfileId::generate(),
      input.role.reference().project_id,
      &input.role,
      settings.clone(),
    )
    .unwrap();
    let package = FactoryRoleExecutionPackage::new(input.clone()).expect("shared skills are selected for each profile");
    assert!(package.inputs().profile.definition().skills.is_empty());
    assert_eq!(package.inputs().skills.len(), shared.len());
    let restored: FactoryRoleExecutionPackage = serde_json::from_slice(&package.canonical_bytes().unwrap()).unwrap();
    assert_eq!(restored.digest().unwrap(), package.digest().unwrap());
  }
}

#[test]
fn agent_avatar_is_retained_raster_content_and_rejects_active_or_invalid_payloads() {
  let project = ProjectId::generate();
  let original = role(project, "Shared instructions");
  let png = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgaPj/HwAEggJ/59habAAAAABJRU5ErkJggg==";
  let mut value = serde_json::to_value(original.definition()).unwrap();
  value["avatar"] = serde_json::json!(png);
  let definition = serde_json::from_value(value.clone()).expect("agent accepts an uploaded avatar");
  let saved = FactoryRole::publish(original.reference().id, project, definition).unwrap();
  let restored: FactoryRole = serde_json::from_value(serde_json::to_value(&saved).unwrap()).unwrap();
  assert_eq!(serde_json::to_value(restored.definition()).unwrap()["avatar"], png);
  assert_ne!(saved.reference().digest, original.reference().digest);
  for invalid in [
    "https://external.invalid/avatar.png".to_string(),
    "data:image/svg+xml;base64,PHN2Zy8+".to_string(),
    "data:image/png;base64,AAAA".to_string(),
    format!("data:image/png;base64,{}", "A".repeat(262144)),
  ] {
    value["avatar"] = serde_json::json!(invalid);
    assert!(serde_json::from_value::<octacity_server_factory::FactoryRoleDefinition>(value.clone()).is_err());
  }
}

fn execution_package_input() -> octacity_server_factory::FactoryRoleExecutionPackageInput {
  use octacity_server_domain::{ArtifactId, ImmutableRevision, RepositoryId};
  use octacity_server_factory::*;
  let project = ProjectId::generate();
  let subject = FactoryTaskSubject::Exact(ExactSubject::new(
    project,
    RepositoryId::generate(),
    ImmutableRevision::new("base-revision").unwrap(),
  ));
  let artifact = |bytes: &[u8]| {
    FactoryArtifactReference::new(
      ArtifactId::generate(),
      FactoryDigest::content_sha256(bytes),
      bytes.len() as u64,
    )
    .unwrap()
  };
  let rules = "Project requires TDD.\n";
  let rule_artifact = artifact(rules.as_bytes());
  let task_artifact = artifact(b"Implement the supplied requirements.");
  let entry = |kind, key, reference: &FactoryArtifactReference| {
    ContextManifestEntry::new(
      kind,
      FactoryKey::new(key).unwrap(),
      subject.clone(),
      FactoryContextReference::Artifact(reference.clone()),
      reference.content_digest(),
      reference.encoded_size(),
      FactorySafeText::new("Required declared input").unwrap(),
      FactoryDigest::from_bytes([9; 32]),
    )
    .unwrap()
  };
  let context = ContextManifest::new(
    ContextManifestId::generate(),
    subject.clone(),
    FactoryDigest::from_bytes([9; 32]),
    vec![
      entry(ContextSourceKind::Policy, "project-rules", &rule_artifact),
      entry(ContextSourceKind::Task, "requirements", &task_artifact),
    ],
  )
  .unwrap();
  let bundle = FactoryRoleSkillBundle::new(
    FactoryKey::new("skills.testing").unwrap(),
    FactoryKey::new("v1").unwrap(),
    [
      (
        FactoryRepositoryPath::new("SKILL.md").unwrap(),
        artifact(b"Write a failing public test first."),
      ),
      (
        FactoryRepositoryPath::new("examples/public.txt").unwrap(),
        artifact(b"Supporting example"),
      ),
    ]
    .into(),
  )
  .unwrap();
  let role = role(project, "Keep changes focused.");
  let mut definition: FactoryRoleProfileDefinition =
    serde_json::from_value(configured_profile_definition(project)).unwrap();
  definition.skills = [(FactoryKey::new("testing").unwrap(), bundle.reference().clone())].into();
  let profile = FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &role, definition).unwrap();
  let task = FactoryTaskEnvelope::new(
    FactoryTaskDeclaration {
      id: TaskEnvelopeId::generate(),
      node_attempt_id: NodeAttemptId::generate(),
      subject,
      mode: FactoryTaskMode::Implement,
    },
    FactoryTaskDefinition {
      task: task_artifact,
      specifications: vec![],
      prior_findings: vec![],
      permissions: profile.definition().permissions.clone(),
      budget: profile.definition().budget,
      result_schema: FactoryTaskResultSchema::ImplementationV1,
      deliverables: vec![FactoryTaskDeliverable::new(
        FactoryKey::new("result").unwrap(),
        FactoryRepositoryPath::new("outputs/result.json").unwrap(),
        true,
      )],
    },
    &context,
    profile.definition().toolchain.clone(),
    FactoryTaskControlDigests {
      policy: FactoryDigest::from_bytes([1; 32]),
      prompt: FactoryDigest::from_bytes([2; 32]),
      prompt_provenance: FactoryDigest::from_bytes([3; 32]),
      provenance: FactoryDigest::from_bytes([4; 32]),
    },
  )
  .unwrap();
  FactoryRoleExecutionPackageInput {
    project_rules: vec![FactoryRoleProjectRule {
      source: FactoryKey::new("project-rules").unwrap(),
      instructions: rules.into(),
    }],
    role,
    profile,
    skills: [(FactoryKey::new("testing").unwrap(), bundle)].into(),
    task,
    context,
  }
}

#[test]
fn execution_package_preserves_ordered_inputs_and_has_reproducible_versioned_identity() {
  use octacity_server_factory::*;
  let input = execution_package_input();
  let package = FactoryRoleExecutionPackage::new(input.clone()).unwrap();
  let repeated = FactoryRoleExecutionPackage::new(input.clone()).unwrap();
  assert_eq!(package.canonical_bytes().unwrap(), repeated.canonical_bytes().unwrap());
  assert_eq!(package.digest().unwrap(), repeated.digest().unwrap());
  assert_eq!(package.inputs(), &input);
  assert_eq!(
    package.inputs().project_rules[0].instructions,
    "Project requires TDD.\n"
  );
  assert_eq!(
    package.inputs().role.definition().instructions.entries(),
    ["Keep changes focused."]
  );
  assert_eq!(
    package.inputs().profile.definition().instructions.entries(),
    ["# Local rules\nUse selected skills.\n"]
  );
  assert_eq!(
    package.inputs().profile.definition().execution.cli_arguments,
    ["--output", "structured", ""]
  );
  assert_eq!(
    package.inputs().skills[&FactoryKey::new("testing").unwrap()]
      .files()
      .len(),
    2
  );
  let json = serde_json::to_value(&package).unwrap();
  assert_eq!(json["schema_version"], 1);
}

#[test]
fn execution_package_rejects_mismatched_pins_and_unselected_inputs_without_fallback() {
  use octacity_server_factory::*;
  let input = execution_package_input();
  let mut cases = vec![];
  let mut changed = input.clone();
  changed.role = role(input.role.reference().project_id, "Different role version");
  cases.push(("role pin", changed));
  let mut changed = input.clone();
  changed.context = ContextManifest::new(
    input.context.id(),
    input.context.subject().clone(),
    FactoryDigest::from_bytes([99; 32]),
    input.context.entries().to_vec(),
  )
  .unwrap();
  cases.push(("context digest", changed));
  for field in ["plugin", "executable", "model"] {
    let mut changed = input.clone();
    let mut definition = serde_json::to_value(changed.profile.definition()).unwrap();
    definition["toolchain"][field] = serde_json::to_value(exact("different.installed-choice")).unwrap();
    changed.profile = FactoryRoleProfile::publish(
      changed.profile.reference().id,
      changed.role.reference().project_id,
      &changed.role,
      serde_json::from_value(definition).unwrap(),
    )
    .unwrap();
    cases.push((field, changed));
  }
  let mut changed = input.clone();
  let mut definition = changed.profile.definition().clone();
  definition
    .context_requirements
    .insert(FactoryKey::new("unselected-context").unwrap());
  changed.profile = FactoryRoleProfile::publish(
    changed.profile.reference().id,
    changed.role.reference().project_id,
    &changed.role,
    definition,
  )
  .unwrap();
  cases.push(("required Context", changed));
  let mut changed = input.clone();
  changed.skills.clear();
  cases.push(("missing skill", changed));
  let mut changed = input.clone();
  changed.skills.insert(
    FactoryKey::new("ambient-skill").unwrap(),
    input.skills.values().next().unwrap().clone(),
  );
  cases.push(("unselected skill", changed));
  let mut changed = input.clone();
  let bundle = input.skills.values().next().unwrap();
  changed.skills.insert(
    FactoryKey::new("testing").unwrap(),
    FactoryRoleSkillBundle::new(
      FactoryKey::new("substituted.bundle").unwrap(),
      FactoryKey::new("v2").unwrap(),
      bundle.files().clone(),
    )
    .unwrap(),
  );
  cases.push(("skill pin", changed));
  let mut changed = input.clone();
  changed.project_rules[0].instructions = "Substituted rule text".into();
  cases.push(("rule byte identity", changed));
  let mut changed = input.clone();
  changed.project_rules[0].source = FactoryKey::new("requirements").unwrap();
  cases.push(("task data is not Project policy", changed));
  let mut changed = input.clone();
  changed.project_rules.push(changed.project_rules[0].clone());
  cases.push(("duplicate rule source", changed));
  let mut changed = input;
  let mut definition = changed.profile.definition().clone();
  definition.budget = BudgetLimit::new(1, 1, 1, 1, 1).unwrap();
  changed.profile = FactoryRoleProfile::publish(
    changed.profile.reference().id,
    changed.role.reference().project_id,
    &changed.role,
    definition,
  )
  .unwrap();
  cases.push(("profile budget", changed));
  for (reason, input) in cases {
    assert!(
      FactoryRoleExecutionPackage::new(input).is_err(),
      "{reason} must be rejected"
    );
  }
}

#[test]
fn execution_package_restores_the_same_identity_and_rejects_substituted_content() {
  use octacity_server_factory::*;
  let package = FactoryRoleExecutionPackage::new(execution_package_input()).unwrap();
  let bytes = package.canonical_bytes().unwrap();
  let restored: FactoryRoleExecutionPackage = serde_json::from_slice(&bytes).unwrap();
  assert_eq!(restored.canonical_bytes().unwrap(), bytes);
  assert_eq!(restored.digest().unwrap(), package.digest().unwrap());
  let original = serde_json::to_value(&package).unwrap();
  for (pointer, replacement) in [
    ("/schema_version", serde_json::json!(2)),
    (
      "/inputs/project_rules/0/instructions",
      serde_json::json!("Changed after freezing"),
    ),
    (
      "/inputs/profile/definition/execution/cli_arguments/1",
      serde_json::json!("substituted-output"),
    ),
    (
      "/inputs/skills/testing/files/SKILL.md/content_digest",
      serde_json::to_value(FactoryDigest::from_bytes([44; 32])).unwrap(),
    ),
  ] {
    let mut changed = original.clone();
    *changed.pointer_mut(pointer).unwrap() = replacement;
    assert!(
      serde_json::from_value::<FactoryRoleExecutionPackage>(changed).is_err(),
      "{pointer}"
    );
  }
}

fn refresh_package_task(input: &mut octacity_server_factory::FactoryRoleExecutionPackageInput) {
  use octacity_server_factory::*;
  let previous = &input.task;
  let digests = previous.digests();
  let task = input
    .context
    .entries()
    .iter()
    .find_map(|entry| match entry.source() {
      FactoryContextReference::Artifact(reference) if entry.source_kind() == ContextSourceKind::Task => {
        Some(reference.clone())
      }
      _ => None,
    })
    .unwrap();
  input.task = FactoryTaskEnvelope::new(
    FactoryTaskDeclaration {
      id: previous.id(),
      node_attempt_id: previous.node_attempt_id(),
      subject: previous.subject().clone(),
      mode: previous.mode(),
    },
    FactoryTaskDefinition {
      task,
      specifications: previous.specifications().to_vec(),
      prior_findings: previous.prior_findings().to_vec(),
      permissions: previous.permissions().clone(),
      budget: previous.budget(),
      result_schema: previous.result_schema(),
      deliverables: previous.deliverables().to_vec(),
    },
    &input.context,
    input.profile.definition().toolchain.clone(),
    FactoryTaskControlDigests {
      policy: digests.policy,
      prompt: digests.prompt,
      prompt_provenance: digests.prompt_provenance,
      provenance: digests.provenance,
    },
  )
  .unwrap();
}

fn replace_package_context_text(
  input: &mut octacity_server_factory::FactoryRoleExecutionPackageInput,
  key: &str,
  text: &str,
) {
  use octacity_server_factory::*;
  let entries = input
    .context
    .entries()
    .iter()
    .map(|entry| {
      if entry.logical_identity().as_str() != key {
        return entry.clone();
      }
      let FactoryContextReference::Artifact(previous) = entry.source() else {
        panic!("fixture Artifact input")
      };
      let reference = FactoryArtifactReference::new(
        previous.artifact_id(),
        FactoryDigest::content_sha256(text.as_bytes()),
        text.len() as u64,
      )
      .unwrap();
      ContextManifestEntry::new(
        entry.source_kind(),
        entry.logical_identity().clone(),
        entry.subject().clone(),
        FactoryContextReference::Artifact(reference.clone()),
        reference.content_digest(),
        reference.encoded_size(),
        entry.inclusion_reason().clone(),
        entry.provenance_digest(),
      )
      .unwrap()
    })
    .collect();
  input.context = ContextManifest::new(
    input.context.id(),
    input.context.subject().clone(),
    input.context.construction_policy_digest(),
    entries,
  )
  .unwrap();
  refresh_package_task(input);
}

#[test]
fn execution_package_identity_covers_settings_rules_skill_tree_and_stage_task() {
  use octacity_server_factory::*;
  let input = execution_package_input();
  let original = FactoryRoleExecutionPackage::new(input.clone())
    .unwrap()
    .digest()
    .unwrap();
  for (pointer, value) in [
    (
      "/toolchain/plugin",
      serde_json::to_value(exact("another.plugin")).unwrap(),
    ),
    (
      "/toolchain/executable",
      serde_json::to_value(exact("another.runtime")).unwrap(),
    ),
    (
      "/toolchain/model",
      serde_json::to_value(exact("another.model")).unwrap(),
    ),
    ("/model_parameters/reasoning_effort", serde_json::json!("high")),
    ("/execution/environment/LANGUAGE/value", serde_json::json!("en")),
    (
      "/execution/environment/MODEL_AUTH/reference/reference",
      serde_json::json!("projects/new-model"),
    ),
    ("/execution/cli_arguments/1", serde_json::json!("text")),
    (
      "/execution/mcp_servers/tools/configuration",
      serde_json::to_value(exact("another.mcp")).unwrap(),
    ),
    ("/execution/integrations/tracker/access_version", serde_json::json!(2)),
    (
      "/instructions",
      serde_json::json!(["First profile entry", "Second profile entry"]),
    ),
    (
      "/budget",
      serde_json::to_value(BudgetLimit::new(2, 2000, 400, 200, 2048).unwrap()).unwrap(),
    ),
  ] {
    let mut changed = input.clone();
    let mut definition = serde_json::to_value(changed.profile.definition()).unwrap();
    *definition.pointer_mut(pointer).unwrap() = value;
    changed.profile = FactoryRoleProfile::publish(
      changed.profile.reference().id,
      changed.role.reference().project_id,
      &changed.role,
      serde_json::from_value(definition).unwrap(),
    )
    .unwrap();
    refresh_package_task(&mut changed);
    assert_ne!(
      FactoryRoleExecutionPackage::new(changed).unwrap().digest().unwrap(),
      original,
      "{pointer}"
    );
  }
  let mut changed = input.clone();
  let mut role_definition = changed.role.definition().clone();
  role_definition.instructions = "Changed shared responsibility".into();
  changed.role = FactoryRole::publish(
    changed.role.reference().id,
    changed.role.reference().project_id,
    role_definition,
  )
  .unwrap();
  changed.profile = FactoryRoleProfile::publish(
    changed.profile.reference().id,
    changed.role.reference().project_id,
    &changed.role,
    changed.profile.definition().clone(),
  )
  .unwrap();
  assert_ne!(
    FactoryRoleExecutionPackage::new(changed).unwrap().digest().unwrap(),
    original
  );
  let mut changed = input.clone();
  changed.project_rules[0].instructions = "Project requires tests and review.\n".into();
  replace_package_context_text(&mut changed, "project-rules", "Project requires tests and review.\n");
  assert_ne!(
    FactoryRoleExecutionPackage::new(changed).unwrap().digest().unwrap(),
    original
  );
  let mut changed = input.clone();
  replace_package_context_text(&mut changed, "requirements", "Implement a different stage task.");
  assert_ne!(
    FactoryRoleExecutionPackage::new(changed).unwrap().digest().unwrap(),
    original
  );
  let mut changed = input;
  let alias = FactoryKey::new("testing").unwrap();
  let bundle = &changed.skills[&alias];
  let mut files = bundle.files().clone();
  let example = files
    .get_mut(&FactoryRepositoryPath::new("examples/public.txt").unwrap())
    .unwrap();
  *example = FactoryArtifactReference::new(
    example.artifact_id(),
    FactoryDigest::content_sha256(b"New supporting example"),
    22,
  )
  .unwrap();
  let bundle = FactoryRoleSkillBundle::new(
    bundle.reference().identity().clone(),
    bundle.reference().version().clone(),
    files,
  )
  .unwrap();
  let mut definition = changed.profile.definition().clone();
  definition.skills.insert(alias.clone(), bundle.reference().clone());
  changed.profile = FactoryRoleProfile::publish(
    changed.profile.reference().id,
    changed.role.reference().project_id,
    &changed.role,
    definition,
  )
  .unwrap();
  changed.skills.insert(alias, bundle);
  assert_ne!(
    FactoryRoleExecutionPackage::new(changed).unwrap().digest().unwrap(),
    original
  );
}

#[test]
fn execution_skill_bundles_require_a_bounded_complete_file_tree() {
  use octacity_server_factory::*;
  let input = execution_package_input();
  let bundle = input.skills.values().next().unwrap();
  let reference = bundle.files().values().next().unwrap().clone();
  let mut cases = vec![std::collections::BTreeMap::new()];
  let mut absent_root = bundle.files().clone();
  absent_root.remove(&FactoryRepositoryPath::new("SKILL.md").unwrap());
  cases.push(absent_root);
  let mut excessive_files = bundle.files().clone();
  for i in 0..MAX_CONTEXT_MANIFEST_ENTRIES {
    excessive_files.insert(
      FactoryRepositoryPath::new(format!("extra/{i}.txt")).unwrap(),
      reference.clone(),
    );
  }
  cases.push(excessive_files);
  let huge = FactoryArtifactReference::new(
    reference.artifact_id(),
    reference.content_digest(),
    MAX_CONTEXT_ENTRY_BYTES,
  )
  .unwrap();
  let mut excessive_bytes = [(FactoryRepositoryPath::new("SKILL.md").unwrap(), huge.clone())]
    .into_iter()
    .collect::<std::collections::BTreeMap<_, _>>();
  for i in 0..4 {
    excessive_bytes.insert(FactoryRepositoryPath::new(format!("large/{i}")).unwrap(), huge.clone());
  }
  cases.push(excessive_bytes);
  let mut conflicting_tree = bundle.files().clone();
  conflicting_tree.insert(FactoryRepositoryPath::new("examples").unwrap(), reference);
  cases.push(conflicting_tree);
  for files in cases {
    assert!(
      FactoryRoleSkillBundle::new(
        bundle.reference().identity().clone(),
        bundle.reference().version().clone(),
        files
      )
      .is_err()
    );
  }
}

#[test]
fn execution_package_rejects_omitted_controlling_policy_inputs() {
  use octacity_server_domain::ArtifactId;
  use octacity_server_factory::*;
  let mut input = execution_package_input();
  let text = "Project requires independent review.\n";
  let reference = FactoryArtifactReference::new(
    ArtifactId::generate(),
    FactoryDigest::content_sha256(text.as_bytes()),
    text.len() as u64,
  )
  .unwrap();
  let mut entries = input.context.entries().to_vec();
  entries.push(
    ContextManifestEntry::new(
      ContextSourceKind::Policy,
      FactoryKey::new("review-policy").unwrap(),
      input.context.subject().clone(),
      FactoryContextReference::Artifact(reference.clone()),
      reference.content_digest(),
      reference.encoded_size(),
      FactorySafeText::new("Required Project review policy").unwrap(),
      FactoryDigest::from_bytes([9; 32]),
    )
    .unwrap(),
  );
  input.context = ContextManifest::new(
    input.context.id(),
    input.context.subject().clone(),
    input.context.construction_policy_digest(),
    entries,
  )
  .unwrap();
  refresh_package_task(&mut input);
  input.project_rules.push(FactoryRoleProjectRule {
    source: FactoryKey::new("review-policy").unwrap(),
    instructions: text.into(),
  });
  let original = FactoryRoleExecutionPackage::new(input.clone()).unwrap();
  for retained in [
    vec![],
    vec![input.project_rules[0].clone()],
    vec![input.project_rules[1].clone()],
  ] {
    let mut incomplete = input.clone();
    incomplete.project_rules = retained;
    let mut wire = serde_json::to_value(&original).unwrap();
    wire["inputs"]["project_rules"] = serde_json::to_value(&incomplete.project_rules).unwrap();
    assert!(
      FactoryRoleExecutionPackage::new(incomplete).is_err(),
      "every selected policy must remain in the controlling instruction layer"
    );
    assert!(
      serde_json::from_value::<FactoryRoleExecutionPackage>(wire).is_err(),
      "restoring a package cannot omit selected policies either"
    );
  }
}

#[test]
fn execution_package_uses_shared_instruction_and_document_bounds() {
  use octacity_server_factory::*;
  let input = execution_package_input();
  let original_rule = input
    .context
    .entries()
    .iter()
    .find(|entry| entry.source_kind() == ContextSourceKind::Policy)
    .unwrap();
  let FactoryContextReference::Artifact(rule_artifact) = original_rule.source() else {
    panic!("fixture policy Artifact")
  };
  for (count, text) in [
    (1, "control\0text".to_owned()),
    (65, "Project rule".to_owned()),
    (2, "x".repeat(MAX_FACTORY_ROLE_INSTRUCTION_BYTES / 2 + 1)),
  ] {
    let mut changed = input.clone();
    let artifact = FactoryArtifactReference::new(
      rule_artifact.artifact_id(),
      FactoryDigest::content_sha256(text.as_bytes()),
      text.len() as u64,
    )
    .unwrap();
    let mut entries = changed
      .context
      .entries()
      .iter()
      .filter(|entry| entry.source_kind() != ContextSourceKind::Policy)
      .cloned()
      .collect::<Vec<_>>();
    changed.project_rules.clear();
    for i in 0..count {
      let key = FactoryKey::new(format!("rule-{i}")).unwrap();
      changed.project_rules.push(FactoryRoleProjectRule {
        source: key.clone(),
        instructions: text.clone(),
      });
      entries.push(
        ContextManifestEntry::new(
          ContextSourceKind::Policy,
          key,
          changed.context.subject().clone(),
          FactoryContextReference::Artifact(artifact.clone()),
          artifact.content_digest(),
          artifact.encoded_size(),
          original_rule.inclusion_reason().clone(),
          original_rule.provenance_digest(),
        )
        .unwrap(),
      );
    }
    changed.context = ContextManifest::new(
      changed.context.id(),
      changed.context.subject().clone(),
      changed.context.construction_policy_digest(),
      entries,
    )
    .unwrap();
    refresh_package_task(&mut changed);
    assert!(
      FactoryRoleExecutionPackage::new(changed).is_err(),
      "rule count={count}, bytes={}",
      text.len()
    );
  }
  let mut changed = input;
  let file = changed
    .skills
    .values()
    .next()
    .unwrap()
    .files()
    .values()
    .next()
    .unwrap()
    .clone();
  let mut files = [(FactoryRepositoryPath::new("SKILL.md").unwrap(), file.clone())]
    .into_iter()
    .collect::<std::collections::BTreeMap<_, _>>();
  for i in 1..MAX_CONTEXT_MANIFEST_ENTRIES {
    files.insert(
      FactoryRepositoryPath::new(format!("files/{i}/{}", "x".repeat(900))).unwrap(),
      file.clone(),
    );
  }
  let mut definition = changed.profile.definition().clone();
  definition.skills.clear();
  changed.skills.clear();
  for i in 0..5 {
    let key = FactoryKey::new(format!("skill-{i}")).unwrap();
    let bundle = FactoryRoleSkillBundle::new(key.clone(), FactoryKey::new("v1").unwrap(), files.clone()).unwrap();
    definition.skills.insert(key.clone(), bundle.reference().clone());
    changed.skills.insert(key, bundle);
  }
  changed.profile = FactoryRoleProfile::publish(
    changed.profile.reference().id,
    changed.role.reference().project_id,
    &changed.role,
    definition,
  )
  .unwrap();
  assert!(
    FactoryRoleExecutionPackage::new(changed).is_err(),
    "complete canonical package must fit the shared document bound"
  );
}

#[test]
fn execution_package_diagnostics_do_not_expose_private_content() {
  let inputs = execution_package_input();
  let rule = inputs.project_rules[0].clone();
  let private = [
    rule.instructions.clone(),
    inputs.role.definition().instructions.entries()[0].clone(),
    inputs.profile.definition().instructions.entries()[0].clone(),
  ];
  let package = octacity_server_factory::FactoryRoleExecutionPackage::new(inputs.clone()).unwrap();
  for diagnostic in [format!("{package:?}"), format!("{inputs:?}"), format!("{rule:?}")] {
    for text in &private {
      assert!(!diagnostic.contains(text), "private instructions leaked through Debug");
    }
  }
  let restored: octacity_server_factory::FactoryRoleExecutionPackage =
    serde_json::from_slice(&package.canonical_bytes().unwrap()).unwrap();
  assert_eq!(restored.inputs().project_rules[0].instructions, rule.instructions);
}

#[path = "role_profiles/stage_selection.rs"]
mod stage_selection;

#[test]
fn profiles_retain_opaque_plugin_model_identifiers_without_a_global_model_enum() {
  let project = ProjectId::generate();
  let agent = role(project, "Shared instructions.");
  let mut wire = serde_json::to_value(profile(project, "medium")).unwrap();
  wire["model_identifier"] = serde_json::json!("Vendor/new-model:preview");
  let definition: FactoryRoleProfileDefinition =
    serde_json::from_value(wire).expect("model identifiers belong to the plugin");
  let saved = FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &agent, definition).unwrap();
  assert_eq!(
    serde_json::to_value(&saved).unwrap()["definition"]["model_identifier"],
    "Vendor/new-model:preview"
  );
  assert_eq!(
    serde_json::from_value::<FactoryRoleProfile>(serde_json::to_value(&saved).unwrap()).unwrap(),
    saved
  );
  for name in ["", " bad", "bad\nmodel"] {
    let mut invalid = serde_json::to_value(saved.definition()).unwrap();
    invalid["model_identifier"] = serde_json::json!(name);
    let definition = serde_json::from_value(invalid).unwrap();
    assert!(FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &agent, definition).is_err());
  }
}

#[test]
fn installed_runtime_catalog_allows_manual_models_without_changing_execution_authority() {
  use octacity_server_factory::*;
  let project = ProjectId::generate();
  let mut definition = profile(project, "medium");
  definition.model_identifier = Some("Vendor/future:preview".into());
  let agent = role(project, "Project rules.");
  let candidate =
    FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &agent, definition.clone()).unwrap();
  let cap = FactoryRoleProfileCapability {
    model_catalog: Some(FactoryRoleModelCatalog {
      available: true,
      manual_model: true,
      models: vec![],
    }),
    ceiling: profile(project, "medium"),
    parameters: role_parameter_schema(),
    public_environment: Default::default(),
    local_permissions: LocalPermissionCeiling::fully_enforced(definition.permissions.clone()),
  };
  // The ordinary Build binding is independently installed, not derived from model text.
  let mut cap = cap;
  cap.ceiling.build_configuration = definition.build_configuration.clone();
  assert_eq!(cap.validate(&candidate), Ok(()));
  cap.model_catalog.as_mut().unwrap().manual_model = false;
  assert_eq!(
    cap.validate(&candidate),
    Err(FactoryRoleProfileIncompatibility::Toolchain)
  );
  cap.model_catalog.as_mut().unwrap().manual_model = true;
  let mut substituted = definition;
  substituted.toolchain.plugin = exact("uninstalled.plugin");
  let substituted =
    FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &agent, substituted).unwrap();
  assert_eq!(
    cap.validate(&substituted),
    Err(FactoryRoleProfileIncompatibility::Toolchain)
  );
}

#[test]
fn legacy_profiles_use_the_effective_models_parameter_contract_without_rewriting_the_pin() {
  use octacity_server_factory::*;
  let project = ProjectId::generate();
  let definition = profile(project, "medium");
  let agent = role(project, "Project rules.");
  let candidate =
    FactoryRoleProfile::publish(FactoryRoleProfileId::generate(), project, &agent, definition.clone()).unwrap();
  let original = serde_json::to_vec(&candidate).unwrap();
  let mut capability = FactoryRoleProfileCapability {
    model_catalog: Some(FactoryRoleModelCatalog {
      available: true,
      manual_model: false,
      models: vec![FactoryRoleModelChoice {
        identifier: "open.model".into(),
        label: "Installed model".into(),
        parameters: FlowDataSchema::new(
          FactoryKey::new("parameters").unwrap(),
          FactoryKey::new("v1").unwrap(),
          FlowValueSchema::Object {
            fields: [(
              FactoryKey::new("reasoning_effort").unwrap(),
              FlowFieldSchema::required(FlowValueSchema::Enum {
                values: vec![serde_json::json!("high")],
              }),
            )]
            .into(),
          },
        )
        .unwrap(),
      }],
    }),
    ceiling: definition.clone(),
    parameters: role_parameter_schema(),
    public_environment: Default::default(),
    local_permissions: LocalPermissionCeiling::fully_enforced(definition.permissions.clone()),
  };
  assert_eq!(
    capability.validate(&candidate),
    Err(FactoryRoleProfileIncompatibility::ModelParameters)
  );
  capability.model_catalog.as_mut().unwrap().models[0].parameters = role_parameter_schema();
  assert_eq!(capability.validate(&candidate), Ok(()));
  assert_eq!(serde_json::to_vec(&candidate).unwrap(), original);
  assert!(candidate.definition().model_identifier.is_none());
}
