//! An expired exact publication cannot be replaced by a live newer version.
use super::*;
use octacity_server_domain::ArtifactId;

struct At(Timestamp);
impl FactoryExecutionClock for At {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(self.0)
  }
}

async fn publish_replacement(f: &MaterialFixture, owned: bool, now: Timestamp) -> ArtifactId {
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let owner = f.owner_at(policy.clone(), Arc::new(At(now)));
  let commands = AuthorizedCommandHandler::new(policy.clone(), owner.clone());
  let queries = AuthorizedQueryHandler::new(policy, owner);
  if owned {
    let doc = commands
      .handle_command(
        &context(),
        PublishIssueDocumentCommand {
          submission: IssueDocumentSubmission {
            project_id: f.issue.project_id,
            issue_id: f.issue.id,
            name: f.owned.name.clone(),
            content: "NEW_LIVE_PUBLICATION".into(),
            idempotency_key: key("live-owned-replacement"),
          },
        },
      )
      .await
      .unwrap();
    assert_eq!(
      queries
        .handle_query(
          &context(),
          ReadIssueDocumentQuery {
            project_id: f.issue.project_id,
            issue_id: f.issue.id,
            artifact_id: doc.artifact.artifact_id(),
          }
        )
        .await
        .unwrap()
        .content,
      "NEW_LIVE_PUBLICATION"
    );
    doc.artifact.artifact_id()
  } else {
    let doc = commands
      .handle_command(
        &context(),
        PublishProjectDocumentCommand {
          submission: ProjectDocumentSubmission {
            project_id: f.issue.project_id,
            name: f.shared.name.clone(),
            content: "NEW_LIVE_PUBLICATION".into(),
            purpose: ProjectDocumentPurpose::Context,
            idempotency_key: key("live-shared-replacement"),
          },
        },
      )
      .await
      .unwrap();
    assert_eq!(
      queries
        .handle_query(
          &context(),
          ReadProjectInputDocumentQuery {
            project_id: f.issue.project_id,
            artifact_id: doc.artifact.artifact_id(),
          }
        )
        .await
        .unwrap()
        .content,
      "NEW_LIVE_PUBLICATION"
    );
    doc.artifact.artifact_id()
  }
}

#[test]
fn issue_material_snapshot_blocks_expired_required_documents_and_retains_optional_omissions() {
  run_ready(async {
    for required in [true, false] {
      for owned in [true, false] {
        let mut f = MaterialFixture::new(false).await;
        let reference = if owned {
          FlowIssueMaterialReference::IssueDocument {
            artifact: f.owned.artifact.clone(),
          }
        } else {
          FlowIssueMaterialReference::ProjectDocument {
            artifact: f.shared.artifact.clone(),
          }
        };
        select_contract(
          &mut f,
          FlowIssueMaterialSelection {
            source: FlowIssueMaterialSource::Exact {
              references: vec![reference],
            },
            required,
            max_bytes: 4096,
          },
        )
        .await;
        let now = f.owned.expires_at.max(f.shared.expires_at);
        let replacement = publish_replacement(&f, owned, now).await;
        let reads = f.bytes.reads.lock().unwrap().len();
        let result = f
          .review(f.readers_at(Arc::new(TrustedNetworkManagementPolicy), Arc::new(At(now))))
          .await;
        if required {
          let error = result.expect_err("expired exact input cannot fall back to a live newer publication");
          let Some(ApplicationError::FactoryValidation(report)) = error.application() else {
            panic!("{error:?}")
          };
          assert_eq!(report.errors[0].code, "issue_snapshot_material_unavailable");
          f.no_runs().await;
        } else {
          let request = result.unwrap().request;
          let published = f
            .publish(
              &request,
              f.readers_at(Arc::new(TrustedNetworkManagementPolicy), Arc::new(At(now))),
            )
            .await
            .unwrap();
          let docs = f
            .factory
            .project_text_documents(
              f.issue.project_id,
              &context().security_scope().to_store(),
              &request.admission_key(),
            )
            .await
            .unwrap()
            .unwrap();
          assert_eq!(
            docs.issue_snapshot().unwrap().omissions[&"context".parse().unwrap()],
            IssueSnapshotOmission::Unavailable
          );
          assert!(
            f.factory
              .factory_run_snapshot(published.association.unwrap().run_id)
              .await
              .unwrap()
              .flow
              .data
              .incoming[0]
              .payload()
              .value()
              .get("context")
              .is_none()
          );
        }
        let source_reads = f.bytes.reads.lock().unwrap();
        assert!(
          !source_reads[reads..]
            .iter()
            .any(|id| *id == f.owned.artifact.artifact_id()
              || *id == f.shared.artifact.artifact_id()
              || *id == replacement),
          "expiry is checked before original or replacement source body I/O"
        );
      }
    }
  });
}
