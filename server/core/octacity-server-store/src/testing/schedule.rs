use async_trait::async_trait;
use octacity_server_domain::{EntityKind, TriggerId, TriggerVersion};

use super::{InMemoryStore, ScheduleMemoryRecord};
use crate::{
  ClaimDueSchedules, CompleteScheduleClaim, CreateSchedule, DueScheduleClaim, MutationDisposition, ScheduleRecord,
  ScheduleStore, StoreError, TriggerDefinitionMutationOutcome,
};

#[async_trait]
impl ScheduleStore for InMemoryStore {
  async fn create_schedule(&self, request: CreateSchedule) -> Result<TriggerDefinitionMutationOutcome, StoreError> {
    request.validate()?;
    let mut state = self.lock()?;
    let trigger_ref = crate::TriggerDefinitionRef {
      id: request.trigger.id,
      version: request.trigger.version,
    };
    if let Some(existing) = state
      .schedules
      .values()
      .find(|existing| existing.idempotency_key == request.trigger.idempotency_key)
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
        idempotency_key: request.trigger.idempotency_key,
        claim: None,
      },
    );
    state.idempotency_outcomes.insert(format!("schedule:{trigger_ref:?}"));
    state.audit_facts.insert(format!("schedule:{trigger_ref:?}"));
    state.outbox_entries.insert(format!("schedule:{trigger_ref:?}"));
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
