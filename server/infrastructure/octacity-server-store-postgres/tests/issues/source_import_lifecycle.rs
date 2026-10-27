//! Removing import controls retains immutable versions and already created tasks.
use super::*;
use octacity_server_domain::{IntegrationId, IntegrationVersion, IssueSourceImportId};
use octacity_server_store::*;
use serde_json::json;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn retired_import_leaves_current_lists_and_cannot_be_resurrected_by_replay_or_edit() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let project = project(&store).await;
  let issue = store
    .create_issue(management_mutation(draft(project.id, "retained-task")))
    .await
    .unwrap()
    .issue;
  let (installed, draft) = testing::integration_connection_fixture("github");
  let connection = IntegrationId::generate();
  let metadata = store
    .publish_integration_connection(management_mutation(PublishIntegrationConnection {
      intent: IntegrationConnectionIntent {
        id: connection,
        expected_current_version: None,
        draft,
      },
      installed,
      idempotency_key: key("import-connection"),
      published_at: at(2),
    }))
    .await
    .unwrap()
    .connection;
  let resource = &metadata.definition.resources[0];
  let capability = &resource.capabilities[0];
  let mut intent = json!({
    "id": IssueSourceImportId::from_uuid(uuid::Uuid::from_u128(1)).unwrap(), "project_id": project.id, "expected_current_version": null,
    "definition": { "enabled": false,
      "source": { "installation": metadata.definition.installation, "resource": resource.reference,
        "capability": capability.capability, "configuration": capability.configuration },
      "connection": { "id": connection, "version": 1, "access_version": 1,
        "source_configuration": capability.configuration, "reporter_configuration": null }
    }
  });
  let original = PublishIssueSourceImport {
    intent: serde_json::from_value(intent.clone()).unwrap(),
    idempotency_key: key("create-import"),
    published_at: at(3),
  };
  let first = store
    .publish_issue_source_import(management_mutation(original.clone()))
    .await
    .unwrap();
  assert!(
    serde_json::to_value(&first.configuration).unwrap()["definition"]
      .get("retired")
      .is_none(),
    "legacy publication shape stays unchanged"
  );
  let mut survivor = original.clone();
  survivor.intent.id = IssueSourceImportId::from_uuid(uuid::Uuid::from_u128(2)).unwrap();
  survivor.idempotency_key = key("another-import");
  let survivor = store
    .publish_issue_source_import(management_mutation(survivor))
    .await
    .unwrap();
  intent["expected_current_version"] = json!(1);
  intent["definition"]["retired"] = json!(true);
  let retirement = PublishIssueSourceImport {
    intent: serde_json::from_value(intent.clone()).expect("reviewed import removal is supported"),
    idempotency_key: key("remove-import"),
    published_at: at(4),
  };
  let mut invalid = retirement.clone();
  invalid.intent.definition.enabled = true;
  invalid.idempotency_key = key("invalid-active-removal");
  assert!(matches!(
    store.publish_issue_source_import(management_mutation(invalid)).await,
    Err(StoreError::InvalidInput { .. })
  ));
  let removed = store
    .publish_issue_source_import(management_mutation(retirement.clone()))
    .await
    .unwrap();
  assert_eq!(removed.configuration.version.get(), 2);
  let scope = ManagementSecurityScope::trusted_network();
  let page = store
    .list_issue_source_imports(project.id, &scope, IntegrationConnectionListVisibility::all(), None, 1)
    .await
    .unwrap();
  assert_eq!(
    page.items,
    [survivor.configuration],
    "removal filters before paging and preserves other imports"
  );
  assert!(page.next_cursor.is_none());
  assert_eq!(
    store
      .issue_source_import(
        project.id,
        &scope,
        first.configuration.id,
        Some(IntegrationVersion::INITIAL)
      )
      .await
      .unwrap(),
    first.configuration
  );
  assert_eq!(store.issue(project.id, issue.id, None).await.unwrap(), issue);
  for command in [original, retirement] {
    let replay = store
      .publish_issue_source_import(management_mutation(command))
      .await
      .unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
  }
  assert_eq!(
    store
      .issue_source_import(project.id, &scope, first.configuration.id, None)
      .await
      .unwrap(),
    removed.configuration
  );
  intent["expected_current_version"] = json!(2);
  intent["definition"]["retired"] = json!(false);
  assert!(matches!(
    store
      .publish_issue_source_import(management_mutation(PublishIssueSourceImport {
        intent: serde_json::from_value(intent).unwrap(),
        idempotency_key: key("revive-import"),
        published_at: at(5),
      }))
      .await,
    Err(StoreError::Conflict { .. })
  ));
  database.cleanup().await;
}
