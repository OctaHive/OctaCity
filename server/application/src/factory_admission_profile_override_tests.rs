use super::*;
use octacity_server_factory::*;
use octacity_server_store::testing;
use serde_json::json;

fn fixture() -> (
  Arc<InMemoryFactoryConfigurationStore>,
  FactoryConfiguration,
  AdmitManualFactoryWorkCommand,
) {
  let (admission, _, _) = testing::factory_profile_selection_contract_fixture(Some("advanced"));
  let project = admission.work.subject().project_id();
  let capabilities = testing::factory_profile_selection_capabilities(&admission.flow);
  let mut entries = capabilities.entries();
  entries.references.extend([
    FactoryReferenceChoice {
      kind: FactoryChoiceKind::AdmissionPolicy,
      alias: key("admission"),
      reference: exact("admission", 1),
    },
    FactoryReferenceChoice {
      kind: FactoryChoiceKind::PermissionCeiling,
      alias: key("permissions"),
      reference: exact("permissions", 2),
    },
  ]);
  let mut choices = FactoryConfigurationChoices::try_new(entries).unwrap();
  for (alias, capability) in capabilities.role_profile_capabilities() {
    choices = choices
      .with_role_profile_capability(alias.clone(), capability.clone())
      .unwrap();
  }
  let configuration = FactoryConfiguration::publish(
    admission.work.configuration().id(),
    FactoryConfigurationVersion::INITIAL,
    project,
    digest(20),
    FactoryConfigurationDraft {
      flow: Some(
        FactoryFlowConfiguration::new(
          admission.flow.closure().clone(),
          admission.flow.limits().clone(),
          admission.flow.data_schemas().to_vec(),
          Default::default(),
        )
        .unwrap(),
      ),
      admission_policy: key("admission"),
      permission_ceiling: key("permissions"),
      hard_budget: admission.flow.limits().budget(),
      wip_limits: FactoryWipLimits::new(2, 4).unwrap(),
      enabled: true,
    },
    &choices,
  )
  .unwrap();
  let repository_id = RepositoryId::generate();
  let command = command(project, configuration.reference().id(), repository_id);
  let store = Arc::new(InMemoryFactoryConfigurationStore::new());
  store.seed_project(project).unwrap();
  store
    .seed_factory_configuration_version(PublishedFactoryConfiguration {
      configuration: configuration.clone(),
      published_at: time(1),
    })
    .unwrap();
  store
    .seed_repository_version(repository(
      project,
      repository_id,
      Some(SourceReference::new("refs/heads/main").unwrap()),
    ))
    .unwrap();
  (store, configuration, command)
}

fn override_value(configuration: &FactoryConfiguration, candidate: &str) -> serde_json::Value {
  let closure = &configuration.flow().closure;
  let node = &closure.definition(closure.root()).unwrap().nodes()[0];
  json!([{"stage":{"root":closure.root(),"calls":[],"node":node.key()},
    "profile":node.profile_selection().unwrap().candidates[&key(candidate)].profile.reference()}])
}

#[test]
fn admission_profile_override_is_retained_and_replayed_as_one_original_intent() {
  run_ready(async {
    let (store, configuration, command) = fixture();
    let mut wire = serde_json::to_value(command).unwrap();
    wire["profile_overrides"] = override_value(&configuration, "specialized");
    let command: AdmitManualFactoryWorkCommand =
      serde_json::from_value(wire).expect("manual admission accepts stage choices");
    let resolver = Arc::new(RecordingResolver::default());
    let handlers = FactoryAdmissionHandlers::new(store.clone(), resolver.clone());
    let accepted = admit(&handlers, command.clone()).await.unwrap();
    let snapshot = store.factory_run_snapshot(accepted.factory_run_id).await.unwrap();
    assert_eq!(
      serde_json::to_value(snapshot.work.profile_overrides()).unwrap(),
      override_value(&configuration, "specialized")
    );
    assert_eq!(snapshot.admitted_flow.closure(), &configuration.flow().closure);
    let replay = admit(
      &handlers,
      AdmitManualFactoryWorkCommand {
        admitted_at: time(200),
        ..command.clone()
      },
    )
    .await
    .unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.factory_run_id, accepted.factory_run_id);
    assert_eq!(resolver.call_count(), 1);
    let mut changed = serde_json::to_value(command).unwrap();
    changed["profile_overrides"] = override_value(&configuration, "baseline");
    assert!(
      admit(&handlers, serde_json::from_value(changed).unwrap())
        .await
        .is_err(),
      "changing only a profile conflicts with replay"
    );
    assert_eq!(resolver.call_count(), 1);
    assert_eq!(
      store.factory_run_snapshot(accepted.factory_run_id).await.unwrap(),
      snapshot
    );
  });
}

#[test]
fn admission_profile_override_rejects_an_undeclared_stage_before_resolution_or_publication() {
  run_ready(async {
    let (store, configuration, command) = fixture();
    let mut wire = serde_json::to_value(command).unwrap();
    wire["profile_overrides"] = override_value(&configuration, "specialized");
    wire["profile_overrides"][0]["stage"]["node"] = json!("undeclared");
    let before = store.mutation_evidence_counts().await;
    let resolver = Arc::new(RecordingResolver::default());
    let handlers = FactoryAdmissionHandlers::new(store.clone(), resolver.clone());
    let error = admit(&handlers, serde_json::from_value(wire).unwrap())
      .await
      .unwrap_err();
    assert_eq!(error.classification(), ApplicationFailure::Invalid);
    assert_eq!(resolver.call_count(), 0);
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}
