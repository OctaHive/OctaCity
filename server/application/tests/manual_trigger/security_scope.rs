use super::*;
use octacity_server_store::TriggerReplayNamespace;

#[test]
fn identical_manual_trigger_commands_are_isolated_by_management_security_scope() {
  run(async {
    let fixture = fixture();
    let store = Arc::new(ScopedRecordingStore::default());
    let service = ManualTriggerService::new(
      store.clone(),
      Arc::new(StaticContext(fixture.context)),
      Arc::new(RecordingResolver::succeed("0123456789abcdef")),
    );

    let first = service
      .accept_management(fixture.command.clone(), time(200), audit_in_scope("operator:first"))
      .await
      .unwrap();
    let second = service
      .accept_management(fixture.command, time(201), audit_in_scope("operator:second"))
      .await
      .unwrap();

    let ManualTriggerOutcome::Accepted {
      disposition: first_disposition,
      build_id: first_build,
      ..
    } = first
    else {
      panic!("enabled configuration must create a Build");
    };
    let ManualTriggerOutcome::Accepted {
      disposition: second_disposition,
      build_id: second_build,
      ..
    } = second
    else {
      panic!("enabled configuration must create a Build");
    };
    assert_eq!(first_disposition, ApplicationDisposition::Applied);
    assert_eq!(second_disposition, ApplicationDisposition::Applied);
    assert_ne!(first_build, second_build);
    assert_eq!(store.entries.lock().unwrap().len(), 2);
  });
}

fn audit_in_scope(scope: &str) -> MutationAuditContext {
  MutationAuditContext::try_new(
    AuditActor {
      kind: AuditActorKind::AuthenticatedManagement,
      identity: Some(scope.to_owned()),
    },
    octacity_server_store::ManagementSecurityScope::new(scope).unwrap(),
    format!("request:{scope}"),
  )
  .unwrap()
}

#[derive(Default)]
struct ScopedRecordingStore {
  entries: Mutex<Vec<(TriggerReplayNamespace, AcceptTrigger)>>,
}

#[async_trait]
impl octacity_server_store::TriggerAcceptanceStore for ScopedRecordingStore {
  async fn replay_trigger_acceptance(
    &self,
    request: TriggerAcceptanceProbe,
  ) -> Result<Option<TriggerEvaluationOutcome>, StoreError> {
    let entries = self.entries.lock().unwrap();
    let Some((_, existing)) = entries.iter().find(|(namespace, existing)| {
      namespace == &request.namespace
        && (existing.trigger.id == request.trigger.id
          || existing.trigger.deduplication_key() == request.trigger.deduplication_key())
    }) else {
      return Ok(None);
    };
    if existing.intent_digest != request.intent_digest {
      return Err(StoreError::Conflict {
        entity: EntityKind::Trigger,
      });
    }
    Ok(Some(TriggerEvaluationOutcome::Accepted(AcceptTriggerOutcome {
      disposition: StoreDisposition::Replayed,
      trigger_occurrence_id: existing.trigger.id,
      build_id: existing.build.id,
      attempt_id: existing.attempt_id,
      ready_jobs: existing
        .jobs
        .iter()
        .filter(|job| job.dependencies.is_empty())
        .map(|job| job.id)
        .collect(),
    })))
  }

  async fn accept_trigger(&self, _request: AcceptTrigger) -> Result<AcceptTriggerOutcome, StoreError> {
    Err(StoreError::Unavailable)
  }

  async fn accept_management_trigger(
    &self,
    request: ManagementMutation<AcceptTrigger>,
  ) -> Result<AcceptTriggerOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let outcome = AcceptTriggerOutcome {
      disposition: StoreDisposition::Applied,
      trigger_occurrence_id: request.trigger.id,
      build_id: request.build.id,
      attempt_id: request.attempt_id,
      ready_jobs: request
        .jobs
        .iter()
        .filter(|job| job.dependencies.is_empty())
        .map(|job| job.id)
        .collect(),
    };
    self.entries.lock().unwrap().push((
      TriggerReplayNamespace::Management(audit.security_scope().clone()),
      request,
    ));
    Ok(outcome)
  }

  async fn suppress_trigger(&self, _request: SuppressTrigger) -> Result<SuppressTriggerOutcome, StoreError> {
    Err(StoreError::Unavailable)
  }

  async fn suppress_management_trigger(
    &self,
    _request: ManagementMutation<SuppressTrigger>,
  ) -> Result<SuppressTriggerOutcome, StoreError> {
    Err(StoreError::Unavailable)
  }
}
