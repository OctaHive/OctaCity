//! Both document owner kinds join explicit supplementary text through their ordinary readers.
use super::*;
use octacity_server_store::{FactoryDiscoveryStore, FactoryRunListVisibility};

type Issues = octacity_server_store::testing::InMemoryProjectStore;
type Factory = octacity_server_store::testing::InMemoryFactoryConfigurationStore;
#[path = "issue_execution_snapshot_material_bytes.rs"]
mod material_bytes;
use material_bytes::Bytes;
#[path = "issue_execution_snapshot_material_access.rs"]
mod access;
#[path = "issue_execution_snapshot_material_bounds.rs"]
mod bounds;
#[path = "issue_execution_snapshot_material_compatibility.rs"]
mod compatibility;
#[path = "issue_execution_snapshot_material_failures.rs"]
mod failures;
#[path = "issue_execution_snapshot_material_replay.rs"]
mod replay;

struct MaterialFixture {
  issues: Arc<Issues>,
  factory: Arc<Factory>,
  issue: IssueRevision,
  command: RequestIssueExecutionCommand,
  bytes: Arc<Bytes>,
  owned: IssueDocumentResource,
  shared: ProjectDocumentResource,
}
impl MaterialFixture {
  fn owner(&self, policy: Arc<dyn ManagementAuthorizationPolicy>) -> Arc<ProjectTextIntake<Factory, Bytes>> {
    self.owner_at(policy, Arc::new(MaterialClock))
  }
  fn owner_at(
    &self,
    policy: Arc<dyn ManagementAuthorizationPolicy>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Arc<ProjectTextIntake<Factory, Bytes>> {
    Arc::new(
      ProjectTextIntake::new(
        self.factory.clone(),
        self.bytes.clone(),
        Arc::new(crate::factory::admission::tests::factory_admission_tests::RecordingResolver::default()),
        policy,
        clock,
      )
      .with_issue_documents(self.issues.clone()),
    )
  }
  fn readers(&self, policy: Arc<dyn ManagementAuthorizationPolicy>) -> Arc<IssueSnapshotMaterialReaders> {
    self.readers_at(policy, Arc::new(MaterialClock))
  }
  fn readers_at(
    &self,
    policy: Arc<dyn ManagementAuthorizationPolicy>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Arc<IssueSnapshotMaterialReaders> {
    let documents = self.owner_at(policy.clone(), clock.clone());
    Arc::new(
      IssueSnapshotMaterialReaders::new(Arc::new(AuthorizedQueryHandler::new(
        policy.clone(),
        Arc::new(IssueDiscussionHandlers::new(self.issues.clone(), policy.clone(), clock)),
      )))
      .with_documents(
        Arc::new(AuthorizedQueryHandler::new(policy.clone(), documents.clone())),
        Arc::new(AuthorizedQueryHandler::new(policy, documents)),
        self.factory.clone(),
      ),
    )
  }
  async fn review(
    &self,
    readers: Arc<IssueSnapshotMaterialReaders>,
  ) -> Result<IssueExecutionOutcome, ManagementHandlerError<ApplicationError>> {
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionHandlers::new(self.issues.clone(), self.factory.clone(), policy).with_materials(readers)),
    )
    .handle_command(&context(), self.command.clone())
    .await
  }
  async fn publish(
    &self,
    request: &IssueExecutionRequest,
    readers: Arc<IssueSnapshotMaterialReaders>,
  ) -> Result<IssueExecutionRequest, ManagementHandlerError<ApplicationError>> {
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let publisher = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        IssueExecutionPublisher::new(
          self.issues.clone(),
          self.factory.clone(),
          self.bytes.clone(),
          Arc::new(crate::factory::admission::tests::factory_admission_tests::RecordingResolver::default()),
          policy,
          Arc::new(MaterialClock),
        )
        .with_materials(readers),
      ),
    );
    publisher
      .handle_command(
        &context(),
        PublishIssueExecutionCommand {
          project_id: self.issue.project_id,
          issue_id: self.issue.id,
          request_id: request.id,
          classification: crate::factory::admission::tests::factory_admission_tests::command(
            self.issue.project_id,
            request.intent.target.configuration_id,
            request.intent.target.repository_id,
          )
          .classification,
        },
      )
      .await
  }
  async fn no_runs(&self) {
    let runs = self
      .factory
      .list_factory_runs(
        ListFactoryRuns::new(
          Some(self.issue.project_id),
          Default::default(),
          None,
          10,
          FactoryRunListVisibility::all(),
        )
        .unwrap(),
      )
      .await
      .unwrap();
    assert!(runs.items.is_empty());
  }
  async fn new(selected: bool) -> Self {
    let (issues, factory, issue, command) = selected_comment_fixture().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let bytes = Arc::new(Bytes::default());
    let owner = Arc::new(
      ProjectTextIntake::new(
        factory.clone(),
        bytes.clone(),
        Arc::new(crate::factory::admission::tests::factory_admission_tests::RecordingResolver::default()),
        policy.clone(),
        Arc::new(MaterialClock),
      )
      .with_issue_documents(issues.clone()),
    );
    let commands = AuthorizedCommandHandler::new(policy.clone(), owner);
    let owned = commands
      .handle_command(
        &context(),
        PublishIssueDocumentCommand {
          submission: IssueDocumentSubmission {
            project_id: issue.project_id,
            issue_id: issue.id,
            name: "Issue notes".parse().unwrap(),
            content: "ISSUE_DOCUMENT\r\nточные байты\n".into(),
            idempotency_key: key("snapshot-owned-document"),
          },
        },
      )
      .await
      .unwrap();
    let shared = commands
      .handle_command(
        &context(),
        PublishProjectDocumentCommand {
          submission: ProjectDocumentSubmission {
            project_id: issue.project_id,
            name: "Shared notes".parse().unwrap(),
            content: "PROJECT_DOCUMENT\r\nreference\n".into(),
            purpose: ProjectDocumentPurpose::Context,
            idempotency_key: key("snapshot-project-document"),
          },
        },
      )
      .await
      .unwrap();
    let mut f = Self {
      issues,
      factory,
      issue,
      command,
      bytes,
      owned,
      shared,
    };
    // Publication alone does not attach either new document or reserve execution.
    assert_eq!(
      f.issues
        .issue(f.issue.project_id, f.issue.id, None)
        .await
        .unwrap()
        .materials
        .entries()
        .len(),
      1
    );
    f.no_runs().await;
    let mut references: Vec<_> = f
      .issue
      .materials
      .entries()
      .iter()
      .map(|m| m.reference.clone())
      .collect();
    references.push(IssueMaterialReference::IssueDocument {
      project_id: f.issue.project_id,
      issue_id: f.issue.id,
      artifact: f.owned.artifact.clone(),
    });
    references.push(IssueMaterialReference::ProjectDocument {
      project_id: f.issue.project_id,
      artifact: f.shared.artifact.clone(),
    });
    let materials = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueMaterialHandlers::new(
        f.issues.clone(),
        f.factory.clone(),
        policy,
        Arc::new(MaterialClock),
      )),
    );
    f.issue = materials
      .handle_command(
        &context(),
        EditIssueMaterialsCommand {
          intent: EditIssueMaterialsIntent {
            project_id: f.issue.project_id,
            id: f.issue.id,
            expected_version: f.issue.version,
            references,
          },
          idempotency_key: key("approve-document-materials"),
        },
      )
      .await
      .unwrap()
      .issue;
    let id = FactoryConfigurationId::generate();
    let source = if selected {
      FlowIssueMaterialSource::Selected { max_entries: 3 }
    } else {
      FlowIssueMaterialSource::Exact {
        references: vec![
          FlowIssueMaterialReference::IssueDocument {
            artifact: f.owned.artifact.clone(),
          },
          FlowIssueMaterialReference::ProjectDocument {
            artifact: f.shared.artifact.clone(),
          },
        ],
      }
    };
    crate::factory::admission::tests::factory_admission_tests::seed_permitted_configuration(
      &f.factory,
      PublishedFactoryConfiguration {
        configuration: material_configuration(f.issue.project_id, id, source, true),
        published_at: at(46),
      },
    )
    .unwrap();
    f.command.intent.target.configuration_id = id;
    f.command.intent.expected_version = f.issue.version;
    f.no_runs().await;
    f
  }
}

#[test]
fn issue_material_snapshot_prepares_owned_and_linked_document_text_with_exact_lineage() {
  run_ready(async {
    for selected in [false, true] {
      let f = MaterialFixture::new(selected).await;
      let readers = f.readers(Arc::new(TrustedNetworkManagementPolicy));
      let request = f
        .review(readers.clone())
        .await
        .expect("approved exact document owners must supply selected text")
        .request;
      f.no_runs().await;
      let published = f.publish(&request, readers).await.unwrap();
      let run = f
        .factory
        .factory_run_snapshot(published.association.unwrap().run_id)
        .await
        .unwrap();
      let expected = if selected {
        serde_json::json!([
          "COMMENT_SELECTED\r\nёж\n",
          "ISSUE_DOCUMENT\r\nточные байты\n",
          "PROJECT_DOCUMENT\r\nreference\n"
        ])
      } else {
        serde_json::json!(["ISSUE_DOCUMENT\r\nточные байты\n", "PROJECT_DOCUMENT\r\nreference\n"])
      };
      assert_eq!(run.flow.data.incoming[0].payload().value()["context"], expected);
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
      let snapshot = serde_json::to_value(documents.issue_snapshot().unwrap()).unwrap();
      let lineage = snapshot["materials"]["context"].as_array().unwrap();
      let offset = usize::from(selected);
      assert_eq!(
        lineage[offset]["selection"],
        serde_json::to_value(&f.issue.materials.entries()[1]).unwrap()
      );
      assert_eq!(
        lineage[offset]["source"]["document"],
        serde_json::to_value(&f.owned).unwrap()
      );
      assert_eq!(
        lineage[offset + 1]["selection"],
        serde_json::to_value(&f.issue.materials.entries()[2]).unwrap()
      );
      assert_eq!(
        lineage[offset + 1]["source"]["document"],
        serde_json::to_value(&f.shared).unwrap()
      );
      assert_eq!(
        documents.issue_snapshot().unwrap().digest().unwrap(),
        request.snapshot_digest.unwrap()
      );
    }
  });
}
