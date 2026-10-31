//! Atomic ordinary Build admission for a persisted common node operation.

use crate::{ImmutableBuildInput, MaterializedJob, StoreError, StoreInputError, StoreOperation};
use async_trait::async_trait;
use octacity_server_domain::{AttemptId, BuildId, JobId, Timestamp};
use octacity_server_factory::{FactoryClaimOwnership, FactoryDigest, FlowBuildIntent};

/// Absolute bound for one managed Build DAG and its retained ordinary observations.
pub const MAX_FACTORY_BUILD_JOBS: usize = 64;

/// Frozen materialization and current dispatch authority for one node operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptFactoryBuild {
  /// Exact intent already committed before any external effect.
  pub intent: FlowBuildIntent,
  /// Current owner, independent of the immutable operation identity.
  pub ownership: FactoryClaimOwnership,
  /// Ordinary immutable Build facts and policy snapshots.
  pub build: ImmutableBuildInput,
  /// First Attempt identity.
  pub attempt_id: AttemptId,
  /// Complete initial Job DAG.
  pub jobs: Vec<MaterializedJob>,
  /// Trusted application clock at dispatch.
  pub accepted_at: Timestamp,
}

impl AcceptFactoryBuild {
  /// Checks retention at new admission; an identical replay keeps its original time.
  pub fn validate_fresh(&self) -> Result<(), StoreError> {
    self.validate()?;
    self
      .build
      .retention
      .validate(self.accepted_at)
      .map_err(|source| StoreError::invalid(StoreOperation::AcceptFactoryBuild, source))
  }
  /// Revalidates the materialized graph and its exact frozen source selection.
  pub fn validate(&self) -> Result<(), StoreError> {
    let invalid = || StoreError::invalid(StoreOperation::AcceptFactoryBuild, StoreInputError::InvalidFactoryBuild);
    let input = self.intent.input();
    input.verify_attempt(self.intent.node()).map_err(|_| invalid())?;
    self.build.validate().map_err(|_| invalid())?;
    let configuration = &self.intent.profile().build_configuration;
    match (input.exact_resource().ok(), self.build.source) {
      (Some(resource), None)
        if self.build.repository_id == Some(resource.repository_id())
          && self.build.immutable_revision.as_ref() == Some(resource.base_revision()) => {}
      (None, Some(octacity_server_domain::BuildSourceMode::SourceFreeV1))
        if matches!(input.code(), Some(octacity_server_factory::FlowNodeCode::None)) => {}
      _ => return Err(invalid()),
    }
    if self.build.project_id != input.subject().project_id()
      || self.build.configuration_id != configuration.id()
      || self.build.configuration_version != configuration.version()
      || self.build.priority != self.intent.priority()
    {
      return Err(invalid());
    }
    crate::model::validate_materialized_jobs(StoreOperation::AcceptFactoryBuild, self.build.id, &self.jobs)?;
    let permissions = crate::factory_job_permissions(self.intent.permissions()).map_err(|_| invalid())?;
    let digest = input.digest().map_err(|_| invalid())?.to_string();
    // Signed output ceilings reserve the complete DAG's worst-case output,
    // including Jobs that are initially blocked by dependencies.
    let output_bytes = self.jobs.iter().try_fold(0_u64, |total, job| {
      let limits = job.job_spec_template.output_limits();
      total
        .checked_add(limits.artifact_bytes)?
        .checked_add(limits.report_bytes)
    });
    if output_bytes.is_none_or(|bytes| bytes > self.intent.budget().max_output_bytes())
      || self.jobs.len() > MAX_FACTORY_BUILD_JOBS
      || self.jobs.iter().any(|job| {
        let template = &job.job_spec_template;
        let Some(managed) = template.managed_intent() else {
          return true;
        };
        let Some(factory) = managed.factory() else {
          return true;
        };
        template.source_revision() != self.build.immutable_revision.as_ref()
          || template.execution_deadline() != Some(self.intent.node().deadline())
          || managed.permissions() != &permissions
          || factory.factory_run_id != input.factory_run_id().to_string()
          || factory.factory_configuration_id != input.configuration().id().to_string()
          || factory.factory_configuration_version != input.configuration().version().get()
          || factory.flow_run_id != input.flow_run_id().to_string()
          || factory.workflow_cycle_id != input.cycle_id().to_string()
          || factory.node_attempt_id != self.intent.node().id().to_string()
          || factory.input_digest != digest
          || factory.subject_digest != input.subject().digest().to_string()
          || self.intent.profile().capture_write_paths.as_ref().is_some_and(|paths| {
            let matches = |wire: &[String]| {
              paths
                .iter()
                .map(|path| path.as_str())
                .eq(wire.iter().map(String::as_str))
            };
            !factory.source_write_paths.as_deref().is_some_and(matches)
              || factory
                .change_set_capture
                .as_ref()
                .is_none_or(|capture| !matches(&capture.policy.allowed_path_prefixes))
          })
          || managed.permissions().resources.elapsed_millis > self.intent.budget().max_elapsed_millis()
          || managed
            .permissions()
            .outputs
            .max_artifact_bytes
            .saturating_add(managed.permissions().outputs.max_report_bytes)
            > self.intent.budget().max_output_bytes()
      })
    {
      return Err(invalid());
    }
    let size = serde_json::to_vec(&(&self.intent, &self.build, &self.jobs))
      .map_err(|_| invalid())?
      .len();
    if size > crate::MAX_ACCEPT_TRIGGER_BYTES {
      return Err(invalid());
    }
    Ok(())
  }
}

/// Stable outcome retained through lost responses, retries and owner replacement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedFactoryBuild {
  /// Accepted ordinary Build.
  pub build_id: BuildId,
  /// Original initial Attempt; ordinary retries retain its operation origin.
  pub attempt_id: AttemptId,
  /// Complete original Job DAG, in identity order.
  pub job_ids: Vec<JobId>,
  /// Exact narrowed permission digest frozen by admission.
  pub effective_policy_digest: FactoryDigest,
}

/// Fenced atomic acceptance using the ordinary Build/Attempt/Job machinery.
#[async_trait]
pub trait FactoryBuildAcceptanceStore: Send + Sync {
  /// Read-only recovery, including after the node deadline or cancellation.
  async fn factory_build_for_operation(
    &self,
    operation: FactoryDigest,
  ) -> Result<Option<AcceptedFactoryBuild>, StoreError>;
  /// Commit one Build, first Attempt, complete DAG and signed root queue entries.
  /// New dispatch must verify the exact persisted intent, current claim and
  /// cancellation under the same transaction. Identical replay creates no work.
  async fn accept_factory_build(&self, request: AcceptFactoryBuild) -> Result<AcceptedFactoryBuild, StoreError>;
}

/// Current authority and stable original operation for ordinary cancellation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelFactoryBuildExecution {
  /// Stable cancellation receipt identity.
  pub operation_id: FactoryDigest,
  /// Accepted creation operation; never admits an absent Build.
  pub build_operation_id: FactoryDigest,
  /// Exact accepted ordinary Build.
  pub build_id: BuildId,
  /// Current observer claim, checked under the Run lock before fresh effects.
  pub ownership: FactoryClaimOwnership,
  /// Original retained cancellation time.
  pub requested_at: Timestamp,
  /// Current trusted clock used to fence the current owner.
  pub observed_at: Timestamp,
}

/// Applies ordinary cancellation only to an exactly bound Factory operation.
#[async_trait]
pub trait FactoryBuildCancellationStore: Send + Sync {
  /// Atomically withdraws ready Jobs and requests leased Job cancellation.
  /// An identical replay returns the original receipt without new effects.
  async fn cancel_factory_build_execution(
    &self,
    request: CancelFactoryBuildExecution,
  ) -> Result<crate::CancellationDisposition, StoreError>;
}
