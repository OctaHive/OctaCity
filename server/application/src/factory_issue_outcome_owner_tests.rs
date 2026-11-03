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
    let review = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueCodeReviewHandlers::new(
        issues,
        Arc::new(Reader {
          hidden_repository: AtomicBool::new(false),
          revoked: AtomicBool::new(false),
          reads: AtomicUsize::new(0),
        }),
        policy.clone(),
        Arc::new(Clock),
      )),
    )
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
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project_id).unwrap();
    let document = ProjectTextIntake::new(
      store.clone(),
      Arc::new(crate::factory_test_support::InMemoryArtifactBytes::default()),
      Arc::new(ExactRevisionResolver),
      policy,
      Arc::new(Clock),
    )
    .publish_document(
      &request,
      ProjectDocumentSubmission {
        project_id,
        name: "Accepted design".parse().unwrap(),
        content: "Store one accepted review per release.".into(),
        idempotency_key: key("reviewed-specification-document"),
      },
    )
    .await
    .unwrap();

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
