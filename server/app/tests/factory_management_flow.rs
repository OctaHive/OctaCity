//! The management authoring contract over real immutable PostgreSQL authority.
use async_trait::async_trait;
use octacity_server_application::*;
use octacity_server_factory::*;
use octacity_server_store::{FactoryConfigurationAvailability, StoreError};
use octacity_server_store_postgres::{FactoryCapabilitySource, PostgresStore};
use std::sync::{
  Arc,
  atomic::{AtomicBool, Ordering},
};
#[allow(dead_code)]
#[path = "../../infrastructure/octacity-server-store-postgres/tests/support/mod.rs"]
mod support;

struct Catalog {
  available: AtomicBool,
}
#[async_trait]
impl FactoryCapabilitySource for Catalog {
  async fn availability(&self, _project: ProjectId) -> Result<FactoryConfigurationAvailability, StoreError> {
    Ok(if self.available.load(Ordering::SeqCst) {
      FactoryConfigurationAvailability::Available(FactoryConfigurationChoices::try_new(Default::default()).unwrap())
    } else {
      FactoryConfigurationAvailability::Disabled
    })
  }
}
fn key(value: &str) -> FactoryKey {
  value.parse().unwrap()
}
fn draft(subflow: Option<FlowDefinitionRef>) -> FactoryFlowDefinitionDraft {
  let schema = FlowDataSchema::new(key("operator.contract"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let budget = BudgetLimit::new(4, 1000, 0, 0, 0).unwrap();
  let node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("operator.selected-step"),
    kind: if subflow.is_some() {
      FlowNodeKind::SubflowCall
    } else {
      FlowNodeKind::DeterministicGate
    },
    input_schema: None,
    outcomes: vec![FlowOutcomeDefinition::new(
      key("done"),
      FlowOutcomeKind::Success,
      schema.reference().clone(),
    )],
    budget,
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow,
  })
  .unwrap();
  FactoryFlowDefinitionDraft {
    root: FactoryFlowRootDraft {
      code_sources: None,
      process: None,
      input_schema: None,
      entry: key("operator.selected-step"),
      nodes: vec![node],
      transitions: vec![FlowTransition::new(
        key("operator.selected-step"),
        key("done"),
        FlowTransitionTarget::Terminal(key("done")),
      )],
      terminals: vec![FlowTerminalDefinition::new(key("done"), schema.reference().clone())],
      context_projections: vec![],
      data_projections: vec![],
      execution: FlowExecutionPolicy::new(budget, FactoryPermissionSet::deny_all(), 1).unwrap(),
    },
    limits: FlowAdmissionLimits::new(3, 10, 1, 1, 1, budget, FactoryPermissionSet::deny_all()).unwrap(),
    data_schemas: vec![schema],
    pools: Default::default(),
  }
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn management_pins_an_arbitrary_nested_graph_and_replays_before_catalogue_resolution() {
  let database = support::TestDatabase::migrated().await;
  let project = ProjectId::generate();
  sqlx::query("INSERT INTO projects (id,name,version,created_at,updated_at) VALUES ($1,'flow authoring',1,to_timestamp(0),to_timestamp(0))").bind(project.as_uuid()).execute(&database.pool).await.unwrap();
  let catalog = Arc::new(Catalog {
    available: AtomicBool::new(true),
  });
  let store = Arc::new(PostgresStore::with_factory_capabilities(
    database.pool.clone(),
    catalog.clone(),
  ));
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let handlers = Arc::new(FactoryFlowDefinitionHandlers::new(store.clone(), policy.clone()));
  let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
  let queries = AuthorizedQueryHandler::new(policy, handlers);
  let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
  let mut invalid_draft = draft(None);
  invalid_draft.root.entry = key("missing-entry");
  let invalid = queries
    .handle_query(
      &context,
      ValidateFactoryFlowDefinitionQuery {
        id: FlowDefinitionId::generate(),
        project_id: project,
        draft: invalid_draft,
      },
    )
    .await
    .unwrap_err();
  let ManagementHandlerError::Application(ApplicationError::FactoryValidation(report)) = invalid else {
    panic!("invalid graph must retain safe authoring diagnostics")
  };
  assert_eq!(report.errors[0].field, "/root");
  assert_eq!(report.errors[0].code, "invalid_contract");
  let child = commands
    .handle_command(
      &context,
      CreateFactoryFlowDefinitionCommand {
        id: FlowDefinitionId::generate(),
        project_id: project,
        draft: draft(None),
        idempotency_key: octacity_server_store::IdempotencyKey::new("child").unwrap(),
        published_at: Timestamp::from_unix_millis(1).unwrap(),
      },
    )
    .await
    .unwrap();
  let child_reference = child.publication.flow.closure.root();
  let root_id = FlowDefinitionId::generate();
  let command = CreateFactoryFlowDefinitionCommand {
    id: root_id,
    project_id: project,
    draft: draft(Some(child_reference)),
    idempotency_key: octacity_server_store::IdempotencyKey::new("parent").unwrap(),
    published_at: Timestamp::from_unix_millis(2).unwrap(),
  };
  let parent = commands.handle_command(&context, command.clone()).await.unwrap();
  assert_eq!(parent.publication.flow.closure.definitions().len(), 2);
  assert_eq!(parent.publication.flow.closure.root().id(), root_id);
  assert_eq!(
    parent
      .publication
      .flow
      .closure
      .definition(parent.publication.flow.closure.root())
      .unwrap()
      .entry()
      .as_str(),
    "operator.selected-step"
  );
  assert_eq!(
    parent.publication.flow.closure.definition(child_reference).unwrap(),
    child.publication.flow.closure.definition(child_reference).unwrap()
  );
  catalog.available.store(false, Ordering::SeqCst);
  let mut replay_command = command.clone();
  replay_command.published_at = Timestamp::from_unix_millis(99).unwrap();
  let replay = commands.handle_command(&context, replay_command).await.unwrap();
  assert_eq!(replay.disposition, StoreMutationDisposition::Replayed);
  assert_eq!(replay.publication, parent.publication);
  assert_eq!(
    queries
      .handle_query(
        &context,
        GetFactoryFlowDefinitionQuery {
          id: root_id,
          version: None
        }
      )
      .await
      .unwrap(),
    parent.publication
  );
  catalog.available.store(true, Ordering::SeqCst);
  let mut drift = command;
  drift.id = FlowDefinitionId::generate();
  drift.idempotency_key = octacity_server_store::IdempotencyKey::new("drift").unwrap();
  let reference = FlowDefinitionRef::new(
    child_reference.id(),
    child_reference.version(),
    FactoryDigest::from_bytes([99; 32]),
  );
  drift.draft = draft(Some(reference));
  let error = commands.handle_command(&context, drift).await.unwrap_err();
  assert!(matches!(
    error,
    ManagementHandlerError::Application(ApplicationError::InvalidInput)
  ));
  database.cleanup().await;
}
