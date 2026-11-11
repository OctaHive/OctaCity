//! Scheduling is chosen by the pinned consumer policy, never a second caller assessment.
use super::*;
#[path = "issue_execution_snapshot_policy_readiness.rs"]
mod readiness;
use crate::factory::admission::tests::factory_admission_tests::{
  configuration_with_flow, seed_permitted_configuration,
};

async fn policy_fixture() -> Fixture {
  let (store, factory, issue, mut command) = snapshot_fixture().await;
  let original = factory
    .factory_admission_context(ReadFactoryAdmissionContext {
      project_id: issue.project_id,
      configuration_id: command.intent.target.configuration_id,
      configuration_version: command.intent.target.configuration_version,
      repository_id: Some(command.intent.target.repository_id),
      repository_version: Some(command.intent.target.repository_version),
    })
    .await
    .unwrap();
  let mut flow = serde_json::to_value(original.configuration.configuration.flow()).unwrap();
  flow["issue_snapshot"]["admission"] = serde_json::json!({
    "priority": {"field":"priority", "values":{"high":80,"medium":40}},
    "risk": {"field":"size", "values":{"small":"low","medium":"medium","large":"high"}},
    "allowed": {"size":["small","medium"]}
  });
  let flow: FactoryFlowConfiguration =
    serde_json::from_value(flow).expect("an authored snapshot can pin its admission mapping");
  let id = FactoryConfigurationId::generate();
  seed_permitted_configuration(
    &factory,
    PublishedFactoryConfiguration {
      configuration: configuration_with_flow(issue.project_id, id, Some(flow)),
      published_at: at(36),
    },
  )
  .unwrap();
  command.intent.target.configuration_id = id;
  (store, factory, issue, command)
}

#[test]
fn issue_snapshot_policy_owns_scheduling_instead_of_the_publishers_rescore() {
  run_ready(async {
    let (store, factory, issue, mut command) = policy_fixture().await;
    command.intent.classification = Some(WorkClassification::new(
      WorkPriority::new(1).unwrap(),
      RiskClass::High,
      serde_json::from_value(serde_json::json!({"accepted":"true","admission_risk":"low"})).unwrap(),
    ));
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let requests = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionHandlers::new(store.clone(), factory.clone(), policy)),
    );
    let request = requests.handle_command(&context(), command).await.unwrap().request;
    // This helper publishes using a different ordinary classification, priority 20.
    let published = publish_snapshot_request(store, factory.clone(), &request).await;
    let run = factory
      .factory_run_snapshot(published.association.unwrap().run_id)
      .await
      .unwrap();
    assert_eq!(run.work.priority().get(), 80);
    assert_eq!(run.work.risk(), RiskClass::Low);
    assert!(run.work.metadata().is_empty());
    assert_eq!(run.flow.data.incoming[0].payload().value()["priority"], "high");
    let retained = serde_json::to_value(&request).unwrap();
    assert_eq!(
      retained["admission_classification"],
      serde_json::json!({"priority":80,"risk":"low","metadata":{}})
    );
    assert_eq!(issue.content.priority, Some(IssuePriority::Choice("high".into())));
  });
}

#[test]
fn issue_snapshot_policy_rejects_a_rescored_envelope_at_the_final_admission_boundary() {
  run_ready(async {
    let (store, factory, issue, command) = policy_fixture().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let requests = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionHandlers::new(store.clone(), factory.clone(), policy)),
    );
    let request = requests.handle_command(&context(), command).await.unwrap().request;
    publish_snapshot_request(store, factory.clone(), &request).await;
    let scope = context().security_scope().to_store();
    let mut preparation = factory
      .project_text_preparation(issue.project_id, &scope, &request.admission_key())
      .await
      .unwrap()
      .unwrap();
    let documents = factory
      .project_text_documents(issue.project_id, &scope, &request.admission_key())
      .await
      .unwrap()
      .unwrap();
    // Simulate a different adapter that changes both intent and Work consistently.
    // Generic envelope equality cannot detect this: the policy result is the authority.
    preparation.intent.classification =
      WorkClassification::new(WorkPriority::new(1).unwrap(), RiskClass::High, Default::default());
    let intent = preparation.admission_intent().unwrap();
    let selected = factory
      .factory_admission_context(preparation.intent.target)
      .await
      .unwrap();
    let configuration = &selected.configuration.configuration;
    let work = WorkEnvelope::new(
      WorkEnvelopeId::generate(),
      configuration.reference().clone(),
      intent.external_identity.clone(),
      ExactSubject::new(
        intent.project_id,
        intent.repository_id.unwrap(),
        preparation.frozen_revision.clone().unwrap(),
      ),
      intent.artifacts.clone(),
      intent.classification.clone(),
    )
    .unwrap();
    let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
    let flow = AdmittedFlow::from_configuration(configuration, &run).unwrap();
    let snapshot = documents.issue_snapshot().unwrap();
    let incoming = snapshot
      .incoming(&work, &flow, flow.data_schema(&snapshot.input.schema).unwrap())
      .unwrap();
    let admission = AdmitFactoryWork {
      probe: FactoryAdmissionProbe {
        source_scope: FactoryWorkSourceScope {
          source: "manual".parse().unwrap(),
          security_scope: scope,
        },
        external_identity: intent.external_identity.clone(),
        idempotency_key: intent.idempotency_key.clone(),
        intent_digest: factory_admission_intent_digest(&intent, "octacity.factory.manual-admission.v1"),
      },
      repository_version: intent.repository_version,
      work,
      run,
      flow,
      incoming: Some(incoming),
      admitted_at: intent.admitted_at,
    };
    assert!(
      matches!(
        snapshot.validate_prepared_admission(&preparation, &admission),
        Err(StoreError::Conflict { .. })
      ),
      "a consistently forged envelope must not override the frozen policy decision"
    );
  });
}
