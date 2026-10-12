mod support;

use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::FactoryFlowConfiguration;
use octacity_server_store::{
  FactoryFlowDefinitionStore as _, FlowDefinitionListVisibility, IdempotencyKey, MutationDisposition,
  PublishFactoryFlowDefinition, PublishedFactoryFlowDefinition,
  testing::{factory_node_journal_contract_fixture, management_mutation_with_request},
};
use octacity_server_store_postgres::PostgresStore;
use support::TestDatabase;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn publishes_and_discovers_an_exact_immutable_flow_with_replay_after_restart() {
  let database = TestDatabase::migrated().await;
  let project = ProjectId::generate();
  sqlx::query("INSERT INTO projects (id, name, version, created_at, updated_at) VALUES ($1, 'flow owner', 1, to_timestamp(0), to_timestamp(0))").bind(project.as_uuid()).execute(&database.pool).await.unwrap();
  let fixture = factory_node_journal_contract_fixture();
  let flow = FactoryFlowConfiguration::new(
    fixture.admission.flow.closure().clone(),
    fixture.admission.flow.limits().clone(),
    vec![fixture.schema],
    Default::default(),
  )
  .unwrap();
  let reference = flow.closure.root();
  let publication = PublishedFactoryFlowDefinition {
    project_id: project,
    flow,
    published_at: Timestamp::from_unix_millis(7).unwrap(),
  };
  let request = PublishFactoryFlowDefinition {
    intent_digest: octacity_server_factory::FactoryDigest::from_bytes([9; 32]),
    publication: publication.clone(),
    expected_current_version: None,
    idempotency_key: IdempotencyKey::new("flow-create").unwrap(),
  };
  let store = PostgresStore::new(database.pool.clone());
  let applied = store
    .publish_factory_flow_definition(management_mutation_with_request(request.clone(), "create-flow"))
    .await
    .unwrap();
  assert_eq!(applied.disposition, MutationDisposition::Applied);
  assert_eq!(
    store
      .factory_flow_definition(reference.id(), Some(reference.version()))
      .await
      .unwrap(),
    publication
  );
  let page = store
    .list_factory_flow_definitions(project, None, 1, FlowDefinitionListVisibility::all())
    .await
    .unwrap();
  assert_eq!(page.total, 1);
  assert_eq!(page.items, [publication]);
  assert_eq!(page.next_cursor, None);
  let replay = PostgresStore::new(database.pool.clone())
    .publish_factory_flow_definition(management_mutation_with_request(request, "replayed-flow"))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.publication, applied.publication);
  let hidden = store
    .list_factory_flow_definitions(project, None, 1, FlowDefinitionListVisibility::none())
    .await
    .unwrap();
  assert_eq!(hidden.total, 0);
  assert!(hidden.items.is_empty());
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn flow_replacement_preserves_history_and_rejects_stale_versions() {
  use octacity_server_factory::{
    FlowDefinition, FlowDefinitionInput, FlowDefinitionVersion, PinnedFlowDefinitionClosure,
  };
  let database = TestDatabase::migrated().await;
  let project = ProjectId::generate();
  sqlx::query("INSERT INTO projects (id, name, version, created_at, updated_at) VALUES ($1, 'flow owner', 1, to_timestamp(0), to_timestamp(0))").bind(project.as_uuid()).execute(&database.pool).await.unwrap();
  let fixture = factory_node_journal_contract_fixture();
  let flow = FactoryFlowConfiguration::new(
    fixture.admission.flow.closure().clone(),
    fixture.admission.flow.limits().clone(),
    vec![fixture.schema],
    Default::default(),
  )
  .unwrap();
  let root = flow.closure.definition(flow.closure.root()).unwrap();
  let version_two = FlowDefinition::new(FlowDefinitionInput {
    id: root.reference().id(),
    version: FlowDefinitionVersion::new(2).unwrap(),
    input_schema: root.input_schema().cloned(),
    entry: root.entry().clone(),
    nodes: root.nodes().to_vec(),
    transitions: root.transitions().to_vec(),
    terminals: root.terminals().to_vec(),
    context_projections: root.context_projections().to_vec(),
    data_projections: root.data_projections().to_vec(),
    execution: root.execution().clone(),
  })
  .unwrap();
  let next = FactoryFlowConfiguration::new(
    PinnedFlowDefinitionClosure::new(version_two.reference(), vec![version_two]).unwrap(),
    flow.limits.clone(),
    flow.data_schemas.clone(),
    flow.pools.clone(),
  )
  .unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let original = PublishedFactoryFlowDefinition {
    project_id: project,
    flow,
    published_at: Timestamp::from_unix_millis(1).unwrap(),
  };
  store
    .publish_factory_flow_definition(management_mutation_with_request(
      PublishFactoryFlowDefinition {
        intent_digest: octacity_server_factory::FactoryDigest::from_bytes([9; 32]),
        publication: original.clone(),
        expected_current_version: None,
        idempotency_key: IdempotencyKey::new("first").unwrap(),
      },
      "first",
    ))
    .await
    .unwrap();
  let request = PublishFactoryFlowDefinition {
    intent_digest: octacity_server_factory::FactoryDigest::from_bytes([9; 32]),
    publication: PublishedFactoryFlowDefinition {
      project_id: project,
      flow: next,
      published_at: Timestamp::from_unix_millis(2).unwrap(),
    },
    expected_current_version: Some(FlowDefinitionVersion::INITIAL),
    idempotency_key: IdempotencyKey::new("next").unwrap(),
  };
  let replaced = store
    .publish_factory_flow_definition(management_mutation_with_request(request.clone(), "next"))
    .await
    .unwrap();
  assert_eq!(
    store
      .factory_flow_definition(original.flow.closure.root().id(), None)
      .await
      .unwrap(),
    replaced.publication
  );
  assert_eq!(
    store
      .factory_flow_definition(original.flow.closure.root().id(), Some(FlowDefinitionVersion::INITIAL))
      .await
      .unwrap(),
    original
  );
  let mut stale = request.clone();
  stale.idempotency_key = IdempotencyKey::new("stale").unwrap();
  assert!(matches!(
    store
      .publish_factory_flow_definition(management_mutation_with_request(stale, "stale"))
      .await,
    Err(octacity_server_store::ReplaceFactoryConfigurationError::PreconditionFailed)
  ));
  let replay = store
    .publish_factory_flow_definition(management_mutation_with_request(request, "lost-response"))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.publication, replaced.publication);
  let evidence: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_facts WHERE target_kind='flow_definition'")
    .fetch_one(&database.pool)
    .await
    .unwrap();
  assert_eq!(evidence, 2);
  database.cleanup().await;
}
