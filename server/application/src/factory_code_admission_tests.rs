//! Admission freezes only explicitly used source selections, independently of Work correlation.
use super::*;
use octacity_server_factory::{FlowDefinition, FlowDefinitionInput, PinnedFlowDefinitionClosure};
use octacity_server_store::{ConfigurationStore as _, CreateRepository, testing::InMemoryConfigurationStore};

#[test]
fn admission_freezes_two_step_sources_and_replay_does_not_resolve_unused_or_mutable_refs() {
  run_ready(async {
    let CodeFixture {
      store,
      repositories,
      request,
      ids,
      root_ref,
    } = code_fixture().await;
    let resolver = Arc::new(RecordingResolver::default());
    let handlers =
      FactoryAdmissionHandlers::new(store.clone(), resolver.clone()).with_repositories(repositories.clone());
    let applied = admit(&handlers, request.clone()).await.unwrap();
    assert_eq!(resolver.call_count(), 2, "unused declarations must never resolve");
    let snapshot = store.factory_run_snapshot(applied.factory_run_id).await.unwrap();
    let admitted = serde_json::to_value(&snapshot.admitted_flow).unwrap();
    let pins = admitted["code_pins"]
      .as_array()
      .expect("admission must retain resource pins");
    assert_eq!(pins.len(), 2);
    for (index, alias) in ["first", "second"].into_iter().enumerate() {
      let pin = pins.iter().find(|pin| pin["source"] == alias).unwrap();
      assert_eq!(pin["definition"], serde_json::to_value(root_ref).unwrap());
      assert_eq!(pin["subject"]["repository_id"], ids[index].to_string());
      assert_eq!(pin["subject"]["base_revision"], "0123456789abcdef");
    }
    assert!(snapshot.work.subject().repository().is_err());
    let restarted =
      FactoryAdmissionHandlers::new(store.clone(), Arc::new(UnavailableResolver)).with_repositories(repositories);
    let replayed = admit(&restarted, request).await.unwrap();
    assert_eq!(replayed.factory_run_id, applied.factory_run_id);
    assert_eq!(replayed.disposition, MutationDisposition::Replayed);
    assert_eq!(
      serde_json::to_value(
        store
          .factory_run_snapshot(applied.factory_run_id)
          .await
          .unwrap()
          .admitted_flow
      )
      .unwrap(),
      admitted
    );
    assert_eq!(resolver.call_count(), 2);
  });
}

struct CodeFixture {
  store: Arc<InMemoryFactoryConfigurationStore>,
  repositories: Arc<InMemoryConfigurationStore>,
  request: AdmitManualFactoryWorkCommand,
  ids: Vec<RepositoryId>,
  root_ref: octacity_server_factory::FlowDefinitionRef,
}
async fn code_fixture() -> CodeFixture {
  code_fixture_with_resources(
    octacity_server_factory::FlowCodeRevision::AdmittedBase,
    octacity_server_factory::FlowCodeScope::Repository,
  )
  .await
}
async fn code_fixture_with_resources(
  revision: octacity_server_factory::FlowCodeRevision,
  scope: octacity_server_factory::FlowCodeScope,
) -> CodeFixture {
  code_fixture_configured(revision, scope, false).await
}
async fn code_fixture_configured(
  revision: octacity_server_factory::FlowCodeRevision,
  scope: octacity_server_factory::FlowCodeScope,
  candidate_consumer: bool,
) -> CodeFixture {
  code_fixture_with_context(revision, scope, candidate_consumer, None).await
}
async fn code_fixture_with_context(
  revision: octacity_server_factory::FlowCodeRevision,
  scope: octacity_server_factory::FlowCodeScope,
  candidate_consumer: bool,
  context_binding: Option<octacity_server_factory::FlowContextBinding>,
) -> CodeFixture {
  code_fixture_for_execution(revision, scope, candidate_consumer, context_binding, false).await
}
async fn code_fixture_for_execution(
  revision: octacity_server_factory::FlowCodeRevision,
  scope: octacity_server_factory::FlowCodeScope,
  candidate_consumer: bool,
  context_binding: Option<octacity_server_factory::FlowContextBinding>,
  consumer_build: bool,
) -> CodeFixture {
  let project = ProjectId::generate();
  let id = FactoryConfigurationId::generate();
  let repositories = Arc::new(InMemoryConfigurationStore::new());
  repositories.seed_project(project).unwrap();
  let mut sources = serde_json::Map::new();
  let mut ids = vec![];
  for alias in ["first", "second", "unused"] {
    let published = repositories
      .create_repository(octacity_server_store::testing::management_mutation_with_request(
        CreateRepository {
          id: RepositoryId::generate(),
          project_id: project,
          name: alias.parse().unwrap(),
          definition: repository(
            project,
            RepositoryId::generate(),
            Some("refs/heads/main".parse().unwrap()),
          )
          .definition,
          idempotency_key: alias.parse().unwrap(),
          published_at: time(1),
        },
        format!("code-{alias}"),
      ))
      .await
      .unwrap()
      .repository;
    ids.push(published.id);
    sources.insert(
      alias.into(),
      serde_json::json!({
        "repository": {
          "project_id":project,"id":published.id,"version":published.version,
          "definition_digest":FactoryDigest::content_sha256(&serde_json::to_vec(&published.definition).unwrap())
        },
        "selection":{"kind":"default_reference"},"access":"read_only","scope":{"kind":"repository"}
      }),
    );
  }
  let original = configuration(project, id);
  let flow = original.flow();
  let root = flow.closure.definition(flow.closure.root()).unwrap();
  if candidate_consumer {
    sources.insert("first".into(), sources["second"].clone());
  }
  let nodes = root
    .nodes()
    .iter()
    .enumerate()
    .map(|(index, node)| {
      let mut wire = serde_json::to_value(node).unwrap();
      wire["code"] = if index < 2 {
        serde_json::json!({
          "kind":"source","source":(["first","second"][index]),"access":"read_only",
          "scope":scope,"revision":revision
        })
      } else {
        serde_json::json!({"kind":"none"})
      };
      if candidate_consumer && index == 0 {
        if consumer_build {
          let mut binding = crate::factory_node_test_support::fixtures::binding(project);
          binding.result_outcome = key("succeeded");
          wire["kind"] = serde_json::json!("build_command");
          wire["action"] = serde_json::Value::Null;
          wire["build"] = serde_json::to_value(binding).unwrap();
        }
        if let Some(binding) = &context_binding {
          wire["context_binding"] = serde_json::to_value(binding).unwrap();
        }
        wire["code"]["revision"] =
          serde_json::json!({"kind":"accepted_candidate","node":root.entry(),"outcome":"succeeded"});
      }
      serde_json::from_value(wire).unwrap()
    })
    .collect();
  let root = FlowDefinition::new(FlowDefinitionInput {
    id: root.reference().id(),
    version: root.reference().version(),
    input_schema: root.input_schema().cloned(),
    entry: root.entry().clone(),
    nodes,
    transitions: root.transitions().to_vec(),
    terminals: root.terminals().to_vec(),
    context_projections: root.context_projections().to_vec(),
    data_projections: root.data_projections().to_vec(),
    execution: root.execution().clone(),
  })
  .unwrap()
  .with_code_sources(serde_json::from_value(serde_json::Value::Object(sources)).unwrap())
  .unwrap();
  let root_ref = root.reference();
  let selected = octacity_server_factory::FactoryFlowConfiguration::new(
    PinnedFlowDefinitionClosure::new(root_ref, vec![root]).unwrap(),
    flow.limits.clone(),
    flow.data_schemas.clone(),
    flow.pools.clone(),
  )
  .unwrap();
  let store = Arc::new(InMemoryFactoryConfigurationStore::new());
  store.seed_project(project).unwrap();
  store
    .seed_factory_configuration_version(PublishedFactoryConfiguration {
      configuration: configuration_with_flow(project, id, Some(selected)),
      published_at: time(1),
    })
    .unwrap();
  let mut request = command(project, id, RepositoryId::generate());
  request.repository_id = None;
  request.repository_version = None;
  request.source = serde_json::from_value(serde_json::json!({"kind":"project_input_v1"})).unwrap();
  CodeFixture {
    store,
    repositories,
    request,
    ids,
    root_ref,
  }
}

#[test]
fn revoked_step_repository_blocks_fresh_admission_and_replay_without_new_vcs_access() {
  run_ready(async {
    let CodeFixture {
      store,
      repositories,
      request,
      ids,
      ..
    } = code_fixture().await;
    let resolver = Arc::new(RecordingResolver::default());
    let policy = Arc::new(HideSelectedDependency(
      ManagementResource::instance(
        crate::ManagementResourceKind::Repository,
        crate::ManagementResourceIdentity::new(ids[1].to_string()).unwrap(),
      )
      .unwrap(),
    ));
    let denied = FactoryAdmissionHandlers::with_policy(store.clone(), resolver.clone(), policy.clone())
      .with_repositories(repositories.clone());
    assert!(admit(&denied, request.clone()).await.is_err());
    assert_eq!(
      resolver.call_count(),
      0,
      "authorize the complete source set before resolving the first source"
    );
    let permitted = FactoryAdmissionHandlers::new(store.clone(), resolver.clone()).with_repositories(repositories);
    admit(&permitted, request.clone()).await.unwrap();
    assert_eq!(resolver.call_count(), 2);
    assert!(
      admit(&denied, request).await.is_err(),
      "a retained admission cannot bypass current source visibility"
    );
    assert_eq!(resolver.call_count(), 2);
  });
}

#[test]
fn all_step_source_authorization_precedes_even_legacy_work_repository_resolution() {
  run_ready(async {
    let CodeFixture {
      store,
      repositories,
      mut request,
      ids,
      ..
    } = code_fixture().await;
    let work_repository = repository(
      request.project_id,
      RepositoryId::generate(),
      Some("refs/heads/main".parse().unwrap()),
    );
    request.repository_id = Some(work_repository.id);
    request.repository_version = Some(work_repository.version);
    request.source = RepositorySourceSelection::DefaultReference.into();
    store.seed_repository_version(work_repository).unwrap();
    let resolver = Arc::new(RecordingResolver::default());
    let policy = Arc::new(HideSelectedDependency(
      ManagementResource::instance(
        crate::ManagementResourceKind::Repository,
        crate::ManagementResourceIdentity::new(ids[1].to_string()).unwrap(),
      )
      .unwrap(),
    ));
    let handlers =
      FactoryAdmissionHandlers::with_policy(store, resolver.clone(), policy).with_repositories(repositories);
    assert!(admit(&handlers, request).await.is_err());
    assert_eq!(
      resolver.call_count(),
      0,
      "deny the complete dependency set before any VCS access"
    );
  });
}

#[test]
fn restored_admitted_flow_rejects_missing_extra_and_substituted_code_pins() {
  run_ready(async {
    let CodeFixture {
      store,
      repositories,
      request,
      ids,
      ..
    } = code_fixture().await;
    let handler = FactoryAdmissionHandlers::new(store.clone(), Arc::new(RecordingResolver::default()))
      .with_repositories(repositories);
    let applied = admit(&handler, request).await.unwrap();
    let original = store
      .factory_run_snapshot(applied.factory_run_id)
      .await
      .unwrap()
      .admitted_flow;
    original.validated().unwrap();
    let wire = serde_json::to_value(&original).unwrap();
    let mut swapped = wire.clone();
    swapped["code_pins"][0]["subject"]["repository_id"] = serde_json::json!(ids[1]);
    let mut missing = wire.clone();
    missing.as_object_mut().unwrap().remove("code_pins");
    let mut extra = wire.clone();
    let duplicate = extra["code_pins"][0].clone();
    extra["code_pins"].as_array_mut().unwrap().push(duplicate);
    for changed in [swapped, missing, extra] {
      let restored: octacity_server_factory::AdmittedFlow = serde_json::from_value(changed).unwrap();
      assert!(
        restored.validated().is_err(),
        "restored exact resources must match the declared used source set"
      );
    }
  });
}

#[path = "factory_code_input_tests.rs"]
mod input_resources;
