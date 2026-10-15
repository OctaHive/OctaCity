//! Trusted read dispatch outside the coding harness.

use std::{
  future::Future,
  pin::Pin,
  sync::{Arc, Mutex},
};

use octacity_protocol::{
  FactoryToolResponseLimitsV3, MAX_FACTORY_TOOL_RESPONSE_BYTES, ProtectedReadChannelOperationV1,
  ProtectedReadChannelRequestV1,
};
use tokio::io::{AsyncRead, AsyncReadExt};

use super::*;

const MAX_TOOL_ACTION_CHANNEL_BYTES: usize = 4 * 1024 * 1024;

// One big-endian u32 length and bounded JSON request, shared by both channel
// operations. Only the versioned neutral request is accepted.
async fn read_tool_action_frame(
  mut input: impl AsyncRead + Unpin,
) -> Result<ProtectedReadChannelRequestV1, AgentToolActionBrokerError> {
  let size = input.read_u32().await.map_err(|_| AgentToolActionBrokerError::Denied)? as usize;
  if size == 0 || size > MAX_TOOL_ACTION_CHANNEL_BYTES {
    return Err(AgentToolActionBrokerError::Denied);
  }
  let mut bytes = vec![0; size];
  input
    .read_exact(&mut bytes)
    .await
    .map_err(|_| AgentToolActionBrokerError::Denied)?;
  serde_json::from_slice(&bytes).map_err(|_| AgentToolActionBrokerError::Denied)
}

/// Installed reader that owns the physical operation and consumes its proof.
///
/// Before I/O, implementations must verify the action's exact installed tool,
/// executable, arguments and read-only effects. An authorized arbitrary command
/// must not be reinterpreted as a read. The backend owns filesystem confinement,
/// bounded allocation and cleanup when cancelled or when its future is dropped;
/// it must not give the harness independent access to protected inputs.
pub trait ProtectedReadBackend: Send + Sync {
  /// Executes precisely the consumed operation, returning at most `max_bytes`.
  /// No proposal, executable override or unverified path is supplied separately.
  fn read<'a>(
    &'a self,
    action: AuthorizedFactoryToolAction,
    max_bytes: u64,
    cancellation: CancellationToken,
  ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AgentToolActionBrokerError>> + Send + 'a>>;
}

/// Authorizes and dispatches one protected read without releasing an executable
/// action to the harness. Authorization and I/O share the broker's deadline and
/// cancellation, and failed or cancelled operations release no read output.
pub struct ProtectedReadDispatcher {
  broker: Arc<AgentToolActionBroker>,
  backend: Arc<dyn ProtectedReadBackend>,
  max_bytes: u64,
  responses: ResponseQuota,
}

impl ProtectedReadDispatcher {
  pub(super) fn channel_scope(&self) -> (std::time::Duration, CancellationToken) {
    (self.broker.deadline, self.broker.cancellation.child_token())
  }

  /// Installs one Job's reader using its verified signed response allocation
  /// and a local per-call ceiling of 1..=4 MiB. All calls share the aggregate;
  /// repeated proposals never receive a fresh byte allocation.
  pub fn new(
    broker: Arc<AgentToolActionBroker>,
    backend: Arc<dyn ProtectedReadBackend>,
    max_bytes: u64,
    signed_limits: FactoryToolResponseLimitsV3,
  ) -> Result<Self, AgentToolActionBrokerError> {
    if max_bytes == 0 || max_bytes > MAX_FACTORY_TOOL_RESPONSE_BYTES || signed_limits.validate().is_err() {
      return Err(AgentToolActionBrokerError::Denied);
    }
    Ok(Self {
      broker,
      backend,
      max_bytes: max_bytes.min(signed_limits.per_call_bytes),
      responses: ResponseQuota {
        total: signed_limits.total_bytes,
        state: Mutex::new(ResponseState {
          reserved_and_spent: 0,
          failed: false,
        }),
      },
    })
  }

  /// Reads only after exact local and lease-bound server authorization succeeds.
  pub async fn read(&self, proposal: FactoryToolActionProposalV3) -> Result<Vec<u8>, AgentToolActionBrokerError> {
    self.dispatch(self.broker.authorize(proposal)).await
  }

  /// Handles one bounded versioned private-channel frame. Authorization-only
  /// checks return an empty response without physical I/O or byte reservation.
  /// Every read revalidates the exact proposal; receipt and dispatch share one
  /// deadline and cancellation scope, with at most one coordinator call.
  pub async fn read_channel(&self, input: impl AsyncRead + Unpin) -> Result<Vec<u8>, AgentToolActionBrokerError> {
    let dispatch = async {
      let request = read_tool_action_frame(input).await?;
      if request.schema_version != 1 {
        return Err(AgentToolActionBrokerError::Denied);
      }
      match request.operation {
        ProtectedReadChannelOperationV1::Authorize => {
          self.broker.authorize(request.proposal).await?;
          Ok(Vec::new())
        }
        ProtectedReadChannelOperationV1::Read => self.read(request.proposal).await,
      }
    };
    tokio::select! {
      biased;
      () = self.broker.cancellation.cancelled() => Err(AgentToolActionBrokerError::Cancelled),
      result = timeout(self.broker.deadline, dispatch) => result.map_err(|_| AgentToolActionBrokerError::TimedOut)?,
    }
  }

  async fn dispatch(
    &self,
    authorization: impl Future<Output = Result<AuthorizedFactoryToolAction, AgentToolActionBrokerError>>,
  ) -> Result<Vec<u8>, AgentToolActionBrokerError> {
    let read = async {
      let action = authorization.await?;
      if self.broker.cancellation.is_cancelled() {
        return Err(AgentToolActionBrokerError::Cancelled);
      }
      let allowance = self.responses.reserve(self.max_bytes)?;
      let bytes = self
        .backend
        .read(action, allowance.bytes, self.broker.cancellation.clone())
        .await?;
      // Produced bytes remain charged even if cancellation withholds them.
      allowance.settle(bytes.len())?;
      if self.broker.cancellation.is_cancelled() {
        return Err(AgentToolActionBrokerError::Cancelled);
      }
      Ok(bytes)
    };
    tokio::select! {
      biased;
      () = self.broker.cancellation.cancelled() => Err(AgentToolActionBrokerError::Cancelled),
      result = timeout(self.broker.deadline, read) => result.map_err(|_| AgentToolActionBrokerError::TimedOut)?,
    }
  }
}

struct ResponseQuota {
  total: u64,
  state: Mutex<ResponseState>,
}

struct ResponseState {
  reserved_and_spent: u64,
  failed: bool,
}

struct ResponseAllowance<'a> {
  quota: &'a ResponseQuota,
  bytes: u64,
}

impl ResponseQuota {
  fn reserve(&self, per_call: u64) -> Result<ResponseAllowance<'_>, AgentToolActionBrokerError> {
    let mut state = self.state.lock().map_err(|_| AgentToolActionBrokerError::Denied)?;
    let remaining = self
      .total
      .checked_sub(state.reserved_and_spent)
      .ok_or(AgentToolActionBrokerError::Denied)?;
    let bytes = per_call.min(remaining);
    if state.failed || bytes == 0 {
      return Err(AgentToolActionBrokerError::Denied);
    }
    state.reserved_and_spent = state
      .reserved_and_spent
      .checked_add(bytes)
      .ok_or(AgentToolActionBrokerError::Denied)?;
    Ok(ResponseAllowance { quota: self, bytes })
  }
}

impl ResponseAllowance<'_> {
  // An error or dropped read cannot establish how many bytes its backend
  // produced. It retains its entire reservation; no Drop refund is safe.
  fn settle(self, produced: usize) -> Result<(), AgentToolActionBrokerError> {
    let mut state = self
      .quota
      .state
      .lock()
      .map_err(|_| AgentToolActionBrokerError::Denied)?;
    let produced = u64::try_from(produced).map_err(|_| AgentToolActionBrokerError::Denied)?;
    if produced > self.bytes {
      state.failed = true;
    }
    if state.failed {
      return Err(AgentToolActionBrokerError::Denied);
    }
    state.reserved_and_spent = state
      .reserved_and_spent
      .checked_sub(self.bytes - produced)
      .ok_or(AgentToolActionBrokerError::Denied)?;
    Ok(())
  }
}
