use std::{fmt, sync::Arc};

use async_trait::async_trait;

use crate::{Command, Query};

use super::{
  ManagementAction, ManagementAuthorizationDenial, ManagementAuthorizationGrant, ManagementAuthorizationPolicy,
  ManagementRequestContext, ManagementRequestId, ManagementResource,
};

/// Typed action and resource mapping owned by a management command or query.
pub trait ManagementAuthorizationTarget {
  /// Returns the stable capability requested by this operation.
  fn management_action(&self) -> ManagementAction;

  /// Returns the smallest resource description known before application dispatch.
  fn management_resource(&self) -> ManagementResource;
}

/// Context-aware implementation seam for one authorized management command.
#[async_trait]
pub trait ManagementCommandUseCase<C>: Send + Sync
where
  C: Command,
{
  /// Typed failure returned by the application use case.
  type Error: Send;

  /// Executes a command only after the decorator has produced a grant.
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: C,
  ) -> Result<C::Outcome, Self::Error>;
}

/// Context-aware implementation seam for one authorized management query.
#[async_trait]
pub trait ManagementQueryUseCase<Q>: Send + Sync
where
  Q: Query,
{
  /// Typed failure returned by the application use case.
  type Error: Send;

  /// Executes a query only after the decorator has produced a grant.
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: Q,
  ) -> Result<Q::Outcome, Self::Error>;
}

/// Stable forbidden failure carrying only safe request correlation.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ManagementAuthorizationFailure {
  request_id: ManagementRequestId,
  denial: ManagementAuthorizationDenial,
}

impl ManagementAuthorizationFailure {
  fn new(request_id: ManagementRequestId, denial: ManagementAuthorizationDenial) -> Self {
    Self { request_id, denial }
  }

  /// Returns the request identity also safe to expose in the transport error envelope.
  #[must_use]
  pub const fn request_id(self) -> ManagementRequestId {
    self.request_id
  }

  /// Returns the opaque denial without policy reasons or resource details.
  #[must_use]
  pub const fn denial(self) -> ManagementAuthorizationDenial {
    self.denial
  }
}

impl fmt::Debug for ManagementAuthorizationFailure {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("ManagementAuthorizationFailure")
      .field("request_id", &self.request_id)
      .field("denial", &self.denial)
      .finish()
  }
}

impl fmt::Display for ManagementAuthorizationFailure {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "management request {} is forbidden", self.request_id)
  }
}

impl std::error::Error for ManagementAuthorizationFailure {}

/// Failure returned by authorized management dispatch.
pub enum ManagementHandlerError<E> {
  /// Policy rejected the operation before application dispatch.
  Forbidden(ManagementAuthorizationFailure),
  /// The authorized application use case failed.
  Application(E),
}

impl<E> ManagementHandlerError<E> {
  /// Borrows the authorization failure when policy denied dispatch.
  #[must_use]
  pub const fn forbidden(&self) -> Option<&ManagementAuthorizationFailure> {
    match self {
      Self::Forbidden(failure) => Some(failure),
      Self::Application(_) => None,
    }
  }

  /// Borrows the application failure when the authorized use case ran and failed.
  #[must_use]
  pub const fn application(&self) -> Option<&E> {
    match self {
      Self::Forbidden(_) => None,
      Self::Application(error) => Some(error),
    }
  }
}

impl<E: fmt::Debug> fmt::Debug for ManagementHandlerError<E> {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Forbidden(failure) => formatter.debug_tuple("Forbidden").field(failure).finish(),
      Self::Application(error) => formatter.debug_tuple("Application").field(error).finish(),
    }
  }
}

impl<E: fmt::Display> fmt::Display for ManagementHandlerError<E> {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Forbidden(failure) => fmt::Display::fmt(failure, formatter),
      Self::Application(error) => fmt::Display::fmt(error, formatter),
    }
  }
}

impl<E> std::error::Error for ManagementHandlerError<E>
where
  E: std::error::Error + 'static,
{
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      Self::Forbidden(failure) => Some(failure),
      Self::Application(error) => Some(error),
    }
  }
}

/// Mandatory policy decorator for context-aware management command use cases.
pub struct AuthorizedCommandHandler<H> {
  policy: Arc<dyn ManagementAuthorizationPolicy>,
  inner: Arc<H>,
}

impl<H> AuthorizedCommandHandler<H> {
  /// Wraps a command use case with the selected application-layer policy.
  #[must_use]
  pub fn new(policy: Arc<dyn ManagementAuthorizationPolicy>, inner: Arc<H>) -> Self {
    Self { policy, inner }
  }

  /// Authorizes and then dispatches one typed management command.
  pub async fn handle_command<C>(
    &self,
    context: &ManagementRequestContext,
    command: C,
  ) -> Result<C::Outcome, ManagementHandlerError<H::Error>>
  where
    C: Command + ManagementAuthorizationTarget,
    H: ManagementCommandUseCase<C>,
  {
    let action = command.management_action();
    let resource = command.management_resource();
    let grant = self
      .policy
      .authorize(context, action, &resource)
      .await
      .map_err(|denial| {
        ManagementHandlerError::Forbidden(ManagementAuthorizationFailure::new(context.request_id(), denial))
      })?;
    self
      .inner
      .execute_management_command(context, &grant, command)
      .await
      .map_err(ManagementHandlerError::Application)
  }
}

/// Mandatory policy decorator for context-aware management query use cases.
pub struct AuthorizedQueryHandler<H> {
  policy: Arc<dyn ManagementAuthorizationPolicy>,
  inner: Arc<H>,
}

impl<H> AuthorizedQueryHandler<H> {
  /// Wraps a query use case with the selected application-layer policy.
  #[must_use]
  pub fn new(policy: Arc<dyn ManagementAuthorizationPolicy>, inner: Arc<H>) -> Self {
    Self { policy, inner }
  }

  /// Authorizes and then dispatches one typed management query.
  pub async fn handle_query<Q>(
    &self,
    context: &ManagementRequestContext,
    query: Q,
  ) -> Result<Q::Outcome, ManagementHandlerError<H::Error>>
  where
    Q: Query + ManagementAuthorizationTarget,
    H: ManagementQueryUseCase<Q>,
  {
    let action = query.management_action();
    let resource = query.management_resource();
    let grant = self
      .policy
      .authorize(context, action, &resource)
      .await
      .map_err(|denial| {
        ManagementHandlerError::Forbidden(ManagementAuthorizationFailure::new(context.request_id(), denial))
      })?;
    self
      .inner
      .execute_management_query(context, &grant, query)
      .await
      .map_err(ManagementHandlerError::Application)
  }
}
