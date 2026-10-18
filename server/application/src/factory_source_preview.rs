//! Source preview has read authority only; it cannot reserve, admit, checkpoint or observe into cards.
use super::*;
use crate::ManagementSecurityScope;
use octacity_server_domain::{ProjectId, Timestamp};
pub use octacity_server_store::WorkSourceFailure;
use serde::Serialize;
use std::collections::BTreeSet;
/// Maximum native summaries returned by one preview request.
pub const MAX_WORK_SOURCE_PREVIEW_ITEMS: u16 = 100;
/// Maximum UTF-8 bytes of display title; never an execution input.
pub const MAX_WORK_SOURCE_PREVIEW_TITLE_BYTES: usize = 512;
pub use octacity_server_factory::WorkSourcePredicateMismatch as WorkSourceSelectionMismatch;
/// One requested input's selector explanation; no reasons means a match.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkSourcePreviewSelection {
  /// Stable automatic input identity, separate from the native task.
  pub input_id: WorkSourceInputId,
  /// Safe closed reasons, never provider error prose.
  pub reasons: Vec<WorkSourceSelectionMismatch>,
}
/// Allowlisted native display facts, without bodies, mapped inputs, credentials or card instructions.
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct WorkSourcePreviewItem {
  /// Verified native container/item, independent of the code Repository.
  pub item: WorkSourceItem,
  /// Human display key.
  pub display_id: String,
  /// Bounded untrusted title summary.
  pub title: String,
  /// Opaque native state.
  pub state: String,
  /// Bounded observed literal labels.
  pub labels: Vec<String>,
  /// Evaluation of every exact requested input.
  pub selections: Vec<WorkSourcePreviewSelection>,
}
impl std::fmt::Debug for WorkSourcePreviewItem {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("WorkSourcePreviewItem").finish_non_exhaustive()
  }
}
/// Read-only request to trusted installed composition; it carries no execution authority or cursor.
#[derive(Clone, Debug)]
pub struct ReadWorkSourcePreview {
  /// Owning authorized Project.
  pub project_id: ProjectId,
  /// Reviewed source selections; no credential material.
  pub inputs: Vec<WorkSourceInputIntent>,
  /// Caller-narrowed per-page ceiling.
  pub limit: u16,
}
/// Trusted read results, never emitted to observation consumers.
#[derive(Clone, Debug)]
pub struct WorkSourcePreviewPage {
  /// Installed owner partition, not a caller/provider supplied identity.
  pub security_scope: ManagementSecurityScope,
  /// Safe summaries only.
  pub candidates: Vec<WorkSourcePreviewItem>,
  /// More provider data exists; no continuation or production progress is exposed.
  pub has_more: bool,
}
/// The only provider capability held by preview; admission, writes and card observation are absent.
#[async_trait]
pub trait FactoryWorkSourcePreview: Send + Sync {
  /// Reads one bounded preview page under current installed read authority.
  async fn preview(&self, request: ReadWorkSourcePreview) -> Result<WorkSourcePreviewPage, ApplicationError>;
}
/// Selector result, never admission eligibility or an execution outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkSourcePreviewDisposition {
  /// No requested predicate matches.
  Filtered,
  /// At least one independent input, or one exclusive member, matches.
  Selected,
  /// Multiple exclusive members match; admission would require correction.
  Ambiguous,
}
/// Native correlation and selector evidence, without an admission claim.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkSourcePreviewCandidate {
  /// Project/partition/installation/container/item correlation, excluding input identity.
  pub native_correlation: FactoryDigest,
  /// Bounded native facts.
  pub summary: WorkSourcePreviewItem,
  /// Read-only predicate disposition.
  pub disposition: WorkSourcePreviewDisposition,
  /// Existing visible executions, distinct from the selector result.
  pub admissions: Vec<WorkSourceAdmissionDiagnostic>,
  /// Optional authoritative currently visible internal task.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub issue: Option<WorkSourceIssueLink>,
}
/// Empty successful reads remain distinct from failures/throttling.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkSourcePreview {
  /// Reviewed exact input/group selections, separate from any published or admitted state.
  pub configuration: WorkSourceConfigurationIntent,
  /// Effective installed predicates; unknown presentation remains explicitly absent.
  pub selectors: Vec<WorkSourceSelectorDiagnostic>,
  /// Trusted request observation time, not an admission timestamp.
  #[serde(rename = "observed_at_unix_ms")]
  pub observed_at: Timestamp,
  /// Safe dependency classification; absent on success, including an empty page.
  pub failure: Option<WorkSourceFailure>,
  /// Bounded provider retry advice, without scheduling a retry or changing progress.
  pub retry_after_millis: Option<u64>,
  /// Read-only summaries and matches.
  pub candidates: Vec<WorkSourcePreviewCandidate>,
  /// This bounded read did not exhaust the provider listing.
  pub has_more: bool,
  /// The globally bounded visible execution lookup has additional rows.
  pub admissions_truncated: bool,
}
/// Non-mutating preview of one draft input or a pinned exclusive group.
#[derive(Clone, Debug)]
pub struct PreviewWorkSourcesQuery {
  /// Credential-free input selections; publication is a separate command.
  pub intent: WorkSourceConfigurationIntent,
  /// Positive bounded summary count.
  pub limit: u16,
  /// Trusted management ingress time.
  pub observed_at: Timestamp,
}
impl Query for PreviewWorkSourcesQuery {
  type Outcome = WorkSourcePreview;
}
impl ManagementAuthorizationTarget for PreviewWorkSourcesQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::Project);
  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::Project, self.intent.project_id)
  }
}
impl<S> FactorySourceConfigurationHandlers<S> {
  /// Installs a bounded read-only provider capability, without workers or task consumers.
  pub fn with_preview(mut self, reader: Arc<dyn FactoryWorkSourcePreview>) -> Self {
    self.preview = Some(reader);
    self
  }
}
impl<
  S: FactorySourceStore + IntegrationProjectAccessStore + IntegrationConnectionStore + IntegrationAdministrationStore,
> FactorySourceConfigurationHandlers<S>
{
  async fn preview_source_choice(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    project: ProjectId,
    input: &WorkSourceInputIntent,
  ) -> Result<WorkSourceInputChoice, ApplicationError> {
    let definition = &input.definition;
    if definition.target.project_id() != project || definition.repository.project_id != project {
      return Err(ApplicationError::invalid());
    }
    self.require_input_use(context, definition).await?;
    let connection = definition
      .connection
      .as_ref()
      .ok_or_else(ApplicationError::capability_unavailable)?;
    let choices = <Self as ManagementQueryUseCase<ListWorkSourceInputChoicesQuery>>::execute_management_query(
      self,
      context,
      grant,
      ListWorkSourceInputChoicesQuery {
        project_id: project,
        connection_id: connection.id,
        connection_version: connection.version,
      },
    )
    .await?;
    if choices.access_version != connection.access_version {
      return Err(ApplicationError::PreconditionFailed);
    }
    choices
      .items
      .into_iter()
      .find(|c| {
        c.purpose == IntegrationPurpose::TaskRead
          && c.binding == definition.source
          && c.connection_configuration == connection.source_configuration
      })
      .ok_or_else(|| FactoryValidationDiagnostics::unavailable("/source"))
  }
}
#[async_trait]
impl<
  S: FactorySourceStore
    + FactorySourceDiagnosticsStore
    + IntegrationProjectAccessStore
    + IntegrationConnectionStore
    + IntegrationAdministrationStore,
> ManagementQueryUseCase<PreviewWorkSourcesQuery> for FactorySourceConfigurationHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    mut query: PreviewWorkSourcesQuery,
  ) -> Result<WorkSourcePreview, ApplicationError> {
    if !(1..=MAX_WORK_SOURCE_PREVIEW_ITEMS).contains(&query.limit)
      || query.intent.inputs.len() > MAX_WORK_SOURCE_GROUP_MEMBERS
      || query.intent.inputs.iter().map(|i| i.id).collect::<BTreeSet<_>>().len() != query.intent.inputs.len()
    {
      return Err(ApplicationError::invalid());
    }
    // Expanding retained members is read preparation, not a replacement publication intent.
    let configuration = query.intent.clone();
    let grouped = query.intent.group.is_some();
    if let Some(group) = &query.intent.group {
      if group.members.is_empty()
        || group.members.len() > MAX_WORK_SOURCE_GROUP_MEMBERS
        || group.members.iter().map(|m| m.id).collect::<BTreeSet<_>>().len() != group.members.len()
        || query
          .intent
          .inputs
          .iter()
          .any(|i| !group.members.iter().any(|m| m.id == i.id))
      {
        return Err(FactoryValidationDiagnostics::invalid("/group/members"));
      }
      let mut inputs = vec![];
      for member in &group.members {
        if let Some(input) = query.intent.inputs.iter().find(|i| i.id == member.id) {
          if input.expected_version.map_or(Some(1), |v| v.get().checked_add(1)) != Some(member.version.get()) {
            return Err(ApplicationError::PreconditionFailed);
          }
          inputs.push(input.clone());
        } else {
          let record = <Self as ManagementQueryUseCase<GetWorkSourceConfigurationQuery>>::execute_management_query(
            self,
            context,
            grant,
            GetWorkSourceConfigurationQuery {
              project_id: query.intent.project_id,
              target: WorkSourceScanTarget::Input {
                id: member.id,
                version: member.version,
              },
            },
          )
          .await?;
          let WorkSourceConfigurationRecord::Input(input) = record else {
            return Err(ApplicationError::invalid());
          };
          inputs.push(WorkSourceInputIntent {
            id: member.id,
            expected_version: Some(member.version),
            definition: input.definition().clone(),
          });
        }
      }
      query.intent.inputs = inputs;
    } else if query.intent.inputs.len() != 1 {
      return Err(ApplicationError::invalid());
    }
    let input = &query.intent.inputs[0];
    let choice = self
      .preview_source_choice(context, grant, query.intent.project_id, input)
      .await?;
    for member in query.intent.inputs.iter().skip(1) {
      let next = self
        .preview_source_choice(context, grant, query.intent.project_id, member)
        .await?;
      if next.native_id != choice.native_id
        || next.binding.installation != choice.binding.installation
        || next.binding.capability != choice.binding.capability
        || next.binding.resource != choice.binding.resource
      {
        return Err(FactoryValidationDiagnostics::invalid("/group/members"));
      }
    }
    let input_ids = query.intent.inputs.iter().map(|i| i.id).collect::<Vec<_>>();
    let selectors = query
      .intent
      .inputs
      .iter()
      .map(|i| self.selector_diagnostic(query.intent.project_id, i.id, &i.definition))
      .collect();
    let mut result = WorkSourcePreview {
      configuration,
      selectors,
      observed_at: query.observed_at,
      failure: None,
      retry_after_millis: None,
      candidates: vec![],
      has_more: false,
      admissions_truncated: false,
    };
    let reader = self
      .preview
      .as_ref()
      .ok_or_else(ApplicationError::capability_unavailable)?;
    let page = match reader
      .preview(ReadWorkSourcePreview {
        project_id: query.intent.project_id,
        inputs: query.intent.inputs.clone(),
        limit: query.limit,
      })
      .await
    {
      Ok(page) => page,
      Err(ApplicationError::SourceDependency {
        class,
        retry_after_millis,
      }) => {
        result.failure = Some(class);
        result.retry_after_millis = retry_after_millis.map(|v| u64::from(v.min(3_600_000)));
        return Ok(result);
      }
      Err(error) => return Err(error),
    };
    if page.candidates.len() > usize::from(query.limit) {
      return Err(ApplicationError::invalid());
    }
    let mut seen = BTreeSet::new();
    for summary in page.candidates {
      if summary.item.resource != choice.native_id
        || !seen.insert(summary.item.item.clone())
        || !valid_summary(&summary, &input_ids)
      {
        return Err(ApplicationError::invalid());
      }
      let native_correlation = source_native_correlation(
        source_native_issue(
          query.intent.project_id,
          input.definition.source.installation.identity(),
          &summary.item,
        ),
        source_security_partition(&page.security_scope.to_store()),
      );
      let matches = summary.selections.iter().filter(|s| s.reasons.is_empty()).count();
      let disposition = match matches {
        0 => WorkSourcePreviewDisposition::Filtered,
        1 => WorkSourcePreviewDisposition::Selected,
        _ if grouped => WorkSourcePreviewDisposition::Ambiguous,
        _ => WorkSourcePreviewDisposition::Selected,
      };
      let issue = self
        .visible_issue_link(context, query.intent.project_id, native_correlation)
        .await;
      result.candidates.push(WorkSourcePreviewCandidate {
        native_correlation,
        summary,
        disposition,
        admissions: vec![],
        issue,
      });
    }
    if !result.candidates.is_empty() {
      let issues = result
        .candidates
        .iter()
        .map(|c| {
          source_native_issue(
            query.intent.project_id,
            input.definition.source.installation.identity(),
            &c.summary.item,
          )
        })
        .collect();
      let associations = self
        .diagnostic_correlations(
          context,
          query.intent.project_id,
          WorkSourceDiagnosticFilter::Native {
            issues,
            security_scope: source_security_partition(&page.security_scope.to_store()),
          },
          None,
          64,
        )
        .await?;
      result.admissions_truncated = associations.next_cursor.is_some();
      for admission in self
        .diagnostic_admissions(context, query.intent.project_id, associations.items)
        .await?
      {
        if let Some(candidate) = result
          .candidates
          .iter_mut()
          .find(|c| c.native_correlation == admission.association.native_correlation)
        {
          candidate.admissions.push(admission);
        }
      }
    }
    result.has_more = page.has_more;
    Ok(result)
  }
}
fn valid_summary(summary: &WorkSourcePreviewItem, inputs: &[WorkSourceInputId]) -> bool {
  fn bounded(text: &str, max: usize) -> bool {
    !text.is_empty() && text.len() <= max && !text.chars().any(char::is_control)
  }
  bounded(&summary.display_id, 256)
    && bounded(&summary.title, MAX_WORK_SOURCE_PREVIEW_TITLE_BYTES)
    && bounded(&summary.state, 128)
    && summary.labels.len() <= 64
    && summary.labels.iter().all(|l| bounded(l, 256))
    && summary.labels.iter().collect::<BTreeSet<_>>().len() == summary.labels.len()
    && summary.selections.len() == inputs.len()
    && summary
      .selections
      .iter()
      .all(|s| inputs.contains(&s.input_id) && s.reasons.iter().collect::<BTreeSet<_>>().len() == s.reasons.len())
    && summary
      .selections
      .iter()
      .map(|s| s.input_id)
      .collect::<BTreeSet<_>>()
      .len()
      == inputs.len()
}
