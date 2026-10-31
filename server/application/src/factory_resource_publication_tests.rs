//! Publication control retains the declared candidate rather than Work's optional repository.
use super::*;
use crate::*;
use std::sync::Mutex;

#[derive(Default)]
struct CancellationProbe(Mutex<Vec<FactoryCandidatePublicationRequest>>);
#[async_trait::async_trait]
impl FactoryCandidatePublisher for CancellationProbe {
  async fn observe(
    &self,
    _: &FactoryCandidatePublicationRequest,
  ) -> Result<Option<FactoryCandidatePublication>, ApplicationError> {
    panic!("cancellation must not observe or dispatch publication")
  }
  async fn publish(
    &self,
    _: &FactoryCandidatePublicationRequest,
  ) -> Result<FactoryCandidatePublication, ApplicationError> {
    panic!("cancellation must not publish")
  }
  async fn cancel(
    &self,
    request: &FactoryCandidatePublicationRequest,
    _: Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError> {
    self.0.lock().unwrap().push(request.clone());
    Ok(Some(BudgetUsage::default()))
  }
}

#[test]
fn candidate_publication_control_binds_the_selected_resource_without_a_work_repository() {
  run_ready(async {
    let fixture = candidate_fixture(None).await;
    let input = FlowNodeInput::prepare(fixture.preparation()).unwrap();
    let subject = input.task_subject().unwrap();
    let entries = [("bundle", artifact(150)), ("manifest", artifact(151))]
      .into_iter()
      .map(|(alias, artifact)| {
        ContextManifestEntry::new(
          ContextSourceKind::Evidence,
          key(alias),
          subject.clone(),
          FactoryContextReference::Artifact(artifact.clone()),
          artifact.content_digest(),
          artifact.encoded_size(),
          FactorySafeText::new("Explicit captured candidate input").unwrap(),
          digest(152),
        )
        .unwrap()
      })
      .collect();
    let input = input
      .with_context(
        ContextManifest::new(ContextManifestId::generate(), subject, digest(153), entries).unwrap(),
        vec![],
      )
      .unwrap();
    let plugin = exact("publisher", 154);
    let binding = FlowActionBinding::new(
      plugin.clone(),
      key("publish_candidate"),
      serde_json::json!({
        "bundle":"bundle", "manifest":"manifest", "target":exact("review-target",155),
      }),
    )
    .unwrap();
    let schema = fixture
      .snapshot
      .admitted_flow
      .data_schema(input.payload().schema())
      .unwrap();
    let outcome = FlowOutcomeDefinition::new(key("published"), FlowOutcomeKind::Success, schema.reference().clone());
    let publisher = Arc::new(CancellationProbe::default());
    let action = FactoryCandidatePublicationAction::new(plugin, publisher.clone(), outcome);
    let permissions = FactoryPermissionSet::deny_all();
    let result = action
      .cancel(
        FactoryFlowActionRequest {
          operation_id: digest(156),
          work_id: fixture.snapshot.work.id(),
          node_attempt_id: NodeAttemptId::generate(),
          external_identity: fixture.snapshot.work.external_identity(),
          subject: fixture.snapshot.work.subject(),
          binding: &binding,
          input: &input,
          schemas: &[],
          permissions: &permissions,
          budget: fixture.snapshot.admitted_flow.limits().budget(),
          deadline: time(100),
          started_at: time(30),
          authority_expires_at: time(100),
          observed_at: time(33),
        },
        time(33),
      )
      .await;
    assert!(
      result.is_ok(),
      "publication controls retain selected code provenance: {result:?}"
    );
    let requests = publisher.0.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].candidate, fixture.candidate);
    assert_eq!(requests[0].accepted_input_digest, input.digest().unwrap());
    assert_eq!(requests[0].work_id, fixture.snapshot.work.id());
    assert!(input.subject().repository().is_err());
  });
}
