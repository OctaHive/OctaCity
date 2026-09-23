use std::collections::BTreeMap;

use octacity_protocol::OctaSpec;
use octacity_server_domain::{
  AttemptId, BuildId, ImmutableRevision, JobId, SourceReference, Timestamp, TriggerOccurrenceId,
};
use octacity_server_job::{JobSpecDerivationError, JobSpecValidity, SourcePluginPolicy};
use octacity_server_store::{
  PublishedBuildConfiguration, PublishedPipeline, PublishedRepository, StoreError, StoreInputError,
  TriggerDefinitionRef, TriggerEvaluationOutcome, TriggerTarget,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::EffectiveProjectPolicy;
use crate::{ApplicationFailure, Command, MutationDisposition};

/// Source expression supplied by a trusted-network manual Build command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case", deny_unknown_fields)]
pub enum ManualSourceSelection {
  /// Resolve the immutable Repository version's configured default reference.
  DefaultReference,
  /// Resolve one explicitly allowed mutable branch or tag.
  Reference(SourceReference),
  /// Verify and use an explicitly supplied immutable revision.
  ExactRevision(ImmutableRevision),
}

/// Transport-independent intent to create one manual Build.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManualTriggerCommand {
  /// Exact immutable Trigger definition selected by the caller.
  pub trigger: TriggerDefinitionRef,
  /// Exact immutable Build Configuration selected by the Trigger.
  pub target: TriggerTarget,
  /// Stable source-scoped idempotency identity for this manual occurrence.
  pub deduplication_identity: octacity_server_domain::TriggerIdentity,
  /// Allowed source expression to resolve or verify exactly once.
  pub source: ManualSourceSelection,
  /// Parameter values supplied by the caller before defaults are applied.
  pub parameters: BTreeMap<String, Value>,
  /// Durable ready-queue priority copied to root Jobs.
  pub priority: i64,
  /// Time at which the management command was observed.
  pub observed_at: Timestamp,
}

/// Complete application command for accepting one manual Trigger occurrence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptManualTriggerCommand {
  /// Stable caller intent and immutable Trigger target.
  pub trigger: ManualTriggerCommand,
  /// Authoritative time persisted for the accepted or suppressed occurrence.
  pub accepted_at: Timestamp,
}

impl Command for AcceptManualTriggerCommand {
  type Outcome = ManualTriggerOutcome;
}

/// Transport-independent result of evaluating one manual Trigger command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ManualTriggerOutcome {
  /// Evaluation created one Build and its first Attempt.
  Accepted {
    /// Whether this evaluation was applied or replayed.
    disposition: MutationDisposition,
    /// Stable normalized occurrence identity.
    trigger_occurrence_id: TriggerOccurrenceId,
    /// Created Build identity.
    build_id: BuildId,
    /// First Attempt identity.
    attempt_id: AttemptId,
    /// Root Jobs inserted into the ready queue.
    ready_job_ids: Vec<JobId>,
  },
  /// Policy intentionally created no Build.
  Suppressed {
    /// Whether this evaluation was applied or replayed.
    disposition: MutationDisposition,
    /// Stable normalized occurrence identity.
    trigger_occurrence_id: TriggerOccurrenceId,
  },
}

impl From<TriggerEvaluationOutcome> for ManualTriggerOutcome {
  fn from(value: TriggerEvaluationOutcome) -> Self {
    match value {
      TriggerEvaluationOutcome::Accepted(outcome) => Self::Accepted {
        disposition: outcome.disposition.into(),
        trigger_occurrence_id: outcome.trigger_occurrence_id,
        build_id: outcome.build_id,
        attempt_id: outcome.attempt_id,
        ready_job_ids: outcome.ready_jobs,
      },
      TriggerEvaluationOutcome::Suppressed(outcome) => Self::Suppressed {
        disposition: outcome.disposition.into(),
        trigger_occurrence_id: outcome.trigger_occurrence_id,
      },
    }
  }
}

/// Immutable server-owned inputs needed to evaluate a manual Trigger.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManualTriggerContext {
  /// Exact Build Configuration version targeted by the Trigger.
  pub configuration: PublishedBuildConfiguration,
  /// Exact Repository version selected by the configuration.
  pub repository: PublishedRepository,
  /// Exact Pipeline version selected by the configuration.
  pub pipeline: PublishedPipeline,
  /// Root-to-leaf effective Project policy frozen for the Build.
  pub effective_policy: EffectiveProjectPolicy,
  /// Operator-owned source-plugin, Octa release, and JobSpec validity policy.
  pub job_spec_toolchain: JobSpecToolchainPolicy,
}

/// Immutable operator policy for executable identities signed into every Job.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JobSpecToolchainPolicy {
  /// Exact source plugin and public Repository-locator parameter.
  pub source: SourcePluginPolicy,
  /// Exact Octa runner and task-plugin set.
  pub octa: OctaSpec,
  /// Number of seconds for which a derived JobSpec remains valid.
  pub validity: JobSpecValidity,
}

impl JobSpecToolchainPolicy {
  /// Revalidates operator configuration after strict deserialization.
  pub fn validate(&self) -> Result<(), JobSpecDerivationError> {
    self.source.validate()?;
    self.octa.validate().map_err(|_| JobSpecDerivationError::InvalidPolicy)
  }
}

/// Failure while loading immutable manual-Trigger context.
#[derive(Debug, Error)]
pub enum ManualTriggerContextError {
  /// An authoritative configuration or Pipeline read failed.
  #[error("authoritative manual-trigger context could not be loaded")]
  Store(#[source] StoreError),
  /// Effective Project policy could not be loaded or resolved.
  #[error("effective project policy could not be loaded")]
  Policy(#[source] EffectiveProjectPolicySourceError),
}

/// Stable failure classification for effective Project policy loading.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum EffectiveProjectPolicySourceError {
  /// The Project or one required immutable policy version does not exist.
  #[error("effective project policy was not found")]
  NotFound,
  /// Persisted layers do not form one valid root-to-leaf policy.
  #[error("effective project policy is invalid")]
  Invalid,
  /// The authoritative policy source is temporarily unavailable.
  #[error("effective project policy is unavailable")]
  Unavailable,
}

/// Complete provider-neutral request for immutable source resolution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevisionResolutionRequest {
  /// Immutable Repository definition and provider integration selection.
  pub repository: PublishedRepository,
  /// Policy-approved reference or exact revision to resolve or verify.
  pub selection: ManualSourceSelection,
}

/// Classified failure from the replaceable VCS revision-resolution seam.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RevisionResolutionError {
  /// The selected reference or exact revision does not exist.
  #[error("source revision was not found")]
  NotFound,
  /// Repository or integration configuration is permanently invalid.
  #[error("source revision request is invalid")]
  Invalid,
  /// The provider failed transiently and the command may be retried.
  #[error("source revision provider is temporarily unavailable")]
  Transient,
  /// Resolution was cooperatively cancelled.
  #[error("source revision resolution was cancelled")]
  Cancelled,
  /// The configured resolver cannot perform revision resolution.
  #[error("source revision resolver is unavailable")]
  Unavailable,
}

/// Stable validation failures for a manual Trigger command and its loaded context.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ManualTriggerInputError {
  /// Loaded immutable context does not match the exact command target or its references.
  #[error("manual-trigger context does not match its immutable references")]
  ContextMismatch,
  /// The selected Build Configuration version does not allow manual Triggers.
  #[error("build configuration does not allow manual triggers")]
  ManualTriggerNotAllowed,
  /// The selected Build Configuration version does not allow scheduled Triggers.
  #[error("build configuration does not allow scheduled triggers")]
  ScheduledTriggerNotAllowed,
  /// The selected Build Configuration version does not allow internal Triggers.
  #[error("build configuration does not allow internal triggers")]
  InternalTriggerNotAllowed,
  /// The selected Build Configuration version does not allow external Triggers.
  #[error("build configuration does not allow external triggers")]
  ExternalTriggerNotAllowed,
  /// No default mutable reference is configured for the Repository version.
  #[error("repository has no default source reference")]
  DefaultReferenceMissing,
  /// The requested mutable reference is outside Repository policy.
  #[error("source reference is not allowed by repository policy")]
  ReferenceNotAllowed,
  /// The Repository version does not permit caller-selected exact revisions.
  #[error("exact source revisions are not allowed by repository policy")]
  ExactRevisionNotAllowed,
  /// One caller-supplied parameter is not declared by the configuration.
  #[error("unknown build parameter: {0}")]
  UnknownParameter(String),
  /// One required parameter has neither a caller value nor a default.
  #[error("required build parameter is missing: {0}")]
  MissingParameter(String),
  /// One parameter value does not match its declared primitive type.
  #[error("build parameter has the wrong type: {0}")]
  InvalidParameterType(String),
  /// One open-schema parameter cannot become a signed execution variable.
  #[error("build parameter cannot be represented as an execution variable: {0}")]
  InvalidParameterValue(String),
  /// Caller-supplied parameters exceed the bounded application request shape.
  #[error("build parameters exceed their item or encoded-byte bound")]
  ParametersTooLarge,
  /// Effective Project policy does not allow the selected Repository.
  #[error("repository is not allowed by effective project policy")]
  RepositoryNotAllowed,
  /// Effective Project policy does not allow the selected runtime class.
  #[error("runtime is not allowed by effective project policy")]
  RuntimeNotAllowed,
  /// Configuration and effective Project policy share no eligible Agent Pool.
  #[error("configuration and project policy share no allowed agent pool")]
  NoAllowedPool,
  /// Configuration cache access exceeds effective Project policy.
  #[error("cache access is not allowed by effective project policy")]
  CacheNotAllowed,
  /// Configuration workload identity is outside effective Project policy.
  #[error("workload identity is not allowed by effective project policy")]
  WorkloadIdentityNotAllowed,
  /// Configuration output ceilings exceed effective Project policy.
  #[error("artifact policy exceeds effective project policy")]
  ArtifactPolicyTooBroad,
}

/// Application-level failure while accepting a manual Trigger.
#[derive(Debug, Error)]
pub enum ManualTriggerError {
  /// The command or loaded immutable context violates policy.
  #[error("manual trigger is invalid: {0}")]
  Invalid(#[source] ManualTriggerInputError),
  /// Immutable server-owned context could not be loaded.
  #[error("manual trigger context could not be loaded")]
  Context(#[source] ManualTriggerContextError),
  /// The selected source could not be resolved to an immutable revision.
  #[error("manual trigger source could not be resolved")]
  Revision(#[source] RevisionResolutionError),
  /// The complete atomic persistence operation failed.
  #[error("manual trigger could not be committed")]
  Store(#[source] StoreError),
  /// A validated immutable snapshot could not be encoded.
  #[error("manual trigger snapshot encoding failed")]
  SnapshotEncoding,
  /// A Pipeline node or immutable policy could not produce stable JobSpec intent.
  #[error("manual trigger JobSpec derivation failed")]
  JobSpec(#[source] JobSpecDerivationError),
  /// The validated Pipeline could not be materialized into a store request.
  #[error("manual trigger DAG materialization failed")]
  Materialization(#[source] StoreInputError),
}

impl ManualTriggerError {
  /// Constructs an unavailable dependency failure for adapter-boundary tests.
  #[must_use]
  pub const fn unavailable() -> Self {
    Self::Store(StoreError::Unavailable)
  }

  /// Returns a transport-neutral failure classification.
  #[must_use]
  pub const fn classification(&self) -> ApplicationFailure {
    match self {
      Self::Invalid(_) | Self::Materialization(_) => ApplicationFailure::Invalid,
      Self::Context(ManualTriggerContextError::Policy(EffectiveProjectPolicySourceError::NotFound))
      | Self::Revision(RevisionResolutionError::NotFound) => ApplicationFailure::NotFound,
      Self::Context(ManualTriggerContextError::Store(StoreError::NotFound { .. })) => ApplicationFailure::NotFound,
      Self::Context(ManualTriggerContextError::Store(StoreError::Unavailable))
      | Self::Context(ManualTriggerContextError::Policy(EffectiveProjectPolicySourceError::Unavailable))
      | Self::Revision(RevisionResolutionError::Transient | RevisionResolutionError::Unavailable)
      | Self::Store(StoreError::Unavailable) => ApplicationFailure::Unavailable,
      Self::Context(ManualTriggerContextError::Store(StoreError::Conflict { .. } | StoreError::Duplicate { .. }))
      | Self::Store(StoreError::Conflict { .. } | StoreError::Duplicate { .. }) => ApplicationFailure::Conflict,
      Self::Context(ManualTriggerContextError::Policy(EffectiveProjectPolicySourceError::Invalid))
      | Self::Context(ManualTriggerContextError::Store(_))
      | Self::Revision(RevisionResolutionError::Invalid | RevisionResolutionError::Cancelled)
      | Self::Store(_)
      | Self::SnapshotEncoding
      | Self::JobSpec(_) => ApplicationFailure::Internal,
    }
  }
}
