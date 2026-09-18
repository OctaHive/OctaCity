use octacity_server_domain::{
  BuildId, ImmutableRevision, IntegrationId, RepositoryId, SourceReference, Timestamp, TriggerIdentity,
  TriggerOccurrenceId,
};
use octacity_server_trigger::{
  NormalizedTriggerOccurrence, TriggerCausality, TriggerCause, TriggerDefinitionRef, TriggerEventKind, TriggerKind,
  TriggerOccurrenceState, TriggerTarget,
};
use serde::Serialize;

/// Provider-neutral initiating facts safe for management reads.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TriggerCauseProjection {
  /// Trusted-network management intent.
  Manual,
  /// A persisted schedule reached its recorded occurrence time.
  Scheduled,
  /// An authenticated external repository event after provider normalization.
  External {
    /// Server-owned integration that authenticated the delivery.
    integration_id: IntegrationId,
    /// Repository named by the normalized event.
    repository_id: RepositoryId,
    /// Provider-neutral event classification.
    event_kind: TriggerEventKind,
    /// Optional mutable source reference.
    reference: Option<SourceReference>,
    /// Optional immutable revision reported by the event.
    revision: Option<ImmutableRevision>,
  },
  /// A server-owned event derived from an earlier Build.
  Internal {
    /// Build whose durable event produced this occurrence.
    source_build_id: BuildId,
    /// Documented server event classification.
    event_kind: TriggerEventKind,
  },
}

impl From<&TriggerCause> for TriggerCauseProjection {
  fn from(cause: &TriggerCause) -> Self {
    match cause {
      TriggerCause::Manual {} => Self::Manual,
      TriggerCause::Scheduled {} => Self::Scheduled,
      TriggerCause::External {
        integration_id,
        repository_id,
        event_kind,
        reference,
        revision,
      } => Self::External {
        integration_id: *integration_id,
        repository_id: *repository_id,
        event_kind: event_kind.clone(),
        reference: reference.clone(),
        revision: revision.clone(),
      },
      TriggerCause::Internal {
        source_build_id,
        event_kind,
      } => Self::Internal {
        source_build_id: *source_build_id,
        event_kind: event_kind.clone(),
      },
    }
  }
}

/// Safe application projection of one normalized Trigger occurrence.
///
/// Provider-owned opaque metadata is intentionally absent. It remains useful
/// for internal adapter diagnostics but is not part of project-readable state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TriggerHistoryProjection {
  /// Stable occurrence identity.
  pub id: TriggerOccurrenceId,
  /// Exact immutable Trigger definition.
  pub trigger: TriggerDefinitionRef,
  /// Exact Build Configuration target.
  pub target: TriggerTarget,
  /// Stable source-scoped deduplication identity.
  pub deduplication_identity: TriggerIdentity,
  /// Normalized Trigger origin.
  pub kind: TriggerKind,
  /// Provider-neutral initiating facts.
  pub cause: TriggerCauseProjection,
  /// Stable root and parent occurrence identities.
  pub causality: TriggerCausality,
  /// Source-observed time.
  pub source_time: Timestamp,
  /// Current durable evaluation state.
  pub state: TriggerOccurrenceState,
  /// Build created by this occurrence, when accepted.
  pub build_id: Option<BuildId>,
  /// Authoritative persistence time.
  pub created_at: Timestamp,
  /// Time of the latest durable evaluation transition.
  pub updated_at: Timestamp,
}

impl TriggerHistoryProjection {
  /// Projects normalized durable facts while deliberately discarding opaque provider metadata.
  #[must_use]
  pub fn from_occurrence(
    occurrence: &NormalizedTriggerOccurrence,
    state: TriggerOccurrenceState,
    build_id: Option<BuildId>,
    created_at: Timestamp,
    updated_at: Timestamp,
  ) -> Self {
    Self {
      id: occurrence.id,
      trigger: occurrence.trigger,
      target: occurrence.target,
      deduplication_identity: occurrence.deduplication_identity.clone(),
      kind: occurrence.cause.kind(),
      cause: (&occurrence.cause).into(),
      causality: occurrence.causality,
      source_time: occurrence.source_time,
      state,
      build_id,
      created_at,
      updated_at,
    }
  }
}
