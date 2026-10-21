//! In-memory parity for the same source-version and observation fences as PostgreSQL.
use crate::factory_configuration_testing::InMemoryFactoryConfigurationStore;
use crate::*;
use octacity_server_domain::{EntityKind, Timestamp};

#[async_trait::async_trait]
impl FactoryActionProjectionStore for InMemoryFactoryConfigurationStore {
  async fn factory_action_projection_work(&self, limit: u16) -> Result<FactoryActionProjectionWorkBatch, StoreError> {
    validate_action_projection_limit(limit)?;
    let state = self.lock()?;
    let mut pending = state
      .factory_runs
      .iter()
      .filter(|(id, run)| {
        state
          .factory_actions
          .get(id)
          .is_none_or(|row| row.source_version != run.run().version())
      })
      .map(|(id, run)| {
        Ok((
          run.updated_at()?,
          FactoryActionProjectionWork {
            run_id: *id,
            source_version: run.run().version(),
          },
        ))
      })
      .collect::<Result<Vec<_>, StoreError>>()?;
    pending.sort_by_key(|(at, work)| (*at, work.run_id));
    let has_more = pending.len() > usize::from(limit);
    pending.truncate(usize::from(limit));
    Ok(FactoryActionProjectionWorkBatch {
      items: pending.into_iter().map(|(_, row)| row).collect(),
      has_more,
    })
  }
  async fn refresh_factory_action_projection(
    &self,
    work: FactoryActionProjectionWork,
    at: Timestamp,
  ) -> Result<(), StoreError> {
    let mut state = self.lock()?;
    let run = state.factory_runs.get(&work.run_id).ok_or(StoreError::NotFound {
      entity: EntityKind::FactoryRun,
    })?;
    if run.run().version() != work.source_version
      || state
        .factory_actions
        .get(&work.run_id)
        .is_some_and(|row| row.observed_at > at)
    {
      return Err(StoreError::Conflict {
        entity: EntityKind::FactoryRun,
      });
    }
    let projection = FactoryActionProjection::from_snapshot(&run.snapshot()?, at)?;
    state.factory_actions.insert(work.run_id, projection);
    Ok(())
  }
  async fn read_factory_action_projection(
    &self,
    query: FactoryActionProjectionQuery,
  ) -> Result<FactoryActionProjectionPage, StoreError> {
    validate_action_projection_limit(query.limit)?;
    let state = self.lock()?;
    let run = state.factory_runs.get(&query.run_id).ok_or(StoreError::NotFound {
      entity: EntityKind::FactoryRun,
    })?;
    Ok(FactoryActionProjection::page(
      state.factory_actions.get(&query.run_id),
      run.run().version(),
      &query,
    ))
  }
}
