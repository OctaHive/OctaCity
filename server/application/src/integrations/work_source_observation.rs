//! Optional execution-neutral handoff. Task import, association and refresh scheduling are consumers.
use crate::{ApplicationError, ManagementRequestContext, ManagementSecurityScope, WorkSourceSnapshot};
use async_trait::async_trait;
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::{FactoryDigest, ImmutableReference, WorkSourceBinding};

/// One page of an explicitly requested installed read; no input identity or Factory target.
#[derive(Clone)]
pub struct ReadWorkSourceObservations {
  /// Original managed connection selection. A reader must resolve its current
  /// Project grant explicitly; it cannot borrow legacy installed authority.
  pub connection: Option<octacity_server_factory::WorkSourceConnectionRef>,
  /// Owning Project, checked against the installed profile and current grant.
  pub project_id: ProjectId,
  /// Exact installed read profile; a consumer can select its own broader installed selector.
  pub source: WorkSourceBinding,
  /// Original bounded pass identity, retained when continuing a page.
  pub scan_id: FactoryDigest,
  /// Inclusive incremental window, or full reconciliation using the existing Source protocol.
  pub window: Option<(Timestamp, Timestamp)>,
  /// Private opaque continuation; never a destination or authorization.
  pub continuation: Option<String>,
  /// Requested ceiling, narrowed by installed page/item/byte limits.
  pub limit: u16,
}

/// Bounded private provenance, never a Work mutation, admission or reporting authorization.
/// Deliberately excludes Serialize/Debug: raw task content is not a diagnostic or REST DTO.
pub struct WorkSourceObservation {
  /// Trusted owning Project.
  pub project_id: ProjectId,
  /// Trusted security partition of the installed reader.
  pub security_scope: ManagementSecurityScope,
  /// Exact installed Source binding, independent of any pipeline input identity.
  pub source: WorkSourceBinding,
  /// Content-verified executable that produced the normalized snapshot.
  pub plugin: ImmutableReference,
  /// Exact provider transport/configuration, excluding credentials.
  pub provider_configuration: FactoryDigest,
  /// Exact selector evidence; never an execution target or task identity.
  pub selection_digest: FactoryDigest,
  /// Existing native Project/security/installation/container/item correlation.
  pub native_correlation: FactoryDigest,
  /// Untrusted normalized data, bounded and scoped by the installed protocol.
  pub snapshot: WorkSourceSnapshot,
  /// Complete normalized snapshot digest, excluding pipeline selection matches.
  pub snapshot_digest: FactoryDigest,
  /// Trusted beginning of the provider read, not a provider timestamp.
  pub observed_at: Timestamp,
  /// Original finite provenance lifetime; rereading retained data cannot renew it.
  pub expires_at: Timestamp,
}

/// Temporal/content evidence for a consumer, never a provider revision ordering guarantee.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkSourceObservationFreshness {
  /// A live new snapshot, or the first live observation of this native task.
  Fresh,
  /// A live reread has the same complete content as the previous native observation.
  Unchanged,
  /// Expired, future-dated or older than the consumer's last observation of this task.
  Stale,
}
impl WorkSourceObservation {
  /// One shared check for explicit and paged Issue consumers before native data gains authority.
  pub(crate) fn validate_issue_read(
    &self,
    context: &ManagementRequestContext,
    selection: &octacity_server_store::IssueSourceSelection,
    now: Timestamp,
    operation: octacity_server_store::StoreOperation,
  ) -> Result<(), ApplicationError> {
    self.issue_receipt().validate(selection, now, operation)?;
    if self.project_id != selection.project_id
      || self.security_scope != *context.security_scope()
      || self.snapshot.item != selection.item
      || self.native_correlation != selection.native_correlation(&context.security_scope().to_store())
      || self.snapshot_digest
        != FactoryDigest::content_sha256(&serde_json::to_vec(&self.snapshot).map_err(|_| ApplicationError::invalid())?)
    {
      return Err(ApplicationError::invalid());
    }
    Ok(())
  }
  /// Retains safe exact provenance for Issue persistence without copying normalized native content.
  pub fn issue_receipt(&self) -> octacity_server_store::IssueSourceObservation {
    octacity_server_store::IssueSourceObservation {
      display_id: Some(self.snapshot.display_id.clone()),
      source: self.source.clone(),
      plugin: self.plugin.clone(),
      provider_configuration: self.provider_configuration,
      selection_digest: self.selection_digest,
      snapshot_digest: self.snapshot_digest,
      observed_at: self.observed_at,
      expires_at: self.expires_at,
    }
  }
  /// Compares trusted read times and complete snapshot digests, never opaque update identifiers.
  /// Consumers retain their own latest accepted evidence; no hidden observation cache exists.
  pub fn freshness_at(&self, now: Timestamp, previous: Option<&Self>) -> WorkSourceObservationFreshness {
    let previous = previous.filter(|p| p.native_correlation == self.native_correlation);
    if now < self.observed_at || now >= self.expires_at || previous.is_some_and(|p| p.observed_at > self.observed_at) {
      WorkSourceObservationFreshness::Stale
    } else if previous.is_some_and(|p| p.snapshot_digest == self.snapshot_digest) {
      WorkSourceObservationFreshness::Unchanged
    } else {
      WorkSourceObservationFreshness::Fresh
    }
  }
}

/// One finite page; the consumer owns durable progress and any task updates.
pub struct WorkSourceObservationPage {
  /// Private bounded observations.
  pub observations: Vec<WorkSourceObservation>,
  /// Existing provider continuation, including for empty pages.
  pub next_cursor: Option<String>,
}

/// Optional trusted internal read capability; callers authorize their consumer operation first.
/// Installing it starts no polling or consumer subscription and creates no management endpoint.
#[async_trait]
pub trait WorkSourceObservations: Send + Sync {
  /// Rechecks current installed read authority without listing another page.
  /// Readers lacking independent reauthorization cannot resume retained private observations.
  async fn authorize(
    &self,
    _context: &ManagementRequestContext,
    _project: ProjectId,
    _source: WorkSourceBinding,
  ) -> Result<(), ApplicationError> {
    Err(ApplicationError::capability_unavailable())
  }
  /// Reauthorizes retained data under its original managed connection without another read.
  /// Implementers must explicitly support managed pins; legacy authorization cannot substitute.
  async fn authorize_connection(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    source: WorkSourceBinding,
    connection: Option<&octacity_server_factory::WorkSourceConnectionRef>,
  ) -> Result<(), ApplicationError> {
    if connection.is_some() {
      return Err(ApplicationError::capability_unavailable());
    }
    self.authorize(context, project, source).await
  }
  /// Reads under current task-read authority, without admission, task storage or Reporter effects.
  async fn read(
    &self,
    context: &ManagementRequestContext,
    request: ReadWorkSourceObservations,
  ) -> Result<WorkSourceObservationPage, ApplicationError>;
}
