use octacity_server_domain::{
  ArtifactId, BuildConfigurationId, BuildConfigurationVersion, ImmutableRevision, ProjectId, RepositoryId,
};

use crate::{
  BudgetLimit, BuildConfigurationRef, DecisionSignalChoices, DecisionSignalFallback, DecisionSignalMode,
  DecisionSignalProfileDraft, DecisionSignalPurpose, DecisionSignalRouteSet, DeliveryPolicyDraft,
  EvaluationPolicyDraft, ExactSubject, ExternalWorkIdentity, FactoryChoiceKind, FactoryConfiguration,
  FactoryConfigurationChoiceEntries, FactoryConfigurationChoices, FactoryConfigurationDraft, FactoryConfigurationId,
  FactoryConfigurationVersion, FactoryDigest, FactoryError, FactoryKey, FactoryMetadata, FactoryReferenceChoice,
  FactoryRun, FactoryRunId, FactoryStageDraft, FactoryStageKind, FactoryWipLimits, ImmutableReference,
  ReworkPolicyDraft, RiskClass, WorkArtifacts, WorkClassification, WorkEnvelope, WorkEnvelopeId, WorkPriority,
};

struct Fixture {
  project_id: ProjectId,
  choices: FactoryConfigurationChoices,
  draft: FactoryConfigurationDraft,
}

fn budget(attempts: u32, elapsed: u64, tokens: u64, cost: u64, output: u64) -> BudgetLimit {
  match BudgetLimit::new(attempts, elapsed, tokens, cost, output) {
    Ok(value) => value,
    Err(_) => panic!("test budget must be valid"),
  }
}

fn hard_budget() -> BudgetLimit {
  budget(20, 20_000, 2_000, 20_000, 20_000)
}

fn nested_budget() -> BudgetLimit {
  budget(4, 4_000, 400, 4_000, 4_000)
}

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).expect("fixture key is valid")
}

fn digest(value: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([value; 32])
}

fn exact(identity: &str, version: &str, digest_byte: u8) -> ImmutableReference {
  ImmutableReference::new(key(identity), key(version), digest(digest_byte))
}

fn stage(stage_key: &str, kind: FactoryStageKind, build_alias: &str) -> FactoryStageDraft {
  FactoryStageDraft {
    key: key(stage_key),
    kind,
    build_configuration: key(build_alias),
    budget: nested_budget(),
  }
}

fn choice_entries(
  project_id: ProjectId,
  signal_provider: &str,
  signal_model_version: &str,
) -> FactoryConfigurationChoiceEntries {
  FactoryConfigurationChoiceEntries {
    references: vec![
      reference_choice(
        FactoryChoiceKind::AdmissionPolicy,
        "admission-default",
        exact("manual.medium-risk", "v2", 12),
      ),
      reference_choice(
        FactoryChoiceKind::PermissionCeiling,
        "permissions-default",
        exact("permissions.restricted", "v3", 2),
      ),
      reference_choice(
        FactoryChoiceKind::DecisionSignalProvider,
        "signal-provider-default",
        exact(signal_provider, "v1", 3),
      ),
      reference_choice(
        FactoryChoiceKind::DecisionSignalAdapter,
        "signal-adapter-default",
        exact("signal.protocol", "v1", 13),
      ),
      reference_choice(
        FactoryChoiceKind::DecisionSignalModel,
        "signal-model-default",
        exact("routing-model", signal_model_version, 4),
      ),
      reference_choice(
        FactoryChoiceKind::DecisionSignalQuestionSet,
        "questions-default",
        exact("routing.questions", "v2", 5),
      ),
      reference_choice(
        FactoryChoiceKind::DecisionSignalPolicy,
        "signal-policy-default",
        exact("routing.policy", "v4", 6),
      ),
      reference_choice(
        FactoryChoiceKind::CriterionPack,
        "criteria-default",
        exact("quality.standard", "v7", 7),
      ),
      reference_choice(
        FactoryChoiceKind::Evaluator,
        "evaluator-default",
        exact("codex.review", "v5", 8),
      ),
      reference_choice(
        FactoryChoiceKind::DeliveryAdapter,
        "delivery-adapter-default",
        exact("github.review", "v2", 9),
      ),
      reference_choice(
        FactoryChoiceKind::DeliveryPolicy,
        "delivery-policy-default",
        exact("human-review", "v1", 10),
      ),
    ],
    build_configurations: vec![(
      key("build-default"),
      BuildConfigurationRef::new(
        BuildConfigurationId::generate(),
        BuildConfigurationVersion::INITIAL,
        project_id,
        digest(1),
      ),
    )],
  }
}

fn reference_choice(kind: FactoryChoiceKind, alias: &str, reference: ImmutableReference) -> FactoryReferenceChoice {
  FactoryReferenceChoice {
    kind,
    alias: key(alias),
    reference,
  }
}

fn valid_fixture() -> Fixture {
  let project_id = ProjectId::generate();
  let choices = FactoryConfigurationChoices::try_new(choice_entries(project_id, "jev", "2026-10"))
    .expect("fixture choices are valid");
  let draft = FactoryConfigurationDraft {
    admission_policy: key("admission-default"),
    stages: vec![
      stage("implement", FactoryStageKind::Implementation, "build-default"),
      stage("validate", FactoryStageKind::Validation, "build-default"),
      stage("evaluate", FactoryStageKind::Evaluation, "build-default"),
      stage("rework", FactoryStageKind::Rework, "build-default"),
    ],
    wip_limits: FactoryWipLimits::new(8, 16).expect("fixture WIP limits are valid"),
    hard_budget: hard_budget(),
    permission_ceiling: key("permissions-default"),
    decision_signals: vec![DecisionSignalProfileDraft {
      purpose: DecisionSignalPurpose::Routing,
      provider: key("signal-provider-default"),
      adapter: key("signal-adapter-default"),
      model: key("signal-model-default"),
      question_set: key("questions-default"),
      policy: key("signal-policy-default"),
      mode: DecisionSignalMode::Shadow,
      fallback: DecisionSignalFallback::Escalate,
      budget: nested_budget(),
      routes: Some(DecisionSignalRouteSet::new(
        key("implementation-complete"),
        DecisionSignalChoices::try_new(vec![key("validate"), key("escalate")]).expect("routes"),
      )),
    }],
    evaluation: EvaluationPolicyDraft {
      criterion_packs: vec![key("criteria-default")],
      evaluators: vec![key("evaluator-default")],
      required_quorum: 1,
      budget: nested_budget(),
    },
    rework: ReworkPolicyDraft {
      max_cycles: 2,
      stage: Some(key("rework")),
    },
    delivery: DeliveryPolicyDraft {
      adapter: key("delivery-adapter-default"),
      policy: key("delivery-policy-default"),
    },
    enabled: true,
  };
  Fixture {
    project_id,
    choices,
    draft,
  }
}

fn publish(fixture: &Fixture) -> FactoryConfiguration {
  FactoryConfiguration::publish(
    FactoryConfigurationId::generate(),
    FactoryConfigurationVersion::INITIAL,
    fixture.project_id,
    digest(11),
    fixture.draft.clone(),
    &fixture.choices,
  )
  .expect("fixture configuration is valid")
}

#[test]
fn publication_resolves_every_alias_to_an_exact_identity() {
  let fixture = valid_fixture();
  let configuration = publish(&fixture);

  assert_eq!(configuration.stages().len(), 4);
  assert_eq!(
    configuration.admission_policy().identity().as_str(),
    "manual.medium-risk"
  );
  assert_eq!(
    configuration.stages()[0].build_configuration().version(),
    BuildConfigurationVersion::INITIAL
  );
  assert_eq!(
    configuration.permission_ceiling().identity().as_str(),
    "permissions.restricted"
  );
  let signal = configuration
    .decision_signal(DecisionSignalPurpose::Routing)
    .expect("routing profile is configured");
  assert_eq!(signal.provider().identity().as_str(), "jev");
  assert_eq!(signal.model().version().as_str(), "2026-10");
  assert_eq!(signal.question_set().digest(), digest(5));
  assert_eq!(signal.policy().digest(), digest(6));
  assert_eq!(configuration.evaluation().criterion_packs()[0].digest(), digest(7));
  assert_eq!(configuration.evaluation().evaluators()[0].digest(), digest(8));
  assert_eq!(configuration.delivery().adapter().digest(), digest(9));
  assert_eq!(configuration.delivery().policy().digest(), digest(10));
  assert!(configuration.is_enabled());
}

#[test]
fn choice_catalogs_reject_duplicates_and_publication_rejects_unknown_aliases() {
  let duplicate = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
    references: vec![
      reference_choice(FactoryChoiceKind::PermissionCeiling, "default", exact("one", "v1", 1)),
      reference_choice(FactoryChoiceKind::PermissionCeiling, "default", exact("two", "v1", 2)),
    ],
    ..FactoryConfigurationChoiceEntries::default()
  });
  assert_eq!(
    duplicate,
    Err(FactoryError::DuplicateChoiceAlias {
      kind: FactoryChoiceKind::PermissionCeiling,
    })
  );

  let mut fixture = valid_fixture();
  fixture.draft.permission_ceiling = key("missing");
  assert_eq!(
    FactoryConfiguration::publish(
      FactoryConfigurationId::generate(),
      FactoryConfigurationVersion::INITIAL,
      fixture.project_id,
      digest(11),
      fixture.draft,
      &fixture.choices,
    ),
    Err(FactoryError::UnknownChoiceAlias {
      kind: FactoryChoiceKind::PermissionCeiling,
    })
  );
}

#[test]
fn stage_validation_enforces_project_required_kinds_unique_keys_and_nested_budgets() {
  let mut fixture = valid_fixture();
  fixture.draft.stages.pop();
  fixture.draft.stages.pop();
  assert_eq!(
    FactoryConfiguration::publish(
      FactoryConfigurationId::generate(),
      FactoryConfigurationVersion::INITIAL,
      fixture.project_id,
      digest(11),
      fixture.draft,
      &fixture.choices,
    ),
    Err(FactoryError::InvalidConfiguration {
      field: "required stages",
    })
  );

  let mut fixture = valid_fixture();
  fixture.draft.stages[1].key = fixture.draft.stages[0].key.clone();
  assert_eq!(
    FactoryConfiguration::publish(
      FactoryConfigurationId::generate(),
      FactoryConfigurationVersion::INITIAL,
      fixture.project_id,
      digest(11),
      fixture.draft,
      &fixture.choices,
    ),
    Err(FactoryError::InvalidConfiguration {
      field: "duplicate stage key",
    })
  );

  let mut fixture = valid_fixture();
  fixture.draft.stages[1].kind = FactoryStageKind::Implementation;
  assert_eq!(
    FactoryConfiguration::publish(
      FactoryConfigurationId::generate(),
      FactoryConfigurationVersion::INITIAL,
      fixture.project_id,
      digest(11),
      fixture.draft,
      &fixture.choices,
    ),
    Err(FactoryError::InvalidConfiguration {
      field: "duplicate stage kind",
    })
  );

  let mut fixture = valid_fixture();
  fixture.draft.stages[0].budget = budget(21, 1, 1, 1, 1);
  assert_eq!(
    FactoryConfiguration::publish(
      FactoryConfigurationId::generate(),
      FactoryConfigurationVersion::INITIAL,
      fixture.project_id,
      digest(11),
      fixture.draft,
      &fixture.choices,
    ),
    Err(FactoryError::InvalidConfiguration { field: "stage budget" })
  );

  let mut fixture = valid_fixture();
  let foreign_project = ProjectId::generate();
  fixture.choices = FactoryConfigurationChoices::try_new(choice_entries(foreign_project, "jev", "2026-10"))
    .expect("foreign fixture choices are structurally valid");
  assert_eq!(
    FactoryConfiguration::publish(
      FactoryConfigurationId::generate(),
      FactoryConfigurationVersion::INITIAL,
      fixture.project_id,
      digest(11),
      fixture.draft,
      &fixture.choices,
    ),
    Err(FactoryError::InvalidConfiguration {
      field: "stage Build Configuration Project",
    })
  );
}

#[test]
fn signal_evaluation_and_rework_policies_reject_ambiguous_or_unsafe_values() {
  let mut fixture = valid_fixture();
  fixture
    .draft
    .decision_signals
    .push(fixture.draft.decision_signals[0].clone());
  assert_eq!(
    FactoryConfiguration::publish(
      FactoryConfigurationId::generate(),
      FactoryConfigurationVersion::INITIAL,
      fixture.project_id,
      digest(11),
      fixture.draft,
      &fixture.choices,
    ),
    Err(FactoryError::InvalidConfiguration {
      field: "duplicate Decision Signal purpose",
    })
  );

  let mut fixture = valid_fixture();
  fixture.draft.evaluation.required_quorum = 2;
  assert_eq!(
    FactoryConfiguration::publish(
      FactoryConfigurationId::generate(),
      FactoryConfigurationVersion::INITIAL,
      fixture.project_id,
      digest(11),
      fixture.draft,
      &fixture.choices,
    ),
    Err(FactoryError::InvalidConfiguration {
      field: "evaluator quorum",
    })
  );

  let mut fixture = valid_fixture();
  fixture.draft.rework.stage = Some(key("implement"));
  assert_eq!(
    FactoryConfiguration::publish(
      FactoryConfigurationId::generate(),
      FactoryConfigurationVersion::INITIAL,
      fixture.project_id,
      digest(11),
      fixture.draft,
      &fixture.choices,
    ),
    Err(FactoryError::InvalidConfiguration { field: "rework stage" })
  );
}

#[test]
fn wip_and_replacement_versions_are_strictly_bounded() {
  assert_eq!(
    FactoryWipLimits::new(0, 1),
    Err(FactoryError::InvalidConfiguration {
      field: "max_active_runs",
    })
  );
  let fixture = valid_fixture();
  let configuration = publish(&fixture);
  assert_eq!(
    configuration.replace(
      FactoryConfigurationVersion::new(3).expect("version is valid"),
      digest(12),
      fixture.draft,
      &fixture.choices,
    ),
    Err(FactoryError::InvalidConfiguration {
      field: "replacement version",
    })
  );
}

#[test]
fn replacement_does_not_mutate_the_configuration_bound_to_an_admitted_run() {
  let fixture = valid_fixture();
  let original = publish(&fixture);
  let work = WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    original.reference().clone(),
    ExternalWorkIdentity::new("tracker:work-42").expect("external identity is valid"),
    ExactSubject::new(
      fixture.project_id,
      RepositoryId::generate(),
      ImmutableRevision::new("0123456789abcdef").expect("revision is valid"),
    ),
    WorkArtifacts::new(ArtifactId::generate(), ArtifactId::generate(), vec![]).expect("work artifacts are distinct"),
    WorkClassification::new(
      WorkPriority::new(10).expect("priority is valid"),
      RiskClass::Medium,
      FactoryMetadata::default(),
    ),
  )
  .expect("work fixture is valid");
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);

  let mut replacement_draft = fixture.draft.clone();
  replacement_draft.enabled = false;
  let replacement_choices =
    FactoryConfigurationChoices::try_new(choice_entries(fixture.project_id, "decisions-api", "2027-01"))
      .expect("replacement choices are valid");
  let replacement = original
    .replace(
      FactoryConfigurationVersion::new(2).expect("version is valid"),
      digest(12),
      replacement_draft,
      &replacement_choices,
    )
    .expect("replacement configuration is valid");

  assert_eq!(original.reference().version(), FactoryConfigurationVersion::INITIAL);
  assert_eq!(original.reference().definition_digest(), digest(11));
  assert!(original.is_enabled());
  assert_eq!(run.configuration(), original.reference());
  assert_eq!(replacement.reference().version().get(), 2);
  assert_eq!(replacement.reference().definition_digest(), digest(12));
  assert_eq!(
    original
      .decision_signal(DecisionSignalPurpose::Routing)
      .expect("original routing profile exists")
      .provider()
      .identity()
      .as_str(),
    "jev"
  );
  assert_eq!(
    replacement
      .decision_signal(DecisionSignalPurpose::Routing)
      .expect("replacement routing profile exists")
      .provider()
      .identity()
      .as_str(),
    "decisions-api"
  );
  assert!(!replacement.is_enabled());
}
