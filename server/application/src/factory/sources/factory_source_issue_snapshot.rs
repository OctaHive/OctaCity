//! Canonical Issue resolution for automatic snapshot input; no producer lookup or provider writes.
use super::*;
use octacity_server_factory::{FactoryDigest, FactoryFlowConfiguration};
use octacity_server_store::*;

/// Installed server reader for the Source's own scoped canonical Issue.
#[async_trait]
pub trait FactorySourceIssueSnapshots: Send + Sync {
  /// Uses the same field/readiness builder as manual Issue execution.
  async fn resolve(
    &self,
    authorized: &FactorySourceAuthorization,
    input: &WorkSourceInput,
    item: &FactorySourceItem,
    flow: &FactoryFlowConfiguration,
    source: &RepositorySourceSelection,
  ) -> Result<ProjectTextDocuments, ApplicationError>;
}

/// One input mode selected from a single authorized target resolution.
pub enum FactorySourcePreparedInput {
  /// Ordinary installed provider mapping.
  Provider,
  /// Canonical Issue requirements and selected fields.
  Issue(Box<ProjectTextDocuments>),
  /// Explicit exact accepted-result handoff.
  Predecessor(Box<FactorySourceHandoff>),
}

/// Composes native links, canonical Issue revisions and Project field definitions.
pub struct IssueSourceSnapshotReader<I> {
  issues: Arc<I>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
  materials: Option<Arc<crate::IssueSnapshotMaterialReaders>>,
}
impl<I> IssueSourceSnapshotReader<I> {
  /// Starts with body reads disabled, independently of Issue visibility.
  pub fn new(issues: Arc<I>, policy: Arc<dyn ManagementAuthorizationPolicy>) -> Self {
    Self {
      issues,
      policy,
      materials: None,
    }
  }
  /// Installs owner-aware readers; only the published Flow's bounded material contract consumes them.
  pub fn with_materials(mut self, readers: Arc<crate::IssueSnapshotMaterialReaders>) -> Self {
    self.materials = Some(readers);
    self
  }
}
#[async_trait]
impl<I: IssueSourceStore + IssuePropertyStore> FactorySourceIssueSnapshots for IssueSourceSnapshotReader<I> {
  async fn resolve(
    &self,
    authorized: &FactorySourceAuthorization,
    input: &WorkSourceInput,
    item: &FactorySourceItem,
    flow: &FactoryFlowConfiguration,
    source: &RepositorySourceSelection,
  ) -> Result<ProjectTextDocuments, ApplicationError> {
    let project = input.project_id();
    if input.id() != authorized.input.id() || project != authorized.input.project_id() {
      return Err(ApplicationError::invalid());
    }
    let scope = authorized.context.security_scope().to_store();
    let native = source_native_correlation(
      source_native_issue(project, input.definition().source.installation.identity(), item),
      source_security_partition(&scope),
    );
    let link = self
      .issues
      .native_issue_source_link(project, &scope, native)
      .await?
      .ok_or_else(|| snapshot_unavailable("issue_snapshot_missing_issue"))?;
    crate::factory::authoring::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      &authorized.context,
      crate::ManagementResourceKind::Issue,
      link.issue_id,
    )
    .await?;
    let issue = self.issues.issue(project, link.issue_id, None).await?;
    let definition = input.definition();
    let target = IssueExecutionTarget {
      configuration_id: definition.target.id(),
      configuration_version: definition.target.version(),
      repository_id: definition.repository.id,
      repository_version: definition.repository.version,
      source: source.clone(),
      mapping: IssueExecutionMapping::TitleDescriptionAcceptanceV1,
      predecessor_run: None,
    };
    let snapshot = crate::issues::issue_execution_snapshot::select(
      self.issues.as_ref(),
      crate::issues::issue_execution_snapshot::IssueSnapshotReadContext {
        policy: self.policy.as_ref(),
        context: &authorized.context,
        materials: self.materials.as_deref(),
      },
      &issue,
      &target,
      flow,
      None,
    )
    .await?;
    let documents = crate::issues::issue_execution::issue_execution_documents(&issue.content, &target.mapping)?;
    Ok(documents.with_issue_snapshot(snapshot)?)
  }
}

pub(super) fn snapshot_unavailable(code: &'static str) -> ApplicationError {
  crate::FactoryValidationDiagnostics::unavailable_with_code("/issue_snapshot", code)
}

impl<S: FactoryAdmissionStore + 'static> FactorySourceAdmissionHandlers<S> {
  /// Pending targets resolve current canonical data; admitted snapshots stay frozen.
  pub async fn prepared_source_input(
    &self,
    authorized: &FactorySourceAuthorization,
    claim: &WorkSourceScanClaim,
    preparation: &WorkSourcePreparation,
    source: &RepositorySourceSelection,
    expected_record: Option<FactoryDigest>,
  ) -> Result<FactorySourcePreparedInput, ApplicationError> {
    preparation.require_selected_owner(claim, &authorized.input)?;
    validate_authorization(
      authorized,
      authorized.input.id(),
      authorized.input.version(),
      &preparation.item,
    )?;
    let selected = self
      .admission
      .require_source_launch(&authorized.context, &preparation.input)
      .await?;
    let flow = selected.configuration.configuration.flow();
    if flow.issue_snapshot.is_none() {
      let target = FactorySourcePredecessorTarget {
        expected_record,
        search: PredecessorSearch::Page(preparation.predecessor_cursor),
        ..FactorySourcePredecessorTarget::new(
          selected.repository.as_ref().ok_or_else(ApplicationError::invalid)?,
          flow,
          Some(source),
          self.admission.revisions.as_ref(),
        )
      };
      let handoff = self
        .source_handoff_for_target(authorized, &preparation.item, &preparation.input, target)
        .await?;
      require_prepared_handoff(preparation, handoff.as_ref())?;
      return Ok(match handoff {
        Some(handoff) => FactorySourcePreparedInput::Predecessor(Box::new(handoff)),
        None => FactorySourcePreparedInput::Provider,
      });
    }
    if preparation.admission.is_some()
      && let Some(documents) = &preparation.issue_input
    {
      return Ok(FactorySourcePreparedInput::Issue(Box::new(documents.clone())));
    }
    let reader = self
      .issue_snapshots
      .as_ref()
      .ok_or_else(|| snapshot_unavailable("issue_snapshot_missing_issue"))?;
    Ok(FactorySourcePreparedInput::Issue(Box::new(
      reader
        .resolve(authorized, &preparation.input, &preparation.item, flow, source)
        .await?,
    )))
  }

  pub(super) async fn require_current_issue_input(
    &self,
    authorized: &FactorySourceAuthorization,
    preparation: &WorkSourcePreparation,
  ) -> Result<(), ApplicationError> {
    let Some(original) = &preparation.issue_input else {
      return Ok(());
    };
    let intent = preparation.intent.as_ref().ok_or_else(ApplicationError::invalid)?;
    let selected = self
      .admission
      .require_source_launch(&authorized.context, &preparation.input)
      .await?;
    let reader = self
      .issue_snapshots
      .as_ref()
      .ok_or_else(|| snapshot_unavailable("issue_snapshot_missing_issue"))?;
    let current = reader
      .resolve(
        authorized,
        &preparation.input,
        &preparation.item,
        selected.configuration.configuration.flow(),
        intent.source.repository()?,
      )
      .await?;
    if &current != original {
      return Err(snapshot_unavailable("issue_snapshot_stale"));
    }
    Ok(())
  }
}

/// Candidate-local input waits; unrelated Source items still use the same finite polling slice.
pub fn source_input_wait(
  error: &ApplicationError,
) -> Option<(WorkSourceWaitReason, Option<WorkSourcePredecessorCursor>)> {
  if let Some(wait) = source_predecessor_wait(error) {
    return Some(wait);
  }
  let ApplicationError::FactoryValidation(diagnostics) = error else {
    return None;
  };
  let [issue] = diagnostics.errors.as_slice() else {
    return None;
  };
  if !issue.field.starts_with("/issue_snapshot") {
    return None;
  }
  let reason = match issue.code {
    "issue_snapshot_missing_issue" | "issue_snapshot_missing_value" => WorkSourceWaitReason::Missing,
    "issue_snapshot_stale" | "issue_snapshot_inapplicable" => WorkSourceWaitReason::Stale,
    "issue_snapshot_evidence_unavailable" | "issue_snapshot_material_unavailable" => WorkSourceWaitReason::Inaccessible,
    "issue_snapshot_wrong_subject" => WorkSourceWaitReason::WrongSubject,
    "issue_snapshot_material_limit_exceeded" | "issue_snapshot_encoding_limit_exceeded" => {
      WorkSourceWaitReason::Incompatible
    }
    _ => return None,
  };
  Some((reason, None))
}
