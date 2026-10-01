use octacity_server_store::{
  AuditActor, AuditActorKind as StoreAuditActorKind, ManagementMutation, MutationAuditContext, StoreInputError,
};

use super::{ManagementActorKind, ManagementRequestContext};
use crate::ApplicationError;

/// Binds business intent to the actor and request identity already accepted at management ingress.
pub(crate) fn audited_mutation<T>(
  context: &ManagementRequestContext,
  mutation: T,
) -> Result<ManagementMutation<T>, ApplicationError> {
  let audit = MutationAuditContext::try_from(context).map_err(|_| ApplicationError::invalid())?;
  Ok(ManagementMutation::new(mutation, audit))
}

impl TryFrom<&ManagementRequestContext> for MutationAuditContext {
  type Error = StoreInputError;

  fn try_from(context: &ManagementRequestContext) -> Result<Self, Self::Error> {
    let kind = match context.actor().kind() {
      ManagementActorKind::UnauthenticatedManagement => StoreAuditActorKind::UnauthenticatedManagement,
      ManagementActorKind::AuthenticatedManagement => StoreAuditActorKind::AuthenticatedManagement,
    };
    MutationAuditContext::try_new(
      AuditActor {
        kind,
        identity: context.actor().identity().map(str::to_owned),
      },
      context.security_scope().to_store(),
      context.request_id().to_string(),
    )
  }
}
