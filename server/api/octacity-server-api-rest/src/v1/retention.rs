use serde::{Deserialize, Serialize};

use super::MutationDisposition;

/// Places a permanent or time-bounded hold on a complete Build Result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlaceBuildResultHoldRequest {
  /// Bounded operator reason retained with the hold.
  pub reason: String,
  /// Optional Unix-millisecond expiry; omitted for a permanent hold.
  pub expires_at_unix_ms: Option<i64>,
}

/// Immutable automatic-retention deadlines captured with a Build.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildResultRetentionDeadlines {
  /// Build metadata and configuration snapshot deadline.
  pub metadata_at_unix_ms: i64,
  /// Archived log deadline.
  pub logs_at_unix_ms: i64,
  /// Produced artifact deadline.
  pub artifacts_at_unix_ms: i64,
  /// Produced report deadline.
  pub reports_at_unix_ms: i64,
}

/// Current logical visibility of all Build Result components.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildResultVisibility {
  /// Build metadata is visible.
  pub metadata: bool,
  /// Archived logs are visible.
  pub logs: bool,
  /// Produced artifacts are visible.
  pub artifacts: bool,
  /// Produced reports are visible.
  pub reports: bool,
}

/// Latest hold state at the response observation time.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildResultHoldState {
  /// The complete Build Result is protected.
  Active,
  /// An operator explicitly released the hold.
  Released,
  /// A time-bounded hold reached its original expiry.
  Expired,
}

/// Safe audit identity retained with a hold.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionAuditIdentity {
  /// Stable actor classification.
  pub actor_kind: String,
  /// Available authenticated actor identity; absent in trusted-network v1.
  pub actor_identity: Option<String>,
  /// Original management request identity.
  pub request_identity: String,
}

/// Latest versioned hold over one complete Build Result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildResultHoldResource {
  /// Positive optimistic version.
  pub version: u64,
  /// Bounded operator reason.
  pub reason: String,
  /// Authoritative creation time.
  pub created_at_unix_ms: i64,
  /// Optional original expiry.
  pub expires_at_unix_ms: Option<i64>,
  /// Explicit release time, when released.
  pub released_at_unix_ms: Option<i64>,
  /// Current logical state.
  pub state: BuildResultHoldState,
  /// Safe audit identity of the placement transition.
  pub creation_audit: RetentionAuditIdentity,
  /// Safe audit identity of the explicit release transition, when released.
  pub release_audit: Option<RetentionAuditIdentity>,
}

/// Complete management view of Build Result retention.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildResultRetentionResource {
  /// Build Result identity.
  pub build_id: String,
  /// Original automatic-retention deadlines, never recomputed by hold commands.
  pub deadlines: BuildResultRetentionDeadlines,
  /// Logical visibility of the complete aggregate.
  pub visibility: BuildResultVisibility,
  /// Latest hold version, including released or expired state.
  pub hold: Option<BuildResultHoldResource>,
}

/// Response from an idempotent hold placement or release command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildResultRetentionMutationResponse {
  /// Whether the command was applied or exactly replayed.
  pub disposition: MutationDisposition,
  /// Resulting retention state.
  pub retention: BuildResultRetentionResource,
}
