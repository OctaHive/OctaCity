use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationVersion, ProjectId, RepositoryId, RepositoryVersion,
};

use crate::{
  BudgetLimit, BuildConfigurationRef, FactoryChoiceKind, FactoryConfiguration, FactoryConfigurationChoiceEntries,
  FactoryConfigurationChoices, FactoryConfigurationDraft, FactoryConfigurationId, FactoryConfigurationVersion,
  FactoryDigest, FactoryKey, FactoryReferenceChoice, FactoryStageKind, FactoryWipLimits, ImmutableReference,
  WorkSourceBinding, WorkSourceCapability, WorkSourceCapabilityRole, WorkSourceInput, WorkSourceInputContext,
  WorkSourceInputDraft, WorkSourceInputId, WorkSourceRepositoryRef,
};

#[path = "source_group_tests.rs"]
mod exclusive_groups;

#[test]
fn input_reporting_maps_arbitrary_statuses_only_within_the_installed_effect_ceiling() {
  let mapping: crate::WorkSourceStatusMapping = serde_json::from_value(serde_json::json!({"statuses":[
    {"status":"needs-human-eyes","add":["ready / review"],"remove":["needs context"],"state":null},
    {"status":"archive","add":[],"remove":[],"state":"native-closed"}
  ]}))
  .unwrap();
  let policy = crate::WorkSourceReportingPolicy {
    ownership: None,
    labels: vec!["ready / review".into(), "needs context".into()],
    states: vec!["native-closed".into()],
  };
  policy.validate_mapping(&mapping).unwrap();
  assert_eq!(mapping.effects("needs-human-eyes").unwrap().add, ["ready / review"]);
  assert!(mapping.effects("approved").is_none());
  for change in ["label", "state", "overlap", "duplicate", "oversized"] {
    let mut invalid = mapping.clone();
    match change {
      "label" => invalid.statuses[0].add = vec!["outside-installed-scope".into()],
      "state" => invalid.statuses[0].state = Some("outside-installed-state".into()),
      "overlap" => invalid.statuses[0].remove = vec!["ready / review".into()],
      "duplicate" => invalid.statuses.push(invalid.statuses[0].clone()),
      _ => invalid.statuses[0].add = vec!["x".repeat(257)],
    }
    assert!(policy.validate_mapping(&invalid).is_err(), "accepted {change}");
  }
}

#[test]
fn input_publication_freezes_reporting_mapping_and_requires_an_explicit_reporter() {
  let project = ProjectId::generate();
  let target = target(project, true);
  let repository = repository(project);
  let source = source(project, "custom-tracker");
  let mut reporter = source.clone();
  reporter.role = WorkSourceCapabilityRole::Reporter;
  let mut definition = draft(&target, &repository, &source);
  assert!(serde_json::to_value(&definition).unwrap().get("reporting").is_none());
  definition.reporting = Some(
    serde_json::from_value(
      serde_json::json!({"statuses":[{"status":"review","add":["arbitrary"],"remove":[],"state":null}]}),
    )
    .unwrap(),
  );
  let context = || WorkSourceInputContext {
    target: &target,
    repository: &repository,
    source: Some(&source),
    reporter: Some(&reporter),
  };
  assert!(WorkSourceInput::publish(WorkSourceInputId::generate(), project, definition.clone(), context()).is_err());
  definition.reporter = Some(reporter.binding.clone());
  let input = WorkSourceInput::publish(WorkSourceInputId::generate(), project, definition.clone(), context()).unwrap();
  assert_eq!(input.definition().reporting, definition.reporting);
  let restored: WorkSourceInput = serde_json::from_value(serde_json::to_value(&input).unwrap()).unwrap();
  assert_eq!(restored, input);
}

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}

fn digest(value: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([value; 32])
}

fn reference(value: &str) -> ImmutableReference {
  ImmutableReference::new(key(value), key("v1"), digest(1))
}

fn target(project: ProjectId, enabled: bool) -> FactoryConfiguration {
  target_version(
    project,
    enabled,
    FactoryConfigurationId::generate(),
    FactoryConfigurationVersion::INITIAL,
  )
}

fn target_version(
  project: ProjectId,
  enabled: bool,
  id: FactoryConfigurationId,
  version: FactoryConfigurationVersion,
) -> FactoryConfiguration {
  let budget = BudgetLimit::new(10, 10_000, 1_000, 10_000, 10_000).unwrap();
  let choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
    references: [FactoryChoiceKind::AdmissionPolicy, FactoryChoiceKind::PermissionCeiling]
      .into_iter()
      .map(|kind| FactoryReferenceChoice {
        kind,
        alias: key("default"),
        reference: reference("default"),
      })
      .collect(),
    build_configurations: vec![(
      key("build"),
      BuildConfigurationRef::new(
        BuildConfigurationId::generate(),
        BuildConfigurationVersion::INITIAL,
        project,
        digest(2),
      ),
    )],
  })
  .unwrap();
  let draft = crate::testing::with_contract_flow(
    FactoryConfigurationDraft {
      admission_policy: key("default"),
      flow: None,
      wip_limits: FactoryWipLimits::new(2, 4).unwrap(),
      hard_budget: budget,
      permission_ceiling: key("default"),
      enabled,
    },
    vec![crate::testing::FactoryStageDraft {
      key: key("work"),
      kind: FactoryStageKind::Implementation,
      build_configuration: key("build"),
      budget,
    }],
    &choices,
  );
  FactoryConfiguration::publish(id, version, project, digest(3), draft, &choices).unwrap()
}

fn source(project: ProjectId, provider: &str) -> WorkSourceCapability {
  WorkSourceCapability {
    project_id: project,
    role: WorkSourceCapabilityRole::Source,
    binding: WorkSourceBinding {
      installation: reference(provider),
      capability: reference("ticket-source"),
      configuration: reference("selected-input-settings"),
      resource: reference("ticket-container"),
    },
  }
}

fn repository(project: ProjectId) -> WorkSourceRepositoryRef {
  WorkSourceRepositoryRef {
    project_id: project,
    id: RepositoryId::generate(),
    version: RepositoryVersion::INITIAL,
    definition_digest: digest(4),
  }
}

fn draft(
  target: &FactoryConfiguration,
  repository: &WorkSourceRepositoryRef,
  source: &WorkSourceCapability,
) -> WorkSourceInputDraft {
  WorkSourceInputDraft {
    reporting: None,
    predicate: None,
    connection: None,
    target: target.reference().clone(),
    repository: repository.clone(),
    source: source.binding.clone(),
    reporter: None,
    enabled: false,
  }
}

#[test]
fn input_publication_pins_exact_targets_for_any_installed_ticket_provider() {
  let project = ProjectId::generate();
  let target = target(project, true);
  let repository = repository(project);
  for provider in ["github", "gitlab", "jira", "operator-custom-tracker"] {
    let source = source(project, provider);
    let definition = draft(&target, &repository, &source);
    let id = WorkSourceInputId::generate();
    let input = WorkSourceInput::publish(
      id,
      project,
      definition.clone(),
      WorkSourceInputContext {
        target: &target,
        repository: &repository,
        source: Some(&source),
        reporter: None,
      },
    )
    .unwrap();

    assert_eq!(input.id(), id);
    assert_eq!(input.project_id(), project);
    assert_eq!(input.version().get(), 1);
    assert_eq!(input.definition(), &definition);
    assert_eq!(input.root_flow(), target.flow().closure.root());
  }
}

#[test]
fn input_publication_rejects_target_substitution_and_cross_project_ownership() {
  let project = ProjectId::generate();
  let target = target(project, true);
  let repository = repository(project);
  let source = source(project, "jira");
  let definition = draft(&target, &repository, &source);
  let context = || WorkSourceInputContext {
    target: &target,
    repository: &repository,
    source: Some(&source),
    reporter: None,
  };
  let foreign_project = ProjectId::generate();
  for (path, value, field) in [
    (
      "/target/id",
      serde_json::json!(FactoryConfigurationId::generate()),
      "source input target",
    ),
    ("/target/version", serde_json::json!(2), "source input target"),
    (
      "/target/definition_digest",
      serde_json::json!(digest(8)),
      "source input target",
    ),
    (
      "/target/project_id",
      serde_json::json!(foreign_project),
      "source input target",
    ),
    (
      "/repository/id",
      serde_json::json!(RepositoryId::generate()),
      "source input repository",
    ),
    ("/repository/version", serde_json::json!(2), "source input repository"),
    (
      "/repository/definition_digest",
      serde_json::json!(digest(8)),
      "source input repository",
    ),
    (
      "/repository/project_id",
      serde_json::json!(foreign_project),
      "source input repository",
    ),
  ] {
    let mut submitted = serde_json::to_value(&definition).unwrap();
    *submitted.pointer_mut(path).unwrap() = value;
    let error = WorkSourceInput::publish(
      WorkSourceInputId::generate(),
      project,
      serde_json::from_value(submitted).unwrap(),
      context(),
    )
    .unwrap_err();
    assert_eq!(error, crate::FactoryError::InvalidConfiguration { field }, "{path}");
  }
  assert_eq!(
    WorkSourceInput::publish(
      WorkSourceInputId::generate(),
      foreign_project,
      definition.clone(),
      context()
    )
    .unwrap_err(),
    crate::FactoryError::InvalidConfiguration {
      field: "source input target"
    }
  );

  let foreign_repository = WorkSourceRepositoryRef {
    project_id: foreign_project,
    ..repository.clone()
  };
  assert_eq!(
    WorkSourceInput::publish(
      WorkSourceInputId::generate(),
      project,
      WorkSourceInputDraft {
        repository: foreign_repository.clone(),
        ..definition
      },
      WorkSourceInputContext {
        repository: &foreign_repository,
        ..context()
      },
    )
    .unwrap_err(),
    crate::FactoryError::InvalidConfiguration {
      field: "source input repository"
    }
  );
}

#[test]
fn replacement_keeps_the_stable_input_identity_and_freezes_prior_settings() {
  let project = ProjectId::generate();
  let target = target(project, true);
  let repository = repository(project);
  let source = source(project, "gitlab");
  let original = WorkSourceInput::publish(
    WorkSourceInputId::generate(),
    project,
    draft(&target, &repository, &source),
    WorkSourceInputContext {
      target: &target,
      repository: &repository,
      source: Some(&source),
      reporter: None,
    },
  )
  .unwrap();
  let retained = serde_json::to_value(&original).unwrap();
  let new_target = target_version(
    project,
    true,
    target.reference().id(),
    FactoryConfigurationVersion::new(2).unwrap(),
  );
  let mut new_source = source.clone();
  new_source.binding.configuration = ImmutableReference::new(key("revised-input-settings"), key("v2"), digest(9));
  let definition = draft(&new_target, &repository, &new_source);
  let context = || WorkSourceInputContext {
    target: &new_target,
    repository: &repository,
    source: Some(&new_source),
    reporter: None,
  };
  let replaced = original
    .replace(
      crate::WorkSourceInputVersion::new(2).unwrap(),
      definition.clone(),
      context(),
    )
    .unwrap();

  assert_eq!(replaced.id(), original.id());
  assert_eq!(replaced.project_id(), project);
  assert_eq!(replaced.version().get(), 2);
  assert_eq!(replaced.definition(), &definition);
  assert_eq!(replaced.root_flow(), new_target.flow().closure.root());
  assert_eq!(original.version().get(), 1);
  assert_eq!(serde_json::to_value(&original).unwrap(), retained);
  for version in [1, 3] {
    assert_eq!(
      original
        .replace(
          crate::WorkSourceInputVersion::new(version).unwrap(),
          definition.clone(),
          context()
        )
        .unwrap_err(),
      crate::FactoryError::InvalidConfiguration {
        field: "source input replacement version"
      }
    );
  }
}

#[test]
fn inputs_default_to_disabled_and_cannot_activate_a_disabled_target() {
  let project = ProjectId::generate();
  let target = target(project, false);
  let repository = repository(project);
  let source = source(project, "github");
  let mut submitted = serde_json::to_value(draft(&target, &repository, &source)).unwrap();
  submitted.as_object_mut().unwrap().remove("enabled");
  submitted.as_object_mut().unwrap().remove("reporter");
  let mut definition: WorkSourceInputDraft = serde_json::from_value(submitted).unwrap();
  assert!(!definition.enabled);
  assert!(definition.reporter.is_none());
  let context = || WorkSourceInputContext {
    target: &target,
    repository: &repository,
    source: Some(&source),
    reporter: None,
  };
  let disabled =
    WorkSourceInput::publish(WorkSourceInputId::generate(), project, definition.clone(), context()).unwrap();
  assert!(!disabled.definition().enabled);

  definition.enabled = true;
  assert_eq!(
    disabled
      .replace(crate::WorkSourceInputVersion::new(2).unwrap(), definition, context())
      .unwrap_err(),
    crate::FactoryError::InvalidConfiguration {
      field: "source input enabled target"
    }
  );
  assert!(!disabled.definition().enabled);
}

#[test]
fn source_selection_requires_the_exact_project_scoped_source_capability_even_when_disabled() {
  let project = ProjectId::generate();
  let target = target(project, true);
  let repository = repository(project);
  let source = source(project, "jira");
  let mut alternatives = vec![
    None,
    Some(WorkSourceCapability {
      project_id: ProjectId::generate(),
      ..source.clone()
    }),
    Some(WorkSourceCapability {
      role: WorkSourceCapabilityRole::Reporter,
      ..source.clone()
    }),
  ];
  for field in ["installation", "capability", "configuration", "resource"] {
    let mut changed = source.clone();
    let replacement = ImmutableReference::new(key("different"), key("v2"), digest(9));
    match field {
      "installation" => changed.binding.installation = replacement,
      "capability" => changed.binding.capability = replacement,
      "configuration" => changed.binding.configuration = replacement,
      "resource" => changed.binding.resource = replacement,
      _ => unreachable!(),
    }
    alternatives.push(Some(changed));
  }
  for enabled in [false, true] {
    for installed in &alternatives {
      let mut definition = draft(&target, &repository, &source);
      definition.enabled = enabled;
      assert_eq!(
        WorkSourceInput::publish(
          WorkSourceInputId::generate(),
          project,
          definition,
          WorkSourceInputContext {
            target: &target,
            repository: &repository,
            source: installed.as_ref(),
            reporter: None,
          },
        )
        .unwrap_err(),
        crate::FactoryError::InvalidConfiguration {
          field: "source input capability"
        }
      );
    }
  }
}

#[test]
fn reporting_is_optional_and_requires_an_independent_compatible_reporter_selection() {
  let project = ProjectId::generate();
  let target = target(project, true);
  let repository = repository(project);
  let source = source(project, "gitlab");
  let reporter = WorkSourceCapability {
    role: WorkSourceCapabilityRole::Reporter,
    binding: WorkSourceBinding {
      capability: reference("ticket-reporter"),
      configuration: reference("configured-status-mapping"),
      ..source.binding.clone()
    },
    ..source.clone()
  };
  let mut definition = draft(&target, &repository, &source);
  definition.enabled = true;
  let context = || WorkSourceInputContext {
    target: &target,
    repository: &repository,
    source: Some(&source),
    reporter: None,
  };
  let read_only =
    WorkSourceInput::publish(WorkSourceInputId::generate(), project, definition.clone(), context()).unwrap();
  assert!(read_only.definition().reporter.is_none());

  definition.reporter = Some(reporter.binding.clone());
  let reporting = read_only
    .replace(
      crate::WorkSourceInputVersion::new(2).unwrap(),
      definition.clone(),
      WorkSourceInputContext {
        reporter: Some(&reporter),
        ..context()
      },
    )
    .unwrap();
  assert_eq!(reporting.definition().reporter.as_ref(), Some(&reporter.binding));

  for invalid in [
    "missing",
    "role",
    "project",
    "capability",
    "configuration",
    "installation",
    "resource",
  ] {
    let mut installed = reporter.clone();
    let mut submitted = definition.clone();
    match invalid {
      "missing" => {}
      "role" => installed.role = WorkSourceCapabilityRole::Source,
      "project" => installed.project_id = ProjectId::generate(),
      "capability" => {
        installed.binding.capability = ImmutableReference::new(key("ticket-reporter"), key("v2"), digest(9))
      }
      "configuration" => installed.binding.configuration = reference("another-status-mapping"),
      "installation" => {
        installed.binding.installation = reference("another-installation");
        submitted.reporter = Some(installed.binding.clone());
      }
      "resource" => {
        installed.binding.resource = reference("another-ticket-container");
        submitted.reporter = Some(installed.binding.clone());
      }
      _ => unreachable!(),
    }
    assert_eq!(
      read_only
        .replace(
          crate::WorkSourceInputVersion::new(2).unwrap(),
          submitted,
          WorkSourceInputContext {
            reporter: (invalid != "missing").then_some(&installed),
            ..context()
          },
        )
        .unwrap_err(),
      crate::FactoryError::InvalidConfiguration {
        field: "source input reporter"
      },
      "{invalid}"
    );
  }
  assert!(read_only.definition().reporter.is_none());
}

#[test]
fn legacy_input_predicate_preserves_exact_bytes_digest_and_unrestricted_labels() {
  let canonical = r#"{"state":"open","all_of":[],"any_of":[],"none_of":[]}"#;
  let predicate: crate::WorkSourcePredicate = serde_json::from_str(canonical).unwrap();
  predicate.validate().unwrap();
  assert!(predicate.mismatches("open", &[]).is_empty());
  assert!(predicate.mismatches("open", &["human-owned".into()]).is_empty());
  let bytes = serde_json::to_vec(&predicate).unwrap();
  assert_eq!(bytes, canonical.as_bytes());
  assert_eq!(
    FactoryDigest::content_sha256(&bytes).to_string(),
    "3f3c8a781028756ccabf932d54cd4a0437d7d8c477889bc54eb9851e28c789b2"
  );
}

#[test]
fn strict_unlabeled_input_predicate_rejects_even_unrelated_observed_labels() {
  let predicate: crate::WorkSourcePredicate = serde_json::from_value(serde_json::json!({
    "state":"open","all_of":[],"any_of":[],"none_of":[],"label_presence":"none"
  }))
  .unwrap();
  predicate.validate().unwrap();
  assert!(predicate.mismatches("open", &[]).is_empty());
  assert!(!predicate.mismatches("open", &["human-owned".into()]).is_empty());
  assert!(!predicate.mismatches("closed", &[]).is_empty());
  let any: crate::WorkSourcePredicate = serde_json::from_value(serde_json::json!({
    "state":"open","all_of":[],"any_of":[],"none_of":[],"label_presence":"any"
  }))
  .unwrap();
  assert!(any.mismatches("open", &["human-owned".into()]).is_empty());
  assert_eq!(
    serde_json::to_string(&any).unwrap(),
    r#"{"state":"open","all_of":[],"any_of":[],"none_of":[]}"#,
    "the explicit default must retain the legacy canonical representation"
  );
}

#[test]
fn strict_unlabeled_input_predicate_rejects_required_labels_but_allows_bounded_exclusions() {
  for (all, any) in [(vec!["bug"], vec![]), (vec![], vec!["bug"]), (vec!["bug"], vec!["bug"])] {
    let contradictory: crate::WorkSourcePredicate = serde_json::from_value(serde_json::json!({
      "state":"open","all_of":all,"any_of":any,"none_of":[],"label_presence":"none"
    }))
    .unwrap();
    assert!(
      contradictory.validate().is_err(),
      "required labels contradict an empty label set"
    );
  }
  let excluded: crate::WorkSourcePredicate = serde_json::from_value(serde_json::json!({
    "state":"open","all_of":[],"any_of":[],"none_of":["blocked"],"label_presence":"none"
  }))
  .unwrap();
  excluded.validate().unwrap();
  assert!(excluded.mismatches("open", &[]).is_empty());
  assert_eq!(
    excluded.mismatches("open", &["blocked".into()]),
    [
      crate::WorkSourcePredicateMismatch::LabelsPresent,
      crate::WorkSourcePredicateMismatch::ExcludedLabelPresent
    ]
  );
}

#[test]
fn input_predicate_is_immutable_bounded_and_uses_literal_normalized_labels() {
  let predicate: crate::WorkSourcePredicate = serde_json::from_value(serde_json::json!({
    "state":"open","all_of":["needs-human-review"],"any_of":["docs","maintenance"],"none_of":["blocked"]
  }))
  .unwrap();
  predicate.validate().unwrap();
  assert!(
    predicate
      .mismatches("open", &["docs".into(), "needs-human-review".into()])
      .is_empty()
  );
  assert!(!predicate.mismatches("open", &["docs".into()]).is_empty());
  assert_eq!(
    predicate.mismatches("open", &["needs-human-review".into()]),
    [crate::WorkSourcePredicateMismatch::AlternativeLabelMissing]
  );
  assert_eq!(
    predicate.mismatches("open", &["needs-human-review".into(), "docs".into(), "blocked".into()]),
    [crate::WorkSourcePredicateMismatch::ExcludedLabelPresent]
  );
  assert!(
    !predicate
      .mismatches("closed", &["docs".into(), "needs-human-review".into()])
      .is_empty()
  );
  let unrestricted: crate::WorkSourcePredicate =
    serde_json::from_value(serde_json::json!({"state":null,"all_of":[],"any_of":[],"none_of":[]})).unwrap();
  assert!(unrestricted.mismatches("provider-specific-state", &[]).is_empty());
  let project = ProjectId::generate();
  let target = target(project, true);
  let repository = repository(project);
  let source = source(project, "custom-tracker");
  let mut definition = draft(&target, &repository, &source);
  definition.predicate = Some(predicate.clone());
  let input = WorkSourceInput::publish(
    WorkSourceInputId::generate(),
    project,
    definition,
    WorkSourceInputContext {
      target: &target,
      repository: &repository,
      source: Some(&source),
      reporter: None,
    },
  )
  .unwrap();
  let retained: WorkSourceInput = serde_json::from_value(serde_json::to_value(&input).unwrap()).unwrap();
  assert_eq!(retained.definition().predicate.as_ref(), Some(&predicate));
  for (all, any, none) in [
    (vec!["x"], vec![], vec!["x"]),
    (vec!["x", "x"], vec![], vec![]),
    (vec![], vec!["x"], vec!["x"]),
  ] {
    let invalid: crate::WorkSourcePredicate =
      serde_json::from_value(serde_json::json!({"state":null,"all_of":all,"any_of":any,"none_of":none})).unwrap();
    assert!(invalid.validate().is_err());
  }
}
#[test]
fn installed_reporting_ownership_resolves_native_groups_without_changing_legacy_policy_bytes() {
  let policy: crate::WorkSourceReportingPolicy = serde_json::from_value(serde_json::json!({
    "labels":["component:server","priority:high","size:small"],"states":["open","closed"],
    "ownership":{"labels":{
      "component:server":["native-triage-group"],"priority:high":["native-triage-group"],"size:small":["native-size-group"]
    },"state":["native-state"]}
  })).unwrap();
  policy.validate().unwrap();
  let effect = |label: &str| crate::WorkSourceStatusEffects {
    status: "reviewed".into(),
    add: vec![label.into()],
    remove: vec![],
    state: None,
  };
  let component = policy.ownership_for(&effect("component:server")).unwrap().unwrap();
  let priority = policy.ownership_for(&effect("priority:high")).unwrap().unwrap();
  let size = policy.ownership_for(&effect("size:small")).unwrap().unwrap();
  assert_eq!(
    component, priority,
    "different labels can own the same installed native field"
  );
  assert!(component.is_disjoint(&size));
  let legacy = serde_json::json!({"labels":["component:server"],"states":[]});
  let legacy_policy: crate::WorkSourceReportingPolicy = serde_json::from_value(legacy.clone()).unwrap();
  assert_eq!(serde_json::to_value(&legacy_policy).unwrap(), legacy);
  assert!(
    legacy_policy
      .ownership_for(&effect("component:server"))
      .unwrap()
      .is_none(),
    "missing metadata is unknown, never inferred singleton ownership"
  );
}

#[test]
fn installed_reporting_ownership_requires_complete_bounded_native_scopes() {
  let valid = serde_json::json!({"labels":["ready"],"states":["open"],
    "ownership":{"labels":{"ready":["labels-group"]},"state":["state"]}});
  for change in [
    "missing_label",
    "foreign_label",
    "empty_label",
    "empty_state",
    "foreign_state",
    "too_many_scopes",
  ] {
    let mut value = valid.clone();
    match change {
      "missing_label" => {
        value["ownership"]["labels"] = serde_json::json!({});
      }
      "foreign_label" => {
        value["ownership"]["labels"]["foreign"] = serde_json::json!(["foreign"]);
      }
      "empty_label" => {
        value["ownership"]["labels"]["ready"] = serde_json::json!([]);
      }
      "empty_state" => {
        value["ownership"]["state"] = serde_json::json!([]);
      }
      "foreign_state" => {
        value["states"] = serde_json::json!([]);
      }
      "too_many_scopes" => {
        value["ownership"]["labels"]["ready"] =
          serde_json::json!((0..129).map(|i| format!("scope-{i}")).collect::<Vec<_>>());
      }
      _ => unreachable!(),
    }
    let policy: crate::WorkSourceReportingPolicy = serde_json::from_value(value).unwrap();
    assert!(
      policy.validate().is_err(),
      "{change} must not assert independent native ownership"
    );
  }
}
