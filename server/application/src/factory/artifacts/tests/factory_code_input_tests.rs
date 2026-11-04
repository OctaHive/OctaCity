//! Frozen node resources must not inherit authority from Work correlation.
use super::*;
use octacity_server_factory::{FlowCodeBinding, FlowNodeInput, FlowPayload};

#[test]
fn explicit_no_code_input_retains_work_correlation_and_roundtrips_without_a_repository_resource() {
  run_ready(async {
    for repository_work in [false, true] {
      let snapshot = resource_snapshot(repository_work).await;
      let flow = &snapshot.admitted_flow;
      let root = flow.closure().definition(flow.closure().root()).unwrap();
      let node = root
        .nodes()
        .iter()
        .find(|node| node.code() == Some(&FlowCodeBinding::None))
        .unwrap();
      let schema = flow.data_schema(node.input_schema().unwrap()).unwrap();
      let input = FlowNodeInput::new(
        &snapshot.work,
        flow,
        flow.root_run(),
        flow.initial_cycle(),
        node.key().clone(),
        FlowPayload::new(schema, serde_json::json!(true)).unwrap(),
      );
      assert!(
        input.is_ok(),
        "explicit no-code execution needs no exact repository subject: {input:?}"
      );
      let input = input.unwrap();
      let wire = serde_json::to_value(&input).unwrap();
      assert_eq!(wire["schema_version"], 2);
      assert_eq!(wire["subject"], serde_json::to_value(snapshot.work.subject()).unwrap());
      assert_eq!(wire["code"], serde_json::json!({"kind":"none"}));
      let retry = input.retry().unwrap();
      let bytes = serde_json::to_vec(&retry).unwrap();
      let restored = FlowNodeInput::restore(
        &bytes,
        &snapshot.work,
        flow,
        flow.root_run(),
        flow.initial_cycle(),
        schema,
        retry.digest().unwrap(),
      )
      .unwrap();
      assert_eq!(restored, retry);
      assert_eq!(
        serde_json::to_value(restored).unwrap()["code"],
        serde_json::json!({"kind":"none"})
      );
    }
  });
}

async fn resource_snapshot(repository_work: bool) -> octacity_server_store::FactoryRunSnapshot {
  let CodeFixture {
    store,
    repositories,
    mut request,
    ..
  } = code_fixture().await;
  if repository_work {
    let selected = repository(
      request.project_id,
      RepositoryId::generate(),
      Some("refs/heads/main".parse().unwrap()),
    );
    request.repository_id = Some(selected.id);
    request.repository_version = Some(selected.version);
    request.source = RepositorySourceSelection::DefaultReference.into();
    store.seed_repository_version(selected).unwrap();
  }
  let handlers = FactoryAdmissionHandlers::new(store.clone(), Arc::new(RecordingResolver::default()))
    .with_repositories(repositories);
  let admitted = admit(&handlers, request).await.unwrap();
  store.factory_run_snapshot(admitted.factory_run_id).await.unwrap()
}

#[test]
fn work_repository_correlation_cannot_authorize_a_candidate_for_a_no_code_step() {
  use octacity_server_factory::{
    CandidateSubject, ChangeSet, ChangeSetId, FactoryClaim, FactoryClaimFence, FactoryClaimOwnership, NodeAttempt,
    NodeAttemptId, NodeAttemptInput, NodeAttemptNumber, NodeExecutionIdentity,
  };
  run_ready(async {
    let snapshot = resource_snapshot(true).await;
    let flow = &snapshot.admitted_flow;
    let root = flow.closure().definition(flow.closure().root()).unwrap();
    let node = root
      .nodes()
      .iter()
      .find(|node| node.code() == Some(&FlowCodeBinding::None))
      .unwrap();
    let schema = flow.data_schema(node.input_schema().unwrap()).unwrap();
    let input = FlowNodeInput::new(
      &snapshot.work,
      flow,
      flow.root_run(),
      flow.initial_cycle(),
      node.key().clone(),
      FlowPayload::new(schema, serde_json::json!(true)).unwrap(),
    )
    .unwrap();
    let attempt = NodeAttempt::new(
      flow.root_run(),
      flow.initial_cycle(),
      root,
      NodeAttemptInput {
        id: NodeAttemptId::generate(),
        node_key: node.key().clone(),
        node_kind: node.kind(),
        number: NodeAttemptNumber::INITIAL,
        input_digest: input.digest().unwrap(),
        budget: node.budget(),
        started_at: time(10),
        deadline: time(20),
        execution: NodeExecutionIdentity::External(node.action().unwrap().plugin().clone()),
        ownership: FactoryClaimOwnership::new(
          key("worker"),
          FactoryClaim::new(FactoryClaimFence::new(digest(99)), time(10), time(20)).unwrap(),
        ),
      },
    )
    .unwrap();
    input.verify_attempt(&attempt).unwrap();
    let candidate = CandidateSubject::new(
      snapshot.work.subject().repository().unwrap().clone(),
      "candidate-derived-from-work".parse().unwrap(),
      digest(98),
    );
    let changes = ChangeSet::new(
      ChangeSetId::generate(),
      &input,
      &attempt,
      candidate,
      ArtifactId::generate(),
      ArtifactId::generate(),
    );
    assert!(
      changes.is_err(),
      "Work's repository metadata must never become an undeclared step resource"
    );
  });
}

#[test]
fn frozen_step_input_uses_its_own_admitted_source_instead_of_the_work_repository() {
  run_ready(async {
    let snapshot = resource_snapshot(true).await;
    let flow = &snapshot.admitted_flow;
    let root = flow.closure().definition(flow.closure().root()).unwrap();
    for alias in ["first", "second"] {
      let node = root
        .nodes()
        .iter()
        .find(|node| matches!(node.code(),Some(FlowCodeBinding::Source {source,..}) if source.as_str() == alias))
        .unwrap();
      let schema = flow.data_schema(node.input_schema().unwrap()).unwrap();
      let input = FlowNodeInput::new(
        &snapshot.work,
        flow,
        flow.root_run(),
        flow.initial_cycle(),
        node.key().clone(),
        FlowPayload::new(schema, serde_json::json!(true)).unwrap(),
      );
      assert!(
        input.is_ok(),
        "a selected code resource must prepare independently of Work's repository: {input:?}"
      );
      let input = input.unwrap();
      let declared = &root.code_sources().unwrap()[&key(alias)];
      assert_eq!(input.exact_resource().unwrap().repository_id(), declared.repository.id);
      assert_eq!(
        input.exact_resource().unwrap().base_revision().as_str(),
        "0123456789abcdef"
      );
      assert_ne!(
        input.exact_resource().unwrap().repository_id(),
        snapshot.work.subject().repository().unwrap().repository_id()
      );
      assert_eq!(input.subject(), snapshot.work.subject());
      let wire = serde_json::to_value(&input).unwrap();
      assert_eq!(wire["schema_version"], 2);
      assert_eq!(wire["code"]["kind"], "source");
      assert_eq!(wire["code"]["source"], alias);
      assert_eq!(wire["code"]["subject"]["kind"], "exact");
      let retry = input.retry().unwrap();
      let restored = FlowNodeInput::restore(
        &serde_json::to_vec(&retry).unwrap(),
        &snapshot.work,
        flow,
        flow.root_run(),
        flow.initial_cycle(),
        schema,
        retry.digest().unwrap(),
      )
      .unwrap();
      assert_eq!(restored, retry);
      assert_eq!(restored.exact_resource().unwrap(), input.exact_resource().unwrap());
    }
  });
}

#[test]
fn exact_step_revision_needs_no_unselected_default_ref_and_survives_retry() {
  run_ready(async {
    let CodeFixture {
      store,
      repositories,
      request,
      ..
    } = code_fixture_with_resources(
      octacity_server_factory::FlowCodeRevision::ExactRevision {
        revision: "reviewed-only".parse().unwrap(),
      },
      octacity_server_factory::FlowCodeScope::Repository,
    )
    .await;
    let resolver = Arc::new(RecordingResolver::default());
    let handlers = FactoryAdmissionHandlers::new(store.clone(), resolver.clone()).with_repositories(repositories);
    let admission = admit(&handlers, request).await.unwrap();
    assert_eq!(
      resolver.call_count(),
      0,
      "an exact step selection must not resolve its unused default branch"
    );
    let snapshot = store.factory_run_snapshot(admission.factory_run_id).await.unwrap();
    let flow = &snapshot.admitted_flow;
    assert!(
      flow.code_pins().is_empty(),
      "exact step literals are already frozen in their immutable definition"
    );
    let root = flow.closure().definition(flow.closure().root()).unwrap();
    for alias in ["first", "second"] {
      let node = root
        .nodes()
        .iter()
        .find(|node| matches!(node.code(),Some(FlowCodeBinding::Source {source,..}) if source.as_str() == alias))
        .unwrap();
      let schema = flow.data_schema(node.input_schema().unwrap()).unwrap();
      let input = FlowNodeInput::new(
        &snapshot.work,
        flow,
        flow.root_run(),
        flow.initial_cycle(),
        node.key().clone(),
        FlowPayload::new(schema, serde_json::json!(true)).unwrap(),
      )
      .unwrap();
      assert_eq!(
        input.exact_resource().unwrap().base_revision().as_str(),
        "reviewed-only"
      );
      assert_eq!(
        input.exact_resource().unwrap().repository_id(),
        root.code_sources().unwrap()[&key(alias)].repository.id
      );
      let retry = input.retry().unwrap();
      let restored = FlowNodeInput::restore(
        &serde_json::to_vec(&retry).unwrap(),
        &snapshot.work,
        flow,
        flow.root_run(),
        flow.initial_cycle(),
        schema,
        retry.digest().unwrap(),
      )
      .unwrap();
      assert_eq!(restored, retry);
    }
    assert_eq!(resolver.call_count(), 0);
  });
}

#[path = "factory_code_context_tests.rs"]
mod context_resources;

#[path = "factory_candidate_resource_tests.rs"]
mod candidate_resources;
