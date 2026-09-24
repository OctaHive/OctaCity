use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::{BuildId, RetentionHoldVersion, Timestamp};
use octacity_server_store::{
  BuildResultRetentionHoldStore, GetBuildResultRetention, IdempotencyKey, PlaceBuildResultHold, ReleaseBuildResultHold,
  RetentionActorIdentity, RetentionHoldReason, RetentionRequestIdentity,
};
use serde::{Deserialize, Serialize};

use crate::{ApplicationError, Command, CommandHandler, MutationDisposition, Query, QueryHandler};

/// Maximum UTF-8 bytes accepted in a Build Result hold reason.
pub const MAX_BUILD_RESULT_HOLD_REASON_BYTES: usize = octacity_server_store::MAX_RETENTION_HOLD_REASON_BYTES;

/// Reads automatic deadlines, visibility, and the latest hold version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetBuildResultRetentionQuery {
  /// Build Result to inspect.
  pub build_id: BuildId,
  /// Authoritative observation time used to derive expiry.
  pub observed_at: Timestamp,
}

impl Query for GetBuildResultRetentionQuery {
  type Outcome = BuildResultRetentionProjection;
}

/// Places a permanent or time-bounded hold on a complete Build Result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaceBuildResultHoldCommand {
  /// Build Result to protect.
  pub build_id: BuildId,
  /// Bounded operator reason.
  pub reason: RetentionHoldReason,
  /// Optional time-bounded expiry; `None` creates a permanent hold.
  pub expires_at: Option<Timestamp>,
  /// Available authenticated actor identity; absent in trusted-network v1.
  pub actor_identity: Option<RetentionActorIdentity>,
  /// Request identity retained in the hold and audit fact.
  pub request_identity: RetentionRequestIdentity,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative placement time.
  pub placed_at: Timestamp,
}

impl Command for PlaceBuildResultHoldCommand {
  type Outcome = BuildResultRetentionCommandOutcome;
}

/// Releases the active hold without changing original automatic deadlines.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseBuildResultHoldCommand {
  /// Build Result whose hold is released.
  pub build_id: BuildId,
  /// Hold version that must still be current.
  pub expected_version: RetentionHoldVersion,
  /// Available authenticated actor identity; absent in trusted-network v1.
  pub actor_identity: Option<RetentionActorIdentity>,
  /// Request identity retained in the release audit fact.
  pub request_identity: RetentionRequestIdentity,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative release time.
  pub released_at: Timestamp,
}

impl Command for ReleaseBuildResultHoldCommand {
  type Outcome = BuildResultRetentionCommandOutcome;
}

/// Safe automatic-retention deadline projection.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BuildResultRetentionDeadlinesProjection {
  /// Build metadata deadline.
  pub metadata: Timestamp,
  /// Archived log deadline.
  pub logs: Timestamp,
  /// Produced artifact deadline.
  pub artifacts: Timestamp,
  /// Produced report deadline.
  pub reports: Timestamp,
}

/// Safe logical visibility projection for the complete Build Result.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BuildResultVisibilityProjection {
  /// Metadata remains visible.
  pub metadata: bool,
  /// Logs remain visible.
  pub logs: bool,
  /// Artifacts remain visible.
  pub artifacts: bool,
  /// Reports remain visible.
  pub reports: bool,
}

/// Stable hold state at the command or query observation time.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildResultHoldStateProjection {
  /// The hold currently protects the complete aggregate.
  Active,
  /// The hold was explicitly released.
  Released,
  /// The recorded time-bounded expiry elapsed.
  Expired,
}

/// Safe audit identity for one retention-hold transition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RetentionAuditIdentityProjection {
  /// Stable actor classification.
  pub actor_kind: String,
  /// Available authenticated actor identity.
  pub actor_identity: Option<String>,
  /// Original request identity.
  pub request_identity: String,
}

/// Safe latest retention hold projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BuildResultHoldProjection {
  /// Monotonically increasing hold resource version.
  pub version: RetentionHoldVersion,
  /// Bounded operator reason.
  pub reason: String,
  /// Authoritative creation time.
  pub created_at: Timestamp,
  /// Optional time-bounded expiry.
  pub expires_at: Option<Timestamp>,
  /// Explicit release time.
  pub released_at: Option<Timestamp>,
  /// State at the operation observation time.
  pub state: BuildResultHoldStateProjection,
  /// Audit identity of the placement transition.
  pub creation_audit: RetentionAuditIdentityProjection,
  /// Audit identity of the explicit release transition, when released.
  pub release_audit: Option<RetentionAuditIdentityProjection>,
}

/// Safe Build Result retention state returned by commands and queries.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BuildResultRetentionProjection {
  /// Build Result identity.
  pub build_id: BuildId,
  /// Immutable automatic-retention deadlines.
  pub deadlines: BuildResultRetentionDeadlinesProjection,
  /// Current component visibility.
  pub visibility: BuildResultVisibilityProjection,
  /// Latest hold version, including released or expired state.
  pub hold: Option<BuildResultHoldProjection>,
}

/// Result of applying or replaying a hold command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BuildResultRetentionCommandOutcome {
  /// Whether the mutation was applied or exactly replayed.
  pub disposition: MutationDisposition,
  /// Resulting retention state.
  pub retention: BuildResultRetentionProjection,
}

/// Typed Build Result hold handlers backed by one atomic store port.
pub struct BuildResultRetentionHandlers<S> {
  store: Arc<S>,
}

impl<S> BuildResultRetentionHandlers<S> {
  /// Creates handlers from a backend-neutral retention-hold port.
  pub fn new(store: Arc<S>) -> Self {
    Self { store }
  }
}

#[async_trait]
impl<S> QueryHandler<GetBuildResultRetentionQuery> for BuildResultRetentionHandlers<S>
where
  S: BuildResultRetentionHoldStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_query(
    &self,
    query: GetBuildResultRetentionQuery,
  ) -> Result<BuildResultRetentionProjection, Self::Error> {
    self
      .store
      .build_result_retention(GetBuildResultRetention {
        build_id: query.build_id,
        observed_at: query.observed_at,
      })
      .await
      .map(project)
      .map_err(Into::into)
  }
}

#[async_trait]
impl<S> CommandHandler<PlaceBuildResultHoldCommand> for BuildResultRetentionHandlers<S>
where
  S: BuildResultRetentionHoldStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: PlaceBuildResultHoldCommand,
  ) -> Result<BuildResultRetentionCommandOutcome, Self::Error> {
    let outcome = self
      .store
      .place_build_result_hold(PlaceBuildResultHold {
        build_id: command.build_id,
        reason: command.reason,
        expires_at: command.expires_at,
        actor_identity: command.actor_identity,
        request_identity: command.request_identity,
        idempotency_key: command.idempotency_key,
        placed_at: command.placed_at,
      })
      .await?;
    Ok(BuildResultRetentionCommandOutcome {
      disposition: outcome.disposition.into(),
      retention: project(outcome.retention),
    })
  }
}

#[async_trait]
impl<S> CommandHandler<ReleaseBuildResultHoldCommand> for BuildResultRetentionHandlers<S>
where
  S: BuildResultRetentionHoldStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: ReleaseBuildResultHoldCommand,
  ) -> Result<BuildResultRetentionCommandOutcome, Self::Error> {
    let outcome = self
      .store
      .release_build_result_hold(ReleaseBuildResultHold {
        build_id: command.build_id,
        expected_version: command.expected_version,
        actor_identity: command.actor_identity,
        request_identity: command.request_identity,
        idempotency_key: command.idempotency_key,
        released_at: command.released_at,
      })
      .await?;
    Ok(BuildResultRetentionCommandOutcome {
      disposition: outcome.disposition.into(),
      retention: project(outcome.retention),
    })
  }
}

fn project(value: octacity_server_store::BuildResultRetentionState) -> BuildResultRetentionProjection {
  BuildResultRetentionProjection {
    build_id: value.build_id,
    deadlines: BuildResultRetentionDeadlinesProjection {
      metadata: value.deadlines.metadata,
      logs: value.deadlines.logs,
      artifacts: value.deadlines.artifacts,
      reports: value.deadlines.reports,
    },
    visibility: BuildResultVisibilityProjection {
      metadata: value.visibility.metadata,
      logs: value.visibility.logs,
      artifacts: value.visibility.artifacts,
      reports: value.visibility.reports,
    },
    hold: value.hold.map(|hold| BuildResultHoldProjection {
      version: hold.version,
      reason: hold.reason,
      created_at: hold.created_at,
      expires_at: hold.expires_at,
      released_at: hold.released_at,
      state: match hold.state {
        octacity_server_store::RetentionHoldState::Active => BuildResultHoldStateProjection::Active,
        octacity_server_store::RetentionHoldState::Released => BuildResultHoldStateProjection::Released,
        octacity_server_store::RetentionHoldState::Expired => BuildResultHoldStateProjection::Expired,
      },
      creation_audit: RetentionAuditIdentityProjection {
        actor_kind: hold.creation_audit.actor_kind,
        actor_identity: hold.creation_audit.actor_identity,
        request_identity: hold.creation_audit.request_identity,
      },
      release_audit: hold.release_audit.map(|audit| RetentionAuditIdentityProjection {
        actor_kind: audit.actor_kind,
        actor_identity: audit.actor_identity,
        request_identity: audit.request_identity,
      }),
    }),
  }
}
