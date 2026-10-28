//! Credential-free scoped identity supplied only after connection authentication.
use super::*;
use crate::{ApplicationError, IssueSummary, ListProjectIssuesQuery, Query};
use octacity_server_domain::{ProjectId, Timestamp};
use std::sync::Arc;

/// Maximum UTF-8 bytes in one bounded title query.
pub const MAX_ISSUE_TITLE_QUERY_BYTES: usize = 256;
/// Maximum bytes in a canonical scope/query-bound search continuation.
pub const MAX_ISSUE_TITLE_CURSOR_BYTES: usize = 1024;

/// One visibility-filtered page; coverage describes only the observed page.
pub struct IssueTitleSearchPage {
  /// Same-Project visible summaries matching the case-insensitive title query.
  pub items: Vec<IssueSummary>,
  /// Original visible-page continuation, including pages with no title matches.
  pub next_cursor: Option<String>,
  /// Number of visible entries examined, excluding hidden identities.
  pub scanned_entries: usize,
}

/// One authenticated connection's bounded Issue-read authority; never a bearer value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssueReadPrincipal {
  actor: ManagementActor,
  scope: ManagementSecurityScope,
  project: ProjectId,
  visibility: ManagementVisibility,
  expires_at: Timestamp,
}

impl IssueReadPrincipal {
  /// Accepts an installation-verified identity and Issue-only visibility.
  /// Transport headers and model arguments cannot construct this proof.
  pub fn new(
    identity: impl Into<String>,
    project: ProjectId,
    visibility: ManagementVisibility,
    expires_at: Timestamp,
  ) -> Result<Self, crate::ApplicationError> {
    let actor = ManagementActor::authenticated(identity).map_err(|_| crate::ApplicationError::invalid())?;
    ManagementAuthorizationGrant::new(visibility.clone())
      .visibility_for::<crate::ListProjectIssuesQuery>()
      .map_err(|_| crate::ApplicationError::InvalidAuthorizationVisibility)?;
    let scope = ManagementSecurityScope::new(format!(
      "issue-read:{}:{}",
      project,
      octacity_server_factory::FactoryDigest::content_sha256(actor.identity().unwrap().as_bytes())
    ))
    .map_err(|_| crate::ApplicationError::invalid())?;
    Ok(Self {
      actor,
      scope,
      project,
      visibility,
      expires_at,
    })
  }

  /// Reports expiry without extending the installed connection's lifetime.
  #[must_use]
  pub fn is_current_at(&self, now: Timestamp) -> bool {
    now < self.expires_at
  }

  /// Exact Project granted by the independently authenticated connection.
  #[must_use]
  pub const fn project_id(&self) -> ProjectId {
    self.project
  }

  /// Searches one bounded authorized title page without reading requirement bodies.
  pub async fn search_titles<H>(
    &self,
    handlers: Arc<H>,
    query: &str,
    cursor: Option<&str>,
    limit: u16,
  ) -> Result<IssueTitleSearchPage, ManagementHandlerError<ApplicationError>>
  where
    H: ManagementQueryUseCase<ListProjectIssuesQuery, Error = ApplicationError>,
  {
    if query.len() > MAX_ISSUE_TITLE_QUERY_BYTES || query.chars().any(char::is_control) {
      return Err(ManagementHandlerError::Application(ApplicationError::invalid()));
    }
    let normalized = query.trim().to_lowercase();
    let query_digest = octacity_server_factory::FactoryDigest::content_sha256(normalized.as_bytes());
    let scope = self.cursor_scope();
    let after = match cursor {
      Some(cursor) => {
        let decoded: TitleCursor = crate::opaque_cursor::decode_canonical(cursor, MAX_ISSUE_TITLE_CURSOR_BYTES)
          .map_err(|_| ManagementHandlerError::Application(ApplicationError::invalid()))?;
        if decoded.version != 1 || decoded.scope != scope || decoded.query != query_digest {
          return Err(ManagementHandlerError::Application(ApplicationError::invalid()));
        }
        Some(decoded.after)
      }
      None => None,
    };
    let input = ListProjectIssuesQuery::try_new(self.project, after, limit)
      .map_err(|_| ManagementHandlerError::Application(ApplicationError::invalid()))?;
    let mut page = self.query(handlers, input).await?;
    let scanned_entries = page.items.len();
    page
      .items
      .retain(|issue| issue.title.to_lowercase().contains(&normalized));
    let next_cursor = page.next_cursor.map(|after| {
      crate::opaque_cursor::encode(&TitleCursor {
        version: 1,
        scope,
        query: query_digest,
        after,
      })
    });
    Ok(IssueTitleSearchPage {
      items: page.items,
      next_cursor,
      scanned_entries,
    })
  }

  /// Reads one permitted current or retained revision within this principal's Project.
  pub async fn read_issue<H>(
    &self,
    handlers: Arc<H>,
    id: octacity_server_domain::IssueId,
    version: Option<octacity_server_domain::IssueVersion>,
  ) -> Result<crate::IssueRevision, ManagementHandlerError<ApplicationError>>
  where
    H: ManagementQueryUseCase<crate::GetIssueQuery, Error = ApplicationError>,
  {
    self
      .query(
        handlers,
        crate::GetIssueQuery {
          project_id: self.project,
          id,
          version,
        },
      )
      .await
  }

  /// Stable verified connection/Project namespace for transient request cancellation.
  #[must_use]
  pub fn connection_scope(&self) -> &str {
    self.scope.as_str()
  }

  fn cursor_scope(&self) -> octacity_server_factory::FactoryDigest {
    let visible = match self.visibility.view() {
      ManagementVisibilityView::All => serde_json::json!({"kind":"all"}),
      ManagementVisibilityView::None => serde_json::json!({"kind":"none"}),
      ManagementVisibilityView::Restricted(resources) => serde_json::json!({"kind":"restricted", "issues":
        resources.iter().map(|resource| resource.identity().expect("validated Issue instance").as_str()).collect::<Vec<_>>()
      }),
    };
    let content = serde_json::to_vec(&(self.scope.as_str(), visible)).expect("safe scope projection");
    octacity_server_factory::FactoryDigest::content_sha256(&content)
  }

  async fn query<H, Q>(
    &self,
    handlers: Arc<H>,
    query: Q,
  ) -> Result<Q::Outcome, ManagementHandlerError<ApplicationError>>
  where
    Q: Query + ManagementAuthorizationTarget,
    H: ManagementQueryUseCase<Q, Error = ApplicationError>,
  {
    let context = ManagementRequestContext::new(
      self.actor.clone(),
      self.scope.clone(),
      ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap(),
      ManagementRequestAttributes::new(
        ManagementIngress::VerifiedIdentity,
        Some(ManagementClientKind::Automation),
      ),
    )
    .map_err(|_| ManagementHandlerError::Application(ApplicationError::invalid()))?;
    let resource = query
      .validated_management_resource()
      .map_err(|_| ManagementHandlerError::Application(ApplicationError::invalid()))?;
    let policy = ExactIssueReadPolicy {
      context: context.clone(),
      resource,
      visibility: self.visibility.clone(),
      expires_at: self.expires_at,
    };
    AuthorizedQueryHandler::new(Arc::new(policy), handlers)
      .handle_query(&context, query)
      .await
  }
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct TitleCursor {
  version: u8,
  scope: octacity_server_factory::FactoryDigest,
  query: octacity_server_factory::FactoryDigest,
  after: octacity_server_domain::IssueId,
}

// This policy is private and instantiated only for the closed Issue-read methods above.
// A verified MCP identity never becomes ordinary management or mutation authority.
struct ExactIssueReadPolicy {
  context: ManagementRequestContext,
  resource: ManagementResource,
  visibility: ManagementVisibility,
  expires_at: Timestamp,
}
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for ExactIssueReadPolicy {
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    use crate::FactoryExecutionClock as _;
    let now = crate::SystemFactoryExecutionClock
      .now()
      .map_err(|_| ManagementAuthorizationDenial::forbidden())?;
    if action != ManagementAction::View
      || context != &self.context
      || resource != &self.resource
      || now >= self.expires_at
    {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    Ok(ManagementAuthorizationGrant::new(self.visibility.clone()))
  }
}
