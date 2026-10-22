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
fn installed_profile_validation_rejects_unsupported_execution_and_authority_without_silent_narrowing() {
  use octacity_server_factory::*;
  let project = ProjectId::generate();
  let definition = profile(project, "medium");
  let empty_environment = Default::default();
  let capability = FactoryRoleProfileCapability {
    ceiling: definition.clone(),
    parameters: role_parameter_schema(),
    public_environment: empty_environment,
    local_permissions: LocalPermissionCeiling::fully_enforced(definition.permissions.clone()),
  };
  let role = role(project, "Project rules");
  let mut cases = vec![];
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
