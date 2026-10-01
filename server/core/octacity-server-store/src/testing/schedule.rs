use async_trait::async_trait;
use octacity_server_domain::{EntityKind, TriggerId, TriggerVersion};

use super::{InMemoryStore, ScheduleMemoryRecord};
use crate::{
  ClaimDueSchedules, CompleteScheduleClaim, CreateSchedule, DueScheduleClaim, ManagementMutation, MutationDisposition,
  ScheduleRecord, ScheduleStore, StoreError, StoreOperation, TriggerDefinitionMutationOutcome,
  testing::recorded_management_audit,
};

#[async_trait]
impl ScheduleStore for InMemoryStore {
  async fn create_schedule(
    &self,
    request: ManagementMutation<CreateSchedule>,
  ) -> Result<TriggerDefinitionMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    request.validate()?;
    let idempotency = audit.scoped_idempotency_key(&request.trigger.idempotency_key);
    let mut state = self.lock()?;
    let trigger_ref = crate::TriggerDefinitionRef {
      id: request.trigger.id,
      version: request.trigger.version,
    };
    if let Some(existing) = state
      .schedules
      .values()
      .find(|existing| existing.idempotency_key == idempotency)
    {
      if existing.record.target.configuration_id == request.trigger.configuration_id
        && existing.record.target.configuration_version == request.trigger.configuration_version
        && existing.record.enabled == request.trigger.enabled
        && existing.record.definition == request.trigger.definition
        && existing.record.schedule == request.schedule
      {
        return Ok(TriggerDefinitionMutationOutcome {
          disposition: MutationDisposition::Replayed,
          trigger_id: existing.record.trigger.id,
          version: existing.record.trigger.version,
        });
      }
      return Err(StoreError::Conflict {
        entity: EntityKind::Trigger,
      });
    }
    if state.schedules.contains_key(&trigger_ref) {
      return Err(StoreError::Conflict {
        entity: EntityKind::Trigger,
      });
    }
    state.schedules.insert(
      trigger_ref,
      ScheduleMemoryRecord {
        record: ScheduleRecord {
          trigger: trigger_ref,
          target: crate::TriggerTarget {
            configuration_id: request.trigger.configuration_id,
            configuration_version: request.trigger.configuration_version,
          },
          enabled: request.trigger.enabled,
          definition: request.trigger.definition,
          schedule: request.schedule,
          next_occurrence_at: request.next_occurrence_at,
        },
        idempotency_key: idempotency.clone(),
        claim: None,
      },
    );
    let evidence_identity = super::management_evidence_identity("schedule", &idempotency);
    state.idempotency_outcomes.insert(evidence_identity.clone());
    state.audit_facts.insert(evidence_identity.clone());
    state.management_audit_facts.insert(recorded_management_audit(
      &audit,
      StoreOperation::CreateSchedule,
      EntityKind::Trigger,
      request.trigger.id,
    ));
    state.outbox_entries.insert(evidence_identity);
    Ok(TriggerDefinitionMutationOutcome {
      disposition: MutationDisposition::Applied,
      trigger_id: request.trigger.id,
      version: request.trigger.version,
    })
  }

  async fn schedule(&self, trigger_id: TriggerId, version: TriggerVersion) -> Result<ScheduleRecord, StoreError> {
    self
      .lock()?
      .schedules
      .get(&crate::TriggerDefinitionRef {
        id: trigger_id,
        version,
      })
      .map(|stored| stored.record.clone())
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Trigger,
      })
  }

  async fn claim_due_schedules(&self, request: ClaimDueSchedules) -> Result<Vec<DueScheduleClaim>, StoreError> {
    let mut state = self.lock()?;
    let mut eligible: Vec<_> = state
      .schedules
      .iter()
      .filter(|(_, stored)| {
        stored.record.enabled
          && stored.record.next_occurrence_at <= request.observed_at
          && stored
            .claim
            .as_ref()
            .is_none_or(|(_, deadline)| *deadline <= request.observed_at)
      })
      .map(|(trigger, stored)| (*trigger, stored.record.next_occurrence_at))
      .collect();
    eligible.sort_unstable_by_key(|(trigger, next)| (*next, *trigger));
    eligible.truncate(usize::from(request.limit.get()));
    Ok(
      eligible
        .into_iter()
        .map(|(trigger, _)| {
          let stored = state.schedules.get_mut(&trigger).expect("selected schedule exists");
          stored.claim = Some((request.owner.clone(), request.claim_expires_at));
          DueScheduleClaim {
            schedule: stored.record.clone(),
            owner: request.owner.clone(),
            claim_expires_at: request.claim_expires_at,
          }
        })
        .collect(),
    )
  }

  async fn complete_schedule_claim(&self, request: CompleteScheduleClaim) -> Result<MutationDisposition, StoreError> {
    let mut state = self.lock()?;
    let stored = state.schedules.get_mut(&request.trigger).ok_or(StoreError::NotFound {
      entity: EntityKind::Trigger,
    })?;
    let owns_live_claim = stored
      .claim
      .as_ref()
      .is_some_and(|(owner, deadline)| owner == &request.owner && *deadline > request.completed_at);
    if !owns_live_claim
      || stored.record.next_occurrence_at != request.expected_next_occurrence_at
      || request.next_occurrence_at <= request.expected_next_occurrence_at
    {
      return Err(StoreError::Conflict {
        entity: EntityKind::Trigger,
      });
    }
    stored.record.next_occurrence_at = request.next_occurrence_at;
    stored.claim = None;
    Ok(MutationDisposition::Applied)
  }
}

#[cfg(test)]
mod tests {
  use octacity_server_domain::{BuildConfigurationId, BuildConfigurationVersion, TriggerVersion};
  use octacity_server_trigger::{MissedRunPolicy, ScheduleDefinition};
  use serde_json::json;

  use super::*;
  use crate::test_support::{id, run_ready, time};
  use crate::testing::{assert_management_audit_facts, expected_management_audit, management_mutation};
  use crate::{CreateTriggerDefinition, IdempotencyKey, TriggerKind};

  #[test]
  fn schedule_creation_records_the_exact_management_fact_once() {
    let store = InMemoryStore::new();
    let trigger_id = id::<TriggerId>(700);
    let schedule = ScheduleDefinition {
      expression: "* * * * * * *".to_owned(),
      timezone: "UTC".to_owned(),
      missed_run_policy: MissedRunPolicy::RunOnce,
    };
    let created_at = time(0);
    let request = CreateSchedule {
      trigger: CreateTriggerDefinition {
        id: trigger_id,
        version: TriggerVersion::INITIAL,
        configuration_id: id::<BuildConfigurationId>(701),
        configuration_version: BuildConfigurationVersion::INITIAL,
        kind: TriggerKind::Scheduled,
        enabled: true,
        definition: json!({}),
        idempotency_key: IdempotencyKey::new("schedule-audit").unwrap(),
        created_at,
      },
      next_occurrence_at: schedule.next_after(created_at).unwrap(),
      schedule,
    };

    run_ready(
      async {
        assert_eq!(
          store
            .create_schedule(management_mutation(request.clone()))
            .await
            .unwrap()
            .disposition,
          MutationDisposition::Applied
        );
        assert_eq!(
          store
            .create_schedule(management_mutation(request))
            .await
            .unwrap()
            .disposition,
          MutationDisposition::Replayed
        );
        assert_management_audit_facts(
          &store,
          [expected_management_audit(
            StoreOperation::CreateSchedule,
            EntityKind::Trigger,
            trigger_id,
          )],
        )
        .await;
      },
      "in-memory schedule audit contract unexpectedly yielded",
    );
  }
}
