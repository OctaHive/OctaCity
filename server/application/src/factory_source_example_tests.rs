//! Editable input settings are qualified through ordinary authorized validation/publication.
use super::*;

// These are three existing WorkSourceInputDraft fields, not another production loader/format.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct InputSettings {
  enabled: bool,
  predicate: WorkSourcePredicate,
  reporting: WorkSourceStatusMapping,
}

async fn publish_examples(provider: &str, names: &[&str]) -> PublishWorkSources {
  let fixture = Fixture::with_writer(provider, true).await;
  let reporter = fixture.reporter.clone().unwrap();
  let policy = WorkSourceReportingPolicy {
    labels: [
      "approved",
      "rejected",
      "bug",
      "feature-request",
      "enhancement",
      "implemented",
      "ready-for-review",
      "documentation",
      "investigating",
      "documentation-done",
    ]
    .map(str::to_owned)
    .to_vec(),
    states: vec![],
  };
  let handlers = Arc::new(
    fixture
      .handlers()
      .with_selection_labels(
        fixture.capability.clone(),
        WorkSourceSelectionLabels {
          label_presence: Default::default(),
          state: key("all"),
          all_of: vec![],
          any_of: vec![],
          none_of: vec![],
        },
      )
      .unwrap()
      .with_predicate_states(fixture.capability.clone(), key("all"), vec![key("open"), key("closed")])
      .unwrap()
      .with_reporting_mapping(
        reporter.clone(),
        WorkSourceStatusMapping {
          statuses: vec![WorkSourceStatusEffects {
            status: "installed-default".into(),
            add: vec![],
            remove: vec![],
            state: None,
          }],
        },
        Some(policy),
      )
      .unwrap(),
  );
  let mut intent = fixture.intent.clone();
  intent.inputs.clear();
  for name in names {
    let path =
      std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../config/factory/work-sources/{name}.json"));
    let bytes =
      std::fs::read(&path).unwrap_or_else(|_| panic!("required editable input example {name}.json is missing"));
    let settings: InputSettings = serde_json::from_slice(&bytes).unwrap();
    assert!(!settings.enabled, "copying an example must not activate intake");
    let mut input = fixture.intent.inputs[0].clone();
    input.id = WorkSourceInputId::generate();
    input.definition.enabled = settings.enabled;
    input.definition.predicate = Some(settings.predicate);
    input.definition.reporting = Some(settings.reporting);
    input.definition.reporter = Some(reporter.binding.clone());
    input.definition.connection.as_mut().unwrap().reporter_configuration = Some(reporter.binding.configuration.clone());
    intent.inputs.push(input);
  }
  if names.len() > 1 {
    intent.group = Some(WorkSourceGroupIntent {
      id: WorkSourceGroupId::generate(),
      expected_version: None,
      members: intent
        .inputs
        .iter()
        .map(|i| WorkSourceMemberRef {
          id: i.id,
          version: WorkSourceInputVersion::INITIAL,
        })
        .collect(),
    });
  }
  let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
  let validated = queries
    .handle_query(
      &context(),
      ValidateWorkSourceConfigurationQuery { intent: intent.clone() },
    )
    .await
    .unwrap();
  assert!(validated.inputs.iter().all(|i| !i.input.definition().enabled));
  // Explicit activation is limited to this isolated in-memory contract test.
  for input in &mut intent.inputs {
    input.definition.enabled = true;
  }
  let result = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers)
    .handle_command(
      &context(),
      PublishWorkSourceConfigurationCommand {
        intent,
        idempotency_key: IdempotencyKey::new("documented-input-example").unwrap(),
        published_at: time(4),
      },
    )
    .await
    .unwrap();
  for retained in &result.publication.inputs {
    let read = fixture
      .store
      .source_input(fixture.intent.project_id, retained.input.id(), retained.input.version())
      .await
      .unwrap();
    assert_eq!(read.definition(), retained.input.definition());
  }
  result.publication
}

#[test]
fn documented_assessment_settings_publish_and_route_without_a_required_label_vocabulary() {
  run_ready(async {
    for provider in ["github", "gitlab", "jira", "custom-tracker"] {
      let publication = publish_examples(provider, &["assessment"]).await;
      let definition = publication.inputs[0].input.definition();
      let predicate = definition.predicate.as_ref().unwrap();
      assert!(
        predicate.mismatches("open", &[]).is_empty(),
        "unlabeled open tasks can enter assessment"
      );
      for (state, labels) in [
        ("closed", vec![]),
        ("open", vec!["approved".into()]),
        ("open", vec!["rejected".into()]),
      ] {
        assert!(!predicate.mismatches(state, &labels).is_empty());
      }
      let mapping = definition.reporting.as_ref().unwrap();
      assert_eq!(mapping.effects("accept-defect").unwrap().add, ["approved", "bug"]);
      assert_eq!(
        mapping.effects("accept-feature").unwrap().add,
        ["approved", "feature-request"]
      );
      let declined = mapping.effects("decline").unwrap();
      assert_eq!(declined.add, ["rejected"]);
      assert!(declined.remove.contains(&"approved".into()));
      assert!(declined.state.is_none());
      assert!(mapping.effects("unconfigured-model-prose").is_none());
    }
  });
}

#[test]
fn documented_defect_and_feature_settings_select_one_target_or_expose_ambiguity() {
  run_ready(async {
    for provider in ["github", "gitlab", "jira", "custom-tracker"] {
      let publication = publish_examples(provider, &["defect", "feature"]).await;
      let group = publication.group.unwrap().group;
      let select = |labels: &[&str]| {
        let labels = labels.iter().map(|s| (*s).into()).collect::<Vec<String>>();
        group
          .select(&labels, |input, labels| {
            input
              .definition()
              .predicate
              .as_ref()
              .unwrap()
              .mismatches("open", labels)
              .is_empty()
          })
          .finish(&group)
          .unwrap()
      };
      for (labels, index) in [
        (vec!["approved", "bug"], 0),
        (vec!["approved", "feature-request"], 1),
        (vec!["approved", "enhancement"], 1),
      ] {
        let WorkSourceSelection::Selected(input) = select(&labels) else {
          panic!("one configured implementation target must match")
        };
        assert_eq!(input.id(), publication.inputs[index].input.id());
        assert_eq!(
          input
            .definition()
            .reporting
            .as_ref()
            .unwrap()
            .effects("implementation-complete")
            .unwrap()
            .add,
          ["implemented"]
        );
      }
      assert!(matches!(select(&["bug"]), WorkSourceSelection::Filtered));
      assert!(matches!(
        select(&["approved", "bug", "rejected"]),
        WorkSourceSelection::Filtered
      ));
      let WorkSourceSelection::Ambiguous(members) = select(&["approved", "bug", "feature-request"]) else {
        panic!("overlapping classification must remain ambiguous")
      };
      assert_eq!(members.len(), 2);
    }
  });
}

#[test]
fn documented_unrelated_labels_publish_with_only_their_configured_selection_and_effects() {
  run_ready(async {
    for provider in ["github", "gitlab", "jira", "custom-tracker"] {
      let publication = publish_examples(provider, &["documentation"]).await;
      let definition = publication.inputs[0].input.definition();
      let predicate = definition.predicate.as_ref().unwrap();
      assert!(
        predicate
          .mismatches(
            "open",
            &["ready-for-review".into(), "documentation".into(), "human-label".into()]
          )
          .is_empty()
      );
      for labels in [
        vec!["approved", "bug"],
        vec!["ready-for-review"],
        vec!["ready-for-review", "documentation", "investigating"],
      ] {
        assert!(
          !predicate
            .mismatches("open", &labels.into_iter().map(str::to_owned).collect::<Vec<_>>())
            .is_empty()
        );
      }
      let effects = definition.reporting.as_ref().unwrap().effects("documented").unwrap();
      assert_eq!(effects.add, ["documentation-done"]);
      assert_eq!(effects.remove, ["ready-for-review"]);
      assert!(effects.state.is_none());
      assert!(definition.reporting.as_ref().unwrap().effects("approved").is_none());
    }
  });
}
