//! Optional metadata/content composition over the retained Artifact reader, never publication.
use super::*;
use octacity_server_factory::{EvidenceProducer, FactoryArtifactReference, FactoryDigest};
use serde::Serialize;
#[path = "issue_artifact_content.rs"]
mod content;
pub use content::*;

/// Retained logical availability; viewing cannot recreate expired bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueArtifactAvailability {
  /// Published within its retained read window.
  Available,
  /// Original reference remains, but retention has expired.
  Expired,
  /// Independently protected metadata/bytes cannot currently be read.
  Unavailable,
}
/// Original publication authority, distinct from the display grouping.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueArtifactOrigin {
  /// Published Project text, without an invented producing Build.
  ProjectInput,
  /// Published Source input, without an invented producing Build.
  SourceInput,
  /// Ordinary immutable Build output.
  BuildOutput,
}
/// Safe metadata supplied by the existing retained Artifact reader without byte I/O.
#[derive(Clone, Debug, Serialize)]
pub struct IssueArtifactMetadata {
  /// Authorized logical name.
  pub name: String,
  /// Verified logical media type.
  pub media_type: String,
  /// Original publication authority.
  pub origin: IssueArtifactOrigin,
  /// Original retained byte availability.
  pub availability: IssueArtifactAvailability,
  /// Original publication time.
  pub published_at: Option<Timestamp>,
  /// Exclusive read deadline, absent for indefinite retention.
  pub expires_at: Option<Timestamp>,
}
/// Read-only extension of the existing byte/publication authority; it creates no new storage owner.
#[async_trait::async_trait]
pub trait IssueArtifactReader: Send + Sync {
  /// Checks exact Project/content/publication and retention without fetching bytes.
  async fn metadata(&self, request: ReadFactoryArtifact) -> Result<IssueArtifactMetadata, ApplicationError>;
  /// Uses the existing bounded digest-verified retained input read on explicit expansion only.
  async fn content(&self, request: ReadFactoryArtifact) -> Result<Vec<u8>, ApplicationError>;
}
/// Meaningful display groups independent of any provider vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueArtifactGroup {
  /// Frozen Work inputs; these are not produced evidence.
  OriginalInput,
  /// Candidate/build output.
  Output,
  /// Verified report.
  Report,
  /// Raster output, still subject to independent preview validation.
  Screenshot,
}
/// One authorized immutable reference, with progressive provenance and no content/provider URL.
#[derive(Clone, Debug, Serialize)]
pub struct IssueArtifactProjection {
  /// Exact content-bound immutable reference.
  pub artifact: FactoryArtifactReference,
  /// Actual display role, not acceptance.
  pub group: IssueArtifactGroup,
  /// Actual original producer when present.
  pub producer: Option<EvidenceProducer>,
  /// Exact retained producing record; original inputs have none.
  pub record: Option<FactoryDigest>,
  /// Original candidate when the producing record retains one.
  pub subject: Option<octacity_server_factory::CandidateSubject>,
  /// Safe original metadata when currently readable.
  pub metadata: Option<IssueArtifactMetadata>,
  /// Retained metadata availability independently of selected execution.
  pub availability: IssueArtifactAvailability,
}
/// Bounded exact execution inventory; partial pages never establish complete evidence.
#[derive(Clone, Debug, Serialize)]
pub struct IssueArtifactsPage {
  /// Original selected execution association.
  pub association: octacity_server_store::IssueWorkAssociation,
  /// Separately observed Run version.
  pub run_version: octacity_server_factory::FactoryRunVersion,
  /// Explicit observation coverage.
  pub coverage: IssueResultCoverage,
  /// Independently authorized references, without bytes or download capability.
  pub items: Vec<IssueArtifactProjection>,
  /// Scope/version/visibility-bound continuation.
  pub next_cursor: Option<String>,
}
/// Exact Work-scoped artifact inventory, independent from a review/ticket provider.
#[derive(Clone, Debug)]
pub struct ListIssueArtifactsQuery {
  execution: GetIssueWorkExecutionQuery,
  after: Option<String>,
  limit: u16,
}
impl ListIssueArtifactsQuery {
  /// Exact original Issue-associated Work selection.
  #[must_use]
  pub fn execution(&self) -> &GetIssueWorkExecutionQuery {
    &self.execution
  }
  /// Uses the common diagnostics page and Issue observation bounds.
  pub fn try_new(
    execution: GetIssueWorkExecutionQuery,
    after: Option<String>,
    limit: u16,
  ) -> Result<Self, ApplicationError> {
    if limit == 0
      || limit > MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE
      || after
        .as_ref()
        .is_some_and(|s| s.len() > MAX_ISSUE_OBSERVATION_CURSOR_BYTES)
    {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      execution,
      after,
      limit,
    })
  }
}
impl Query for ListIssueArtifactsQuery {
  type Outcome = IssueArtifactsPage;
}
impl ManagementAuthorizationTarget for ListIssueArtifactsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    self.execution.management_resource()
  }
}
/// Optional result-only composition; manual Issues require none of these dependencies.
pub struct IssueArtifactHandlers<S, F> {
  executions: Arc<IssueExecutionHandlers<S, F>>,
  reader: Arc<dyn IssueArtifactReader>,
  clock: Arc<dyn FactoryExecutionClock>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
}
impl<S, F> IssueArtifactHandlers<S, F> {
  /// Composes existing authorities without scheduling work or acquiring credentials.
  pub fn new(
    executions: Arc<IssueExecutionHandlers<S, F>>,
    reader: Arc<dyn IssueArtifactReader>,
    clock: Arc<dyn FactoryExecutionClock>,
    policy: Arc<dyn ManagementAuthorizationPolicy>,
  ) -> Self {
    Self {
      executions,
      reader,
      clock,
      policy,
    }
  }
}
impl<S: IssueExecutionStore + IssueStore, F: FactoryDiscoveryStore + octacity_server_store::FactoryRunStore + 'static>
  IssueArtifactHandlers<S, F>
{
  async fn visible_artifact(&self, context: &ManagementRequestContext, item: &IssueArtifactProjection) -> bool {
    if crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Artifact,
      item.artifact.artifact_id(),
    )
    .await
    .is_err()
    {
      return false;
    }
    if let Some(subject) = &item.subject
      && crate::factory_configuration_choices::require_visible_reference(
        self.policy.as_ref(),
        context,
        ManagementResourceKind::Repository,
        subject.exact().repository_id(),
      )
      .await
      .is_err()
    {
      return false;
    }
    match &item.producer {
      Some(p) => super::producer_visible(self.policy.as_ref(), context, p).await,
      None => true,
    }
  }
  async fn visible_inventory(
    &self,
    context: &ManagementRequestContext,
    snapshot: &octacity_server_store::FactoryRunSnapshot,
  ) -> Result<Vec<IssueArtifactProjection>, ApplicationError> {
    let mut items = Vec::new();
    for artifact in [snapshot.work.artifacts().task(), snapshot.work.artifacts().acceptance()]
      .into_iter()
      .chain(snapshot.work.artifacts().specifications())
    {
      items.push(IssueArtifactProjection {
        artifact: artifact.clone(),
        group: IssueArtifactGroup::OriginalInput,
        producer: None,
        record: None,
        subject: None,
        metadata: None,
        availability: IssueArtifactAvailability::Unavailable,
      });
    }
    for record in &snapshot.flow.data.records {
      let Some(provenance) = record.build_provenance() else {
        continue;
      };
      for output in provenance.outputs() {
        items.push(IssueArtifactProjection {
          artifact: output.artifact.clone(),
          group: if output.output_kind == octacity_server_factory::EvidenceOutputKind::Report {
            IssueArtifactGroup::Report
          } else {
            IssueArtifactGroup::Output
          },
          producer: Some(output.producer.clone()),
          record: Some(record.digest().map_err(|_| ApplicationError::invalid())?),
          subject: record
            .discovery()
            .and_then(|d| d.candidate_subject())
            .or_else(|| record.input().context().and_then(|c| c.subject().candidate()))
            .cloned(),
          metadata: None,
          availability: IssueArtifactAvailability::Unavailable,
        });
      }
    }
    let mut visible = Vec::new();
    for item in items {
      if self.visible_artifact(context, &item).await {
        visible.push(item);
      }
    }
    Ok(visible)
  }
}
#[async_trait::async_trait]
impl<S: IssueExecutionStore + IssueStore, F: FactoryDiscoveryStore + octacity_server_store::FactoryRunStore + 'static>
  ManagementQueryUseCase<ListIssueArtifactsQuery> for IssueArtifactHandlers<S, F>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIssueArtifactsQuery,
  ) -> Result<IssueArtifactsPage, ApplicationError> {
    let association = self.executions.visible_issue_work(grant, &query.execution).await?;
    self.executions.visible_execution_run(context, &association).await?;
    let snapshot = self.executions.factory.factory_run_snapshot(association.run_id).await?;
    let now = self.clock.now()?;
    let mut items = self.visible_inventory(context, &snapshot).await?;
    items.sort_by_key(|row| row.artifact.artifact_id());
    let membership = items
      .iter()
      .map(|r| {
        FactoryDigest::sha256(
          "octacity.issue-artifact.v1",
          &[r.artifact.artifact_id().as_uuid().as_bytes()],
        )
      })
      .collect::<Vec<_>>();
    let mut pagination = super::panels::PanelPagination::new(
      context,
      grant,
      serde_json::json!({"kind":"artifacts","association":association}),
      snapshot.run.version(),
      query.after,
      query.limit,
      &membership,
    )?;
    let mut next_cursor = pagination.apply(&mut items);
    for item in &mut items {
      let metadata = self
        .reader
        .metadata(ReadFactoryArtifact {
          project_id: association.project_id,
          artifact: item.artifact.clone(),
          maximum_bytes: octacity_artifact_store::MAX_ARTIFACT_READ_BYTES,
          observed_at: now,
          required_until: now,
        })
        .await
        .ok();
      item.availability = metadata
        .as_ref()
        .map_or(IssueArtifactAvailability::Unavailable, |m| m.availability);
      if item.group == IssueArtifactGroup::Output
        && metadata
          .as_ref()
          .is_some_and(|m| matches!(m.media_type.as_str(), "image/png" | "image/jpeg" | "image/webp"))
      {
        item.group = IssueArtifactGroup::Screenshot;
      }
      item.metadata = metadata;
    }
    if pagination.coverage == IssueResultCoverage::Partial {
      pagination.coverage = self
        .executions
        .result_coverage(context, &association, snapshot.run.version())
        .await;
    }
    if pagination.coverage == IssueResultCoverage::Partial {
      for item in &items {
        if !self.visible_artifact(context, item).await {
          pagination.coverage = IssueResultCoverage::Unavailable;
          break;
        }
      }
    }
    if pagination.coverage != IssueResultCoverage::Partial {
      items.clear();
      next_cursor = None;
    }
    Ok(IssueArtifactsPage {
      association,
      run_version: snapshot.run.version(),
      coverage: pagination.coverage,
      items,
      next_cursor,
    })
  }
}
