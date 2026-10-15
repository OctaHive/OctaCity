//! Public owner/store audit and redaction contract in an isolated tracing process.
fn captured_trace() -> Arc<std::sync::Mutex<Vec<u8>>> {
  static CAPTURE: std::sync::OnceLock<Arc<std::sync::Mutex<Vec<u8>>>> = std::sync::OnceLock::new();
  CAPTURE
    .get_or_init(|| {
      struct Capture(Arc<std::sync::Mutex<Vec<u8>>>);
      impl std::io::Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
          self.0.lock().unwrap().extend_from_slice(bytes);
          Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
          Ok(())
        }
      }
      let captured = Arc::new(std::sync::Mutex::new(Vec::new()));
      let writer = captured.clone();
      let subscriber = tracing_subscriber::fmt()
        .json()
        .without_time()
        .with_writer(move || Capture(writer.clone()))
        .finish();
      tracing::subscriber::set_global_default(subscriber).unwrap();
      captured
    })
    .clone()
}
use async_trait::async_trait;
use octacity_server_application::*;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store::*;
use std::{
  future::Future,
  sync::Arc,
  task::{Context, Poll, Waker},
};
struct Clock;
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(11))
  }
}
fn at(millis: i64) -> Timestamp {
  Timestamp::from_unix_millis(millis).unwrap()
}
fn run_ready<T>(future: impl Future<Output = T>) -> T {
  let mut future = std::pin::pin!(future);
  match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
    Poll::Ready(value) => value,
    Poll::Pending => panic!("in-memory contract unexpectedly yielded"),
  }
}
struct Pending;
#[async_trait]
impl FactoryNodeExecutor for Pending {
  async fn cancel(
    &self,
    _: FactoryNodeExecutionRequest<'_>,
    _: Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    unreachable!("this contract only freezes inputs")
  }
  async fn observe(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    let definition = request
      .admitted
      .closure()
      .definition(request.input.definition())
      .unwrap();
    let outcome = &definition.node(request.input.node()).unwrap().outcomes()[0];
    let record = FlowNodeRecord::new(
      request.input,
      request.attempt,
      definition,
      FlowRecordObservation {
        outcome: outcome.key().clone(),
        payload: FlowPayload::new(
          request.admitted.data_schema(outcome.schema()).unwrap(),
          serde_json::json!({"count":3}),
        )
        .unwrap(),
        producer: request.attempt.execution().clone(),
        observed_at: request.observed_at,
      },
    )
    .unwrap();
    let intent = request.build.unwrap();
    let profile = intent.profile().clone();
    let execution = FlowBuildExecution::new(
      intent,
      octacity_server_domain::BuildId::generate(),
      octacity_server_domain::AttemptId::generate(),
      vec![octacity_server_domain::JobId::generate()],
    )
    .unwrap();
    let bytes = br#"{"count":3}"#;
    let record = record
      .with_build_provenance(
        FlowBuildProvenance::new(
          profile.clone(),
          vec![FlowVerifiedBuildOutput {
            name: profile.result_output.clone(),
            output_kind: EvidenceOutputKind::Report,
            schema: outcome.schema().clone(),
            artifact: FactoryArtifactReference::new(
              octacity_server_domain::ArtifactId::generate(),
              FactoryDigest::content_sha256(bytes),
              bytes.len() as u64,
            )
            .unwrap(),
            producer: EvidenceProducer::new(
              execution.build_id(),
              execution.attempt_id(),
              execution.jobs()[0],
              profile.tool,
              profile.plugin,
            ),
            verified_at: request.observed_at,
            fresh_until: at(1000),
            data: None,
          }],
        )
        .unwrap(),
        definition,
      )
      .unwrap();
    Ok(FactoryNodeExecutionStep::Completed {
      execution: Some(Box::new(execution)),
      record: Box::new(record),
      usage: BudgetUsage {
        tokens: 7,
        elapsed_millis: 4,
        ..Default::default()
      },
      outbox: vec![],
    })
  }
}
#[test]
fn node_transition_audit_distinguishes_exact_frozen_contexts_at_the_same_run_version() {
  use octacity_server_store::testing;
  let captured = captured_trace();
  run_ready(async {
    let fixture = testing::factory_node_journal_contract_fixture();
    let context = fixture.input.context().unwrap();
    let entries = context.entries().iter().map(|entry| ContextManifestEntry::new(entry.source_kind(), entry.logical_identity().clone(), entry.subject().clone(), entry.source().clone(), entry.content_digest(), entry.encoded_size(), FactorySafeText::new("task-body private-command provider-payload finding-private-message private-prompt hidden-test-source /private/repository https://private.invalid provider-secret hidden-reasoning").unwrap(), entry.provenance_digest()).unwrap()).collect();
    let different_context = ContextManifest::new(
      ContextManifestId::generate(),
      context.subject().clone(),
      context.construction_policy_digest(),
      entries,
    )
    .unwrap();
    let different_input = fixture.input.clone().with_context(different_context, vec![]).unwrap();
    let mut identities = vec![];
    for input in [fixture.input.clone(), different_input] {
      let input_digest = input.digest().unwrap();
      let expected_context_policy = input.context().unwrap().construction_policy_digest();
      let expected_context_entries = input.context().unwrap().entries().len();
      let expected_permissions = fixture
        .admission
        .flow
        .closure()
        .definition(input.definition())
        .unwrap()
        .node(input.node())
        .unwrap()
        .permissions()
        .digest();
      let expected_generation = input.generation();
      let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
      let audit = testing::management_mutation_with_request((), "node-owner")
        .audit()
        .clone();
      store
        .seed_factory_run(fixture.admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
        .unwrap();
      let run = &fixture.admission.run;
      let claim = FactoryRunClaimRecord::new(run.id(), fixture.attempt.owner().clone(), fixture.attempt.claim());
      store
        .claim_factory_run(ClaimFactoryRun {
          run_id: run.id(),
          expected_version: run.version(),
          record: claim.clone(),
          audit: FactoryAuditFact::new(
            run.id(),
            AuditActorKind::Worker,
            None,
            FactoryKey::new("factory.claimed").unwrap(),
            FactoryDigest::from_bytes([1; 32]),
            FactoryKey::new("accepted").unwrap(),
            claim.claim.claimed_at(),
          ),
        })
        .await
        .unwrap();
      let target = FactoryNodeTarget {
        flow_run_id: input.flow_run_id(),
        cycle_id: input.cycle_id(),
        node: input.node().clone(),
      };
      let runner = FactoryNodeRunner::new(store.clone(), Arc::new(Pending), Arc::new(Clock));
      runner
        .run_once(run.id(), claim.id, target.clone(), Some(input), at(11))
        .await
        .unwrap();
      runner.run_once(run.id(), claim.id, target, None, at(15)).await.unwrap();
      let flow_owner = FactoryFlowRunner::new(
        store.clone(),
        Arc::new(Pending),
        Arc::new(UnusedInputs),
        Arc::new(Clock),
      );
      let terminal = fixture
        .admission
        .flow
        .closure()
        .definition(input_definition(&fixture))
        .unwrap()
        .terminals()[0]
        .key()
        .clone();
      assert_eq!(
        flow_owner.reconcile_flow(run.id(), claim.id, at(16)).await.unwrap(),
        FactoryFlowStep::Resolved(terminal.clone())
      );
      assert_eq!(
        flow_owner.reconcile_flow(run.id(), claim.id, at(17)).await.unwrap(),
        FactoryFlowStep::Resolved(terminal)
      );
      let page = store
        .list_factory_run_diagnostics(
          ListFactoryRunDiagnostics::new(run.id(), FactoryRunDiagnosticKind::Audit, None, 100).unwrap(),
        )
        .await
        .unwrap();
      let text = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
      assert!(
        text.contains("octacity.factory.transition.committed"),
        "committed transitions must emit safe structured diagnostics"
      );
      assert!(text.contains(&fixture.admission.run.id().to_string()));
      assert!(text.contains("factory.run.version"));
      assert!(text.contains("factory.input.digest"));
      assert!(text.contains("factory.context.digest"));
      assert!(text.contains("factory.execution.digest"));
      let frozen = text
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|event| {
          event["fields"]["event.name"] == "octacity.factory.input.accepted"
            && event["fields"]["factory.input.digest"] == input_digest.to_string()
        })
        .unwrap();
      // Diagnostics are ordered by digest, and several transitions share the
      // operation. Correlate the audit with this exact frozen input's commit.
      let audit = page
        .items
        .iter()
        .find_map(|row| match row {
          FactoryRunDiagnosticRecord::Audit(fact) if frozen["span"]["factory.audit.id"] == fact.id.to_string() => {
            Some(fact)
          }
          _ => None,
        })
        .expect("the frozen input trace must identify its committed audit fact");
      assert_eq!(audit.operation.as_str(), "factory.node.advance");
      assert_eq!(
        frozen["fields"]["factory.context.policy.digest"],
        expected_context_policy.to_string()
      );
      assert_eq!(frozen["fields"]["factory.context.entries"], expected_context_entries);
      assert_eq!(
        frozen["fields"]["factory.permissions.digest"],
        expected_permissions.to_string()
      );
      assert_eq!(frozen["fields"]["factory.input.generation"], expected_generation);
      assert_eq!(
        frozen["span"]["factory.configuration.digest"],
        fixture.admission.run.configuration().definition_digest().to_string()
      );

      for field in [
        "factory.node.attempt.id",
        "factory.node.state",
        "factory.node.attempt.number",
        "factory.budget.elapsed_ms",
        "factory.budget.tokens",
        "factory.budget.cost_micro_units",
        "factory.budget.output_bytes",
        "factory.node.deadline_ms",
        "factory.node.fence.digest",
      ] {
        assert!(text.contains(field), "committed node admission must expose {field}");
      }
      assert!(text.contains("octacity.factory.node.started"));
      let completed = text
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|event| {
          event["fields"]["event.name"] == "octacity.factory.node.completed"
            && event["span"]["factory.run.id"] == run.id().to_string()
        })
        .expect("terminal node state must be observable after commit");
      assert_eq!(completed["fields"]["factory.node.latency_ms"], 4);
      assert_eq!(completed["fields"]["factory.node.usage.tokens"], 7);
      assert_eq!(completed["fields"]["factory.node.state"], "completed");

      let transitions = events_for(&captured, run.id(), "octacity.factory.transition.committed");
      let completed_run = transitions
        .iter()
        .filter(|event| event["fields"]["factory.run.state"] == "completed")
        .collect::<Vec<_>>();
      // Each iteration independently starts and resolves one Run at the same version.
      assert_eq!(completed_run.len(), identities.len() + 1);
      let running = events_for(&captured, run.id(), "octacity.factory.flow.running");
      assert_eq!(
        running.len(),
        identities.len() + 1,
        "the initial admitted-to-running lifecycle change must be observable once"
      );
      assert_eq!(
        running.last().unwrap()["fields"]["factory.flow.id"],
        fixture.input.flow_run_id().to_string()
      );
      let resolved = events_for(&captured, run.id(), "octacity.factory.flow.resolved");
      assert_eq!(resolved.len(), identities.len() + 1);
      assert_eq!(
        resolved.last().unwrap()["fields"]["factory.flow.id"],
        fixture.input.flow_run_id().to_string()
      );
      assert_eq!(
        resolved.last().unwrap()["fields"]["factory.flow.terminal.digest"],
        FactoryDigest::sha256(
          "octacity.factory.trace-terminal.v1",
          &[fixture
            .admission
            .flow
            .closure()
            .definition(input_definition(&fixture))
            .unwrap()
            .terminals()[0]
            .key()
            .as_str()
            .as_bytes()]
        )
        .to_string()
      );
      for private in [
        "task-body",
        "private-command",
        "provider-payload",
        "finding-private-message",
        "private-prompt",
        "hidden-test-source",
        "/private/repository",
        "private.invalid",
        "provider-secret",
        "hidden-reasoning",
      ] {
        assert!(!text.contains(private), "diagnostics must omit {private}");
      }
      identities.push(audit.request_identity_digest);
    }
    assert_ne!(
      identities[0], identities[1],
      "the audit must identify the accepted transition including its frozen context, rather than only the Run version"
    );
  });
}
struct Failed;
#[async_trait]
impl FactoryNodeExecutor for Failed {
  async fn cancel(
    &self,
    _: FactoryNodeExecutionRequest<'_>,
    _: Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    panic!("retained stop must not dispatch cancellation")
  }
  async fn observe(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    Ok(FactoryNodeExecutionStep::ExecutionStopped {
      execution: Some(Box::new(
        FlowBuildExecution::new(
          request.build.unwrap(),
          octacity_server_domain::BuildId::generate(),
          octacity_server_domain::AttemptId::generate(),
          vec![octacity_server_domain::JobId::generate()],
        )
        .unwrap(),
      )),
      usage: BudgetUsage {
        tokens: 7,
        elapsed_millis: 4,
        ..Default::default()
      },
    })
  }
}
fn events_for(captured: &Arc<std::sync::Mutex<Vec<u8>>>, run: FactoryRunId, name: &str) -> Vec<serde_json::Value> {
  String::from_utf8(captured.lock().unwrap().clone())
    .unwrap()
    .lines()
    .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
    .filter(|event| event["fields"]["event.name"] == name && event["span"]["factory.run.id"] == run.to_string())
    .collect()
}
#[test]
fn stopped_attempt_emits_exact_committed_usage_once_without_semantic_completion() {
  use octacity_server_store::testing;
  let captured = captured_trace();
  run_ready(async {
    let fixture = testing::factory_node_journal_contract_fixture_with_retries(1);
    let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
    let audit = testing::management_mutation_with_request((), "stopped-owner")
      .audit()
      .clone();
    store
      .seed_factory_run(fixture.admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
      .unwrap();
    let run = &fixture.admission.run;
    let claim = FactoryRunClaimRecord::new(run.id(), fixture.attempt.owner().clone(), fixture.attempt.claim());
    store
      .claim_factory_run(ClaimFactoryRun {
        run_id: run.id(),
        expected_version: run.version(),
        record: claim.clone(),
        audit: FactoryAuditFact::new(
          run.id(),
          AuditActorKind::Worker,
          None,
          FactoryKey::new("factory.claimed").unwrap(),
          FactoryDigest::from_bytes([1; 32]),
          FactoryKey::new("accepted").unwrap(),
          at(10),
        ),
      })
      .await
      .unwrap();
    let target = FactoryNodeTarget {
      flow_run_id: fixture.input.flow_run_id(),
      cycle_id: fixture.input.cycle_id(),
      node: fixture.input.node().clone(),
    };
    let runner = FactoryNodeRunner::new(store.clone(), Arc::new(Failed), Arc::new(Clock));
    assert_eq!(
      runner
        .run_once(run.id(), claim.id, target.clone(), Some(fixture.input.clone()), at(11))
        .await
        .unwrap(),
      FactoryNodeStep::Advanced
    );
    assert_eq!(
      runner
        .run_once(run.id(), claim.id, target.clone(), None, at(15))
        .await
        .unwrap(),
      FactoryNodeStep::ExecutionStopped
    );
    let stopped = store.factory_run_snapshot(run.id()).await.unwrap();
    let original = &stopped.flow.attempts[0];
    assert_eq!(
      runner
        .run_once(run.id(), claim.id, target.clone(), None, at(16))
        .await
        .unwrap(),
      FactoryNodeStep::ExecutionStopped
    );
    assert_eq!(store.factory_run_snapshot(run.id()).await.unwrap(), stopped);
    let events = events_for(&captured, run.id(), "octacity.factory.node.stopped");
    assert_eq!(
      events.len(),
      1,
      "one persisted stop must emit once; replay must stay quiet"
    );
    let fields = &events[0]["fields"];
    assert_eq!(fields["factory.node.attempt.id"], original.id().to_string());
    assert_eq!(fields["factory.node.state"], "execution_failed");
    assert_eq!(fields["factory.flow.id"], target.flow_run_id.to_string());
    assert_eq!(fields["factory.cycle.id"], target.cycle_id.to_string());
    assert_eq!(fields["factory.node.latency_ms"], 4);
    assert_eq!(fields["factory.node.usage.tokens"], 7);
    assert_eq!(fields["factory.node.usage.elapsed_ms"], 4);
    assert_eq!(
      fields["factory.node.fence.digest"],
      claim.claim.fence().digest().to_string()
    );
    assert!(events_for(&captured, run.id(), "octacity.factory.node.completed").is_empty());
    let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
    let handlers = FactoryControlHandlers::new(store.clone());
    let grant = ManagementAuthorizationGrant::new(ManagementVisibility::all());
    handlers
      .execute_management_command(
        &context,
        &grant,
        RetryFactoryNodeCommand {
          run_id: run.id(),
          node_attempt_id: original.id(),
          expected_version: stopped.run.version(),
          idempotency_key: IdempotencyKey::new("retry-observed-node").unwrap(),
          requested_at: at(17),
        },
      )
      .await
      .unwrap();
    let before_denied = events_for(&captured, run.id(), "octacity.factory.transition.committed");
    assert!(
      runner
        .run_once(run.id(), claim.id, target.clone(), None, at(18))
        .await
        .is_err()
    );
    assert_eq!(
      events_for(&captured, run.id(), "octacity.factory.transition.committed"),
      before_denied,
      "a rejected stale claim must not emit a committed event"
    );
    let requested = store.factory_run_snapshot(run.id()).await.unwrap();
    let successor = FactoryRunClaimRecord::new(
      run.id(),
      claim.owner.clone(),
      FactoryClaim::new(
        FactoryClaimFence::new(FactoryDigest::from_bytes([3; 32])),
        at(18),
        at(200),
      )
      .unwrap(),
    );
    store
      .claim_factory_run(ClaimFactoryRun {
        run_id: run.id(),
        expected_version: requested.run.version(),
        record: successor.clone(),
        audit: FactoryAuditFact::new(
          run.id(),
          AuditActorKind::Worker,
          None,
          FactoryKey::new("factory.claimed").unwrap(),
          FactoryDigest::from_bytes([3; 32]),
          FactoryKey::new("accepted").unwrap(),
          at(18),
        ),
      })
      .await
      .unwrap();
    assert_eq!(
      runner
        .run_once(run.id(), successor.id, target.clone(), None, at(19))
        .await
        .unwrap(),
      FactoryNodeStep::Advanced
    );
    let retried = store.factory_run_snapshot(run.id()).await.unwrap();
    let retry = retried
      .flow
      .attempts
      .iter()
      .find(|attempt| attempt.retry_of() == Some(original.id()))
      .unwrap();
    let started = events_for(&captured, run.id(), "octacity.factory.node.started");
    let retry_event = started
      .iter()
      .find(|event| event["fields"]["factory.node.attempt.id"] == retry.id().to_string())
      .unwrap();
    assert_eq!(
      retry_event["fields"]["factory.node.retry_of.id"],
      original.id().to_string()
    );
    let inputs = events_for(&captured, run.id(), "octacity.factory.input.accepted");
    let retry_input = inputs
      .iter()
      .find(|event| event["fields"]["factory.input.digest"] == retry.input_digest().to_string())
      .unwrap();
    assert_eq!(retry_input["fields"]["factory.input.generation"], 1);
    assert_eq!(
      events_for(&captured, run.id(), "octacity.factory.node.stopped").len(),
      1
    );
  });
}

struct UnusedInputs;
#[async_trait]
impl FactoryFlowInputPreparer for UnusedInputs {
  async fn prepare(&self, _: &FactoryRunSnapshot, _: &FactoryNodeTarget) -> Result<FlowNodeInput, ApplicationError> {
    panic!("a completed one-node graph must resolve without new inputs")
  }
}
fn input_definition(fixture: &octacity_server_store::testing::FactoryNodeJournalContractFixture) -> FlowDefinitionRef {
  fixture.input.definition()
}
#[path = "support/factory_human_fixture.rs"]
mod human_fixture_support;
struct BuiltInInputs;
#[async_trait]
impl FactoryFlowInputPreparer for BuiltInInputs {
  async fn prepare(
    &self,
    snapshot: &FactoryRunSnapshot,
    target: &FactoryNodeTarget,
  ) -> Result<FlowNodeInput, ApplicationError> {
    FactoryNodeRunner::<octacity_server_store::testing::InMemoryFactoryConfigurationStore, Pending>::prepare_input(
      snapshot, target,
    )
  }
}
async fn claim_snapshot(
  store: &impl FactoryRunStore,
  snapshot: &FactoryRunSnapshot,
  time: u8,
) -> FactoryRunClaimRecord {
  let claim = FactoryRunClaimRecord::new(
    snapshot.run.id(),
    FactoryKey::new("trace.owner").unwrap(),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([time; 32])),
      at(i64::from(time)),
      at(100),
    )
    .unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: snapshot.run.id(),
      expected_version: snapshot.run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        snapshot.run.id(),
        AuditActorKind::Worker,
        None,
        FactoryKey::new("factory.claimed").unwrap(),
        FactoryDigest::from_bytes([time; 32]),
        FactoryKey::new("accepted").unwrap(),
        at(i64::from(time)),
      ),
    })
    .await
    .unwrap();
  claim
}
#[test]
fn configured_cycle_restart_and_cancellation_emit_only_committed_lifecycle_facts() {
  use octacity_server_store::testing;
  let captured = captured_trace();
  run_ready(async {
    let response = FlowDataSchema::new(
      FactoryKey::new("operator.response").unwrap(),
      FactoryKey::new("v1").unwrap(),
      FlowValueSchema::Boolean,
    )
    .unwrap();
    let admission = human_fixture_support::human_fixture_with_contract(response, None, true);
    let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
    let audit = testing::management_mutation_with_request((), "cycle-trace")
      .audit()
      .clone();
    store
      .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
      .unwrap();
    let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
    let claim = claim_snapshot(store.as_ref(), &snapshot, 10).await;
    let owner = FactoryFlowRunner::new(
      store.clone(),
      Arc::new(Pending),
      Arc::new(BuiltInInputs),
      Arc::new(Clock),
    );
    assert_eq!(
      owner
        .reconcile_flow(admission.run.id(), claim.id, at(11))
        .await
        .unwrap(),
      FactoryFlowStep::Advanced
    );
    let pending = store.factory_run_snapshot(admission.run.id()).await.unwrap();
    let first = &pending.flow.attempts[0];
    let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
    let grant = ManagementAuthorizationGrant::new(ManagementVisibility::all());
    let controls = FactoryControlHandlers::new(store.clone());
    controls
      .execute_management_command(
        &context,
        &grant,
        SubmitFactoryNodeResponseCommand {
          run_id: admission.run.id(),
          node_attempt_id: first.id(),
          outcome: first_node_outcome(&admission, first),
          value: serde_json::json!(true),
          expected_version: pending.run.version(),
          idempotency_key: IdempotencyKey::new("cycle-trace-response").unwrap(),
          requested_at: at(12),
        },
      )
      .await
      .unwrap();
    let responded = store.factory_run_snapshot(admission.run.id()).await.unwrap();
    let successor = claim_snapshot(store.as_ref(), &responded, 13).await;
    for time in [14, 15, 16] {
      owner
        .reconcile_flow(admission.run.id(), successor.id, at(time))
        .await
        .unwrap();
    }
    let restarted = store.factory_run_snapshot(admission.run.id()).await.unwrap();
    let cycle = restarted
      .flow
      .cycles
      .iter()
      .find(|cycle| cycle.number().get() == 2)
      .unwrap();
    let events = events_for(&captured, admission.run.id(), "octacity.factory.cycle.started");
    assert_eq!(events.len(), 1, "a configured restart must emit exactly one new cycle");
    assert_eq!(events[0]["fields"]["factory.cycle.id"], cycle.id().to_string());
    assert_eq!(events[0]["fields"]["factory.cycle.number"], 2);
    assert_eq!(
      events[0]["fields"]["factory.cycle.predecessor.id"],
      first.workflow_cycle_id().to_string()
    );
    assert_eq!(
      events[0]["fields"]["factory.cycle.source.attempt.id"],
      first.id().to_string()
    );
    controls
      .execute_management_command(
        &context,
        &grant,
        CancelFactoryRunCommand {
          run_id: admission.run.id(),
          expected_version: restarted.run.version(),
          idempotency_key: IdempotencyKey::new("cancel-cycle-trace").unwrap(),
          requested_at: at(17),
        },
      )
      .await
      .unwrap();
    let requested = store.factory_run_snapshot(admission.run.id()).await.unwrap();
    let cancelling = claim_snapshot(store.as_ref(), &requested, 18).await;
    assert_eq!(
      owner
        .reconcile_flow(admission.run.id(), cancelling.id, at(19))
        .await
        .unwrap(),
      FactoryFlowStep::Cancelled
    );
    assert_eq!(
      owner
        .reconcile_flow(admission.run.id(), cancelling.id, at(20))
        .await
        .unwrap(),
      FactoryFlowStep::Cancelled
    );
    let cancelled = events_for(&captured, admission.run.id(), "octacity.factory.flow.cancelled");
    assert_eq!(
      cancelled.len(),
      1,
      "cancelled recovery must not duplicate a lifecycle event"
    );
    assert_eq!(
      cancelled[0]["fields"]["factory.flow.id"],
      admission.flow.root_run().id().to_string()
    );
    assert_eq!(
      events_for(&captured, admission.run.id(), "octacity.factory.cycle.started").len(),
      1
    );
    let final_transitions = events_for(&captured, admission.run.id(), "octacity.factory.transition.committed");
    assert_eq!(
      final_transitions.last().unwrap()["fields"]["factory.run.state"],
      "cancelled"
    );
  });
}
fn first_node_outcome(admission: &PublishedFactoryAdmission, attempt: &NodeAttempt) -> FactoryKey {
  admission
    .flow
    .closure()
    .definition(admission.flow.root_run().definition())
    .unwrap()
    .node(attempt.node_key())
    .unwrap()
    .outcomes()[0]
    .key()
    .clone()
}
