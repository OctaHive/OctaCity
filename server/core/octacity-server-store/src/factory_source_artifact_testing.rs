use crate::factory_configuration_testing::InMemoryFactoryConfigurationStore;
use crate::factory_source_testing::SourceMemoryState;
use crate::*;
use async_trait::async_trait;
use octacity_server_domain::{ArtifactId, ProjectId, Timestamp};
use octacity_server_factory::FactoryDigest;
impl SourceMemoryState {
  pub(super) fn artifact_owner(
    &self,
    claim: &WorkSourceScanClaim,
    id: FactoryDigest,
    now: Timestamp,
  ) -> Result<WorkSourcePreparation, StoreError> {
    self.require_current(claim.project_id, claim.target)?;
    require_source_scan(
      self.scans.get(&claim.target.identity()).ok_or_else(source_conflict)?,
      claim,
      now,
    )?;
    let preparation = self
      .preparations
      .get(&id)
      .filter(|v| v.input.project_id() == claim.project_id)
      .cloned()
      .ok_or_else(source_conflict)?;
    preparation.require_selected_owner(claim, &self.selected_input(claim, preparation.input.id())?)?;
    Ok(preparation)
  }
}
#[async_trait]
impl WorkSourceArtifactStore for InMemoryFactoryConfigurationStore {
  async fn reject_source_preparation(
    &self,
    request: RejectSourcePreparation,
  ) -> Result<SourceMaterialization, StoreError> {
    let mut state = self.lock()?;
    let preparation = state
      .sources
      .artifact_owner(&request.claim, request.preparation_id, request.now)?;
    let manifest = request.retain(
      &preparation,
      state.sources.materializations.get(&request.preparation_id),
    )?;
    state
      .sources
      .materializations
      .insert(manifest.preparation_id, manifest.clone());
    Ok(manifest)
  }
  async fn expired_source_preparations(
    &self,
    now: Timestamp,
    after: Option<FactoryDigest>,
    limit: u16,
  ) -> Result<Vec<SourcePreparationCleanup>, StoreError> {
    if !(1..=100).contains(&limit) {
      return Err(source_conflict());
    }
    let state = self.lock()?;
    Ok(
      state
        .sources
        .preparations
        .values()
        .filter(|p| {
          after.is_none_or(|id| p.id > id)
            && (!p.snapshot.is_empty()
              || state
                .sources
                .materializations
                .get(&p.id)
                .is_some_and(|m| !m.objects.is_empty()))
        })
        .map(|p| SourcePreparationCleanup::from_preparation(p, state.sources.materializations.get(&p.id)))
        .filter(|p| p.is_due(now))
        .take(usize::from(limit))
        .collect(),
    )
  }
  async fn complete_source_preparation_cleanup(
    &self,
    expected: SourcePreparationCleanup,
    now: Timestamp,
    recheck_at: Timestamp,
  ) -> Result<(), StoreError> {
    let mut state = self.lock()?;
    let preparation = state
      .sources
      .preparations
      .get(&expected.preparation_id)
      .ok_or_else(source_conflict)?;
    let current = SourcePreparationCleanup::from_preparation(
      preparation,
      state.sources.materializations.get(&expected.preparation_id),
    );
    if !expected.requires_acknowledgement(&current, now, recheck_at)? {
      return Ok(());
    }
    if preparation.snapshot.is_empty() && current.objects.is_empty() {
      return Ok(());
    }
    if let Some(manifest) = state.sources.materializations.get_mut(&expected.preparation_id) {
      manifest.cleaned_at.get_or_insert(now);
      manifest.next_cleanup_at = (!manifest.objects.is_empty()).then_some(recheck_at);
    }
    state
      .sources
      .preparations
      .get_mut(&expected.preparation_id)
      .ok_or_else(source_conflict)?
      .snapshot
      .clear();
    Ok(())
  }
  async fn reserve_source_artifacts(
    &self,
    request: ReserveSourceArtifacts,
  ) -> Result<SourceMaterialization, StoreError> {
    let mut state = self.lock()?;
    let preparation = state
      .sources
      .artifact_owner(&request.claim, request.preparation_id, request.now)?;
    let manifest = request.retain(
      &preparation,
      state.sources.materializations.get(&request.preparation_id),
    )?;
    if state.sources.materializations.values().any(|other| {
      other.preparation_id != manifest.preparation_id
        && other.objects.iter().any(|old| {
          manifest
            .objects
            .iter()
            .any(|new| old.artifact.artifact_id() == new.artifact.artifact_id() || old.upload_id == new.upload_id)
        })
    }) {
      return Err(source_conflict());
    }
    state
      .sources
      .materializations
      .insert(manifest.preparation_id, manifest.clone());
    Ok(manifest)
  }
  async fn publish_source_artifacts(
    &self,
    request: PublishSourceArtifacts,
  ) -> Result<SourceMaterialization, StoreError> {
    let mut state = self.lock()?;
    state
      .sources
      .artifact_owner(&request.claim, request.preparation_id, request.now)?;
    let manifest = state
      .sources
      .materializations
      .get(&request.preparation_id)
      .ok_or_else(source_conflict)?
      .publish(&request)?;
    state
      .sources
      .materializations
      .insert(manifest.preparation_id, manifest.clone());
    Ok(manifest)
  }
  async fn source_materialization(
    &self,
    project: ProjectId,
    id: FactoryDigest,
  ) -> Result<Option<SourceMaterialization>, StoreError> {
    Ok(
      self
        .lock()?
        .sources
        .materializations
        .get(&id)
        .filter(|v| v.project_id == project)
        .cloned(),
    )
  }
}
#[async_trait]
impl SourceArtifactQueryStore for InMemoryFactoryConfigurationStore {
  async fn source_artifact(
    &self,
    project: ProjectId,
    id: ArtifactId,
  ) -> Result<Option<PublishedSourceArtifact>, StoreError> {
    Ok(
      self
        .lock()?
        .sources
        .materializations
        .values()
        .filter(|v| v.project_id == project)
        .find_map(|v| v.artifact(id)),
    )
  }
}
