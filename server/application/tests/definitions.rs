use std::{
  future::Future,
  str::FromStr,
  sync::{Arc, Mutex},
  task::{Context, Poll, Waker},
};

use async_trait::async_trait;
use octacity_server_application::{
  CommandHandler as _, CreateTriggerDefinitionCommand, DefinitionHandlers, ProjectPolicyDefinition,
  PublishProjectPolicyCommand,
};
use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationVersion, ProjectId, Timestamp, TriggerId, TriggerVersion,
};
use octacity_server_store::{
  CreateTriggerDefinition, DefinitionStore, IdempotencyKey, MutationDisposition, ProjectPolicyMutationOutcome,
  PublishProjectPolicy, StoreError, TriggerDefinitionMutationOutcome,
};
use octacity_server_trigger::TriggerKind;
use serde_json::{Value, json};

#[derive(Default)]
struct RecordingDefinitionStore {
  policy: Mutex<Option<Value>>,
  trigger: Mutex<Option<Value>>,
}

#[async_trait]
impl DefinitionStore for RecordingDefinitionStore {
  async fn publish_project_policy(
    &self,
    request: PublishProjectPolicy,
  ) -> Result<ProjectPolicyMutationOutcome, StoreError> {
    *self.policy.lock().unwrap() = Some(request.policy);
    Ok(ProjectPolicyMutationOutcome {
      disposition: MutationDisposition::Applied,
      project_id: request.project_id,
      version: octacity_server_domain::ProjectPolicyVersion::INITIAL,
    })
  }

  async fn create_trigger_definition(
    &self,
    request: CreateTriggerDefinition,
  ) -> Result<TriggerDefinitionMutationOutcome, StoreError> {
    *self.trigger.lock().unwrap() = Some(request.definition);
    Ok(TriggerDefinitionMutationOutcome {
      disposition: MutationDisposition::Applied,
      trigger_id: request.id,
      version: request.version,
    })
  }
}

#[test]
fn definition_handlers_keep_policy_and_trigger_persistence_transport_independent() {
  run_ready(async {
    let store = Arc::new(RecordingDefinitionStore::default());
    let handlers = DefinitionHandlers::new(store.clone());
    let project_id = ProjectId::from_str("11111111-1111-4111-8111-111111111111").unwrap();
    let policy: ProjectPolicyDefinition = serde_json::from_value(policy_document()).unwrap();
    let policy_outcome = handlers
      .handle_command(PublishProjectPolicyCommand {
        project_id,
        expected_current_version: None,
        policy,
        idempotency_key: IdempotencyKey::new("policy-v1").unwrap(),
        published_at: Timestamp::from_unix_millis(1).unwrap(),
      })
      .await
      .unwrap();
    assert_eq!(policy_outcome.project_id, project_id);
    assert_eq!(store.policy.lock().unwrap().as_ref(), Some(&policy_document()));

    let trigger_id = TriggerId::from_str("22222222-2222-4222-8222-222222222222").unwrap();
    let definition = json!({"reason": "manual"});
    let trigger_outcome = handlers
      .handle_command(CreateTriggerDefinitionCommand {
        id: trigger_id,
        version: TriggerVersion::INITIAL,
        configuration_id: BuildConfigurationId::from_str("33333333-3333-4333-8333-333333333333").unwrap(),
        configuration_version: BuildConfigurationVersion::INITIAL,
        kind: TriggerKind::Manual,
        enabled: true,
        definition: definition.clone(),
        idempotency_key: IdempotencyKey::new("trigger-v1").unwrap(),
        created_at: Timestamp::from_unix_millis(2).unwrap(),
      })
      .await
      .unwrap();
    assert_eq!(trigger_outcome.trigger_id, trigger_id);
    assert_eq!(store.trigger.lock().unwrap().as_ref(), Some(&definition));
  });
}

fn policy_document() -> Value {
  json!({
    "pools": {"mode": "replace", "value": []},
    "repositories": {"mode": "replace", "value": []},
    "secret_profiles": {"mode": "replace", "value": []},
    "identity_profiles": {"mode": "replace", "value": []},
    "runtimes": {"mode": "replace", "value": []},
    "cache": {"mode": "replace", "value": {"namespaces": [], "read": false, "write": false, "max_bytes": 0}},
    "artifacts": {"mode": "replace", "value": {"artifact_count": 0, "artifact_bytes": 0, "report_count": 0, "report_bytes": 0, "single_output_bytes": 0}},
    "concurrency": {"mode": "replace", "value": {"active_builds": 1, "active_jobs": 1}},
    "retention": {"mode": "replace", "value": {"build_seconds": 1, "log_seconds": 1, "artifact_seconds": 1, "cache_seconds": 1}}
  })
}

fn run_ready<T>(future: impl Future<Output = T>) -> T {
  let mut future = std::pin::pin!(future);
  let mut context = Context::from_waker(Waker::noop());
  match future.as_mut().poll(&mut context) {
    Poll::Ready(value) => value,
    Poll::Pending => panic!("in-memory definition handler unexpectedly waited for external I/O"),
  }
}
