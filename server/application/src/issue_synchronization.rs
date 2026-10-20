//! Bounded read projection over original Issue inputs and the existing Reporter journal.
use super::*;
pub use octacity_server_store::{FactoryOutboxFailure, FactoryOutboxState};
use octacity_server_store::{IssueSourceProjectionStore, IssueStore};
use serde::{Deserialize, Serialize};

const OBSERVATION: &str = "live-reporter-v1";
const MAX_CURSOR_BYTES: usize = 1024;

/// Retained reads need no Source reader, provider profile or execution service.
pub struct IssueSynchronizationHandlers<S> {
  store: Arc<S>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<S> IssueSynchronizationHandlers<S> {
  /// Composes the existing store with trusted observation time only.
  pub fn new(store: Arc<S>, clock: Arc<dyn FactoryExecutionClock>) -> Self {
    Self { store, clock }
  }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
  scope: FactoryDigest,
  after: FactoryDigest,
}
/// Live Reporter traversal: a fresh traversal is required to discover newly accepted operations.
#[derive(Clone, Debug)]
pub struct ListIssueSourceProjectionsQuery {
  project_id: ProjectId,
  issue_id: IssueId,
  after: Option<Cursor>,
  limit: u16,
}
impl ListIssueSourceProjectionsQuery {
  /// Uses the shared history bound and canonical opaque-cursor codec.
  pub fn try_new(
    project_id: ProjectId,
    issue_id: IssueId,
    after: Option<String>,
    limit: u16,
  ) -> Result<Self, ApplicationError> {
    if limit == 0 || limit > octacity_server_store::MAX_ISSUE_HISTORY_PAGE_SIZE {
      return Err(ApplicationError::invalid());
    }
    let after = after
      .map(|s| crate::opaque_cursor::decode_canonical(&s, MAX_CURSOR_BYTES).map_err(|()| ApplicationError::invalid()))
      .transpose()?;
    Ok(Self {
      project_id,
      issue_id,
      after,
      limit,
    })
  }
}
impl Query for ListIssueSourceProjectionsQuery {
  type Outcome = IssueSourceProjectionPage;
}
impl ManagementAuthorizationTarget for ListIssueSourceProjectionsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue_id)
  }
}
/// Safe exact provenance; excludes worker ownership, credentials, command keys and private grants.
#[derive(Clone, Debug, Serialize)]
pub struct IssueSourceProjection {
  /// Original effect and current append-only receipt for reviewed resolution.
  pub operation_id: FactoryDigest,
  /// Exact latest Reporter receipt, not a command permit.
  pub record_id: FactoryDigest,
  /// Original accepted Issue revision, even after later edits or mapping replacement.
  pub issue_version: crate::IssueVersion,
  /// Original immutable field policy.
  pub mapping: ImmutableReference,
  /// Original native association, never a mutable current target lookup.
  pub native_correlation: FactoryDigest,
  /// Original workflow independent of native vocabulary.
  pub workflow: crate::IssueWorkflowPin,
  /// Original internal status.
  pub status_id: octacity_server_domain::IssueStatusId,
  /// Exact configured desired values; absence explicitly means unmapped.
  pub effects: Option<octacity_server_factory::WorkSourceStatusEffects>,
  /// Original accepted action, absent for manual metadata commands.
  pub pipeline: Option<octacity_server_store::IssuePipelineActionProvenance>,
  /// Exact previous failed operation addressed by an explicit policy repair.
  pub resolves: Option<FactoryDigest>,
  /// Original acceptance time, not the latest retry time.
  pub accepted_at: Timestamp,
  /// Current technical delivery state, independently of execution.
  pub state: FactoryOutboxState,
  /// Safe terminal reason without provider diagnostics.
  pub failure: Option<FactoryOutboxFailure>,
  /// Zero-based retry ordinal from Reporter.
  pub attempt: u16,
  /// Last recorded technical change.
  pub recorded_at: Timestamp,
  /// Earliest next delivery time; meaningful for pending operations.
  pub available_at: Timestamp,
}
/// Bounded independent live observation; no aggregate execution or acceptance verdict.
#[derive(Clone, Debug, Serialize)]
pub struct IssueSourceProjectionPage {
  /// Only visible safe operation metadata.
  pub items: Vec<IssueSourceProjection>,
  /// Continuation bound to Project, Issue, security partition and current visibility.
  pub next_cursor: Option<String>,
  /// Explicitly live membership and latest delivery facts, not an atomic provider snapshot.
  pub observation: &'static str,
  /// Trusted read time; never the time of a provider observation.
  pub observed_at: Timestamp,
}
#[async_trait::async_trait]
impl<S: IssueStore + IssueSourceProjectionStore> ManagementQueryUseCase<ListIssueSourceProjectionsQuery>
  for IssueSynchronizationHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIssueSourceProjectionsQuery,
  ) -> Result<IssueSourceProjectionPage, ApplicationError> {
    crate::issue::visible_issue(grant, query.issue_id)?;
    self.store.issue(query.project_id, query.issue_id, None).await?;
    let visibility = grant
      .visibility_for::<ListIssueSourceProjectionsQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    let normalized = match visibility.view() {
      octacity_server_store::ReadVisibilityView::All => serde_json::json!({"kind":"all"}),
      octacity_server_store::ReadVisibilityView::None => serde_json::json!({"kind":"none"}),
      octacity_server_store::ReadVisibilityView::Restricted(ids) => serde_json::json!({"kind":"restricted","ids":ids}),
    };
    let scope = FactoryDigest::content_sha256(
      serde_json::json!({
        "project":query.project_id,"issue":query.issue_id,"security":context.security_scope().as_str(),
        "visibility":normalized,"observation":OBSERVATION
      })
      .to_string()
      .as_bytes(),
    );
    if query.after.as_ref().is_some_and(|c| c.scope != scope) {
      return Err(ApplicationError::invalid());
    }
    let mut rows = self
      .store
      .issue_push_projections(
        query.project_id,
        query.issue_id,
        &context.security_scope().to_store(),
        query.after.map(|c| c.after),
        query.limit + 1,
      )
      .await?;
    let has_more = rows.len() > usize::from(query.limit);
    rows.truncate(usize::from(query.limit));
    let next_cursor = has_more.then(|| {
      crate::opaque_cursor::encode(&Cursor {
        scope,
        after: rows.last().expect("nonempty bounded page").record.operation_id,
      })
    });
    let items = rows
      .into_iter()
      .map(|p| {
        let desired = p.intent.desired.ok_or_else(ApplicationError::invalid)?;
        Ok(IssueSourceProjection {
          operation_id: p.record.operation_id,
          record_id: p.record.id,
          issue_version: p.intent.version,
          mapping: p.intent.mapping,
          native_correlation: desired.link.native_correlation,
          workflow: desired.workflow,
          status_id: desired.status_id,
          effects: desired.effects,
          pipeline: desired.command.pipeline,
          resolves: desired.command.resolves,
          accepted_at: p.accepted_at,
          state: p.record.state,
          failure: p.record.failure,
          attempt: p.record.attempt,
          recorded_at: p.record.recorded_at,
          available_at: p.record.available_at,
        })
      })
      .collect::<Result<Vec<_>, ApplicationError>>()?;
    Ok(IssueSourceProjectionPage {
      items,
      next_cursor,
      observation: OBSERVATION,
      observed_at: self.clock.now()?,
    })
  }
}
