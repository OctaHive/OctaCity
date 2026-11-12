//! Encoding limits are candidate-local blockers, independent of raw text limits.
use super::*;

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn source_encoded_material_overflow_waits_while_another_issue_progresses() {
  for required in [true, false] {
    for escaped in [true, false] {
      let database = TestDatabase::migrated().await;
      let flow = snapshot_flow_with_inputs(
        false,
        Some(FlowIssueMaterialSelection {
          source: FlowIssueMaterialSource::Selected { max_entries: 1 },
          required,
          max_bytes: MAX_FLOW_DATA_BYTES,
        }),
      );
      let f = Fixture::enabled_with_flow("github", Some(flow))
        .await
        .into_postgres(database.pool.clone())
        .await;
      let oversized = linked_issue(&f, "oversized", true).await;
      let ready = linked_issue(&f, "ready", true).await;
      // Both fit the raw-text contract. Escaping exceeds Incoming's encoded ceiling;
      // plain text fits Incoming, but its retained source metadata exceeds the private ceiling.
      let content = if escaped {
        format!("x{}", "\n".repeat(600_000))
      } else {
        "x".repeat(1_048_000)
      };
      let published = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), documents(&f))
        .handle_command(
          &context(),
          PublishProjectDocumentCommand {
            submission: ProjectDocumentSubmission {
              project_id: oversized.project_id,
              name: "Encoded input bound".parse().unwrap(),
              content,
              purpose: ProjectDocumentPurpose::Context,
              idempotency_key: IdempotencyKey::new("large-text").unwrap(),
            },
          },
        )
        .await
        .unwrap();
      select(
        &f,
        &oversized,
        vec![IssueMaterialReference::ProjectDocument {
          project_id: oversized.project_id,
          artifact: published.artifact,
        }],
        "approve-large-text",
      )
      .await;
      let approved = comment(&f, &ready, "Ready approved text", "ready-text").await;
      select(
        &f,
        &ready,
        vec![IssueMaterialReference::Comment {
          project_id: ready.project_id,
          issue_id: ready.id,
          comment_id: approved.id,
          body_digest: approved.body_digest,
        }],
        "approve-ready-text",
      )
      .await;
      let candidates: Vec<_> = ["oversized", "ready"].into_iter().map(|item| serde_json::json!({
        "container_id":"native-container", "item_id":item, "display_id":item, "update_identity":"unchanged",
        "title":"Provider text", "body":"Provider acceptance", "fields":{}, "labels":["arbitrary"], "state":"native-state"
      })).collect();
      std::fs::write(
        f.root.path().join("github/mode.json"),
        serde_json::to_vec(&serde_json::json!({"candidates":candidates})).unwrap(),
      )
      .unwrap();
      let result = worker(&f).run_once(CancellationToken::new()).await.unwrap();
      assert_eq!(
        (result.completed, result.deferred),
        (1, 0),
        "encoding overflow must not fail the Source scan"
      );
      let runs = f.runs().await;
      assert_eq!(
        runs.len(),
        1,
        "the eligible Issue following the oversized one must progress"
      );
      let run = f.store.factory_run_snapshot(runs[0].id).await.unwrap();
      assert_eq!(
        run.flow.data.incoming[0].payload().value(),
        &serde_json::json!({
          "title":"Canonical ready", "size":"small", "priority":"high", "materials":["Ready approved text"]
        })
      );
      assert_eq!(run.flow.data.incoming[0].issue_snapshot().unwrap().issue_id, ready.id);
      let waiting = pending(&f).await;
      assert_eq!(waiting.item.item.as_str(), "oversized");
      assert_eq!(
        waiting.waiting.as_ref().unwrap().reason,
        WorkSourceWaitReason::Incompatible
      );
      assert!(waiting.issue_input.is_none());
      assert!(waiting.admission.is_none());
      assert_eq!(
        f.bytes.0.lock().unwrap().len(),
        3,
        "only the named document and eligible Work inputs are written"
      );
      database.cleanup().await;
    }
  }
}

#[test]
fn unrelated_snapshot_validation_and_store_errors_do_not_become_input_waits() {
  let invalid_schema = ApplicationError::FactoryValidation(Box::new(FactoryValidationDiagnostics {
    errors: vec![FactoryValidationIssue {
      field: "/issue_snapshot/fields".into(),
      code: "invalid_contract",
      ..Default::default()
    }],
    ..Default::default()
  }));
  assert!(source_input_wait(&invalid_schema).is_none());
  for source in [
    StoreInputError::InvalidProjectTextInput,
    StoreInputError::JsonDocumentTooLarge,
  ] {
    let invalid_store_input = ApplicationError::Store(StoreError::InvalidInput {
      operation: StoreOperation::ReserveProjectTextInput,
      source,
    });
    assert!(source_input_wait(&invalid_store_input).is_none());
  }
}
