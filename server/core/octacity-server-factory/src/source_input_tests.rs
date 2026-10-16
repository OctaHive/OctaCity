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
