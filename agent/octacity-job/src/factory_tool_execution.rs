//! Neutral handoff to the Agent's lease-bound controlled execution owner.
use crate::FactoryPreflightError;
use octacity_execution::{ExecutionBackend, ExecutionTarget};
use octacity_protocol::{FactoryPermissionSetV3, FactoryToolControlV3};
use std::sync::Arc;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

/// Immutable signed scope delivered before any protected input or Source I/O.
/// This value grants no authority itself; the installed owner must verify its
/// exact operation and route, retain the original deadline and attach cleanup.
pub struct FactoryToolExecutionScope {
  /// Exact verified Job identity, independently compared with the active lease.
  pub job_id: String,
  /// Exact verified attempt, independently compared with the active lease.
  pub attempt: u32,
  /// Exact configured backend selected for this signed execution.
  pub backend: Arc<dyn ExecutionBackend>,
  /// Concrete immutable target on that backend, including platform and isolation.
  pub target: ExecutionTarget,
  /// Signed blocking-tool contract, independent from plugin capability alone.
  pub control: FactoryToolControlV3,
  /// Complete already admitted signed permissions; never expanded by the owner.
  pub permissions: FactoryPermissionSetV3,
  /// Original monotonic Job deadline shared by main execution and helper.
  pub deadline: Instant,
  /// Lease monitor's cancellation scope, shared with all controlled operations.
  pub cancellation: CancellationToken,
}

/// Trusted per-Job Agent composition, independent from provider formats and HTTP.
/// Ordinary execution has no owner and controlled execution fails closed. An
/// implementation must return a backend that owns its private channel, physical
/// helper, resource partition and final teardown under this exact scope.
pub trait FactoryToolExecutionOwner: Send + Sync {
  /// Qualifies installed components and the exact route before materialization.
  fn prepare(&self, scope: FactoryToolExecutionScope) -> Result<Arc<dyn ExecutionBackend>, FactoryPreflightError>;
}
