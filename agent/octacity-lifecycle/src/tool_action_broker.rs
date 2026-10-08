//! Lease-bound Agent broker for blocking coding-harness tool actions.

use std::{sync::Arc, time::Duration};

use octacity_coordinator::{Registration, ToolActionCoordinator};
use octacity_job::{AuthorizedFactoryToolAction, FactoryToolActionEnforcementError, enforce_factory_tool_action};
use octacity_protocol::{
  AuthorizeToolActionRequest, COORDINATOR_PROTOCOL_VERSION, FactoryPermissionSetV3, FactoryToolActionProposalV3,
  LeaseAssignment,
};
use thiserror::Error;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Secret-safe reason the blocking broker refused to release an action.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AgentToolActionBrokerError {
  /// The Job or lease lifecycle ended before an authoritative disposition.
  #[error("protected tool-action authorization was cancelled")]
  Cancelled,
  /// The server did not answer inside the Agent-owned deadline.
  #[error("protected tool-action authorization timed out")]
  TimedOut,
  /// Transport, authentication, fencing, or server policy rejected the exchange.
  #[error("protected tool-action authorization was denied")]
  Denied,
  /// The response did not authorize the exact unchanged proposal under both ceilings.
  #[error("protected tool-action authorization did not match the blocked action")]
  Mismatch,
}

/// Blocking broker bound to one exact registration, Job, attempt, lease, and fence.
///
/// The coordinator adapter retains the Agent bearer and all provider credentials
/// remain server-side. The harness supplies only a bounded proposal; it receives
/// an executable value only after the server response and final local revalidation.
pub struct AgentToolActionBroker {
  coordinator: Arc<dyn ToolActionCoordinator>,
  registration: Registration,
  lease: LeaseAssignment,
  signed_permissions: FactoryPermissionSetV3,
  local_permissions: FactoryPermissionSetV3,
  deadline: Duration,
  cancellation: CancellationToken,
}

impl std::fmt::Debug for AgentToolActionBroker {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("AgentToolActionBroker")
      .field("registration_id", &self.registration.registration_id)
      .field("job_id", &self.lease.job_id)
      .field("attempt", &self.lease.attempt)
      .field("lease_id", &self.lease.lease_id)
      .field("deadline", &self.deadline)
      .finish_non_exhaustive()
  }
}

impl AgentToolActionBroker {
  /// Constructs one fail-closed broker for the active leased execution.
  pub fn new(
    coordinator: Arc<dyn ToolActionCoordinator>,
    registration: Registration,
    lease: LeaseAssignment,
    signed_permissions: FactoryPermissionSetV3,
    local_permissions: FactoryPermissionSetV3,
    deadline: Duration,
    cancellation: CancellationToken,
  ) -> Result<Self, AgentToolActionBrokerError> {
    if deadline.is_zero()
      || registration.validate().is_err()
      || signed_permissions.validate().is_err()
      || local_permissions.validate().is_err()
    {
      return Err(AgentToolActionBrokerError::Denied);
    }
    Ok(Self {
      coordinator,
      registration,
      lease,
      signed_permissions,
      local_permissions,
      deadline,
      cancellation,
    })
  }

  /// Blocks until the server decides and the Agent revalidates the exact action.
  pub async fn authorize(
    &self,
    proposal: FactoryToolActionProposalV3,
  ) -> Result<AuthorizedFactoryToolAction, AgentToolActionBrokerError> {
    if self.cancellation.is_cancelled() {
      return Err(AgentToolActionBrokerError::Cancelled);
    }
    let canonical = proposal
      .clone()
      .canonicalize()
      .map_err(|_| AgentToolActionBrokerError::Denied)?;
    let inside_signed = canonical
      .is_permitted_by(&self.signed_permissions)
      .map_err(|_| AgentToolActionBrokerError::Denied)?;
    let inside_local = canonical
      .is_permitted_by(&self.local_permissions)
      .map_err(|_| AgentToolActionBrokerError::Denied)?;
    if !inside_signed || !inside_local {
      return Err(AgentToolActionBrokerError::Denied);
    }
    let request = AuthorizeToolActionRequest {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: Uuid::new_v4().simple().to_string(),
      registration_id: self.registration.registration_id.clone(),
      lease: (&self.lease).into(),
      proposal_sha256: canonical.proposal_sha256(),
      proposal_summary: canonical.redacted_summary(),
    };
    request.validate().map_err(|_| AgentToolActionBrokerError::Denied)?;
    let exchange =
      self
        .coordinator
        .authorize_tool_action(&self.registration, &self.lease, &request, self.cancellation.clone());
    let response = tokio::select! {
      () = self.cancellation.cancelled() => return Err(AgentToolActionBrokerError::Cancelled),
      response = timeout(self.deadline, exchange) => match response {
        Err(_) => return Err(AgentToolActionBrokerError::TimedOut),
        Ok(Err(_)) => return Err(AgentToolActionBrokerError::Denied),
        Ok(Ok(response)) => response,
      },
    };
    response
      .validate(&request)
      .map_err(|_| AgentToolActionBrokerError::Mismatch)?;
    enforce_factory_tool_action(
      proposal,
      &response.decision,
      &self.signed_permissions,
      &self.local_permissions,
    )
    .map_err(map_enforcement)
  }
}

const fn map_enforcement(error: FactoryToolActionEnforcementError) -> AgentToolActionBrokerError {
  match error {
    FactoryToolActionEnforcementError::ProposalChanged => AgentToolActionBrokerError::Mismatch,
    FactoryToolActionEnforcementError::InvalidProposal
    | FactoryToolActionEnforcementError::InvalidPermissionCeiling
    | FactoryToolActionEnforcementError::NotAllowed
    | FactoryToolActionEnforcementError::OutsidePermissionCeiling => AgentToolActionBrokerError::Denied,
  }
}

#[cfg(test)]
mod tests {
  use std::sync::Mutex;

  use async_trait::async_trait;
  use octacity_coordinator::CoordinatorError;
  use octacity_protocol::{
    AuthorizeToolActionResponse, FactoryCommandArgumentV3, FactoryCommandPermissionV3, FactoryImmutableReferenceV3,
    FactoryMountModeV3, FactoryMountPermissionV3, FactoryOutputPermissionsV3, FactoryResourceLimitsV3,
    FactoryToolActionDecisionSourceV3, FactoryToolActionDecisionV3, FactoryToolActionDispositionV3,
    FactoryToolPathAccessV3, FactoryToolPathV3, SIGNATURE_ALGORITHM, SignedEnvelope,
  };
  use tokio::sync::Notify;

  use super::*;

  const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

  struct BlockingCoordinator {
    entered: Notify,
    release: Notify,
    disposition: FactoryToolActionDispositionV3,
    mismatch: bool,
    requests: Mutex<Vec<AuthorizeToolActionRequest>>,
  }

  impl BlockingCoordinator {
    fn new(disposition: FactoryToolActionDispositionV3) -> Self {
      Self {
        entered: Notify::new(),
        release: Notify::new(),
        disposition,
        mismatch: false,
        requests: Mutex::new(Vec::new()),
      }
    }

    fn mismatched() -> Self {
      Self {
        mismatch: true,
        ..Self::new(FactoryToolActionDispositionV3::Allow)
      }
    }
  }

  #[async_trait]
  impl ToolActionCoordinator for BlockingCoordinator {
    async fn authorize_tool_action(
      &self,
      _registration: &Registration,
      _lease: &LeaseAssignment,
      request: &AuthorizeToolActionRequest,
      cancellation: CancellationToken,
    ) -> Result<AuthorizeToolActionResponse, CoordinatorError> {
      self.requests.lock().unwrap().push(request.clone());
      self.entered.notify_one();
      tokio::select! {
        () = self.release.notified() => {},
        () = cancellation.cancelled() => return Err(CoordinatorError::Cancelled),
      }
      let decision = FactoryToolActionDecisionV3::new(
        request.proposal_sha256.clone(),
        self.disposition,
        if self.mismatch {
          FactoryToolActionDecisionSourceV3::DecisionSignal
        } else {
          FactoryToolActionDecisionSourceV3::HardPolicy
        },
        self.mismatch.then(|| "a".repeat(64)),
      )
      .unwrap();
      let mut response = AuthorizeToolActionResponse::new(request, decision).unwrap();
      if self.mismatch {
        response.authorization_sha256 = "f".repeat(64);
      }
      Ok(response)
    }
  }

  #[tokio::test]
  async fn action_remains_blocked_until_the_exact_lease_bound_allow_arrives() {
    let coordinator = Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Allow));
    let broker = Arc::new(broker(
      coordinator.clone(),
      Duration::from_secs(1),
      CancellationToken::new(),
    ));
    let proposal = proposal();
    let authorization = tokio::spawn({
      let broker = broker.clone();
      let proposal = proposal.clone();
      async move { broker.authorize(proposal).await }
    });

    coordinator.entered.notified().await;
    assert!(!authorization.is_finished(), "action escaped before server disposition");
    let request = coordinator.requests.lock().unwrap()[0].clone();
    assert_eq!(request.registration_id, "registration");
    assert_eq!(request.lease.lease_id, "lease");
    assert_eq!(request.lease.fencing_token, "fence");
    let wire = serde_json::to_string(&request).unwrap();
    assert!(!wire.contains("provider-api-key"));
    assert!(!wire.contains("cargo test"));
    assert!(!wire.contains("/workspace/source"));

    coordinator.release.notify_one();
    assert_eq!(authorization.await.unwrap().unwrap().into_proposal(), proposal);
  }

  #[tokio::test]
  async fn cancellation_timeout_deny_and_receipt_mismatch_never_release_an_action() {
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let cancelled = broker(
      Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Allow)),
      Duration::from_secs(1),
      cancellation,
    );
    assert_eq!(
      cancelled.authorize(proposal()).await.unwrap_err(),
      AgentToolActionBrokerError::Cancelled
    );

    let timed_out = broker(
      Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Allow)),
      Duration::from_millis(1),
      CancellationToken::new(),
    );
    assert_eq!(
      timed_out.authorize(proposal()).await.unwrap_err(),
      AgentToolActionBrokerError::TimedOut
    );

    for coordinator in [
      Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Deny)),
      Arc::new(BlockingCoordinator::mismatched()),
    ] {
      let expected = if coordinator.mismatch {
        AgentToolActionBrokerError::Mismatch
      } else {
        AgentToolActionBrokerError::Denied
      };
      let broker = Arc::new(broker(
        coordinator.clone(),
        Duration::from_secs(1),
        CancellationToken::new(),
      ));
      let authorization = tokio::spawn({
        let broker = broker.clone();
        async move { broker.authorize(proposal()).await }
      });
      coordinator.entered.notified().await;
      coordinator.release.notify_one();
      assert_eq!(authorization.await.unwrap().unwrap_err(), expected);
    }
  }

  #[tokio::test]
  async fn out_of_envelope_actions_never_cross_the_coordinator_boundary() {
    let coordinator = Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Allow));
    let broker = broker(coordinator.clone(), Duration::from_secs(1), CancellationToken::new());
    let mut outside = proposal();
    outside.paths[0].path = "/workspace/outside".to_owned();

    assert_eq!(
      broker.authorize(outside).await.unwrap_err(),
      AgentToolActionBrokerError::Denied
    );
    assert!(coordinator.requests.lock().unwrap().is_empty());
  }

  fn broker(
    coordinator: Arc<dyn ToolActionCoordinator>,
    deadline: Duration,
    cancellation: CancellationToken,
  ) -> AgentToolActionBroker {
    AgentToolActionBroker::new(
      coordinator,
      Registration {
        agent_id: "agent".to_owned(),
        registration_id: "registration".to_owned(),
        execution_contract_version: 3,
        max_retry_delay: Duration::from_secs(1),
      },
      LeaseAssignment {
        lease_id: "lease".to_owned(),
        job_id: "job".to_owned(),
        attempt: 1,
        fencing_token: "fence".to_owned(),
        issued_at: 1,
        expires_at: 2,
        signed_job_spec: SignedEnvelope {
          key_id: "key".to_owned(),
          algorithm: SIGNATURE_ALGORITHM.to_owned(),
          payload: "payload".to_owned(),
          signature: "signature".to_owned(),
        },
        protected_inputs: vec![],
      },
      permissions(),
      permissions(),
      deadline,
      cancellation,
    )
    .unwrap()
  }

  fn proposal() -> FactoryToolActionProposalV3 {
    FactoryToolActionProposalV3 {
      tool: reference("bash"),
      executable: reference("codex-cli"),
      arguments: vec!["cargo test".to_owned()],
      paths: vec![FactoryToolPathV3 {
        path: "/workspace/source".to_owned(),
        access: FactoryToolPathAccessV3::Write,
      }],
      network_hosts: vec![],
      secret_profiles: vec![],
      workload_identity_profiles: vec![],
      descendants: 1,
      resources: limits(),
      outputs: outputs(),
    }
  }

  fn permissions() -> FactoryPermissionSetV3 {
    let executable = reference("codex-cli");
    FactoryPermissionSetV3 {
      plugins: vec![reference("codex")],
      executables: vec![executable.clone()],
      tools: vec![reference("bash")],
      commands: vec![FactoryCommandPermissionV3 {
        executable,
        arguments: vec![FactoryCommandArgumentV3::Any { max_bytes: 256 }],
      }],
      max_descendants: 1,
      mounts: vec![
        FactoryMountPermissionV3 {
          root: "/octacity/protected".to_owned(),
          mode: FactoryMountModeV3::ReadOnly,
        },
        FactoryMountPermissionV3 {
          root: "/workspace/output".to_owned(),
          mode: FactoryMountModeV3::ReadWrite,
        },
        FactoryMountPermissionV3 {
          root: "/workspace/scratch".to_owned(),
          mode: FactoryMountModeV3::ReadWrite,
        },
        FactoryMountPermissionV3 {
          root: "/workspace/source".to_owned(),
          mode: FactoryMountModeV3::ReadWrite,
        },
      ],
      network_hosts: vec![],
      secret_profiles: vec![],
      workload_identity_profiles: vec![],
      resources: limits(),
      outputs: outputs(),
    }
  }

  const fn limits() -> FactoryResourceLimitsV3 {
    FactoryResourceLimitsV3 {
      cpu_millis: 500,
      memory_bytes: 1024,
      disk_bytes: 1024,
      process_count: 2,
      elapsed_millis: 30_000,
    }
  }

  fn outputs() -> FactoryOutputPermissionsV3 {
    FactoryOutputPermissionsV3 {
      kinds: vec!["codex-result".to_owned()],
      max_artifact_count: 1,
      max_artifact_bytes: 1024,
      max_report_count: 0,
      max_report_bytes: 0,
    }
  }

  fn reference(identity: &str) -> FactoryImmutableReferenceV3 {
    FactoryImmutableReferenceV3 {
      identity: identity.to_owned(),
      version: "1.0.0".to_owned(),
      sha256: DIGEST.to_owned(),
    }
  }
}
