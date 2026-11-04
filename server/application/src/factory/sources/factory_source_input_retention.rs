//! Bounded private source input deletion, including unacknowledged and late writes.
use crate::ApplicationError;
use octacity_artifact_store::{ArtifactObject, ArtifactStore};
use octacity_server_domain::Timestamp;
use octacity_server_factory::FactoryDigest;
use octacity_server_store::WorkSourceArtifactStore;
use std::sync::Arc;
/// One bounded cleanup page. Persist the cursor between passes and restart a sweep at its end.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceInputCleanupPage {
  /// Preparations whose exact physical objects and private snapshot were cleared.
  pub completed: usize,
  /// Expired private polling pages cleared independently of artifact preparation progress.
  pub poll_pages_expired: usize,
  /// Preparations left retryable after an isolated deletion or acknowledgement failure.
  pub failed: usize,
  /// Exclusive continuation; completed tombstones are revisited on later sweeps for late writes.
  pub next_cursor: Option<FactoryDigest>,
}
/// Reconciles owned expired byte identities; ticket text never chooses a deletion target.
pub struct FactorySourceInputRetentionWorker<S, B: ?Sized> {
  store: Arc<S>,
  bytes: Arc<B>,
  recheck_millis: i64,
}
impl<S: WorkSourceArtifactStore, B: ArtifactStore + ?Sized> FactorySourceInputRetentionWorker<S, B> {
  /// Uses a trusted positive minimum recheck interval; successful checks grow sparser with age.
  pub fn new(store: Arc<S>, bytes: Arc<B>, recheck_after: std::time::Duration) -> Result<Self, ApplicationError> {
    let recheck_millis = i64::try_from(recheck_after.as_millis())
      .ok()
      .filter(|v| *v > 0)
      .ok_or_else(ApplicationError::invalid)?;
    Ok(Self {
      store,
      bytes,
      recheck_millis,
    })
  }
  /// Deletes at most two objects per preparation and at most `limit` preparations.
  /// Also purges at most `min(limit, 64)` expired private polling pages.
  /// Failure acknowledges nothing for that preparation; restart safely repeats deletion.
  pub async fn run_once(
    &self,
    now: Timestamp,
    after: Option<FactoryDigest>,
    limit: u16,
  ) -> Result<SourceInputCleanupPage, ApplicationError> {
    let expired = self.store.expired_source_preparations(now, after, limit).await?;
    let poll_pages_expired = self.store.expire_source_poll_pages(now, limit.min(64)).await?;
    let next_cursor = if expired.len() == usize::from(limit) {
      expired.last().map(|v| v.preparation_id)
    } else {
      None
    };
    let mut completed = 0;
    let mut failed = 0;
    for preparation in expired {
      if self.clean_preparation(preparation, now).await.is_ok() {
        completed += 1;
      } else {
        failed += 1;
      }
    }
    Ok(SourceInputCleanupPage {
      completed,
      poll_pages_expired,
      failed,
      next_cursor,
    })
  }
  async fn clean_preparation(
    &self,
    preparation: octacity_server_store::SourcePreparationCleanup,
    now: Timestamp,
  ) -> Result<(), ApplicationError> {
    for object in &preparation.objects {
      let object = ArtifactObject::new(
        object.artifact.artifact_id(),
        object.upload_id,
        object.artifact.encoded_size(),
        object.artifact.content_digest().to_string(),
        object.media_type.as_str(),
      )
      .map_err(|_| ApplicationError::invalid())?;
      self
        .bytes
        .delete(&object)
        .await
        .map_err(|_| ApplicationError::unavailable())?;
    }
    // Successful tombstones back off with age; no arbitrary last-write deadline is assumed.
    let elapsed = preparation
      .cleaned_at
      .map_or(0, |first| now.unix_millis().saturating_sub(first.unix_millis()));
    let next = now
      .unix_millis()
      .checked_add(self.recheck_millis.max(elapsed))
      .ok_or_else(ApplicationError::invalid)?;
    let recheck_at = Timestamp::from_unix_millis(next).map_err(|_| ApplicationError::invalid())?;
    self
      .store
      .complete_source_preparation_cleanup(preparation, now, recheck_at)
      .await?;
    Ok(())
  }
}
