//! Issue execution rights do not substitute for the independently authorized body readers.
use super::*;

pub(super) struct Deny(pub(super) ManagementResourceKind);
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for Deny {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if resource.kind() == self.0 {
      Err(ManagementAuthorizationDenial::forbidden())
    } else {
      Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
    }
  }
}

#[test]
fn issue_material_snapshot_requires_independent_discussion_and_document_body_permissions() {
  run_ready(async {
    let f = MaterialFixture::new(true).await;
    // Live readable publications establish the positive control before permission denial.
    let documents = AuthorizedQueryHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      f.owner(Arc::new(TrustedNetworkManagementPolicy)),
    );
    let content = documents
      .handle_query(
        &context(),
        ReadIssueDocumentQuery {
          project_id: f.issue.project_id,
          issue_id: f.issue.id,
          artifact_id: f.owned.artifact.artifact_id(),
        },
      )
      .await
      .unwrap();
    assert_eq!(content.content, "ISSUE_DOCUMENT\r\nточные байты\n");
    let content = documents
      .handle_query(
        &context(),
        ReadProjectInputDocumentQuery {
          project_id: f.issue.project_id,
          artifact_id: f.shared.artifact.artifact_id(),
        },
      )
      .await
      .unwrap();
    assert_eq!(content.content, "PROJECT_DOCUMENT\r\nreference\n");
    for resource in [
      ManagementResourceKind::IssueDiscussion,
      ManagementResourceKind::Artifact,
    ] {
      let before = f.factory.mutation_evidence_counts().await;
      let issue_before = f.issues.mutation_evidence_counts().await;
      let read_count = f.bytes.reads.lock().unwrap().len();
      let error = f
        .review(f.readers(Arc::new(Deny(resource))))
        .await
        .expect_err("execute permission cannot read separately denied material bodies");
      let Some(ApplicationError::FactoryValidation(report)) = error.application() else {
        panic!("expected safe material diagnostic: {error:?}")
      };
      assert_eq!(report.errors[0].field, "/issue_snapshot/materials/context");
      assert_eq!(report.errors[0].code, "issue_snapshot_material_unavailable");
      assert_eq!(
        f.bytes.reads.lock().unwrap().len(),
        read_count,
        "denial precedes private document I/O"
      );
      assert_eq!(f.factory.mutation_evidence_counts().await, before);
      assert_eq!(f.issues.mutation_evidence_counts().await, issue_before);
      f.no_runs().await;
    }
  });
}

#[test]
fn issue_material_snapshot_recovery_rechecks_body_permissions_before_first_admission() {
  run_ready(async {
    for required in [true, false] {
      let mut f = MaterialFixture::new(true).await;
      if !required {
        super::bounds::select_contract(
          &mut f,
          FlowIssueMaterialSelection {
            source: FlowIssueMaterialSource::Selected { max_entries: 3 },
            required: false,
            max_bytes: 4096,
          },
        )
        .await;
      }
      let readers = f.readers(Arc::new(TrustedNetworkManagementPolicy));
      let request = f.review(readers.clone()).await.unwrap().request;
      f.bytes.lose_ack.store(true, std::sync::atomic::Ordering::SeqCst);
      assert!(f.publish(&request, readers).await.is_err());
      let original = f
        .factory
        .project_text_documents(
          f.issue.project_id,
          &context().security_scope().to_store(),
          &request.admission_key(),
        )
        .await
        .unwrap()
        .unwrap();
      let error = f
        .publish(
          &request,
          f.readers(Arc::new(Deny(ManagementResourceKind::IssueDiscussion))),
        )
        .await
        .expect_err("pending preparation cannot bypass newly denied body permission");
      let Some(ApplicationError::FactoryValidation(report)) = error.application() else {
        panic!("expected current material access blocker: {error:?}")
      };
      assert_eq!(report.errors[0].field, "/issue_snapshot/materials/context");
      assert_eq!(report.errors[0].code, "issue_snapshot_material_unavailable");
      f.no_runs().await;
      assert_eq!(
        f.factory
          .project_text_documents(
            f.issue.project_id,
            &context().security_scope().to_store(),
            &request.admission_key(),
          )
          .await
          .unwrap()
          .unwrap(),
        original,
        "failure must not rewrite the frozen snapshot"
      );
      let published = f
        .publish(&request, f.readers(Arc::new(TrustedNetworkManagementPolicy)))
        .await
        .unwrap();
      assert_eq!(
        f.factory
          .project_text_documents(
            f.issue.project_id,
            &context().security_scope().to_store(),
            &request.admission_key(),
          )
          .await
          .unwrap()
          .unwrap(),
        original
      );
      let before = f.bytes.reads.lock().unwrap().len();
      assert_eq!(
        f.publish(&request, f.readers(Arc::new(Deny(ManagementResourceKind::Artifact))))
          .await
          .unwrap(),
        published,
        "an admitted receipt does not reopen source bodies"
      );
      assert_eq!(f.bytes.reads.lock().unwrap().len(), before);
    }
  });
}
