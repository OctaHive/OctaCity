//! Encoded newly available text cannot invalidate an already reviewed optional omission.
use super::*;

async fn large_fixture(content: String, required: bool) -> MaterialFixture {
  let mut f = MaterialFixture::new(true).await;
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let document = AuthorizedCommandHandler::new(policy.clone(), f.owner(policy.clone()))
    .handle_command(
      &context(),
      PublishProjectDocumentCommand {
        submission: ProjectDocumentSubmission {
          project_id: f.issue.project_id,
          name: "Large selected text".parse().unwrap(),
          content,
          purpose: ProjectDocumentPurpose::Context,
          idempotency_key: key("review-large-legacy"),
        },
      },
    )
    .await
    .unwrap();
  f.issue = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(IssueMaterialHandlers::new(
      f.issues.clone(),
      f.factory.clone(),
      policy.clone(),
      Arc::new(MaterialClock),
    )),
  )
  .handle_command(
    &context(),
    EditIssueMaterialsCommand {
      intent: EditIssueMaterialsIntent {
        project_id: f.issue.project_id,
        id: f.issue.id,
        expected_version: f.issue.version,
        references: vec![IssueMaterialReference::ProjectDocument {
          project_id: f.issue.project_id,
          artifact: document.artifact,
        }],
      },
      idempotency_key: key("review-approve-large"),
    },
  )
  .await
  .unwrap()
  .issue;
  let id = FactoryConfigurationId::generate();
  let text = FlowValueSchema::Array {
    item: Box::new(FlowValueSchema::String {
      min_bytes: 0,
      max_bytes: 1_048_576,
    }),
    min_items: u16::from(required),
    max_items: 1,
  };
  let base = snapshot_configuration_with_schema(
    f.issue.project_id,
    id,
    0,
    [(
      "title".parse().unwrap(),
      FlowIssueFieldSelection {
        source: FlowIssueFieldSource::Title,
        required: true,
      },
    )]
    .into(),
    FlowValueSchema::Object {
      fields: [
        (
          "title".parse().unwrap(),
          FlowFieldSchema::required(FlowValueSchema::String {
            min_bytes: 1,
            max_bytes: 512,
          }),
        ),
        (
          "context".parse().unwrap(),
          if required {
            FlowFieldSchema::required(text)
          } else {
            FlowFieldSchema::optional(text)
          },
        ),
      ]
      .into(),
    },
  );
  let mut flow = base.flow().clone();
  flow.issue_snapshot.as_mut().unwrap().materials.insert(
    "context".parse().unwrap(),
    FlowIssueMaterialSelection {
      source: FlowIssueMaterialSource::Selected { max_entries: 1 },
      required,
      max_bytes: 1_048_576,
    },
  );
  let configuration = crate::factory::admission::tests::factory_admission_tests::configuration_with_flow(
    f.issue.project_id,
    id,
    Some(flow),
  );
  crate::factory::admission::tests::factory_admission_tests::seed_permitted_configuration(
    &f.factory,
    PublishedFactoryConfiguration {
      configuration,
      published_at: at(46),
    },
  )
  .unwrap();
  f.command.intent.target.configuration_id = id;
  f.command.intent.expected_version = f.issue.version;
  f
}

async fn assert_original_omission(f: MaterialFixture) {
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let request = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(IssueExecutionHandlers::new(f.issues.clone(), f.factory.clone(), policy)),
  )
  .handle_command(&context(), f.command.clone())
  .await
  .unwrap()
  .request;
  let published = f
    .publish(&request, f.readers(Arc::new(TrustedNetworkManagementPolicy)))
    .await
    .expect("newly available text must not invalidate a reviewed omission");
  let run = f
    .factory
    .factory_run_snapshot(published.association.as_ref().unwrap().run_id)
    .await
    .unwrap();
  assert_eq!(
    run.flow.data.incoming[0].payload().value(),
    &serde_json::json!({"title":"Manual feature"})
  );
  let documents = f
    .factory
    .project_text_documents(
      f.issue.project_id,
      &context().security_scope().to_store(),
      &request.admission_key(),
    )
    .await
    .unwrap()
    .unwrap();
  let snapshot = documents.issue_snapshot().unwrap();
  assert_eq!(Some(snapshot.digest().unwrap()), request.snapshot_digest);
  assert!(snapshot.materials.is_empty());
  assert_eq!(
    snapshot.omissions[&"context".parse().unwrap()],
    IssueSnapshotOmission::Unavailable
  );
  assert_eq!(
    f.publish(&request, f.readers(Arc::new(TrustedNetworkManagementPolicy)))
      .await
      .unwrap(),
    published
  );
}

#[test]
fn issue_material_snapshot_keeps_a_legacy_omission_before_incoming_encoding_limits() {
  run_ready(async {
    // Raw UTF-8 fits the one-MiB consumer limit; escaping newlines exceeds Incoming's encoded limit.
    assert_original_omission(large_fixture(format!("x{}", "\n".repeat(600_000)), false).await).await;
  });
}

#[test]
fn issue_material_snapshot_keeps_a_legacy_omission_before_private_snapshot_encoding_limits() {
  run_ready(async {
    // Incoming text fits, but retaining source metadata makes the new private snapshot too large.
    assert_original_omission(large_fixture("x".repeat(1_048_000), false).await).await;
  });
}

#[test]
fn issue_material_snapshot_rejects_new_oversized_text_without_a_reviewed_omission() {
  run_ready(async {
    for required in [true, false] {
      for escaped in [true, false] {
        let content = if escaped {
          format!("x{}", "\n".repeat(600_000))
        } else {
          "x".repeat(1_048_000)
        };
        let f = large_fixture(content, required).await;
        let before = f.issues.mutation_evidence_counts().await;
        let error = f
          .review(f.readers(Arc::new(TrustedNetworkManagementPolicy)))
          .await
          .expect_err("newly selected text must still satisfy Incoming and private snapshot encoding limits");
        if escaped {
          let Some(ApplicationError::FactoryValidation(report)) = error.application() else {
            panic!("{error:?}")
          };
          assert_eq!(report.errors[0].field, "/issue_snapshot/fields");
          assert_eq!(report.errors[0].code, "invalid_contract");
        } else {
          assert!(matches!(
            error.application(),
            Some(ApplicationError::Store(StoreError::InvalidInput { .. }))
          ));
        }
        assert_eq!(f.issues.mutation_evidence_counts().await, before);
        f.no_runs().await;
      }
    }
  });
}
