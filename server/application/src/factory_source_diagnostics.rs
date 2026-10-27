//! Read-only Source history; no provider calls, admission or task-state refresh.
use super::*;
use octacity_server_domain::ProjectId;
use serde::Serialize;
/// Optional authoritative task reference. Association owners must apply current visibility.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkSourceIssueLink {
  /// Owning Project, checked before exposure.
  pub project_id: ProjectId,
  /// Opaque task identity, never inferred from title or input identity.
  #[serde(serialize_with = "serialize_issue_identity")]
  pub issue_id: ManagementResourceIdentity,
}
pub use octacity_server_store::{
  MAX_WORK_SOURCE_REPORTING_OPERATIONS, WorkSourceReportingDiagnostic, WorkSourceReportingState,
};
/// Original visible execution association, independent of optional task cards.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkSourceAdmissionDiagnostic {
  /// Original immutable input/Work/Run association.
  pub association: WorkSourceDiagnosticAssociation,
  /// Current status projection operations.
  pub reporting: Vec<WorkSourceReportingDiagnostic>,
  /// Additional visible projection operations exist beyond this bounded summary.
  pub reporting_truncated: bool,
  /// Only an authoritative currently visible task link, when enabled.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub issue: Option<WorkSourceIssueLink>,
}
/// Bounded exact-definition diagnostics; no private cursor, raw ticket input or credential.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkSourceDiagnostics {
  /// Exact inspected definitions, including disabled historical revisions.
  pub configuration: WorkSourceConfigurationRecord,
  /// Exact installed predicates, or unknown when their presentation is unavailable.
  pub selectors: Vec<WorkSourceSelectorDiagnostic>,
  /// Last retained safe scan/progress facts for this exact target version.
  pub scan: Option<WorkSourceScanDiagnostics>,
  /// Existing admitted executions; a selected candidate is not an admission promise.
  pub admissions: Vec<WorkSourceAdmissionDiagnostic>,
  /// Last visible association iff another visible association exists.
  pub next_cursor: Option<FactoryDigest>,
}
/// Read existing progress and original visible executions without running the Source.
#[derive(Clone, Debug)]
pub struct GetWorkSourceDiagnosticsQuery {
  /// Authorized owner.
  pub project_id: ProjectId,
  /// Exact retained input/group version.
  pub target: WorkSourceScanTarget,
  /// Exclusive visible association position.
  pub after: Option<FactoryDigest>,
  /// Positive result ceiling, at most 64.
  pub limit: u16,
}
impl Query for GetWorkSourceDiagnosticsQuery {
  type Outcome = WorkSourceDiagnostics;
}
impl ManagementAuthorizationTarget for GetWorkSourceDiagnosticsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::Project);
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
impl<
  S: FactorySourceStore
    + FactorySourceDiagnosticsStore
    + IntegrationProjectAccessStore
    + IntegrationConnectionStore
    + IntegrationAdministrationStore,
> FactorySourceConfigurationHandlers<S>
{
  pub(super) async fn diagnostic_admissions(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    items: Vec<WorkSourceDiagnosticAssociation>,
  ) -> Result<Vec<WorkSourceAdmissionDiagnostic>, ApplicationError> {
    let mut admissions = vec![];
    for association in items {
      let issue = self
        .visible_issue_link(context, project, association.native_correlation)
        .await?;
      let reporting = self
        .store
        .source_reporting_diagnostics(project, &association, MAX_WORK_SOURCE_REPORTING_OPERATIONS)
        .await?;
      admissions.push(WorkSourceAdmissionDiagnostic {
        association,
        reporting: reporting.items,
        reporting_truncated: reporting.truncated,
        issue,
      });
    }
    Ok(admissions)
  }
  pub(super) async fn diagnostic_correlations(
    &self,
    context: &ManagementRequestContext,
    project_id: ProjectId,
    filter: WorkSourceDiagnosticFilter,
    after: Option<FactoryDigest>,
    limit: u16,
  ) -> Result<WorkSourceDiagnosticPage, ApplicationError> {
    let visibility = self.source_visibility(context, project_id).await?;
    let grant = self
      .policy
      .authorize(
        context,
        ManagementAction::View,
        &ManagementResource::owned_collection(
          ManagementResourceKind::FactoryRun,
          ManagementResourceKind::Project,
          ManagementResourceIdentity::new(project_id.to_string()).map_err(|_| ApplicationError::invalid())?,
        )
        .map_err(|_| ApplicationError::invalid())?,
      )
      .await
      .map_err(|_| ApplicationError::invalid())?;
    let runs = grant
      .visibility_for::<ListFactoryRunsQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?;
    Ok(
      self
        .store
        .source_diagnostic_correlations(ReadWorkSourceCorrelations {
          project_id,
          filter,
          visibility,
          runs,
          after,
          limit,
        })
        .await?,
    )
  }
}
#[async_trait]
impl<
  S: FactorySourceStore
    + FactorySourceDiagnosticsStore
    + IntegrationProjectAccessStore
    + IntegrationConnectionStore
    + IntegrationAdministrationStore,
> ManagementQueryUseCase<GetWorkSourceDiagnosticsQuery> for FactorySourceConfigurationHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetWorkSourceDiagnosticsQuery,
  ) -> Result<WorkSourceDiagnostics, ApplicationError> {
    if !grant
      .visibility_for::<GetWorkSourceDiagnosticsQuery>()
      .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?
      .allows(&query.project_id)
    {
      return Err(ApplicationError::invalid());
    }
    if !(1..=64).contains(&query.limit) {
      return Err(ApplicationError::invalid());
    }
    let configuration = <Self as ManagementQueryUseCase<GetWorkSourceConfigurationQuery>>::execute_management_query(
      self,
      context,
      grant,
      GetWorkSourceConfigurationQuery {
        project_id: query.project_id,
        target: query.target,
      },
    )
    .await?;
    let page = self
      .diagnostic_correlations(
        context,
        query.project_id,
        WorkSourceDiagnosticFilter::InputVersions(
          configuration
            .inputs()
            .iter()
            .map(|input| WorkSourceMemberRef {
              id: input.id(),
              version: input.version(),
            })
            .collect(),
        ),
        query.after,
        query.limit,
      )
      .await?;
    let admissions = self
      .diagnostic_admissions(context, query.project_id, page.items)
      .await?;
    let scan = self
      .store
      .source_scan_diagnostics(query.project_id, query.target)
      .await?;
    let selectors = configuration
      .inputs()
      .iter()
      .map(|input| self.selector_diagnostic(query.project_id, input.id(), input.definition()))
      .collect();
    Ok(WorkSourceDiagnostics {
      configuration,
      selectors,
      scan,
      admissions,
      next_cursor: page.next_cursor,
    })
  }
}

fn serialize_issue_identity<S: serde::Serializer>(
  id: &ManagementResourceIdentity,
  serializer: S,
) -> Result<S::Ok, S::Error> {
  id.as_str().serialize(serializer)
}

/// Read-only capability of the authoritative task owner; no import/refresh/association commands.
#[async_trait]
pub trait FactoryWorkSourceIssueLinks: Send + Sync {
  /// Returns only a current visible authoritative link for this Project/native partition.
  async fn visible_issue(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    native: FactoryDigest,
  ) -> Result<Option<WorkSourceIssueLink>, ApplicationError>;
}
impl<S> FactorySourceConfigurationHandlers<S> {
  /// Enables optional authoritative task links without installing an observation consumer.
  pub fn with_issue_links(mut self, links: Arc<dyn FactoryWorkSourceIssueLinks>) -> Self {
    self.issue_links = Some(links);
    self
  }
  pub(super) async fn visible_issue_link(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    native: FactoryDigest,
  ) -> Result<Option<WorkSourceIssueLink>, ApplicationError> {
    let Some(owner) = &self.issue_links else {
      return Ok(None);
    };
    // Hidden references retain the protected read's indistinguishable absence.
    // An authoritative dependency failure is not evidence that no Issue exists.
    let link = match owner.visible_issue(context, project, native).await {
      Ok(link) => link,
      Err(ApplicationError::InvalidInput) => None,
      Err(error) => return Err(error),
    };
    Ok(link.filter(|link| link.project_id == project))
  }
}

impl ManagementVisibilityTarget for GetWorkSourceDiagnosticsQuery {
  type Visibility = ProjectListVisibility;
  const RESTRICTED_RESOURCE_KINDS: &'static [ManagementResourceKind] = &[ManagementResourceKind::Project];
  fn visibility_from(grant: &ManagementAuthorizationGrant) -> Result<Self::Visibility, ManagementVisibilityError> {
    <ListProjectsQuery as ManagementVisibilityTarget>::visibility_from(grant)
  }
}

/// Exact effective selector presentation; absent predicates mean unknown, never unrestricted.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkSourceSelectorDiagnostic {
  /// Separately identified automatic input.
  pub input_id: WorkSourceInputId,
  /// Exact installed immutable source and selector configuration.
  pub binding: WorkSourceBinding,
  /// Safe installed predicate presentation, if explicitly declared by its owner.
  pub selection: Option<WorkSourceSelectionLabels>,
}
impl<S> FactorySourceConfigurationHandlers<S> {
  pub(super) fn selector_diagnostic(
    &self,
    project: ProjectId,
    input_id: WorkSourceInputId,
    definition: &WorkSourceInputDraft,
  ) -> WorkSourceSelectorDiagnostic {
    WorkSourceSelectorDiagnostic {
      input_id,
      binding: definition.source.clone(),
      selection: self
        .selection_labels
        .iter()
        .find(|(c, _)| {
          c.project_id == project && c.role == WorkSourceCapabilityRole::Source && c.binding == definition.source
        })
        .map(|(_, selection)| match &definition.predicate {
          Some(predicate) => WorkSourceSelectionLabels {
            label_presence: predicate.label_presence,
            state: predicate.state.clone().unwrap_or_else(|| selection.state.clone()),
            all_of: predicate.all_of.clone(),
            any_of: predicate.any_of.clone(),
            none_of: predicate.none_of.clone(),
          },
          None => selection.clone(),
        }),
    }
  }
}
