use std::num::NonZeroU16;

use octacity_server_domain::{BuildId, ImmutableRevision, Timestamp, TriggerId, TriggerIdentity, TriggerVersion};

use crate::{
  CreateTriggerDefinition, IdempotencyKey, NormalizedTriggerOccurrence, StoreError, StoreInputError, StoreOperation,
  TerminalBuildEvent, TriggerDefinitionRef, TriggerKind, TriggerTarget, WorkerOwner,
  model::require_bounded_json_object,
};

/// Maximum terminal Build events acquired by one internal-Trigger worker pass.
pub const MAX_INTERNAL_TRIGGER_EVENT_BATCH_SIZE: u16 = 100;
/// Maximum matching Trigger definitions expanded from one terminal Build event.
pub const MAX_INTERNAL_TRIGGER_MATCHES_PER_EVENT: usize = 256;
/// Maximum internal Trigger definitions returned by one management page.
pub const MAX_INTERNAL_TRIGGER_PAGE_SIZE: u16 = 200;

/// Atomic creation of an internal Trigger definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateInternalTriggerDefinition {
  /// Common immutable Trigger fields and kind-specific definition.
  pub trigger: CreateTriggerDefinition,
}

impl CreateInternalTriggerDefinition {
  /// Revalidates the common definition and internal kind at the persistence seam.
  pub fn validate(&self) -> Result<(), StoreError> {
    self.trigger.validate()?;
    if self.trigger.kind != TriggerKind::Internal {
      return Err(StoreError::invalid(
        StoreOperation::CreateInternalTriggerDefinition,
        StoreInputError::InvalidNormalizedTrigger,
      ));
    }
    Ok(())
  }
}

/// Atomic publication of the next immutable version of an internal Trigger.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishInternalTriggerVersion {
  /// Stable Trigger identity.
  pub id: TriggerId,
  /// Version that must still be current.
  pub expected_current_version: TriggerVersion,
  /// Exact downstream Build Configuration target.
  pub target: TriggerTarget,
  /// Whether events occurring after publication may create occurrences.
  pub enabled: bool,
  /// Application-validated internal Trigger definition.
  pub definition: serde_json::Value,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

impl PublishInternalTriggerVersion {
  /// Revalidates bounded persistence input.
  pub fn validate(&self) -> Result<(), StoreError> {
    require_bounded_json_object(&self.definition).map_err(|source| StoreError::InvalidInput {
      operation: StoreOperation::PublishInternalTriggerVersion,
      source,
    })
  }
}

/// Durable representation of one exact internal Trigger version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternalTriggerDefinitionRecord {
  /// Exact immutable Trigger identity and version.
  pub trigger: TriggerDefinitionRef,
  /// Exact downstream Build Configuration target.
  pub target: TriggerTarget,
  /// Whether new matching source events may be accepted.
  pub enabled: bool,
  /// Strict kind-specific definition decoded by the application layer.
  pub definition: serde_json::Value,
  /// Authoritative publication time.
  pub created_at: Timestamp,
}

/// Bounded current-version listing request for internal Triggers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListInternalTriggerDefinitions {
  /// Exclusive stable-identity cursor.
  pub after: Option<TriggerId>,
  /// Positive bounded page size.
  pub limit: NonZeroU16,
}

impl ListInternalTriggerDefinitions {
  /// Validates and constructs a listing request.
  pub fn new(after: Option<TriggerId>, limit: u16) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|limit| limit.get() <= MAX_INTERNAL_TRIGGER_PAGE_SIZE)
      .ok_or_else(|| {
        StoreError::invalid(
          StoreOperation::ListInternalTriggerDefinitions,
          StoreInputError::InvalidInternalTriggerPageSize,
        )
      })?;
    Ok(Self { after, limit })
  }
}

/// One deterministic page of current internal Trigger versions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternalTriggerDefinitionPage {
  /// Current definitions ordered by stable Trigger identity.
  pub items: Vec<InternalTriggerDefinitionRecord>,
  /// Exclusive cursor for the next page.
  pub next_after: Option<TriggerId>,
}

/// Bounded request to claim terminal Build events from the transactional outbox.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimInternalTriggerEvents {
  /// Process instance acquiring work.
  pub owner: WorkerOwner,
  /// Authoritative time used to select available work and stale claims.
  pub observed_at: Timestamp,
  /// Exclusive deadline after which another replica may reclaim work.
  pub claim_expires_at: Timestamp,
  /// Positive bounded number of source events.
  pub limit: NonZeroU16,
}

impl ClaimInternalTriggerEvents {
  /// Validates ownership duration and batch size.
  pub fn new(
    owner: WorkerOwner,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
    limit: u16,
  ) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|limit| limit.get() <= MAX_INTERNAL_TRIGGER_EVENT_BATCH_SIZE)
      .ok_or_else(|| {
        StoreError::invalid(
          StoreOperation::ClaimInternalTriggerEvents,
          StoreInputError::InvalidWorkerClaim,
        )
      })?;
    if claim_expires_at <= observed_at {
      return Err(StoreError::invalid(
        StoreOperation::ClaimInternalTriggerEvents,
        StoreInputError::InvalidWorkerClaim,
      ));
    }
    Ok(Self {
      owner,
      observed_at,
      claim_expires_at,
      limit,
    })
  }
}

/// One immutable internal Trigger definition matched to a source event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternalTriggerMatch {
  /// Exact immutable Trigger definition.
  pub trigger: TriggerDefinitionRef,
  /// Exact immutable Build Configuration selected by the Trigger.
  pub target: TriggerTarget,
  /// Strict kind-specific definition decoded by the application worker.
  pub definition: serde_json::Value,
}

/// One terminal Build event exclusively owned until its claim deadline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternalTriggerEventClaim {
  /// Stable transactional-outbox identity used for retry deduplication.
  pub event_identity: TriggerIdentity,
  /// Build whose terminal transition produced the event.
  pub source_build_id: BuildId,
  /// Exact immutable revision built by the upstream Build.
  pub source_revision: ImmutableRevision,
  /// Exact upstream Build Configuration version used for definition matching.
  pub source_target: TriggerTarget,
  /// Persisted occurrence that created the source Build.
  pub source_occurrence: NormalizedTriggerOccurrence,
  /// Root-to-parent immutable Trigger lineage used for cycle detection.
  pub trigger_ancestry: Vec<TriggerDefinitionRef>,
  /// Documented server-owned terminal event classification.
  pub event_kind: TerminalBuildEvent,
  /// Time at which the source transition committed.
  pub occurred_at: Timestamp,
  /// Enabled definitions that matched at source-event time.
  pub matches: Vec<InternalTriggerMatch>,
  /// Owner that must complete this outbox delivery.
  pub owner: WorkerOwner,
  /// Exclusive claim deadline.
  pub claim_expires_at: Timestamp,
}

/// Marks one claimed source event delivered after every candidate was handled.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteInternalTriggerEvent {
  /// Stable transactional-outbox identity returned by the claim.
  pub event_identity: TriggerIdentity,
  /// Owner returned by the claim operation.
  pub owner: WorkerOwner,
  /// Authoritative completion time, which must precede claim expiry.
  pub completed_at: Timestamp,
}
