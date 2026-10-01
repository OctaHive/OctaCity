use std::fmt;

use crate::{
  AuditActor, AuditActorKind, IdempotencyKey, MAX_AUDIT_ACTOR_IDENTITY_BYTES, MAX_AUDIT_REQUEST_IDENTITY_BYTES,
  ManagementIdempotencyKey, ManagementSecurityScope, StoreInputError,
};

/// Validated, credential-free attribution for one authoritative management mutation.
///
/// The value records only facts accepted by the application layer. Store
/// adapters must persist these facts as supplied and must never replace them
/// with an infrastructure-selected actor. Its security scope also partitions
/// caller-selected idempotency keys, so equal keys in different accepted scopes
/// have independent outcomes.
#[derive(Clone, Eq, PartialEq)]
pub struct MutationAuditContext {
  actor: AuditActor,
  security_scope: ManagementSecurityScope,
  request_identity: String,
}

impl MutationAuditContext {
  /// Validates management actor evidence and its safe request correlation identity.
  pub fn try_new(
    actor: AuditActor,
    security_scope: ManagementSecurityScope,
    request_identity: impl Into<String>,
  ) -> Result<Self, StoreInputError> {
    validate_management_actor(&actor)?;
    let request_identity = request_identity.into();
    if !is_canonical_text(&request_identity, MAX_AUDIT_REQUEST_IDENTITY_BYTES) {
      return Err(StoreInputError::InvalidMutationAuditContext);
    }
    Ok(Self {
      actor,
      security_scope,
      request_identity,
    })
  }

  /// Borrows the validated actor evidence.
  #[must_use]
  pub const fn actor(&self) -> &AuditActor {
    &self.actor
  }

  /// Borrows the accepted management idempotency partition.
  #[must_use]
  pub const fn security_scope(&self) -> &ManagementSecurityScope {
    &self.security_scope
  }

  /// Binds a caller-selected key to this accepted management security scope.
  #[must_use]
  pub fn scoped_idempotency_key(&self, caller_key: &IdempotencyKey) -> ManagementIdempotencyKey {
    ManagementIdempotencyKey::new(self.security_scope.clone(), caller_key.clone())
  }

  /// Borrows the safe request correlation identity.
  #[must_use]
  pub fn request_identity(&self) -> &str {
    &self.request_identity
  }
}

impl fmt::Debug for MutationAuditContext {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("MutationAuditContext")
      .field("actor_kind", &self.actor.kind)
      .field("actor_identity", &self.actor.identity.as_ref().map(|_| "<redacted>"))
      .field("security_scope", &self.security_scope)
      .field("request_identity", &self.request_identity)
      .finish()
  }
}

/// Complete authoritative input that binds business intent to accepted management attribution.
///
/// The wrapper makes omission of audit context unrepresentable at store port
/// call sites. It deliberately keeps actor evidence outside the business
/// mutation so idempotency fingerprints remain independent of authentication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagementMutation<T> {
  mutation: T,
  audit: MutationAuditContext,
}

impl<T> ManagementMutation<T> {
  /// Binds one mutation to its already validated audit context.
  #[must_use]
  pub const fn new(mutation: T, audit: MutationAuditContext) -> Self {
    Self { mutation, audit }
  }

  /// Borrows the business mutation.
  #[must_use]
  pub const fn mutation(&self) -> &T {
    &self.mutation
  }

  /// Borrows the accepted audit context.
  #[must_use]
  pub const fn audit(&self) -> &MutationAuditContext {
    &self.audit
  }

  /// Separates the complete input into its business and attribution values.
  #[must_use]
  pub fn into_parts(self) -> (T, MutationAuditContext) {
    (self.mutation, self.audit)
  }
}

fn validate_management_actor(actor: &AuditActor) -> Result<(), StoreInputError> {
  let valid = match actor.kind {
    AuditActorKind::UnauthenticatedManagement => actor.identity.is_none(),
    AuditActorKind::AuthenticatedManagement => actor
      .identity
      .as_deref()
      .is_some_and(|identity| is_canonical_text(identity, MAX_AUDIT_ACTOR_IDENTITY_BYTES)),
    AuditActorKind::Agent
    | AuditActorKind::Trigger
    | AuditActorKind::Orchestrator
    | AuditActorKind::Adapter
    | AuditActorKind::Worker => false,
  };
  if valid {
    Ok(())
  } else {
    Err(StoreInputError::InvalidMutationAuditContext)
  }
}

fn is_canonical_text(value: &str, maximum_bytes: usize) -> bool {
  !value.is_empty() && value.len() <= maximum_bytes && value.trim() == value && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[derive(Default)]
  struct AuthoritativeMutationFake {
    mutations: usize,
  }

  impl AuthoritativeMutationFake {
    fn mutate(&mut self, audit: Option<MutationAuditContext>) -> Result<(), StoreInputError> {
      let audit = audit.ok_or(StoreInputError::InvalidMutationAuditContext)?;
      let _input = ManagementMutation::new((), audit);
      self.mutations += 1;
      Ok(())
    }
  }

  #[test]
  fn authoritative_fake_rejects_absent_or_invalid_actor_evidence_before_mutation() {
    let invalid = [
      AuditActor {
        kind: AuditActorKind::UnauthenticatedManagement,
        identity: Some("invented-subject".to_owned()),
      },
      AuditActor {
        kind: AuditActorKind::AuthenticatedManagement,
        identity: None,
      },
      AuditActor {
        kind: AuditActorKind::AuthenticatedManagement,
        identity: Some(" subject ".to_owned()),
      },
      AuditActor {
        kind: AuditActorKind::AuthenticatedManagement,
        identity: Some("a".repeat(MAX_AUDIT_ACTOR_IDENTITY_BYTES + 1)),
      },
      AuditActor {
        kind: AuditActorKind::Agent,
        identity: Some("agent-1".to_owned()),
      },
    ];
    let mut store = AuthoritativeMutationFake::default();
    assert_eq!(store.mutate(None), Err(StoreInputError::InvalidMutationAuditContext));
    for actor in invalid {
      assert_eq!(
        MutationAuditContext::try_new(actor, ManagementSecurityScope::trusted_network(), "request-1"),
        Err(StoreInputError::InvalidMutationAuditContext)
      );
    }
    assert_eq!(store.mutations, 0);
  }

  #[test]
  fn authoritative_fake_accepts_both_valid_management_actor_shapes() {
    let mut store = AuthoritativeMutationFake::default();
    store
      .mutate(Some(
        MutationAuditContext::try_new(
          AuditActor {
            kind: AuditActorKind::UnauthenticatedManagement,
            identity: None,
          },
          ManagementSecurityScope::trusted_network(),
          "request-1",
        )
        .unwrap(),
      ))
      .unwrap();
    store
      .mutate(Some(
        MutationAuditContext::try_new(
          AuditActor {
            kind: AuditActorKind::AuthenticatedManagement,
            identity: Some("operator-1".to_owned()),
          },
          ManagementSecurityScope::new("operator:1").unwrap(),
          "request-2",
        )
        .unwrap(),
      ))
      .unwrap();
    assert_eq!(store.mutations, 2);
  }

  #[test]
  fn audit_context_rejects_invalid_request_identity_and_redacts_actor_identity() {
    let actor = AuditActor {
      kind: AuditActorKind::AuthenticatedManagement,
      identity: Some("private-subject".to_owned()),
    };
    for request_identity in ["", " request-1", "request-1\n"] {
      assert_eq!(
        MutationAuditContext::try_new(
          actor.clone(),
          ManagementSecurityScope::new("operator:1").unwrap(),
          request_identity,
        ),
        Err(StoreInputError::InvalidMutationAuditContext)
      );
    }
    assert_eq!(
      MutationAuditContext::try_new(
        actor.clone(),
        ManagementSecurityScope::new("operator:1").unwrap(),
        "r".repeat(MAX_AUDIT_REQUEST_IDENTITY_BYTES + 1),
      ),
      Err(StoreInputError::InvalidMutationAuditContext)
    );

    let context =
      MutationAuditContext::try_new(actor, ManagementSecurityScope::new("operator:1").unwrap(), "request-1").unwrap();
    let debug = format!("{context:?}");
    assert!(!debug.contains("private-subject"));
    assert!(!debug.contains("operator:1"));
    assert!(debug.contains("<redacted>"));

    let caller_key = IdempotencyKey::new("caller-key").unwrap();
    let scoped_key = context.scoped_idempotency_key(&caller_key);
    assert_eq!(scoped_key.security_scope().as_str(), "operator:1");
    assert_eq!(scoped_key.caller_key(), &caller_key);
  }
}
