use std::{fmt, str::FromStr};

use octacity_server_domain::{AuditFactId, Timestamp};

use crate::AuditMetadata;

/// Stable actor classifications recorded by authoritative mutations.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AuditActorKind {
  /// A request accepted on the unauthenticated trusted-network listener.
  UnauthenticatedManagement,
  /// A management subject verified before entering the application layer.
  AuthenticatedManagement,
  /// An authenticated Agent process.
  Agent,
  /// A normalized Trigger decision.
  Trigger,
  /// A server-owned orchestration decision.
  Orchestrator,
  /// A verified external adapter operation.
  Adapter,
  /// A durable background worker.
  Worker,
}

impl AuditActorKind {
  /// Returns the stable storage and wire spelling.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::UnauthenticatedManagement => "unauthenticated_management",
      Self::AuthenticatedManagement => "authenticated_management",
      Self::Agent => "agent",
      Self::Trigger => "trigger",
      Self::Orchestrator => "orchestrator",
      Self::Adapter => "adapter",
      Self::Worker => "worker",
    }
  }
}

impl fmt::Display for AuditActorKind {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(self.as_str())
  }
}

impl FromStr for AuditActorKind {
  type Err = crate::AuditInputError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    match value {
      "unauthenticated_management" => Ok(Self::UnauthenticatedManagement),
      "authenticated_management" => Ok(Self::AuthenticatedManagement),
      "agent" => Ok(Self::Agent),
      "trigger" => Ok(Self::Trigger),
      "orchestrator" => Ok(Self::Orchestrator),
      "adapter" => Ok(Self::Adapter),
      "worker" => Ok(Self::Worker),
      _ => Err(crate::AuditInputError::InvalidActorKind),
    }
  }
}

/// Available actor identity without inventing an authenticated principal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditActor {
  /// Stable actor classification.
  pub kind: AuditActorKind,
  /// Authenticated or durable worker identity when one actually exists.
  pub identity: Option<String>,
}

/// Stable result recorded for an accepted authoritative mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditOutcome {
  /// The mutation was accepted and committed with this fact.
  Accepted,
}

impl AuditOutcome {
  /// Returns the stable storage and wire spelling.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Accepted => "accepted",
    }
  }
}

impl FromStr for AuditOutcome {
  type Err = crate::AuditInputError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    match value {
      "accepted" => Ok(Self::Accepted),
      _ => Err(crate::AuditInputError::InvalidOutcome),
    }
  }
}

/// One immutable structured fact committed with an authoritative mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditFact {
  /// Stable fact identity.
  pub id: AuditFactId,
  /// Honest available actor information.
  pub actor: AuditActor,
  /// Stable mutation operation name.
  pub operation: String,
  /// Stable target classification.
  pub target_kind: String,
  /// Logical target identity.
  pub target_identity: String,
  /// Request identity when the ingress or worker supplied one.
  pub request_identity: Option<String>,
  /// Idempotency identity when the mutation supports replay.
  pub idempotency_key: Option<String>,
  /// Stable accepted outcome.
  pub outcome: AuditOutcome,
  /// Explicitly selected bounded non-sensitive metadata.
  pub metadata: AuditMetadata,
  /// Authoritative commit time.
  pub occurred_at: Timestamp,
}

#[cfg(test)]
mod tests {
  use std::str::FromStr as _;

  use super::AuditActorKind;

  #[test]
  fn authenticated_management_actor_has_one_stable_representation() {
    let kind = AuditActorKind::AuthenticatedManagement;
    assert_eq!(kind.as_str(), "authenticated_management");
    assert_eq!(AuditActorKind::from_str(kind.as_str()), Ok(kind));
  }
}
