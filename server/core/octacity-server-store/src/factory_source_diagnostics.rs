//! Bounded diagnostic reads over the existing Source and admission journals.
use crate::*;
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::*;
use serde::Serialize;
/// Safe scan facts; ownership fences, private snapshots and continuations are deliberately absent.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkSourceScanDiagnostics {
  /// Last scan acquisition time, not proof that its page completed.
  pub started_at: Timestamp,
  /// Covered window bounds, present only after a committed checkpoint.
  pub window: Option<(Timestamp, Timestamp)>,
  /// Last checkpoint belonged to reconciliation.
  pub reconciliation: bool,
  /// Durable retry/failure state.
  pub schedule: Option<WorkSourceSchedule>,
  /// Last selection evidence; never an admission promise.
  pub selection: Option<WorkSourceSelectionRecord>,
}
impl WorkSourceScanDiagnostics {
  /// Projects only allowlisted retained facts without renewing a lease.
  pub fn from_scan(scan: WorkSourceScan) -> Self {
    Self {
      started_at: scan.claim.acquired_at,
      window: scan.progress.as_ref().map(|p| (p.window_start, p.window_end)),
      reconciliation: scan.progress.as_ref().is_some_and(|p| p.reconciliation),
      schedule: scan.schedule,
      selection: scan.last_selection,
    }
  }
}
/// Filter over original settled Source associations; it cannot create an admission.
#[derive(Clone, Debug)]
pub enum WorkSourceDiagnosticFilter {
  /// All retained versions of these stable inputs.
  Inputs(Vec<WorkSourceInputId>),
  /// Exact native observations in one installed security partition.
  Native {
    /// Existing native item hashes, independent of input identity.
    issues: Vec<FactoryDigest>,
    /// Installed partition, never a browser-supplied scope.
    security_scope: FactoryDigest,
  },
}
/// Original safe association, including native correlation separately from replay input identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkSourceDiagnosticAssociation {
  /// Stable journal position, never a private content reference.
  pub id: FactoryDigest,
  /// Project/partition/installation/container/item hash; not admission authority.
  pub native_correlation: FactoryDigest,
  /// Original automatic input.
  pub input_id: WorkSourceInputId,
  /// Original immutable version, retained after edits.
  pub input_version: WorkSourceInputVersion,
  /// Existing authoritative Work.
  pub work_id: WorkEnvelopeId,
  /// Existing authoritative Run.
  pub run_id: FactoryRunId,
}
/// Visible associations only; hidden rows never influence the cursor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkSourceDiagnosticPage {
  /// Bounded visible rows in journal identity order.
  pub items: Vec<WorkSourceDiagnosticAssociation>,
  /// Last visible row iff another visible row exists.
  pub next_cursor: Option<FactoryDigest>,
}
/// Store-owned scope applied before ordering and pagination.
#[derive(Clone, Debug)]
pub struct ReadWorkSourceCorrelations {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Input/native filter, at most 100 identities.
  pub filter: WorkSourceDiagnosticFilter,
  /// Policy-derived dependency visibility.
  pub visibility: WorkSourceConfigurationVisibility,
  /// Independently visible Runs.
  pub runs: FactoryRunListVisibility,
  /// Exclusive stable journal position.
  pub after: Option<FactoryDigest>,
  /// Positive result ceiling, at most 64.
  pub limit: u16,
}
impl ReadWorkSourceCorrelations {
  /// Validates finite query bounds independently of management ingress.
  pub fn validate(&self) -> Result<(), StoreError> {
    let count = match &self.filter {
      WorkSourceDiagnosticFilter::Inputs(ids) => ids.len(),
      WorkSourceDiagnosticFilter::Native { issues, .. } => issues.len(),
    };
    if !(1..=64).contains(&self.limit) || count == 0 || count > 100 {
      return Err(source_conflict());
    }
    Ok(())
  }
}
/// Read-only projections; this port has no claims, admission, provider or card mutations.
#[async_trait::async_trait]
pub trait FactorySourceDiagnosticsStore: Send + Sync {
  /// Reads latest reporting states for an already authorized exact association.
  /// Uses operations registered atomically with accepted actions, never a locking Run snapshot.
  async fn source_reporting_diagnostics(
    &self,
    project: ProjectId,
    association: &WorkSourceDiagnosticAssociation,
    limit: u16,
  ) -> Result<WorkSourceReportingPage, StoreError>;
  /// Reads safe retained scan facts for exactly this Project and target version.
  async fn source_scan_diagnostics(
    &self,
    project: ProjectId,
    target: WorkSourceScanTarget,
  ) -> Result<Option<WorkSourceScanDiagnostics>, StoreError>;
  /// Reads settled associations with all visibility predicates before pagination.
  async fn source_diagnostic_correlations(
    &self,
    request: ReadWorkSourceCorrelations,
  ) -> Result<WorkSourceDiagnosticPage, StoreError>;
}

/// Maximum accepted reporting operations in one Source diagnostic summary.
pub const MAX_WORK_SOURCE_REPORTING_OPERATIONS: u16 = 64;
/// Safe reporting state independently of task business status and Run execution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkSourceReportingDiagnostic {
  /// Stable accepted status action operation.
  pub operation_id: FactoryDigest,
  /// Safe dispatch classification.
  pub state: WorkSourceReportingState,
  /// Current bounded dispatch attempt.
  pub attempt: u16,
  /// Earliest permitted retry/dispatch time; not a completed effect timestamp.
  pub available_at: Timestamp,
}
/// Projection health; never a task business status.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkSourceReportingState {
  /// Queued dispatch/retry.
  Pending,
  /// An owned dispatch is in progress, not yet confirmed.
  Claimed,
  /// The adapter effect has been confirmed.
  Applied,
  /// The finite dispatch budget is exhausted.
  Exhausted,
}

impl From<FactoryOutboxState> for WorkSourceReportingState {
  fn from(state: FactoryOutboxState) -> Self {
    match state {
      FactoryOutboxState::Pending => Self::Pending,
      FactoryOutboxState::Claimed => Self::Claimed,
      FactoryOutboxState::Delivered => Self::Applied,
      FactoryOutboxState::Failed => Self::Exhausted,
    }
  }
}
/// Bounded safe projection; no private record payloads or live execution claims.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkSourceReportingPage {
  /// Latest states in operation identity order, restricted to the original Source Reporter.
  pub items: Vec<WorkSourceReportingDiagnostic>,
  /// Additional accepted operations exist beyond the requested limit.
  pub truncated: bool,
}
