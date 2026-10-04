use std::{collections::BTreeSet, num::NonZeroU16, sync::Arc};

use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_store::{ListOperatorAttention, OperatorAttentionPagePosition, OperatorAttentionStore};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::opaque_cursor;
use crate::{
  ApplicationError, ManagementAction, ManagementAuthorizationMapping, ManagementAuthorizationTarget,
  ManagementQueryUseCase, ManagementResource, ManagementResourceKind, ManagementResourceResult, Query,
};

const OPERATOR_ATTENTION_CURSOR_VERSION: u8 = 1;

/// Maximum number of selected resource identities in one attention scope.
pub const MAX_OPERATOR_ATTENTION_SCOPE_TARGETS: usize = octacity_server_store::MAX_OPERATOR_ATTENTION_TARGETS;
/// Maximum number of safe items returned by one attention page.
pub const MAX_OPERATOR_ATTENTION_RESULT_PAGE_SIZE: u16 = octacity_server_store::MAX_OPERATOR_ATTENTION_PAGE_SIZE;
/// Maximum encoded UTF-8 bytes accepted for one opaque attention cursor.
pub const MAX_OPERATOR_ATTENTION_CURSOR_BYTES: usize = 1_024;
/// Maximum UTF-8 bytes returned in one machine-readable attention code.
pub const MAX_OPERATOR_ATTENTION_CODE_BYTES: usize = octacity_server_store::MAX_OPERATOR_ATTENTION_CODE_BYTES;
/// Maximum UTF-8 bytes returned in one safe operator summary.
pub const MAX_OPERATOR_ATTENTION_SUMMARY_BYTES: usize = octacity_server_store::MAX_OPERATOR_ATTENTION_SUMMARY_BYTES;

pub use octacity_server_store::{
  OperatorAttentionCategory, OperatorAttentionId, OperatorAttentionSeverity, OperatorAttentionTarget,
};

/// Typed bounded target, critical-condition, and occurrence-time selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperatorAttentionScopeInput {
  scope: octacity_server_store::OperatorAttentionScope,
}

impl OperatorAttentionScopeInput {
  /// Validates a duplicate-free bounded scope and supported Unix-millisecond time bounds.
  pub fn try_new(
    targets: impl IntoIterator<Item = OperatorAttentionTarget>,
    include_critical_conditions: bool,
    occurred_from_unix_ms: Option<i64>,
    occurred_through_unix_ms: Option<i64>,
  ) -> Result<Self, OperatorAttentionInputError> {
    let mut selected = BTreeSet::new();
    for target in targets {
      if !selected.insert(target) || selected.len() > MAX_OPERATOR_ATTENTION_SCOPE_TARGETS {
        return Err(OperatorAttentionInputError::InvalidScope);
      }
    }
    let occurred_from = parse_time(occurred_from_unix_ms)?;
    let occurred_through = parse_time(occurred_through_unix_ms)?;
    let scope = octacity_server_store::OperatorAttentionScope::new(
      selected,
      include_critical_conditions,
      occurred_from,
      occurred_through,
    )
    .map_err(|error| match error {
      octacity_server_store::OperatorAttentionValueError::InvalidTimeRange => {
        OperatorAttentionInputError::InvalidTimeRange
      }
      _ => OperatorAttentionInputError::InvalidScope,
    })?;
    Ok(Self { scope })
  }

  /// Returns the selected typed resource targets.
  #[must_use]
  pub const fn targets(&self) -> &BTreeSet<OperatorAttentionTarget> {
    self.scope.targets()
  }

  /// Returns whether critical system conditions were explicitly requested.
  #[must_use]
  pub const fn includes_critical_conditions(&self) -> bool {
    self.scope.includes_critical_conditions()
  }

  /// Returns the inclusive lower occurrence bound in Unix milliseconds.
  #[must_use]
  pub fn occurred_from_unix_ms(&self) -> Option<i64> {
    self.scope.occurred_from().map(Timestamp::unix_millis)
  }

  /// Returns the inclusive upper occurrence bound in Unix milliseconds.
  #[must_use]
  pub fn occurred_through_unix_ms(&self) -> Option<i64> {
    self.scope.occurred_through().map(Timestamp::unix_millis)
  }
}

fn parse_time(value: Option<i64>) -> Result<Option<Timestamp>, OperatorAttentionInputError> {
  value
    .map(|value| Timestamp::from_unix_millis(value).map_err(|_| OperatorAttentionInputError::InvalidTimeRange))
    .transpose()
}

/// Invalid operator-attention request input.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OperatorAttentionInputError {
  /// The target/critical-condition scope is empty, repeated, or oversized.
  #[error("operator attention scope is invalid")]
  InvalidScope,
  /// One time is unsupported or the lower bound follows the upper bound.
  #[error("operator attention time range is invalid")]
  InvalidTimeRange,
  /// The requested page size is zero or exceeds its fixed bound.
  #[error("operator attention page size is invalid")]
  InvalidLimit,
  /// The cursor belongs to another target, critical-condition, or time scope.
  #[error("operator attention cursor does not match the scope")]
  CursorScopeMismatch,
}

/// Invalid, non-canonical, or unsupported attention continuation cursor.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("operator attention cursor is invalid")]
pub struct OperatorAttentionCursorError;

/// Opaque versioned cursor bound to the complete attention scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperatorAttentionCursor {
  scope_digest: [u8; 32],
  position: OperatorAttentionPagePosition,
}

#[derive(Deserialize, Serialize)]
struct OperatorAttentionCursorWire {
  version: u8,
  scope_digest: [u8; 32],
  position: OperatorAttentionPagePosition,
}

impl OperatorAttentionCursor {
  fn new(scope: &OperatorAttentionScopeInput, position: OperatorAttentionPagePosition) -> Self {
    Self {
      scope_digest: scope.scope.digest(),
      position,
    }
  }

  /// Decodes and canonically revalidates one bounded URL-safe cursor.
  pub fn decode(encoded: &str) -> Result<Self, OperatorAttentionCursorError> {
    let wire: OperatorAttentionCursorWire =
      opaque_cursor::decode_canonical(encoded, MAX_OPERATOR_ATTENTION_CURSOR_BYTES)
        .map_err(|()| OperatorAttentionCursorError)?;
    if wire.version != OPERATOR_ATTENTION_CURSOR_VERSION {
      return Err(OperatorAttentionCursorError);
    }
    Ok(Self {
      scope_digest: wire.scope_digest,
      position: wire.position,
    })
  }

  /// Encodes this cursor as canonical URL-safe opaque text.
  #[must_use]
  pub fn encode(&self) -> String {
    opaque_cursor::encode(&OperatorAttentionCursorWire {
      version: OPERATOR_ATTENTION_CURSOR_VERSION,
      scope_digest: self.scope_digest,
      position: self.position,
    })
  }

  fn matches(&self, scope: &OperatorAttentionScopeInput) -> bool {
    self.scope_digest == scope.scope.digest()
  }
}

/// Lists safe visible attention items for one explicit bounded scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListOperatorAttentionQuery {
  scope: OperatorAttentionScopeInput,
  after: Option<OperatorAttentionCursor>,
  limit: NonZeroU16,
}

impl ListOperatorAttentionQuery {
  /// Validates page bounds and binds any cursor to the supplied scope.
  pub fn try_new(
    scope: OperatorAttentionScopeInput,
    after: Option<OperatorAttentionCursor>,
    limit: u16,
  ) -> Result<Self, OperatorAttentionInputError> {
    let limit = NonZeroU16::new(limit)
      .filter(|limit| limit.get() <= MAX_OPERATOR_ATTENTION_RESULT_PAGE_SIZE)
      .ok_or(OperatorAttentionInputError::InvalidLimit)?;
    if after.as_ref().is_some_and(|cursor| !cursor.matches(&scope)) {
      return Err(OperatorAttentionInputError::CursorScopeMismatch);
    }
    Ok(Self { scope, after, limit })
  }

  /// Returns the validated target, critical-condition, and time scope.
  #[must_use]
  pub const fn scope(&self) -> &OperatorAttentionScopeInput {
    &self.scope
  }

  /// Returns the validated exclusive continuation cursor, when supplied.
  #[must_use]
  pub const fn after(&self) -> Option<&OperatorAttentionCursor> {
    self.after.as_ref()
  }

  /// Returns the positive bounded page size.
  #[must_use]
  pub const fn limit(&self) -> u16 {
    self.limit.get()
  }
}

impl Query for ListOperatorAttentionQuery {
  type Outcome = OperatorAttentionPageProjection;
}

impl ManagementAuthorizationTarget for ListOperatorAttentionQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::collection(ManagementAction::View, ManagementResourceKind::ControlPlane);

  fn management_resource(&self) -> ManagementResourceResult {
    ManagementResource::collection(ManagementResourceKind::ControlPlane)
  }
}

/// Safe attention item returned by the application boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct OperatorAttentionItemProjection {
  /// Stable attention identity.
  pub id: OperatorAttentionId,
  /// Typed server-owned category.
  pub category: OperatorAttentionCategory,
  /// Server-owned severity.
  pub severity: OperatorAttentionSeverity,
  /// Bounded machine-readable classification code.
  pub code: String,
  /// Bounded non-secret operator summary.
  pub summary: String,
  /// Occurrence time in Unix milliseconds.
  pub occurred_at_unix_ms: i64,
  /// Optional resolution time in Unix milliseconds.
  pub resolved_at_unix_ms: Option<i64>,
  /// Optional visible resource target.
  pub target: Option<OperatorAttentionTarget>,
}

impl From<octacity_server_store::OperatorAttentionItem> for OperatorAttentionItemProjection {
  fn from(item: octacity_server_store::OperatorAttentionItem) -> Self {
    Self {
      id: item.id,
      category: item.category,
      severity: item.severity,
      code: item.code,
      summary: item.summary,
      occurred_at_unix_ms: item.occurred_at.unix_millis(),
      resolved_at_unix_ms: item.resolved_at.map(Timestamp::unix_millis),
      target: item.target,
    }
  }
}

/// One deterministic newest-first page of safe attention items.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperatorAttentionPageProjection {
  /// Safe visible items in newest-first order.
  pub items: Vec<OperatorAttentionItemProjection>,
  /// Opaque cursor bound to the complete request scope.
  pub next_cursor: Option<OperatorAttentionCursor>,
}

/// Application handlers for scoped operator attention.
pub struct OperatorAttentionHandlers<S> {
  store: Arc<S>,
}

impl<S> OperatorAttentionHandlers<S> {
  /// Creates handlers backed by one operator-attention store.
  #[must_use]
  pub const fn new(store: Arc<S>) -> Self {
    Self { store }
  }
}

#[async_trait]
impl<S> ManagementQueryUseCase<ListOperatorAttentionQuery> for OperatorAttentionHandlers<S>
where
  S: OperatorAttentionStore + 'static,
{
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &crate::ManagementRequestContext,
    grant: &crate::ManagementAuthorizationGrant,
    query: ListOperatorAttentionQuery,
  ) -> Result<OperatorAttentionPageProjection, Self::Error> {
    let visibility = grant
      .visibility_for::<ListOperatorAttentionQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    let page = self
      .store
      .list_operator_attention(ListOperatorAttention::new(
        query.scope.scope.clone(),
        query.after.as_ref().map(|cursor| cursor.position),
        query.limit(),
        visibility,
      )?)
      .await?;
    Ok(OperatorAttentionPageProjection {
      items: page.items.into_iter().map(Into::into).collect(),
      next_cursor: page
        .next_cursor
        .map(|position| OperatorAttentionCursor::new(&query.scope, position)),
    })
  }
}
