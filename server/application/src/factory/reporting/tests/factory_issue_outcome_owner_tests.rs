//! Published Issue outcome fixtures read exact references from retained owners.
use super::*;
use crate::factory_flow_tests::{Inputs, configured_fixture_with_work};
use crate::factory_node_test_support::{clock, fixtures};
use octacity_server_factory::{
  ExactSubject, FactoryClaim, FactoryClaimFence, FactoryConfigurationRef, FactoryDigest, FactoryKey, FlowDataMapping,
  FlowGateProgram, FlowIssuePresentation, FlowValueMapping, WorkClassification, WorkEnvelope, WorkEnvelopeId,
};
use octacity_server_store::{
  AuditActorKind, ClaimFactoryRun, FactoryAuditFact, FactoryRunClaimRecord, FactoryRunStore, FactoryWorkSourceScope,
  testing::InMemoryFactoryConfigurationStore,
};
use serde_json::{Value, json};

#[test]
fn published_issue_outcomes_use_accepted_producer_records_with_owner_references() {
  run_ready(async {
    let issues = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&issues).await;
    let request = context();
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let issue = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueHandlers::new(issues.clone(), policy.clone())),
    )
    .handle_command(&request, create(project_id, "reviewed-specification"))
    .await
    .unwrap()
    .issue;
    let reader = Arc::new(Reader {
      hidden_repository: AtomicBool::new(false),
      revoked: AtomicBool::new(false),
      reads: AtomicUsize::new(0),
    });
    let reviews = Arc::new(IssueCodeReviewHandlers::new(
      issues.clone(),
      reader.clone(),
      policy.clone(),
      Arc::new(Clock),
    ));
    let review = AuthorizedCommandHandler::new(policy.clone(), reviews.clone())
      .handle_command(
        &request,
        AttachIssueCodeReviewCommand {
          intent: AttachIssueCodeReviewIntent {
            project_id,
            issue_id: issue.id,
            expected_version: issue.version,
            selection: selection(),
            binding: None,
          },
          idempotency_key: key("reviewed-specification-link"),
        },
      )
      .await
      .unwrap()
      .association;
    AuthorizedCommandHandler::new(policy.clone(), reviews.clone())
      .handle_command(
        &request,
        RefreshIssueCodeReviewCommand {
          project_id,
          issue_id: issue.id,
          association_id: review.id,
          idempotency_key: key("rename-outcome-review"),
        },
      )
      .await
      .unwrap();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project_id).unwrap();
    let document = ProjectTextIntake::new(
      store.clone(),
      Arc::new(crate::factory::execution::factory_test_support::InMemoryArtifactBytes::default()),
      Arc::new(ExactRevisionResolver),
      policy.clone(),
      Arc::new(Clock),
    )
    .publish_document(
      &request,
      ProjectDocumentSubmission {
        purpose: Default::default(),
        project_id,
        name: "Accepted design".parse().unwrap(),
        content: "Store one accepted review per release.".into(),
        idempotency_key: key("reviewed-specification-document"),
      },
    )
    .await
    .unwrap();

    let work = fixtures::work();
    let scope = crate::issues::issue_outcome_links::IssueOutcomeLinkScope {
      project: project_id,
      issue: issue.id,
      work_id: work.id(),
      inputs: work.artifacts(),
    };
    let selected = [
      octacity_server_store::IssueOutcomeReference::Document(document.artifact.clone()),
      octacity_server_store::IssueOutcomeReference::Review(review.id),
    ];
    let links = IssueOutcomeLinkReaders::new(
      store.clone(),
      Arc::new(LinkArtifacts(Some(at(30)), IssueArtifactOrigin::BuildOutput)),
      reviews.clone(),
      Arc::new(Clock),
    );
    let visible = links
      .resolve(&request, policy.as_ref(), scope, &selected)
      .await
      .unwrap();
    assert_eq!(
      visible,
      vec![
        IssueOutcomeLink {
          title: Some("Accepted design".into()),
          target: IssueOutcomeLinkTarget::Document(document.artifact.artifact_id()),
        },
        IssueOutcomeLink {
          title: Some("Renamed code change".into()),
          target: IssueOutcomeLinkTarget::Review(review.id),
        },
      ]
    );
    let foreign = project(&issues).await;
    assert!(
      links
        .resolve(
          &request,
          policy.as_ref(),
          crate::issues::issue_outcome_links::IssueOutcomeLinkScope {
            project: foreign,
            ..scope
          },
          &selected
        )
        .await
        .unwrap()
        .is_empty()
    );
    reader.hidden_repository.store(true, Ordering::SeqCst);
    assert_eq!(
      links
        .resolve(&request, policy.as_ref(), scope, &selected)
        .await
        .unwrap(),
      visible[..1]
    );
    reader.hidden_repository.store(false, Ordering::SeqCst);
    reader.revoked.store(true, Ordering::SeqCst);
    assert_eq!(
      links
        .resolve(&request, policy.as_ref(), scope, &selected)
        .await
        .unwrap(),
      visible[..1]
    );
    assert_eq!(
      reader.reads.load(Ordering::SeqCst),
      2,
      "link resolution never refreshes the provider"
    );
    let expired = IssueOutcomeLinkReaders::new(
      store.clone(),
      Arc::new(LinkArtifacts(Some(at(30)), IssueArtifactOrigin::BuildOutput)),
      reviews.clone(),
      Arc::new(LinkClock(document.expires_at)),
    );
    assert_eq!(
      expired
        .resolve(&request, policy.as_ref(), scope, &selected)
        .await
        .unwrap(),
      vec![IssueOutcomeLink {
        title: Some("Accepted design".into()),
        target: IssueOutcomeLinkTarget::Unavailable
      },]
    );
    let output = octacity_server_factory::FactoryArtifactReference::new(
      octacity_server_domain::ArtifactId::generate(),
      FactoryDigest::from_bytes([5; 32]),
      100,
    )
    .unwrap();
    let artifact = octacity_server_store::IssueOutcomeReference::Artifact(output.clone());
    assert_eq!(
      links
        .resolve(&request, policy.as_ref(), scope, std::slice::from_ref(&artifact))
        .await
        .unwrap(),
      vec![IssueOutcomeLink {
        title: Some("Retained output".into()),
        target: IssueOutcomeLinkTarget::Artifact(output.artifact_id())
      },]
    );
    assert_eq!(
      expired
        .resolve(&request, policy.as_ref(), scope, std::slice::from_ref(&artifact))
        .await
        .unwrap(),
      vec![IssueOutcomeLink {
        title: Some("Retained output".into()),
        target: IssueOutcomeLinkTarget::Unavailable,
      }]
    );
    assert!(
      links
        .resolve(
          &request,
          &HideLinkResource(ManagementResourceKind::Build),
          scope,
          std::slice::from_ref(&artifact)
        )
        .await
        .unwrap()
        .is_empty()
    );
    let private = IssueOutcomeLinkReaders::new(
      store.clone(),
      Arc::new(LinkArtifacts(Some(at(30)), IssueArtifactOrigin::SourceInput)),
      reviews.clone(),
      Arc::new(Clock),
    );
    let task = work.artifacts().task();
    assert_eq!(
      private
        .resolve(
          &request,
          policy.as_ref(),
          scope,
          &[
            octacity_server_store::IssueOutcomeReference::Artifact(task.clone()),
            artifact.clone(),
          ]
        )
        .await
        .unwrap(),
      vec![IssueOutcomeLink {
        title: Some("Retained output".into()),
        target: IssueOutcomeLinkTarget::WorkArtifact {
          work_id: work.id(),
          artifact_id: task.artifact_id()
        },
      }],
    );
    let unpublished = IssueOutcomeLinkReaders::new(
      store.clone(),
      Arc::new(LinkArtifacts(None, IssueArtifactOrigin::BuildOutput)),
      reviews,
      Arc::new(LinkClock(document.expires_at)),
    );
    assert!(
      unpublished
        .resolve(&request, policy.as_ref(), scope, std::slice::from_ref(&artifact))
        .await
        .unwrap()
        .is_empty(),
      "expiry must not make an unpublished artifact visible"
    );
    let hidden = HideLinkResource(ManagementResourceKind::Artifact);
    assert!(
      links
        .resolve(&request, &hidden, scope, &[selected[0].clone(), artifact])
        .await
        .unwrap()
        .is_empty()
    );

    for (kind, value) in [
      (
        "specifications",
        json!({"conclusion":"The specification is ready.","documents":[document.artifact]}),
      ),
      (
        "review",
        json!({"conclusion":"A regression test is missing.","verdict":"changes_required","review":review.id}),
      ),
      ("custom", json!({"conclusion":"Acceptance checks are ready."})),
    ] {
      assert_accepted_outcome(&store, project_id, kind, value).await;
    }
  });
}

async fn assert_accepted_outcome(
  store: &Arc<InMemoryFactoryConfigurationStore>,
  project_id: ProjectId,
  kind: &str,
  value: Value,
) {
  let mut template = fixtures::supplied_flow_template("issue-outcomes-flow.json");
  let producer = template
    .nodes
    .iter_mut()
    .find(|node| node.key == template.entry)
    .unwrap();
  producer.gate = Some(
    FlowGateProgram::new(vec![], producer.outcomes[0].key.clone())
      .unwrap()
      .with_default_output_mapping(
        FlowDataMapping::new(FlowValueMapping::Constant {
          value: json!({kind:value}),
        })
        .unwrap(),
      )
      .unwrap(),
  );
  let fixture = fixtures::work();
  let configuration = fixture.configuration();
  let subject = fixture.subject().repository().unwrap();
  let work = WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    FactoryConfigurationRef::new(
      configuration.id(),
      configuration.version(),
      project_id,
      configuration.definition_digest(),
    ),
    kind.parse().unwrap(),
    ExactSubject::new(project_id, subject.repository_id(), subject.base_revision().clone()),
    fixture.artifacts().clone(),
    WorkClassification::new(fixture.priority(), fixture.risk(), Default::default()),
  )
  .unwrap();
  let (admission, execution) = configured_fixture_with_work(template, work);
  let run = admission.run.id();
  store
    .seed_factory_admission(
      admission.clone(),
      FactoryWorkSourceScope {
        source: "manual".parse().unwrap(),
        security_scope: context().security_scope().to_store(),
      },
      FactoryDigest::content_sha256(kind.as_bytes()),
      management_mutation(()).audit(),
    )
    .unwrap();
  let claim = FactoryRunClaimRecord::new(
    run,
    "outcome-worker".parse().unwrap(),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([42; 32])),
      at(10),
      at(900),
    )
    .unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run,
      expected_version: admission.run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        run,
        AuditActorKind::Worker,
        None,
        "factory.claimed".parse().unwrap(),
        FactoryDigest::from_bytes([1; 32]),
        "accepted".parse().unwrap(),
        at(10),
      ),
    })
    .await
    .unwrap();
  let runner = FactoryFlowRunner::new(
    store.clone(),
    Arc::new(FactoryNodeBuildAdapter::new(execution.clone(), execution)),
    Arc::new(Inputs),
    clock(),
  );
  let mut resolved = None;
  for time in 31..45 {
    match runner.reconcile_flow(run, claim.id, at(time)).await.unwrap() {
      FactoryFlowStep::Advanced => {}
      FactoryFlowStep::Resolved(outcome) => {
        resolved = Some(outcome);
        break;
      }
      other => panic!("ordinary admission must resolve the accepted producer record, got {other:?}"),
    }
  }
  assert_eq!(resolved, Some(kind.parse::<FactoryKey>().unwrap()));
  let snapshot = store.factory_run_snapshot(run).await.unwrap();
  assert!(snapshot.flow.data.incoming.is_empty());
  let records = &snapshot.flow.data.records;
  assert_eq!(records.len(), 2);
  let record_for = |node: &str| {
    records
      .iter()
      .find(|record| record.input().node().as_str() == node)
      .unwrap()
  };
  let producer = record_for("produce_outcome");
  let accepted = record_for("retain_outcome");
  let [source] = accepted.input().sources() else {
    panic!("expected one exact accepted producer");
  };
  assert_eq!(source.node_attempt_id(), producer.node_attempt_id());
  assert_eq!(source.digest(), producer.digest().unwrap());
  assert_eq!(accepted.payload().value(), &value);
  let declaration = snapshot
    .admitted_flow
    .closure()
    .definition(accepted.input().definition())
    .unwrap()
    .node(accepted.input().node())
    .unwrap()
    .outcome(accepted.outcome())
    .unwrap();
  match (kind, declaration.issue_presentation()) {
    ("specifications", Some(FlowIssuePresentation::SpecificationsV1 { slot: None }))
    | ("review", Some(FlowIssuePresentation::ReviewV1 { slot: None })) => {}
    (
      "custom",
      Some(FlowIssuePresentation::CustomV1 {
        slot: None,
        title,
        conclusion_field,
        references,
      }),
    ) => {
      assert_eq!(title.as_str(), "Acceptance checks");
      assert_eq!(conclusion_field.as_str(), "conclusion");
      assert!(references.is_empty());
    }
    other => panic!("unexpected Issue presentation: {other:?}"),
  }
}

struct LinkClock(Timestamp);
impl FactoryExecutionClock for LinkClock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(self.0)
  }
}
struct LinkArtifacts(Option<Timestamp>, IssueArtifactOrigin);
#[async_trait::async_trait]
impl IssueArtifactReader for LinkArtifacts {
  async fn metadata(&self, _: ReadFactoryArtifact) -> Result<IssueArtifactMetadata, ApplicationError> {
    Ok(IssueArtifactMetadata {
      build_owner: (self.1 == IssueArtifactOrigin::BuildOutput).then(|| {
        (
          octacity_server_domain::BuildId::from_uuid(uuid::Uuid::from_u128(1)).unwrap(),
          octacity_server_domain::JobId::from_uuid(uuid::Uuid::from_u128(2)).unwrap(),
        )
      }),
      name: "Retained output".into(),
      media_type: "text/plain".into(),
      origin: self.1,
      availability: IssueArtifactAvailability::Available,
      published_at: self.0,
      expires_at: Some(at(10_000)),
    })
  }

  async fn content(&self, _: ReadFactoryArtifact) -> Result<Vec<u8>, ApplicationError> {
    panic!("link resolution must never read artifact bytes")
  }
}
struct HideLinkResource(ManagementResourceKind);
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for HideLinkResource {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if resource.kind() == self.0 {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
  }
}
