//! Automatic intake waits for canonical Issue data without blocking another eligible item.
use super::*;
use crate::runtime::postgres_test_support::TestDatabase;
use octacity_server_store::testing::management_mutation;
use octacity_server_store_postgres::PostgresStore;

#[path = "issue_snapshot_authorization_tests.rs"]
mod authorization;
#[path = "issue_snapshot_recovery_tests.rs"]
mod recovery;

fn snapshot_flow() -> FactoryFlowConfiguration {
  let schema = FlowDataSchema::new(
    key("source.issue"),
    key("v1"),
    FlowValueSchema::Object {
      fields: [
        (
          key("title"),
          FlowFieldSchema::required(FlowValueSchema::String {
            min_bytes: 1,
            max_bytes: 512,
          }),
        ),
        (
          key("size"),
          FlowFieldSchema::required(FlowValueSchema::Enum {
            values: ["small", "medium", "large"].map(serde_json::Value::from).into(),
          }),
        ),
        (
          key("priority"),
          FlowFieldSchema::required(FlowValueSchema::String {
            min_bytes: 1,
            max_bytes: 256,
          }),
        ),
      ]
      .into(),
    },
  )
  .unwrap();
  let base = configuration(ProjectId::generate(), FactoryConfigurationId::generate())
    .flow()
    .clone();
  let root = base.closure.definition(base.closure.root()).unwrap();
  let mut nodes = root.nodes().to_vec();
  let entry = nodes.iter().position(|node| node.key() == root.entry()).unwrap();
  nodes[entry] = nodes[entry]
    .clone()
    .with_input_binding(
      FlowInputBinding::new(
        [(
          key("issue"),
          FlowInputSource::Incoming {
            schema: schema.reference().clone(),
            path: String::new(),
          },
        )]
        .into(),
        FlowDataMapping::new(FlowValueMapping::Constant {
          value: serde_json::json!(true),
        })
        .unwrap(),
      )
      .unwrap(),
    )
    .unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
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
  .unwrap();
  let mut schemas = base.data_schemas;
  schemas.push(schema.clone());
  FactoryFlowConfiguration::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    base.limits,
    schemas,
    base.pools,
  )
  .unwrap()
  .with_issue_snapshot(FlowIssueSnapshotInput {
    version: 1,
    schema: schema.reference().clone(),
    definition_version: 0,
    admission: None,
    fields: [
      ("title", FlowIssueFieldSource::Title),
      ("size", FlowIssueFieldSource::Size),
      ("priority", FlowIssueFieldSource::Priority),
    ]
    .into_iter()
    .map(|(name, source)| (key(name), FlowIssueFieldSelection { source, required: true }))
    .collect(),
    materials: Default::default(),
    evidence: Default::default(),
  })
  .unwrap()
}

async fn linked_issue(f: &Fixture<PostgresStore>, item: &str, classified: bool) -> IssueRevision {
  let project = f.input.project_id();
  let mut issue = f
    .store
    .create_issue(management_mutation(CreateIssue {
      id: IssueId::generate(),
      assignees: Default::default(),
      intent: CreateIssueIntent {
        project_id: project,
        planning: Default::default(),
        content: IssueContent {
          title: format!("Canonical {item}"),
          description: "Reviewed canonical requirements".into(),
          acceptance: Some("Canonical implementation acceptance".into()),
          priority: Some(IssuePriority::Choice("high".into())),
        },
      },
      idempotency_key: IdempotencyKey::new(format!("create-{item}")).unwrap(),
      created_at: time(10),
    }))
    .await
    .unwrap()
    .issue;
  if classified {
    issue = f
      .store
      .edit_issue_planning(management_mutation(EditIssuePlanning {
        intent: EditIssuePlanningIntent {
          project_id: project,
          id: issue.id,
          expected_version: issue.version,
          planning: IssuePlanning {
            size: Some(IssueSize::Small),
            ..issue.planning
          },
        },
        assignees: Default::default(),
        idempotency_key: IdempotencyKey::new(format!("classify-{item}")).unwrap(),
        edited_at: time(20),
      }))
      .await
      .unwrap()
      .issue;
  }
  f.store
    .attach_issue_source(management_mutation(AttachIssueSource {
      intent: AttachIssueSourceIntent {
        selection: IssueSourceSelection {
          project_id: project,
          connection: None,
          source: f.profile.read.binding.clone(),
          item: WorkSourceItem {
            resource: ExternalWorkIdentity::new("native-container").unwrap(),
            item: ExternalWorkIdentity::new(item).unwrap(),
          },
        },
        issue_id: issue.id,
        expected_version: issue.version,
        expected_field_policy: Default::default(),
      },
      observation: IssueSourceObservation {
        display_id: Some(item.into()),
        source: f.profile.read.binding.clone(),
        plugin: f.profile.read.plugin.clone(),
        provider_configuration: f.profile.read.provider_configuration,
        selection_digest: FactoryDigest::from_bytes([2; 32]),
        snapshot_digest: FactoryDigest::from_bytes([3; 32]),
        observed_at: time(30),
        expires_at: time(10_000),
      },
      idempotency_key: IdempotencyKey::new(format!("link-{item}")).unwrap(),
      attached_at: time(30),
    }))
    .await
    .unwrap();
  issue
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn automatic_issue_snapshot_waits_for_classification_while_another_item_progresses() {
  let database = TestDatabase::migrated().await;
  let fixture = Fixture::enabled_with_flow("github", Some(snapshot_flow()))
    .await
    .into_postgres(database.pool.clone())
    .await;
  let pending = linked_issue(&fixture, "pending", false).await;
  let ready = linked_issue(&fixture, "ready", true).await;
  let candidates: Vec<_> = ["pending", "ready"].into_iter().map(|item| serde_json::json!({
    "container_id":"native-container", "item_id":item, "display_id":item, "update_identity":"unchanged",
    "title":"Provider title must not replace canonical requirements", "body":"Provider body is not implementation acceptance",
    "fields":{}, "labels":["arbitrary"], "state":"native-state"
  })).collect();
  std::fs::write(
    fixture.root.path().join("github/mode.json"),
    serde_json::to_vec(&serde_json::json!({"candidates":candidates})).unwrap(),
  )
  .unwrap();
  let result = fixture
    .worker()
    .with_issue_snapshots(Arc::new(IssueSourceSnapshotReader::new(
      fixture.store.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
    )))
    .run_once(CancellationToken::new())
    .await
    .unwrap();
  assert_eq!((result.completed, result.deferred), (1, 0));
  let runs = fixture.runs().await;
  assert_eq!(
    runs.len(),
    1,
    "the ready Issue must progress while the first Issue lacks classification"
  );
  let run = fixture.store.factory_run_snapshot(runs[0].id).await.unwrap();
  let incoming = &run.flow.data.incoming[0];
  assert_eq!(
    incoming.payload().value(),
    &serde_json::json!({"title":"Canonical ready", "size":"small", "priority":"high"})
  );
  assert!(incoming.predecessor().is_none());
  assert_eq!(incoming.issue_snapshot().unwrap().issue_id, ready.id);
  let target = WorkSourceScanTarget::Input {
    id: fixture.input.id(),
    version: fixture.input.version(),
  };
  let preparations = fixture
    .store
    .pending_source_preparations(fixture.input.project_id(), target, None, 10)
    .await
    .unwrap();
  let waiting = preparations.iter().find(|p| p.item.item.as_str() == "pending").unwrap();
  assert!(waiting.waiting.is_some());
  assert!(waiting.intent.is_none());
  let classified = fixture
    .store
    .edit_issue_planning(management_mutation(EditIssuePlanning {
      intent: EditIssuePlanningIntent {
        project_id: pending.project_id,
        id: pending.id,
        expected_version: pending.version,
        planning: IssuePlanning {
          size: Some(IssueSize::Medium),
          ..pending.planning
        },
      },
      assignees: Default::default(),
      idempotency_key: IdempotencyKey::new("classify-pending").unwrap(),
      edited_at: time(105),
    }))
    .await
    .unwrap()
    .issue;
  fixture.clock.0.store(110, Ordering::SeqCst);
  assert_eq!(
    fixture
      .worker()
      .with_issue_snapshots(Arc::new(IssueSourceSnapshotReader::new(
        fixture.store.clone(),
        Arc::new(TrustedNetworkManagementPolicy)
      )))
      .run_once(CancellationToken::new())
      .await
      .unwrap()
      .completed,
    1
  );
  let runs = fixture.runs().await;
  assert_eq!(runs.len(), 2);
  let preparation = fixture
    .store
    .source_preparation(fixture.input.project_id(), waiting.id)
    .await
    .unwrap();
  let frozen = preparation.issue_input.as_ref().unwrap().issue_snapshot().unwrap();
  assert_eq!(frozen.version, classified.version);
  assert_eq!(frozen.definitions.len(), 1);
  assert!(frozen.attribution.contains_key(&key("size")));
  assert!(preparation.admission.is_some());
  let mut issues = std::collections::BTreeSet::new();
  for run in runs {
    let data = fixture.store.factory_run_snapshot(run.id).await.unwrap();
    let incoming = &data.flow.data.incoming[0];
    issues.insert(incoming.issue_snapshot().unwrap().issue_id);
    if incoming.issue_snapshot().unwrap().issue_id == pending.id {
      assert_eq!(incoming.issue_snapshot().unwrap().digest, frozen.digest().unwrap());
      assert_eq!(
        incoming.payload().value(),
        &serde_json::json!({"title":"Canonical pending", "size":"medium", "priority":"high"})
      );
    }
  }
  assert_eq!(issues, [pending.id, ready.id].into());
  database.cleanup().await;
}
