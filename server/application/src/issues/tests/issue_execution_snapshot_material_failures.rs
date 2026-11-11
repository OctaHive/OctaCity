//! Unavailable exact sources never trigger implicit thread/catalog substitutions.
use super::bounds::select_contract;
use super::*;

#[test]
fn issue_material_snapshot_explicitly_omits_denied_optional_text() {
  run_ready(async {
    let mut f = MaterialFixture::new(true).await;
    select_contract(
      &mut f,
      FlowIssueMaterialSelection {
        source: FlowIssueMaterialSource::Selected { max_entries: 3 },
        required: false,
        max_bytes: 4096,
      },
    )
    .await;
    let readers = f.readers(Arc::new(super::access::Deny(ManagementResourceKind::IssueDiscussion)));
    let request = f.review(readers.clone()).await.unwrap().request;
    let published = f.publish(&request, readers).await.unwrap();
    let run = f
      .factory
      .factory_run_snapshot(published.association.unwrap().run_id)
      .await
      .unwrap();
    assert_eq!(
      run.flow.data.incoming[0].payload().value(),
      &serde_json::json!({"title":"Manual feature"})
    );
    let retained = f
      .factory
      .project_text_documents(
        f.issue.project_id,
        &context().security_scope().to_store(),
        &request.admission_key(),
      )
      .await
      .unwrap()
      .unwrap();
    let snapshot = retained.issue_snapshot().unwrap();
    assert_eq!(
      snapshot.omissions[&"context".parse().unwrap()],
      IssueSnapshotOmission::Unavailable
    );
    assert!(snapshot.materials.is_empty());
  });
}

#[test]
fn issue_material_snapshot_rejects_unapproved_digest_size_and_owner_substitutions_without_body_reads() {
  run_ready(async {
    let f = MaterialFixture::new(true).await;
    let IssueMaterialReference::Comment { comment_id, .. } = f.issue.materials.entries()[0].reference else {
      panic!()
    };
    let wrong = FactoryDigest::content_sha256(b"different source");
    let artifact = &f.owned.artifact;
    for source in [
      FlowIssueMaterialReference::Comment {
        comment_id,
        body_digest: wrong,
      },
      FlowIssueMaterialReference::IssueDocument {
        artifact: FactoryArtifactReference::new(artifact.artifact_id(), wrong, artifact.encoded_size()).unwrap(),
      },
      FlowIssueMaterialReference::IssueDocument {
        artifact: FactoryArtifactReference::new(
          artifact.artifact_id(),
          artifact.content_digest(),
          artifact.encoded_size() + 1,
        )
        .unwrap(),
      },
      // Claiming Project ownership cannot promote an approved Issue publication.
      FlowIssueMaterialReference::ProjectDocument {
        artifact: artifact.clone(),
      },
      FlowIssueMaterialReference::IssueDocument {
        artifact: f.shared.artifact.clone(),
      },
    ] {
      let id = FactoryConfigurationId::generate();
      let configuration = material_configuration(
        f.issue.project_id,
        id,
        FlowIssueMaterialSource::Exact {
          references: vec![source],
        },
        true,
      );
      crate::factory::admission::tests::factory_admission_tests::seed_permitted_configuration(
        &f.factory,
        PublishedFactoryConfiguration {
          configuration,
          published_at: at(46),
        },
      )
      .unwrap();
      let mut command = f.command.clone();
      command.intent.target.configuration_id = id;
      let before = f.bytes.reads.lock().unwrap().len();
      let policy = Arc::new(TrustedNetworkManagementPolicy);
      let error = AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(
          IssueExecutionHandlers::new(f.issues.clone(), f.factory.clone(), policy.clone())
            .with_materials(f.readers(policy)),
        ),
      )
      .handle_command(&context(), command)
      .await
      .expect_err("an exact selection must match its approved source/owner pin");
      let Some(ApplicationError::FactoryValidation(report)) = error.application() else {
        panic!("{error:?}")
      };
      assert_eq!(report.errors[0].code, "issue_snapshot_material_unavailable");
      assert_eq!(f.bytes.reads.lock().unwrap().len(), before);
      f.no_runs().await;
    }
  });
}

#[test]
fn issue_material_snapshot_rejects_corrupted_required_document_bytes() {
  run_ready(async {
    for shared in [false, true] {
      let f = MaterialFixture::new(false).await;
      let reader = f.readers(Arc::new(TrustedNetworkManagementPolicy));
      f.review(reader.clone()).await.unwrap(); // establishes valid original input
      *f.bytes.corrupt.lock().unwrap() = Some(if shared {
        f.shared.artifact.artifact_id()
      } else {
        f.owned.artifact.artifact_id()
      });
      let mut command = f.command.clone();
      command.idempotency_key = key("corrupt-source-review");
      let policy = Arc::new(TrustedNetworkManagementPolicy);
      let error = AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(IssueExecutionHandlers::new(f.issues.clone(), f.factory.clone(), policy).with_materials(reader)),
      )
      .handle_command(&context(), command)
      .await
      .expect_err("changed raw bytes must not match an immutable source");
      let Some(ApplicationError::FactoryValidation(report)) = error.application() else {
        panic!("{error:?}")
      };
      assert_eq!(report.errors[0].code, "issue_snapshot_material_unavailable");
      f.no_runs().await;
    }
  });
}

#[path = "issue_execution_snapshot_material_expiry.rs"]
mod expiry;

#[test]
fn issue_material_snapshot_cannot_select_another_issues_document_in_the_same_project() {
  run_ready(async {
    let mut f = MaterialFixture::new(false).await;
    let other = f
      .issues
      .create_issue(management_mutation(CreateIssue {
        id: IssueId::generate(),
        assignees: Default::default(),
        intent: CreateIssueIntent {
          project_id: f.issue.project_id,
          content: f.issue.content.clone(),
          planning: Default::default(),
        },
        idempotency_key: key("other-document-owner"),
        created_at: at(40),
      }))
      .await
      .unwrap()
      .issue;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let foreign = AuthorizedCommandHandler::new(policy.clone(), f.owner(policy.clone()))
      .handle_command(
        &context(),
        PublishIssueDocumentCommand {
          submission: IssueDocumentSubmission {
            project_id: f.issue.project_id,
            issue_id: other.id,
            name: "Other Issue notes".parse().unwrap(),
            content: "OTHER_ISSUE_PRIVATE_TEXT".into(),
            idempotency_key: key("other-owned-publication"),
          },
        },
      )
      .await
      .unwrap();
    let before = f.bytes.reads.lock().unwrap().len();
    let selection = AuthorizedCommandHandler::new(
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
          references: vec![IssueMaterialReference::IssueDocument {
            project_id: f.issue.project_id,
            issue_id: f.issue.id,
            artifact: foreign.artifact.clone(),
          }],
        },
        idempotency_key: key("forge-same-project-parent"),
      },
    )
    .await;
    assert!(
      selection.is_err(),
      "Project visibility cannot transfer a publication's parent"
    );
    select_contract(
      &mut f,
      FlowIssueMaterialSelection {
        source: FlowIssueMaterialSource::Exact {
          references: vec![FlowIssueMaterialReference::IssueDocument {
            artifact: foreign.artifact,
          }],
        },
        required: true,
        max_bytes: 4096,
      },
    )
    .await;
    let error = f
      .review(f.readers(policy))
      .await
      .expect_err("another Issue's publication is not an approved local source");
    let Some(ApplicationError::FactoryValidation(report)) = error.application() else {
      panic!("{error:?}")
    };
    assert_eq!(report.errors[0].code, "issue_snapshot_material_unavailable");
    assert_eq!(f.bytes.reads.lock().unwrap().len(), before);
    f.no_runs().await;
  });
}
