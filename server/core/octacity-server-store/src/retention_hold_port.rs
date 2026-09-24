use async_trait::async_trait;

use crate::{
  BuildResultRetentionState, GetBuildResultRetention, PlaceBuildResultHold, ReleaseBuildResultHold,
  ReleaseBuildResultHoldError, RetentionHoldMutationOutcome, StoreError,
};

/// Authoritative atomic operations for whole-Build-Result retention holds.
#[async_trait]
pub trait BuildResultRetentionHoldStore: Send + Sync {
  /// Reads immutable deadlines, visibility, and the latest hold version.
  async fn build_result_retention(
    &self,
    query: GetBuildResultRetention,
  ) -> Result<BuildResultRetentionState, StoreError>;

  /// Places a permanent or time-bounded hold before any component is hidden.
  async fn place_build_result_hold(
    &self,
    request: PlaceBuildResultHold,
  ) -> Result<RetentionHoldMutationOutcome, StoreError>;

  /// Releases the active hold without changing any original deadline.
  async fn release_build_result_hold(
    &self,
    request: ReleaseBuildResultHold,
  ) -> Result<RetentionHoldMutationOutcome, ReleaseBuildResultHoldError>;
}
